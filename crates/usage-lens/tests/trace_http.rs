//! Same-process synthetic HTTP checks; no sockets, clients or real trace files.
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use usage_lens::{
    core::{
        UsageStore,
        trace::{TRACE_ADAPTER_VERSION, TRACE_SOURCE_VERSION},
    },
    server::http,
};

fn store(import: bool) -> UsageStore {
    populate(
        UsageStore::with_clock_ms(":memory:", 1791028800000).unwrap(),
        import,
    )
}
fn populate(store: UsageStore, import: bool) -> UsageStore {
    store.create_source(&json!({"id":"synthetic","displayName":"Synthetic trace HTTP","mode":"imported","provider":"synthetic","coverageDescription":"Synthetic only"})).unwrap();
    if import {
        store
            .update_settings(&json!({"contentCaptureEnabled":true}))
            .unwrap();
        let reported = |value: &str| json!({"state":"reported","value":value});
        let attempts: Vec<Value> = ["one", "two"].into_iter().map(|id| json!({
            "attemptId":id,"threadId":"trace-thread","turnId":"trace-turn","inferenceId":id,
            "startedAt":"2026-10-03T00:00:00Z","completedAt":"2026-10-03T00:00:01Z","status":"completed",
            "request":{"model":reported("requested-model"),"reasoningEffort":reported("high"),"serviceTier":reported("priority")},
            "observed":{"model":reported("observed-model"),"serviceTier":{"state":"not_reported","value":null}},
            "responseId":format!("response-{id}"),"upstreamRequestId":null,
            "tokens":{"inputTokens":reported("9007199254740993"),"cachedInputTokens":reported("0"),"cacheWriteInputTokens":{"state":"omitted","value":null},"outputTokens":reported("2"),"reasoningOutputTokens":{"state":"invalid","value":null},"totalTokens":{"state":"not_reported","value":null}},
            "requestProjection":{"projection":"visible_text_only","messages":[{"role":"user","text":"PRIVATE TRACE SENTINEL api_key=synthetic-secret"}]},
            "responseProjection":{"projection":"visible_text_only","messages":[{"role":"assistant","text":"VISIBLE RESPONSE SENTINEL"}]},
            "evidence":"prepared_request"
        })).collect();
        store.import_trace_bundle(&json!({"sourceId":"synthetic","fingerprint":"a".repeat(64),"adapterVersion":TRACE_ADAPTER_VERSION,"sourceVersion":TRACE_SOURCE_VERSION,"importedAt":"2026-10-03T00:00:02Z","bundleId":"synthetic-http-bundle","attempts":attempts,"warningCodes":[]})).unwrap();
    }
    store
}
async fn get(
    app: axum::Router,
    path: &str,
    headers: &[(&str, &str)],
) -> (u16, Value, axum::http::HeaderMap) {
    let mut request = Request::builder()
        .uri(path)
        .header("host", "127.0.0.1:4319");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = app
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap(), headers)
}

