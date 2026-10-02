use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use usage_lens::{
    adapters::demo::seed_demo,
    core::UsageStore,
    server::{http, mcp},
};

const NOW: i64 = 1790920800000;
fn demo() -> UsageStore {
    let store = UsageStore::with_clock_ms(":memory:", NOW).unwrap();
    seed_demo(&store, NOW).unwrap();
    store
}
async fn request(
    app: axum::Router,
    path: &str,
    method: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> (u16, Value, axum::http::HeaderMap, String) {
    let mut builder = Request::builder()
        .uri(path)
        .method(method)
        .header("host", "127.0.0.1:4319");
    for (key, value) in headers {
        builder = builder.header(*key, *value);
    }
    let response = app
        .oneshot(builder.body(Body::from(body.to_owned())).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).to_string();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        headers,
        text,
    )
}
const MUTATION: [(&str, &str); 3] = [
    ("origin", "http://127.0.0.1:4319"),
    ("content-type", "application/json"),
    ("x-usage-lens-request", "local-ui"),
];
#[tokio::test]
async fn http_queries_mutations_and_exact_embedded_assets() {
    let app = http::router(demo(), 4319);
    for path in [
        "/api/status",
        "/api/settings",
        "/api/response-tokens?sourceId=demo&fromDate=2026-09-01&toDate=2026-10-02&model=demo-model-a",
        "/api/response-tokens/records?sourceId=demo&limit=1",
        "/api/overview?sourceId=demo",
        "/api/quota?sourceId=demo&maxAgeMs=1",
        "/api/daily?sourceId=demo&fromDate=2026-09-01&toDate=2026-10-02",
        "/api/events?sourceId=demo&fromDate=2026-10-01&toDate=2026-10-02&eventType=tool_call&model=demo-model-a&limit=1",
        "/api/skills?sourceId=demo&kind=loaded&limit=1",
        "/api/tools?sourceId=demo",
        "/api/quota/history?sourceId=demo&limit=2&maxAgeMs=1",
        "/api/events/detail?sourceId=demo&eventId=demo-event-0",
        "/api/events/search?sourceId=demo&query=Synthetic&limit=2",
    ] {
        let (status, value, _, text) = request(app.clone(), path, "GET", "", &[]).await;
        assert_eq!(status, 200, "{path}: {text}");
        assert!(value.is_object());
    }
    for (path, body) in [
        (
            "/api/settings",
            json!({"capturePaused":true,"contentCaptureEnabled":false,"retentionDays":7}),
        ),
        (
            "/api/delete",
            json!({"target":"content","sourceId":"demo","confirmation":"DELETE"}),
        ),
        ("/api/retention", json!({"confirmation":"APPLY RETENTION"})),
        (
            "/api/delete",
            json!({"target":"all","confirmation":"DELETE"}),
        ),
    ] {
        assert_eq!(
            request(app.clone(), path, "POST", &body.to_string(), &MUTATION)
                .await
                .0,
            200
        );
    }
    let (status, _, headers, text) = request(app.clone(), "/", "GET", "", &[]).await;
    assert_eq!(status, 200);
    assert!(text.contains("Usage Lens"));
    assert_eq!(headers["x-frame-options"], "DENY");
    assert_eq!(headers["cache-control"], "no-store");
    assert!(
        headers["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("'wasm-unsafe-eval'")
    );
    assert!(headers.get("access-control-allow-origin").is_none());
    for path in ["/usage_lens_ui.js", "/usage_lens_ui_bg.wasm"] {
        assert_eq!(request(app.clone(), path, "GET", "", &[]).await.0, 200);
    }
    assert_eq!(
        http::embedded_asset("/usage_lens_ui_bg.wasm").unwrap().0,
        "application/wasm"
    );
}
#[tokio::test]
async fn http_rejects_injection_csrf_traversal_oversized_or_bad_requests() {
    let app = http::router(demo(), 4319);
    for path in [
        "/api/unknown",
        "/missing",
        "/assets/missing.js",
        "/assets",
        "/?query=x",
        "/assets/app.js.map",
    ] {
        assert_eq!(
            request(app.clone(), path, "GET", "", &[]).await.0,
            404,
            "{}",
            path
        );
    }
    for path in [
        "/../index.html",
        "/%2e%2e/index.html",
        "/assets/%2e%2e/index.html",
        "/assets/%5csecret",
        "//evil.test/x",
        "/%00",
        "/%ZZ",
        "/%FF",
        "/api/status?sql=DROP",
        "/api/overview",
        "/api/overview?sourceId=demo&sourceId=demo",
        "/api/events?sourceId=demo&limit=-1",
        "/api/daily?sourceId=demo&fromDate=bad&toDate=bad",
        "/api/overview?sourceId=missing",
    ] {
        assert_eq!(
            request(app.clone(), path, "GET", "", &[]).await.0,
            400,
            "{}",
            path
        );
    }
    assert_eq!(
        request(
            app.clone(),
            &format!("/api/events?sourceId=demo&model={}", "x".repeat(513)),
            "GET",
            "",
            &[]
        )
        .await
        .0,
        400
    );
    assert_eq!(
        request(
            app.clone(),
            &format!("/{}", "x".repeat(4100)),
            "GET",
            "",
            &[]
        )
        .await
        .0,
        400
    );
    for (key, value) in [
        ("host", "attacker.test"),
        ("origin", "https://attacker.test"),
        ("origin", "null"),
        ("sec-fetch-site", "cross-site"),
    ] {
        assert_eq!(
            request(app.clone(), "/api/status", "GET", "", &[(key, value)])
                .await
                .0,
            403
        );
    }
    assert_eq!(
        request(app.clone(), "/api/settings", "POST", "{}", &[])
            .await
            .0,
        403
    );
    assert_eq!(
        request(
            app.clone(),
            "/api/settings",
            "POST",
            "{}",
            &[MUTATION[0], MUTATION[2]]
        )
        .await
        .0,
        415
    );
    for (path, body) in [
        ("/api/delete", "{}"),
        ("/api/retention", "{}"),
        ("/api/settings", "{SECRET"),
        ("/api/settings", "[]"),
        ("/api/settings", "{\"unknown\":true}"),
        ("/api/settings", "{\"retentionDays\":-1}"),
        ("/api/settings?x=1", "{}"),
        (
            "/api/delete",
            "{\"target\":\"all\",\"sourceId\":3,\"confirmation\":\"DELETE\"}",
        ),
    ] {
        let (status, _, _, text) = request(app.clone(), path, "POST", body, &MUTATION).await;
        assert_eq!(status, 400, "{path} {body}: {text}");
        assert!(!text.contains("SECRET"));
    }
    assert_eq!(
        request(
            app.clone(),
            "/api/settings",
            "POST",
            &"x".repeat(17000),
            &MUTATION
        )
        .await
        .0,
        413
    );
    assert_eq!(
        request(app.clone(), "/api/unknown", "POST", "{}", &MUTATION)
            .await
            .0,
        404
    );
    assert_eq!(
        request(app.clone(), "/api/status", "DELETE", "", &[])
            .await
            .0,
        405
    );
    assert!(http::validate_host(Some("0.0.0.0")).is_err());
    assert!(http::validate_host(None).is_ok());
    assert!(http::validate_host(Some("127.0.0.1")).is_ok());
}
#[tokio::test]
async fn port_eighty_is_canonical_without_privileged_bind() {
    assert_eq!(http::canonical_origin(80), "http://127.0.0.1");
    let response = http::router(demo(), 80)
        .oneshot(
            Request::builder()
                .uri("/api/status")
                .header("host", "127.0.0.1")
                .header("origin", "http://127.0.0.1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}
fn initialize(session: &mut mcp::McpSession, store: &UsageStore) -> Value {
    session.handle(store,json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"0"}}})).unwrap()
}
#[test]
fn mcp_exact_allowlist_aggregates_only_and_safe_failures() {
    let store = demo();
    let mut session = mcp::McpSession::default();
    let initialized = initialize(&mut session, &store);
    assert_eq!(initialized["result"]["serverInfo"]["name"], "usage-lens");
    assert_eq!(
        initialized["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    let list = session
        .handle(
            &store,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .unwrap();
    let tools = list["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 7);
    for tool in tools {
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        let name = tool["name"].as_str().unwrap();
        let args = match name {
            "usage_status" => json!({}),
            "usage_daily" => {
                json!({"sourceId":"demo","fromDate":"2026-09-01","toDate":"2026-10-02"})
            }
            _ => json!({"sourceId":"demo"}),
        };
        let result=session.handle(&store,json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":name,"arguments":args}})).unwrap();
        assert!(result["error"].is_null(), "{result}");
        assert!(result["result"]["isError"].is_null(), "{result}");
        for secret in [
            "Synthetic request:",
            "toolArguments",
            "toolResult",
            "example.csv",
            "Synthetic response:",
        ] {
            assert!(!result.to_string().contains(secret));
        }
    }
    for (name, args) in [
        ("usage_status", json!({"path":"/private"})),
        ("usage_overview", json!({"sourceId":"missing"})),
        ("collect", json!({})),
        ("usage_overview", json!({"sourceId":"demo","maxAgeMs":-1})),
        ("usage_daily", json!({"sourceId":"demo","fromDate":"x"})),
        ("usage_tools", json!({"sourceId":"demo","model":3})),
        (
            "usage_tools",
            json!({"sourceId":"demo","model":"x".repeat(161)}),
        ),
        (
            "usage_daily",
            json!({"sourceId":"demo","fromDate":3,"toDate":"2026-10-02"}),
        ),
        ("usage_overview", json!({"sourceId":"/bad"})),
        ("usage_overview", json!({"sourceId":""})),
        ("usage_overview", json!({})),
        ("usage_overview", json!([])),
    ] {
        let r=session.handle(&store,json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":name,"arguments":args}})).unwrap();
        assert_eq!(r["result"]["isError"], true, "{r}");
    }
    for message in [
        Value::Null,
        json!([]),
        json!({"jsonrpc":"1.0","id":1,"method":"ping"}),
        json!({"jsonrpc":"2.0","id":{},"method":"ping"}),
        json!({"jsonrpc":"2.0","id":1,"method":"ping","params":[]}),
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        json!({"jsonrpc":"2.0","id":1,"method":"unknown"}),
    ] {
        assert!(session.handle(&store, message).unwrap()["error"].is_object());
    }
    assert!(
        session
            .handle(
                &store,
                json!({"jsonrpc":"2.0","method":"notifications/initialized"})
            )
            .is_none()
    );
    assert_eq!(
        session
            .handle(&store, json!({"jsonrpc":"2.0","id":1,"method":"ping"}))
            .unwrap()["result"],
        json!({})
    );
    assert!(
        mcp::McpSession::default()
            .handle(
                &store,
                json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})
            )
            .unwrap()["error"]
            .is_object()
    );
}
#[tokio::test]
async fn mcp_bounded_stdio_and_json_errors() {
    let store = demo();
    let mut output = Vec::new();
    mcp::serve_stdio(
        &store,
        &b"\n\t\n{bad\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n"[..],
        &mut output,
    )
    .await
    .unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("Parse error"));
    assert!(text.contains("\"result\":{}"));
    let long = vec![b'x'; 65537];
    assert_eq!(
        mcp::serve_stdio(&store, long.as_slice(), Vec::new())
            .await
            .unwrap_err()
            .0,
        "mcp_frame_limit"
    );
}

#[test]
fn mcp_version_negotiation_and_integral_decimal_arguments_match_sdk() {
    let store = demo();
    let mut session = mcp::McpSession::default();
    let response=session.handle(&store,json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"unknown-version","capabilities":{},"clientInfo":{}}})).unwrap();
    assert_eq!(response["result"]["protocolVersion"], "2025-11-25");
    let result=session.handle(&store,json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"usage_overview","arguments":{"sourceId":"demo","maxAgeMs":1.0}}})).unwrap();
    assert!(result["result"]["isError"].is_null(), "{result}");
    let result=session.handle(&store,json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"usage_daily","arguments":{"sourceId":"demo"}}})).unwrap();
    assert_eq!(result["result"]["isError"], true);
}
