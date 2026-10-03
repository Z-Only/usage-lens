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
    let store = UsageStore::with_clock_ms(":memory:", 1791028800000).unwrap();
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
        "threadId",
        "turnId",
        "responseId",
        "trace-thread",
        "response-one",
        "synthetic-http-bundle",
    ] {
        assert!(!summary.to_string().contains(hidden), "leaked {hidden}");
    }
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