#[tokio::test]
async fn local_trace_routes_keep_projection_content_in_detail_and_summary_aggregate_only() {
    let app = http::router(store(true), 4319);
    let (status, list, headers) =
        get(app.clone(), "/api/traces?sourceId=synthetic&limit=1", &[]).await;
    assert_eq!(status, 200);
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(headers["cross-origin-resource-policy"], "same-origin");
    assert_eq!(list["source"]["id"], "synthetic");
    assert_eq!(list["attempts"].as_array().unwrap().len(), 1);
    assert_eq!(
        list["attempts"][0]["request"]["model"]["value"],
        "requested-model"
    );
    assert_eq!(
        list["attempts"][0]["observed"]["model"]["value"],
        "observed-model"
    );
    assert!(!list.to_string().contains("SENTINEL"));
    assert!(!list.to_string().contains("requestProjection"));
    let cursor = list["nextCursor"].as_str().unwrap();
    let (status, next, _) = get(
        app.clone(),
        &format!("/api/traces?sourceId=synthetic&limit=1&cursor={cursor}"),
        &[],
    )
    .await;
    assert_eq!(status, 200);
    assert_ne!(
        list["attempts"][0]["attemptId"],
        next["attempts"][0]["attemptId"]
    );
    assert!(next["nextCursor"].is_null());
    let (status, summary, _) = get(
        app.clone(),
        "/api/traces/summary?sourceId=synthetic&fromDate=2026-10-01&toDate=2026-10-07",
        &[],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(summary["attemptCount"], "2");
    assert_eq!(summary["totals"]["inputTokens"], "18014398509481986");
    assert!(summary["totals"]["totalTokens"].is_null());
    assert_eq!(summary["byRequestedServiceTier"][0]["value"], "priority");
    for hidden in [
        "SENTINEL",
        "sourceId",
        "attemptId",
        "turnId",
        "responseId",
        "response-one",
        "synthetic-http-bundle",
    ] {
        assert!(!summary.to_string().contains(hidden), "leaked {hidden}");
    }
    assert!(summary["threadId"].is_null());
    // Thread identifiers are intentional in the separate local-only summary.
    assert_eq!(summary["byThread"][0]["threadId"], "trace-thread");
    let (status, detail, _) = get(
        app.clone(),
        "/api/traces/detail?sourceId=synthetic&attemptId=one",
        &[],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(detail["attempt"]["sourceId"], "synthetic");
    assert_eq!(detail["attempt"]["attemptId"], "one");
    assert_eq!(detail["attempt"]["evidence"], "prepared_request");
    assert_eq!(detail["contentRetained"], true);
    assert!(detail.to_string().contains("PRIVATE TRACE SENTINEL"));
    assert!(!detail.to_string().contains("synthetic-secret"));
    assert!(detail.to_string().contains("REDACTED"));
    let (status, empty, _) = get(
        app,
        "/api/traces?sourceId=synthetic&fromDate=2026-10-04&toDate=2026-10-05",
        &[],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(empty["attempts"], json!([]));
}

#[tokio::test]
async fn trace_routes_enforce_loopback_scope_query_shapes_and_old_schema_read_only_behavior() {
    let app = http::router(store(false), 4319);
    for route in ["traces", "traces/summary"] {
        let (status, value, _) = get(
            app.clone(),
            &format!("/api/{route}?sourceId=synthetic"),
            &[],
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(value["coverage"]["capture"], "not_captured");
        let (status, value, _) = get(
            app.clone(),
            &format!("/api/{route}?sourceId=synthetic"),
            &[("origin", "https://untrusted.invalid")],
        )
        .await;
        assert_eq!(status, 403);
        assert_eq!(value["error"]["code"], "invalid_origin");
    }
    let (status, missing, _) = get(
        app.clone(),
        "/api/traces/detail?sourceId=synthetic&attemptId=one",
        &[],
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(missing["error"]["code"], "trace_attempt_not_found");
    for path in [
        "/api/traces",
        "/api/traces/detail?sourceId=synthetic",
        "/api/traces/detail?attemptId=one",
        "/api/traces?sourceId=synthetic&sourceId=other",
        "/api/traces?sourceId=synthetic&limit=abc",
        "/api/traces?sourceId=synthetic&content=true",
        "/api/traces/summary?sourceId=synthetic&limit=1",
        "/api/traces/detail?sourceId=synthetic&attemptId=one&cursor=x",
        "/api/traces?sourceId=synthetic&fromDate=2026-10-01",
        "/api/traces/summary?sourceId=synthetic&fromDate=2026-10-07&toDate=2026-10-01",
        "/api/traces?sourceId=synthetic&cursor=malformed",
    ] {
        let (status, _, _) = get(app.clone(), path, &[]).await;
        assert_eq!(status, 400, "{path}");
    }
    let (status, _, _) = get(
        app.clone(),
        "/api/traces/detail?sourceId=synthetic&attemptId=one",
        &[("sec-fetch-site", "cross-site")],
    )
    .await;
    assert_eq!(status, 403);
    let (status, value, _) = get(app, "/api/status", &[]).await;
    assert_eq!(status, 200);
    assert_eq!(value["schemaVersion"], 2);
}

#[tokio::test]
async fn trace_http_filters_echo_exact_scope_and_bind_cursor_across_routes() {
    let app = http::router(store(true), 4319);
    let scope = "sourceId=synthetic&threadId=trace-thread&status=completed&requestedModel=requested-model&requestedReasoningEffort=high&requestedServiceTier=priority&fromDate=2026-10-03&toDate=2026-10-03";
    let (status, list, _) = get(app.clone(), &format!("/api/traces?{scope}&limit=1"), &[]).await;
    assert_eq!(status, 200);
    assert_eq!(list["attempts"][0]["attemptId"], "two");
    let (status, summary, _) = get(app.clone(), &format!("/api/traces/summary?{scope}"), &[]).await;
    assert_eq!(status, 200);
    assert_eq!(summary["attemptCount"], "2");
    for (key, value) in [
        ("threadId", "trace-thread"),
        ("status", "completed"),
        ("requestedModel", "requested-model"),
        ("requestedReasoningEffort", "high"),
        ("requestedServiceTier", "priority"),
    ] {
        assert_eq!(list[key], value);
        assert_eq!(summary[key], value);
    }
    assert!(!summary.to_string().contains("attemptId"));
    assert!(!summary.to_string().contains("SENTINEL"));
    let cursor = list["nextCursor"].as_str().unwrap();
    let (status, next, _) = get(
        app.clone(),
        &format!("/api/traces?{scope}&limit=1&cursor={cursor}"),
        &[],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(next["attempts"][0]["attemptId"], "one");
    assert!(next["nextCursor"].is_null());
    let (status, error, _) = get(
        app.clone(),
        &format!(
            "/api/traces?{}&cursor={cursor}",
            scope.replace(
                "requestedServiceTier=priority",
                "requestedServiceTier=default"
            )
        ),
        &[],
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(error["error"]["code"], "invalid_input");
    for route in ["traces", "traces/summary"] {
        for filters in [
            "threadId=trace-Thread",
            "status=failed",
            "requestedModel=observed-model",
            "requestedReasoningEffort=HIGH",
            "requestedServiceTier=default",
            "requestedModel=requested-model%20",
            "requestedModel=x%27%20OR%201%3D1%20--",
        ] {
            let (status, value, _) = get(
                app.clone(),
                &format!("/api/{route}?sourceId=synthetic&{filters}"),
                &[],
            )
            .await;
            assert_eq!(status, 200, "{route} {filters}");
            if route == "traces" {
                assert_eq!(value["attempts"], json!([]));
            } else {
                assert_eq!(value["attemptCount"], "0");
            }
        }
    }
}

#[tokio::test]
async fn trace_http_rejects_duplicate_invalid_and_non_trace_filter_parameters() {
    let app = http::router(store(false), 4319);
    for route in ["traces", "traces/summary"] {
        for filters in [
            "threadId=one&threadId=two",
            "status=completed&status=failed",
            "requestedModel=a&requestedModel=b",
            "requestedReasoningEffort=high&requestedReasoningEffort=low",
            "requestedServiceTier=priority&requestedServiceTier=default",
            "status=unknown",
            "status=COMPLETED",
            "threadId=",
            "threadId=bad%20id",
            "requestedModel=",
            "requestedModel=bad%00value",
            "requestedReasoningEffort=bad%7Fvalue",
            "requestedServiceTier=bad%0Avalue",
            "requestedEffort=high",
            "observedModel=observed-model",
        ] {
            let (status, _, _) = get(
                app.clone(),
                &format!("/api/{route}?sourceId=synthetic&{filters}"),
                &[],
            )
            .await;
            assert_eq!(status, 400, "{route} {filters}");
        }
        let (status,value,_) = get(app.clone(), &format!("/api/{route}?sourceId=synthetic&requestedModel=model&threadId=thread&status=incomplete&requestedReasoningEffort=high&requestedServiceTier=priority"), &[]).await;
        assert_eq!(status, 200);
        assert_eq!(value["coverage"]["capture"], "not_captured");
    }
    for path in [
        "/api/traces/detail?sourceId=synthetic&attemptId=one&threadId=trace-thread",
        "/api/overview?sourceId=synthetic&threadId=trace-thread",
        "/api/skill-summary?sourceId=synthetic&requestedModel=requested-model",
        "/api/response-tokens?sourceId=synthetic&requestedServiceTier=priority",
    ] {
        assert_eq!(get(app.clone(), path, &[]).await.0, 400, "{path}");
    }
    assert_eq!(get(app, "/api/status", &[]).await.1["schemaVersion"], 2);
}

#[tokio::test]
async fn maximum_encoded_trace_scope_and_cursor_fit_a_trace_only_bounded_uri_budget() {
    let store = store(false);
    let source = format!("a{}", ":".repeat(79));
    let thread = format!("a{}", ":".repeat(159));
    let metadata = "模".repeat(128);
    store.create_source(&json!({"id":source,"displayName":"Maximum synthetic filters","mode":"imported","provider":"synthetic","coverageDescription":"Synthetic only"})).unwrap();
    let rows: Vec<Value> = ["a", "b"].into_iter().map(|id| json!({
        "attemptId":id.repeat(160),"threadId":thread,"turnId":"turn","inferenceId":id,
        "startedAt":"2026-10-03T00:00:00Z","completedAt":"2026-10-03T00:00:00Z","status":"completed",
        "request":{"model":{"state":"reported","value":metadata},"reasoningEffort":{"state":"reported","value":metadata},"serviceTier":{"state":"reported","value":metadata}},
        "observed":{"model":{"state":"not_reported","value":null},"serviceTier":{"state":"not_reported","value":null}},
        "responseId":null,"upstreamRequestId":null,"tokens":null,"requestProjection":null,"responseProjection":null,"evidence":"prepared_request"
    })).collect();
    store.import_trace_bundle(&json!({"sourceId":source,"fingerprint":"f".repeat(64),"adapterVersion":TRACE_ADAPTER_VERSION,"sourceVersion":TRACE_SOURCE_VERSION,"importedAt":"2026-10-03T00:00:02Z","bundleId":"maximum-http","attempts":rows,"warningCodes":[]})).unwrap();
    let app = http::router(store, 4319);
    let mut params = vec![
        ("sourceId", source.as_str()),
        ("threadId", thread.as_str()),
        ("status", "completed"),
        ("requestedModel", metadata.as_str()),
        ("requestedReasoningEffort", metadata.as_str()),
        ("requestedServiceTier", metadata.as_str()),
        ("fromDate", "2026-10-03"),
        ("toDate", "2026-10-03"),
    ];
    let query = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(&params)
        .finish();
    let summary_path = format!("/api/traces/summary?{query}");
    assert!(summary_path.len() > 4096);
    let (status, summary, _) = get(app.clone(), &summary_path, &[]).await;
    assert_eq!(status, 200, "{summary}");
    assert_eq!(summary["attemptCount"], "2");
    let list_path = format!("/api/traces?{query}&limit=1");
    assert!(list_path.len() > 4096);
    let (status, first, _) = get(app.clone(), &list_path, &[]).await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["attempts"][0]["attemptId"], "b".repeat(160));
    params.extend([
        ("limit", "1"),
        ("cursor", first["nextCursor"].as_str().unwrap()),
    ]);
    // Clients may percent-encode every byte, including ASCII parameter names and
    // the returned base64url cursor. This is still the same valid exact scope.
    let encode = |value: &str| {
        value
            .bytes()
            .map(|byte| format!("%{byte:02X}"))
            .collect::<String>()
    };
    let encoded_query = params
        .iter()
        .map(|(key, value)| format!("{}={}", encode(key), encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    let page_path = format!("/api/traces?{encoded_query}");
    assert!(page_path.len() > 4096 && page_path.len() <= 8192);
    let (status, next, _) = get(app.clone(), &page_path, &[]).await;
    assert_eq!(status, 200, "{next}");
    assert_eq!(next["attempts"][0]["attemptId"], "a".repeat(160));
    assert!(next["nextCursor"].is_null());
    for route in ["/api/traces", "/api/traces/summary"] {
        let oversized = format!("{route}?{}", "x".repeat(8193 - route.len() - 1));
        assert_eq!(oversized.len(), 8193);
        let (status, error, _) = get(app.clone(), &oversized, &[]).await;
        assert_eq!(status, 400);
        assert_eq!(error["error"]["code"], "invalid_path");
    }
    for route in [
        "/api/status",
        "/api/events",
        "/api/traces/detail",
        "/api/traces/other",
    ] {
        let oversized = format!("{route}?{}", "x".repeat(4097 - route.len() - 1));
        assert_eq!(oversized.len(), 4097);
        let (status, error, _) = get(app.clone(), &oversized, &[]).await;
        assert_eq!(status, 400);
        assert_eq!(error["error"]["code"], "invalid_path", "{route}");
    }
}

#[tokio::test]
async fn trace_timeline_order_is_local_explicit_and_cursor_bound() {
    let app = http::router(store(true), 4319);
    let scope = "sourceId=synthetic&threadId=trace-thread&order=oldest_first";
    let (status, first, _) = get(app.clone(), &format!("/api/traces?{scope}&limit=1"), &[]).await;
    assert_eq!(status, 200);
    assert_eq!(first["order"], "oldest_first");
    assert_eq!(first["attempts"][0]["attemptId"], "one");
    assert_eq!(first["attempts"][0]["timestampAnomaly"], false);
    let cursor = first["nextCursor"].as_str().unwrap();
    let (status, next, _) = get(
        app.clone(),
        &format!("/api/traces?{scope}&limit=1&cursor={cursor}"),
        &[],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(next["attempts"][0]["attemptId"], "two");
    assert!(next["nextCursor"].is_null());
    for order in ["", "&order=newest_first"] {
        assert_eq!(
            get(
                app.clone(),
                &format!(
                    "/api/traces?sourceId=synthetic&threadId=trace-thread{order}&cursor={cursor}"
                ),
                &[]
            )
            .await
            .0,
            400
        );
    }
    for path in [
        "/api/traces?sourceId=synthetic&order=oldest_first&order=newest_first",
        "/api/traces?sourceId=synthetic&order=ASC",
        "/api/traces?sourceId=synthetic&order=oldest_first%20",
        "/api/traces?sourceId=synthetic&order=",
        "/api/traces/summary?sourceId=synthetic&order=oldest_first",
        "/api/traces/detail?sourceId=synthetic&attemptId=one&order=oldest_first",
        "/api/events?sourceId=synthetic&order=oldest_first",
    ] {
        assert_eq!(get(app.clone(), path, &[]).await.0, 400, "{path}");
    }
    let (status, summary, _) = get(app.clone(), "/api/traces/summary?sourceId=synthetic&threadId=trace-thread&requestedModel=requested-model", &[]).await;
    assert_eq!(status, 200);
    for key in ["byThread", "byRequestedSettings", "byDay"] {
        assert_eq!(summary[key].as_array().unwrap().len(), 1);
        assert_eq!(summary[key][0]["count"], "2");
        assert_eq!(
            summary[key][0]["totals"]["inputTokens"],
            "18014398509481986"
        );
        assert_eq!(summary[key][0]["timestampAnomalyCount"], "0");
    }
    assert_eq!(summary["byThread"][0]["statusCounts"]["completed"], "2");
    assert_eq!(summary["byThread"][0]["statusCounts"]["failed"], "0");
    assert_eq!(
        summary["byRequestedSettings"][0]["request"]["serviceTier"]["value"],
        "priority"
    );
    assert_eq!(summary["byDay"][0]["date"], "2026-10-03");
    assert!(summary.get("order").is_none());
    assert_eq!(summary["timestampAnomalyCount"], "0");
    for key in [
        "threadsTruncated",
        "requestedSettingsTruncated",
        "daysTruncated",
    ] {
        assert_eq!(summary[key], false);
    }
    assert!(!summary.to_string().contains("SENTINEL"));
    assert_eq!(
        get(
            app.clone(),
            "/api/traces?sourceId=synthetic&order=oldest_first",
            &[("origin", "https://untrusted.invalid")]
        )
        .await
        .0,
        403
    );
    assert_eq!(get(app, "/api/status", &[]).await.1["schemaVersion"], 4);
}

#[tokio::test]
async fn trace_cli_exposes_order_and_insights_without_writing_or_expanding_other_queries() {
    use usage_lens::cli::{parse_arguments, run_main};
    async fn cli(args: &[&str]) -> (i32, Value, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let args = args
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let code = run_main(&args, &b""[..], &mut stdout, &mut stderr).await;
        (
            code,
            serde_json::from_slice(&stdout).unwrap_or(Value::Null),
            String::from_utf8(stderr).unwrap(),
        )
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic-insights.sqlite");
    drop(populate(UsageStore::open(&path).unwrap(), true));
    let before = std::fs::read(&path).unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let db = path.to_str().unwrap();
    let (code, first, error) = cli(&[
        "trace-attempts",
        "--db",
        db,
        "--source",
        "synthetic",
        "--thread",
        "trace-thread",
        "--order",
        "oldest_first",
        "--limit",
        "1",
    ])
    .await;
    assert_eq!(code, 0, "{error}");
    assert_eq!(first["order"], "oldest_first");
    assert_eq!(first["attempts"][0]["attemptId"], "one");
    let cursor = first["nextCursor"].as_str().unwrap();
    let (code, next, error) = cli(&[
        "trace-attempts",
        "--db",
        db,
        "--source",
        "synthetic",
        "--thread",
        "trace-thread",
        "--order",
        "oldest_first",
        "--cursor",
        cursor,
    ])
    .await;
    assert_eq!(code, 0, "{error}");
    assert_eq!(next["attempts"][0]["attemptId"], "two");
    assert!(next["nextCursor"].is_null());
    let (code, default, error) =
        cli(&["trace-attempts", "--db", db, "--source", "synthetic"]).await;
    assert_eq!(code, 0, "{error}");
    assert_eq!(default["order"], "newest_first");
    assert_eq!(default["attempts"][0]["attemptId"], "two");
    let (code, summary, error) = cli(&[
        "trace-summary",
        "--db",
        db,
        "--source",
        "synthetic",
        "--from",
        "2026-10-03",
        "--to",
        "2026-10-03",
        "--requested-effort",
        "high",
    ])
    .await;
    assert_eq!(code, 0, "{error}");
    assert_eq!(summary["byThread"][0]["count"], "2");
    assert_eq!(
        summary["byRequestedSettings"][0]["request"]["reasoningEffort"]["value"],
        "high"
    );
    assert_eq!(summary["byDay"][0]["date"], "2026-10-03");
    assert!(!summary.to_string().contains("SENTINEL"));
    for order in ["bad", "OLDEST_FIRST", "newest_first "] {
        let (code, output, error) = cli(&[
            "trace-attempts",
            "--db",
            db,
            "--source",
            "synthetic",
            "--order",
            order,
        ])
        .await;
        assert_eq!(code, 1);
        assert!(output.is_null());
        assert!(error.contains("invalid_input"));
    }
    for command in ["trace-summary", "trace-detail", "events", "overview", "mcp"] {
        let args = [command, "--order", "oldest_first"].map(str::to_string);
        assert!(parse_arguments(&args).is_err(), "{command}");
    }
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
