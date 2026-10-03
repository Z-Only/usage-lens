//! Synthetic-only period summaries never read transcripts or contact providers.
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{collections::BTreeMap, ffi::OsString, path::Path};
use tower::ServiceExt;
use usage_lens::{
    core::{CoreError, UsageStore, response_tokens::TOKEN_SCHEMA},
    server::http,
};

fn source(store: &UsageStore, id: &str) {
    store.create_source(&json!({"id":id,"displayName":id,"mode":"imported","provider":"user_import","coverageDescription":"partial"})).unwrap();
}
fn store() -> UsageStore {
    let store = UsageStore::in_memory().unwrap();
    source(&store, "a");
    source(&store, "b");
    store
}
fn query() -> Value {
    json!({"sourceId":"a","fromDate":"2024-02-28","toDate":"2024-03-02"})
}
fn response(id: &str, model: Option<&str>, occurred: Option<&str>, input: &str) -> Value {
    let counts = json!({"input_tokens":input,"cached_input_tokens":"2","cache_write_input_tokens":"3","output_tokens":"4","reasoning_output_tokens":"1","total_tokens":"7"});
    json!({
        "sourceId":"a","importedAt":"2024-02-29T12:00:00.000Z",
        "occurredAt":occurred,"model":model,"collectorVersion":"COLLECTOR_CANARY",
        "raw":{
            "thread_id":"THREAD_CANARY","turn_id":"TURN_CANARY","session_id":"SESSION_CANARY",
            "root_turn_id":"ROOT_CANARY","response_id":id,"usage":counts,
            "turn_token_usage":counts,"thread_token_usage":counts,
            "body":"BODY_CANARY","reasoning_effort":"INFERRED_EFFORT_CANARY","service_tier":"INFERRED_TIER_CANARY"
        }
    })
}
fn add(store: &UsageStore, response: Value) {
    store.ingest_response_token(&response).unwrap();
}
fn totals(
    input: &str,
    cached: &str,
    write: &str,
    output: &str,
    reasoning: &str,
    total: &str,
) -> Value {
    json!({"inputTokens":input,"cachedInputTokens":cached,"cacheWriteInputTokens":write,"outputTokens":output,"reasoningOutputTokens":reasoning,"totalTokens":total})
}
fn snapshot(dir: &Path) -> BTreeMap<OsString, Vec<u8>> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), std::fs::read(entry.path()).unwrap())
        })
        .collect()
}
fn import(store: &UsageStore, source: &str, index: usize, code: &str) {
    store.import_data(&json!({"sourceId":source,"events":[],"observations":[],"importMetadata":{
        "fingerprint":format!("{index:064x}"),"adapterVersion":"synthetic-1","sourceVersion":TOKEN_SCHEMA,
        "importedAt":"2026-10-03T00:00:00.000Z","warningCodes":[code]
    }})).unwrap();
}

#[test]
fn whole_utc_days_preserve_exact_counters_and_never_use_import_time() {
    let store = store();
    for (id, model, occurred, amount) in [
        (
            "before",
            Some("outside"),
            Some("2024-02-27T23:59:59.999Z"),
            "9",
        ),
        (
            "start",
            Some("Model-A"),
            Some("2024-02-28T00:00:00Z"),
            "9223372036854775807",
        ),
        (
            "middle",
            Some("Model-A"),
            Some("2024-02-29T12:00:00Z"),
            "9223372036854775807",
        ),
        (
            "end",
            None,
            Some("2024-03-02T23:59:59.999Z"),
            "9223372036854775807",
        ),
        ("after", Some("outside"), Some("2024-03-03T00:00:00Z"), "9"),
        ("undated", Some("unknown-time"), None, "9"),
    ] {
        add(&store, response(id, model, occurred, amount));
    }
    let mut undated = response("undated-old-import", None, None, "9");
    undated["importedAt"] = json!("2020-01-01T00:00:00Z");
    add(&store, undated);
    let result = store.get_response_token_period(&query()).unwrap();
    assert_eq!(result["responseCount"], "3");
    assert_eq!(
        result["totals"],
        totals("27670116110564327421", "6", "9", "12", "3", "21")
    );
    assert_eq!(
        result["byModel"][0],
        json!({"model":"Model-A","responseCount":"2","totals":totals("18446744073709551614", "4", "6", "8", "2", "14")})
    );
    assert_eq!(
        result["byModel"][1],
        json!({"model":null,"responseCount":"1","totals":totals("9223372036854775807", "2", "3", "4", "1", "7")})
    );
    assert_eq!(result["byModelTruncated"], false);
    assert_eq!(result["undatedResponseCount"], "2");
    assert_eq!(result["undatedResponseScope"], "all_retained_source");
    assert_eq!(result["fromDate"], "2024-02-28");
    assert_eq!(result["toDate"], "2024-03-02");
    assert_eq!(result["coverage"]["dateBasis"], "occurred_at_utc");
    assert_eq!(result["coverage"]["undatedResponses"], "excluded");
    assert_eq!(result["coverage"]["quotaAttribution"], "not_provided");
    assert_eq!(result["reasoningEffort"], json!({"status":"not_recorded"}));
    assert_eq!(result["serviceTier"], json!({"status":"not_recorded"}));
    let text = result.to_string();
    for forbidden in [
        "CANARY",
        "responseId",
        "threadId",
        "turnId",
        "sessionId",
        "importedAt",
        "nextCursor",
        "outside",
        "unknown-time",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
    assert!(result.get("records").is_none());
    // The existing API deliberately retains its previous fallback behavior.
    assert_eq!(
        store.get_response_token_usage(&query()).unwrap()["responseCount"],
        "4"
    );
    let empty = store
        .get_response_token_period(
            &json!({"sourceId":"a","fromDate":"2020-01-01","toDate":"2020-01-01"}),
        )
        .unwrap();
    assert_eq!(empty["responseCount"], "0");
    assert_eq!(empty["totals"], totals("0", "0", "0", "0", "0", "0"));
    assert_eq!(empty["byModel"], json!([]));
    assert_eq!(empty["undatedResponseCount"], "2");
    assert!(
        empty["warnings"]
            .to_string()
            .contains("not zero historical activity")
    );
}

#[test]
fn models_and_source_namespaces_stay_exact_and_isolated() {
    let store = store();
    for (index, model) in [Some("Model-A"), Some("model-a"), Some("model-a/fast"), None]
        .iter()
        .enumerate()
    {
        add(
            &store,
            response(
                &format!("r{index}"),
                *model,
                Some("2024-02-29T00:00:00Z"),
                "10",
            ),
        );
    }
    for (id, occurred) in [
        ("other", Some("2024-02-29T00:00:00Z")),
        ("other-undated", None),
    ] {
        let mut other = response(id, Some("other-model"), occurred, "99");
        other["sourceId"] = json!("b");
        add(&store, other);
    }
    let result = store.get_response_token_period(&query()).unwrap();
    assert_eq!(result["responseCount"], "4");
    assert_eq!(result["undatedResponseCount"], "0");
    assert_eq!(
        result["byModel"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["model"].clone())
            .collect::<Vec<_>>(),
        vec![
            Value::Null,
            json!("Model-A"),
            json!("model-a"),
            json!("model-a/fast")
        ]
    );
    let mut q = query();
    q["sourceId"] = json!("b");
    let other = store.get_response_token_period(&q).unwrap();
    assert_eq!(other["responseCount"], "1");
    assert_eq!(other["totals"]["inputTokens"], "99");
    assert_eq!(other["undatedResponseCount"], "1");
}

#[test]
fn required_canonical_dates_have_an_inclusive_366_day_bound() {
    let store = store();
    for (from, to) in [
        ("2024-01-01", "2024-12-31"),
        ("2024-02-29", "2024-02-29"),
        ("2023-01-01", "2024-01-01"),
        ("9999-12-31", "9999-12-31"),
        ("0000-01-01", "0000-01-01"),
    ] {
        assert!(
            store
                .get_response_token_period(&json!({"sourceId":"a","fromDate":from,"toDate":to}))
                .is_ok()
        );
    }
    for invalid in [
        json!({}),
        Value::Null,
        json!([]),
        json!({"sourceId":"a"}),
        json!({"sourceId":"a","fromDate":"2024-01-01"}),
        json!({"sourceId":"a","toDate":"2024-01-01"}),
        json!({"sourceId":"a","fromDate":null,"toDate":null}),
        json!({"sourceId":"a","fromDate":"2024-01-01","toDate":"2025-01-01"}),
        json!({"sourceId":"a","fromDate":"2024-01-02","toDate":"2024-01-01"}),
        json!({"sourceId":"a","fromDate":"2023-02-29","toDate":"2023-03-01"}),
        json!({"sourceId":"a","fromDate":"2024-1-01","toDate":"2024-01-02"}),
        json!({"sourceId":"a","fromDate":"2024-01-01T00:00:00Z","toDate":"2024-01-02"}),
        json!({"sourceId":"a","fromDate":"0000-01-01","toDate":"9999-12-31"}),
    ] {
        assert_eq!(
            store.get_response_token_period(&invalid).unwrap_err(),
            CoreError::InvalidInput,
            "{invalid}"
        );
    }
    for field in [
        "model",
        "maxAgeMs",
        "limit",
        "cursor",
        "reasoningEffort",
        "serviceTier",
        "query",
    ] {
        let mut q = query();
        q[field] = json!("not-allowed");
        assert_eq!(
            store.get_response_token_period(&q).unwrap_err(),
            CoreError::InvalidInput
        );
    }
    for source in [Value::Null, json!(1), json!(""), json!("bad/source")] {
        let mut q = query();
        q["sourceId"] = source;
        assert_eq!(
            store.get_response_token_period(&q).unwrap_err(),
            CoreError::InvalidInput
        );
    }
}

#[test]
fn missing_and_closed_stores_return_typed_errors() {
    let mut store = store();
    let mut q = query();
    q["sourceId"] = json!("missing");
    assert_eq!(
        store.get_response_token_period(&q).unwrap_err(),
        CoreError::SourceNotFound
    );
    store.close().unwrap();
    assert_eq!(
        store.get_response_token_period(&query()).unwrap_err(),
        CoreError::StoreClosed
    );
}

#[test]
fn bounded_models_keep_unknown_bucket_and_totals_include_hidden_models() {
    let store = store();
    for index in 0..501 {
        for copy in 0..2 {
            add(
                &store,
                response(
                    &format!("r{index}-{copy}"),
                    Some(&format!("model-{index:03}")),
                    Some("2024-02-29T00:00:00Z"),
                    "1",
                ),
            );
        }
    }
    let known = store.get_response_token_period(&query()).unwrap();
    assert_eq!(known["byModel"].as_array().unwrap().len(), 500);
    assert_eq!(known["byModelTruncated"], true);
    assert_eq!(known["byModel"][499]["model"], "model-499");
    assert_eq!(known["responseCount"], "1002");
    add(
        &store,
        response("unknown", None, Some("2024-02-29T00:00:00Z"), "1"),
    );
    let result = store.get_response_token_period(&query()).unwrap();
    assert_eq!(result["responseCount"], "1003");
    assert_eq!(
        result["totals"],
        totals("1003", "2006", "3009", "4012", "1003", "7021")
    );
    assert_eq!(result["byModel"].as_array().unwrap().len(), 500);
    assert_eq!(result["byModel"][498]["model"], "model-498");
    assert_eq!(result["byModel"][499]["model"], Value::Null);
    assert_eq!(result["byModel"][499]["responseCount"], "1");
    assert!(
        result["warnings"]
            .to_string()
            .contains("retaining the unknown-model group")
    );
}

#[test]
fn import_warnings_are_bounded_source_metadata_not_period_coverage() {
    let store = store();
    import(&store, "b", 0, "other_source_warning");
    for index in 0..101 {
        import(&store, "a", index, &format!("fixture_warning_{index}"));
    }
    let result = store.get_response_token_period(&query()).unwrap();
    assert_eq!(result["importWarnings"]["scope"], "all_retained_source");
    assert_eq!(result["importWarnings"]["truncated"], true);
    assert_eq!(
        result["importWarnings"]["codes"].as_array().unwrap().len(),
        100
    );
    assert!(!result.to_string().contains("other_source_warning"));
    assert!(
        result["warnings"]
            .to_string()
            .contains("additional warnings are unknown")
    );
    assert!(
        result["warnings"]
            .to_string()
            .contains("not proven coverage of this selected period")
    );
    let mut q = query();
    q["sourceId"] = json!("b");
    let other = store.get_response_token_period(&q).unwrap();
    assert_eq!(
        other["importWarnings"]["codes"],
        json!(["other_source_warning"])
    );
    assert_eq!(other["importWarnings"]["truncated"], false);
}

#[test]
fn read_only_projection_ignores_response_payloads_and_unrelated_tables() {
    for version in [2, 3] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.sqlite");
        let mut writer = UsageStore::open(&path).unwrap();
        source(&writer, "a");
        add(
            &writer,
            response("r", None, Some("2024-02-29T00:00:00Z"), "5"),
        );
        writer.close().unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute_batch(&format!("UPDATE response_tokens SET payload='not-json'; DROP TABLE events; DROP TABLE observations; PRAGMA user_version={version};")).unwrap();
        drop(db);
        let before = snapshot(dir.path());
        let mut reader = UsageStore::open_read_only(&path).unwrap();
        assert_eq!(
            reader.get_response_token_period(&query()).unwrap()["responseCount"],
            "1"
        );
        reader.close().unwrap();
        assert_eq!(snapshot(dir.path()), before);
    }
}

#[test]
fn malformed_storage_fails_without_echoing_unsafe_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let mut writer = UsageStore::open(&path).unwrap();
    source(&writer, "a");
    add(
        &writer,
        response("r", None, Some("2024-02-29T00:00:00Z"), "5"),
    );
    writer.close().unwrap();
    let db = Connection::open(&path).unwrap();
    for statement in [
        "UPDATE response_tokens SET input_tokens='PRIVATE_INVALID_COUNT'",
        "UPDATE response_tokens SET input_tokens=x'80ff'",
        "DROP TABLE response_tokens",
    ] {
        db.execute_batch(statement).unwrap();
        let reader = UsageStore::open_read_only(&path).unwrap();
        assert_eq!(
            reader.get_response_token_period(&query()).unwrap_err(),
            CoreError::StorageError
        );
    }
}

async fn http_query(app: axum::Router, suffix: &str) -> (u16, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/response-tokens/period{suffix}"))
                .header("host", "127.0.0.1:4319")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn local_http_contract_is_exact_required_and_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let mut writer = UsageStore::open(&path).unwrap();
    source(&writer, "a");
    add(
        &writer,
        response("r", Some("model-a"), Some("2024-02-29T00:00:00Z"), "5"),
    );
    let expected = writer.get_response_token_period(&query()).unwrap();
    writer.close().unwrap();
    let before = snapshot(dir.path());
    let app = http::router(UsageStore::open_read_only(&path).unwrap(), 4319);
    let valid = "?sourceId=a&fromDate=2024-02-28&toDate=2024-03-02";
    assert_eq!(http_query(app.clone(), valid).await, (200, expected));
    for invalid in [
        "",
        "?sourceId=a",
        "?fromDate=2024-02-28&toDate=2024-03-02",
        "?sourceId=a&fromDate=2024-02-28",
        "?sourceId=a&toDate=2024-03-02",
        "?sourceId=&fromDate=2024-02-28&toDate=2024-03-02",
        "?sourceId=a&fromDate=2024-02-28&toDate=",
        "?sourceId=a&fromDate=2024-02-28&toDate=2024-02-27",
        "?sourceId=a&fromDate=2024-01-01&toDate=2025-01-01",
        "?sourceId=a&fromDate=2023-02-29&toDate=2023-03-01",
        "?sourceId=missing&fromDate=2024-02-28&toDate=2024-03-02",
    ] {
        assert_eq!(http_query(app.clone(), invalid).await.0, 400, "{invalid}");
    }
    for extra in [
        "model=model-a",
        "maxAgeMs=1",
        "limit=1",
        "cursor=private",
        "sourceId=a",
        "fromDate=2024-02-28",
        "toDate=2024-03-02",
    ] {
        assert_eq!(
            http_query(app.clone(), &format!("{valid}&{extra}")).await.0,
            400
        );
    }
    let forbidden = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/response-tokens/period{valid}"))
                .header("host", "127.0.0.1:4319")
                .header("origin", "https://untrusted.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forbidden.status().as_u16(), 403);
    assert_eq!(snapshot(dir.path()), before);
}
