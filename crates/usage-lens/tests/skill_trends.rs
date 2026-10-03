use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{collections::BTreeMap, ffi::OsString, path::Path};
use tower::ServiceExt;
use usage_lens::{
    cli::run_main,
    core::{CoreError, UsageStore, response_tokens::TOKEN_SCHEMA},
    server::{http, mcp},
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
fn event(
    id: &str,
    kind: &str,
    subtype: Option<&str>,
    skill: &str,
    occurred: Option<&str>,
) -> Value {
    json!({
        "sourceId":"a","eventId":id,"sourceEventId":format!("SOURCE_EVENT_CANARY_{id}"),
        "eventType":format!("skill_{kind}"),
        "evidenceType":match (kind,subtype) {
            ("requested",_)=>"explicit_skill_input",("invoked",_)=>"explicit_execution_record",
            (_,Some("instruction_injection"))=>"typed_skill_injection",_=>"successful_skill_read"
        },
        "observedAt":"2026-10-03T00:00:00.000Z","occurredAt":occurred,
        "skillName":skill,"skillEvidenceKind":subtype,"collectorVersion":"COLLECTOR_CANARY",
        "sessionId":"SESSION_CANARY","turnId":"TURN_CANARY","model":"MODEL_CANARY",
        "skillVersion":"VERSION_CANARY","retryOfEventId":"RETRY_CANARY","status":"error",
        "content":{"body":"BODY_CANARY","toolArguments":{"value":"ARGS_CANARY"},"toolResult":"RESULT_CANARY"}
    })
}
fn query() -> Value {
    json!({"sourceId":"a","fromDate":"2024-02-28","toDate":"2024-03-02"})
}
fn counts(
    requested: &str,
    loaded: &str,
    invoked: &str,
    main: &str,
    injection: &str,
    unknown: &str,
) -> Value {
    json!({"requested":requested,"loaded":loaded,"invoked":invoked,"loadedEvidence":{"mainRead":main,"instructionInjection":injection,"unknown":unknown}})
}
fn add(store: &UsageStore, value: Value) {
    store.ingest_event(&value).unwrap();
}
fn import(store: &UsageStore, index: usize, code: &str) {
    store.import_data(&json!({"sourceId":"a","events":[],"observations":[],"importMetadata":{
        "fingerprint":format!("{index:064x}"),"adapterVersion":"fixture-1","sourceVersion":TOKEN_SCHEMA,
        "importedAt":"2026-10-03T00:00:00.000Z","warningCodes":[code]
    }})).unwrap();
}
fn snapshot(dir: &Path) -> BTreeMap<OsString, Vec<u8>> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (e.file_name(), std::fs::read(e.path()).unwrap())
        })
        .collect()
}

#[test]
fn utc_occurrence_buckets_keep_evidence_types_and_unknowns_separate() {
    let store = store();
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    for (id, kind, subtype, date) in [
        (
            "before",
            "requested",
            None,
            Some("2024-02-27T23:59:59.999Z"),
        ),
        ("start", "requested", None, Some("2024-02-28T00:00:00Z")),
        (
            "main",
            "loaded",
            Some("main_read"),
            Some("2024-02-29T10:00:00Z"),
        ),
        (
            "inject",
            "loaded",
            Some("instruction_injection"),
            Some("2024-02-29T10:00:00Z"),
        ),
        ("legacy", "loaded", None, Some("2024-02-29T10:00:00Z")),
        ("invoke", "invoked", None, Some("2024-02-29T10:00:00Z")),
        ("end", "invoked", None, Some("2024-03-02T23:59:59.999Z")),
        ("after", "requested", None, Some("2024-03-03T00:00:00Z")),
        ("undated", "loaded", Some("main_read"), None),
    ] {
        add(&store, event(id, kind, subtype, "docs", date));
    }
    // Independent evidence is retained, even if status conflicts or correlation IDs match.
    let result = store.get_skill_summary(&query()).unwrap();
    assert_eq!(result["totals"], counts("1", "3", "2", "1", "1", "1"));
    assert_eq!(result["totalsScope"], "dated_range");
    assert_eq!(result["basis"], "occurred_at_utc");
    assert_eq!(result["unknownOccurredAtCount"], "1");
    assert_eq!(
        result["unknownOccurredAtScope"],
        "all_retained_source_matching_skill"
    );
    let mut middle = counts("0", "3", "1", "1", "1", "1");
    middle["date"] = json!("2024-02-29");
    assert_eq!(result["daily"][1], middle);
    assert_eq!(result["daily"].as_array().unwrap().len(), 3);
    assert_eq!(result["daily"][0]["date"], "2024-02-28");
    assert_eq!(result["daily"][2]["date"], "2024-03-02");
    assert_eq!(result["coverage"]["tokenAttribution"], "not_provided");
    let text = result.to_string();
    for canary in [
        "CANARY",
        "2026-10-03T00:00:00.000Z",
        "eventId",
        "sourceEventId",
        "sessionId",
        "turnId",
        "toolArguments",
        "toolResult",
        "skillVersion",
        "retryOfEventId",
        "nextCursor",
    ] {
        assert!(!text.contains(canary), "{canary}");
    }
    // An import date never substitutes for unknown occurrence time.
    let imported_day = store
        .get_skill_summary(&json!({"sourceId":"a","fromDate":"2026-10-03","toDate":"2026-10-03"}))
        .unwrap();
    assert_eq!(imported_day["daily"], json!([]));
    assert_eq!(imported_day["totals"], counts("0", "0", "0", "0", "0", "0"));
    assert_eq!(imported_day["unknownOccurredAtCount"], "1");
}

#[test]
fn exact_skill_and_source_filters_never_mix_evidence() {
    let store = store();
    for (id, name) in [("1", "Docs"), ("2", "docs"), ("3", "docs%"), ("4", "docs'")] {
        add(
            &store,
            event(id, "requested", None, name, Some("2024-02-29T12:00:00Z")),
        );
        add(
            &store,
            event(&format!("u{id}"), "invoked", None, name, None),
        );
    }
    let mut other = event(
        "other",
        "loaded",
        None,
        "docs",
        Some("2024-02-29T12:00:00Z"),
    );
    other["sourceId"] = json!("b");
    add(&store, other);
    let mut q = query();
    q["skillName"] = json!("docs");
    let result = store.get_skill_summary(&q).unwrap();
    assert_eq!(result["totals"], counts("1", "0", "0", "0", "0", "0"));
    assert_eq!(result["unknownOccurredAtCount"], "1");
    assert_eq!(result["skills"][0]["name"], "docs");
    q["sourceId"] = json!("b");
    assert_eq!(
        store.get_skill_summary(&q).unwrap()["totals"],
        counts("0", "1", "0", "0", "0", "1")
    );
    q["sourceId"] = json!("a");
    q["skillName"] = json!("docs%");
    assert_eq!(
        store.get_skill_summary(&q).unwrap()["totals"]["requested"],
        "1"
    );
    q["skillName"] = json!("missing");
    assert_eq!(store.get_skill_summary(&q).unwrap()["daily"], json!([]));
    let only = store
        .get_skill_summary(&json!({"sourceId":"a","skillName":"docs"}))
        .unwrap();
    assert_eq!(only["totals"], counts("1", "0", "1", "0", "0", "0"));
    assert_eq!(only["totalsScope"], "all_retained_matching_evidence");
    assert_eq!(only["unknownOccurredAtCount"], "1");
    assert!(only["fromDate"].is_null());
    assert!(only["toDate"].is_null());
    assert!(only.get("daily").is_none());
}

#[test]
fn range_validation_is_inclusive_leap_aware_and_bounded() {
    let store = store();
    for params in [
        json!({"fromDate":"2024-01-01","toDate":"2024-12-31"}),
        json!({"fromDate":"2024-02-29","toDate":"2024-02-29"}),
        json!({"skillName":"中文 ✓"}),
        json!({"skillName":"😀".repeat(128)}),
        json!({"maxAgeMs":2592000000u64}),
    ] {
        let mut q = json!({"sourceId":"a"});
        q.as_object_mut()
            .unwrap()
            .extend(params.as_object().unwrap().clone());
        assert!(store.get_skill_summary(&q).is_ok(), "{q}");
    }
    for params in [
        json!({"fromDate":"2024-01-01"}),
        json!({"toDate":"2024-01-01"}),
        json!({"fromDate":null,"toDate":null}),
        json!({"fromDate":"2023-02-29","toDate":"2023-03-01"}),
        json!({"fromDate":"2024-1-01","toDate":"2024-01-02"}),
        json!({"fromDate":"2024-01-02","toDate":"2024-01-01"}),
        json!({"fromDate":"2024-01-01","toDate":"2025-01-01"}),
        json!({"fromDate":"0000-01-01","toDate":"9999-12-31"}),
        json!({"skillName":""}),
        json!({"skillName":null}),
        json!({"skillName":3}),
        json!({"skillName":"x\n"}),
        json!({"skillName":"x".repeat(257)}),
        json!({"skillName":"😀".repeat(129)}),
        json!({"model":"not-allowed"}),
        json!({"maxAgeMs":-1}),
    ] {
        let mut q = json!({"sourceId":"a"});
        q.as_object_mut()
            .unwrap()
            .extend(params.as_object().unwrap().clone());
        assert_eq!(
            store.get_skill_summary(&q).unwrap_err(),
            CoreError::InvalidInput,
            "{q}"
        );
    }
}

#[test]
fn aggregate_bounds_do_not_truncate_daily_or_overall_counts() {
    let store = store();
    let events: Vec<_> = (0..501)
        .map(|i| {
            event(
                &format!("e{i}"),
                "requested",
                None,
                &format!("skill-{i:03}"),
                Some("2024-02-29T00:00:00Z"),
            )
        })
        .collect();
    store.ingest_events(&json!(events)).unwrap();
    let result = store.get_skill_summary(&query()).unwrap();
    assert_eq!(result["skills"].as_array().unwrap().len(), 500);
    assert_eq!(result["skillsTruncated"], true);
    assert_eq!(result["totals"]["requested"], "501");
    assert_eq!(result["daily"][0]["requested"], "501");
    assert!(result["warnings"].to_string().contains("limited to 500"));
    let legacy = store.get_skill_summary(&json!({"sourceId":"a"})).unwrap();
    assert_eq!(legacy.as_object().unwrap().len(), 4);
    assert_eq!(legacy["skills"].as_array().unwrap().len(), 500);
}

#[test]
fn import_warnings_are_source_scoped_bounded_and_not_date_coverage() {
    let store = store();
    for i in 0..101 {
        import(&store, i, &format!("fixture_warning_{i}"));
    }
    let result = store.get_skill_summary(&query()).unwrap();
    assert_eq!(result["importWarnings"]["scope"], "all_retained_source");
    assert_eq!(result["importWarnings"]["truncated"], true);
    assert_eq!(
        result["importWarnings"]["codes"].as_array().unwrap().len(),
        100
    );
    assert!(
        result["warnings"]
            .to_string()
            .contains("additional warnings are unknown")
    );
    let mut q = query();
    q["sourceId"] = json!("b");
    assert_eq!(
        store.get_skill_summary(&q).unwrap()["importWarnings"]["codes"],
        json!([])
    );
}

#[test]
fn skill_summary_does_not_read_unrelated_account_observations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixture.sqlite");
    let mut writer = UsageStore::open(&path).unwrap();
    source(&writer, "a");
    writer.ingest_observation(&json!({"sourceId":"a","method":"account/read","observedAt":"2026-10-03T00:00:00Z","adapterVersion":"synthetic-1","schemaBaseline":"synthetic","raw":{}})).unwrap();
    writer.close().unwrap();
    Connection::open(&path)
        .unwrap()
        .execute("UPDATE observations SET payload='not-json'", [])
        .unwrap();
    let before = snapshot(dir.path());
    let store = UsageStore::open_read_only(&path).unwrap();
    assert!(store.get_overview(&json!({"sourceId":"a"})).is_err());
    assert!(store.get_skill_summary(&json!({"sourceId":"a"})).is_ok());
    assert!(store.get_skill_summary(&query()).is_ok());
    assert_eq!(snapshot(dir.path()), before);
}

async fn cli(args: &[String]) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run_main(args, &b""[..], &mut out, &mut err).await;
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}
fn args(path: &Path) -> Vec<String> {
    [
        "skill-summary",
        "--db",
        path.to_str().unwrap(),
        "--source",
        "a",
        "--from",
        "2024-02-28",
        "--to",
        "2024-03-02",
        "--skill",
        "docs",
    ]
    .map(String::from)
    .to_vec()
}
async fn http_query(app: axum::Router, query: &str) -> (u16, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/skill-summary{query}"))
                .header("host", "127.0.0.1:4319")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn mcp_call(store: &UsageStore, args: Value) -> Value {
    let mut session = mcp::McpSession::default();
    session.handle(store,json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"0"}}})).unwrap();
    session.handle(store,json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"usage_skills","arguments":args}})).unwrap()["result"].clone()
}

#[tokio::test]
async fn cli_mcp_http_share_projection_and_read_only_behavior() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixture.sqlite");
    let mut writer = UsageStore::open(&path).unwrap();
    source(&writer, "a");
    add(
        &writer,
        event(
            "id",
            "loaded",
            Some("main_read"),
            "docs",
            Some("2024-02-29T00:00:00Z"),
        ),
    );
    add(&writer, event("undated", "invoked", None, "docs", None));
    writer.close().unwrap();
    let before = snapshot(dir.path());
    let (code, out, err) = cli(&args(&path)).await;
    assert_eq!(code, 0, "{err}");
    let expected: Value = serde_json::from_str(&out).unwrap();
    let reader = UsageStore::open_read_only(&path).unwrap();
    let mut q = query();
    q["skillName"] = json!("docs");
    let result = mcp_call(&reader, q.clone());
    assert!(result["isError"].is_null());
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        expected
    );
    let (status, value) = http_query(
        http::router(reader, 4319),
        "?sourceId=a&fromDate=2024-02-28&toDate=2024-03-02&skillName=docs",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(value, expected);
    assert_eq!(snapshot(dir.path()), before);
    let tools = mcp::tools_list();
    let tools = tools["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 8);
    let schema = &tools.iter().find(|v| v["name"] == "usage_skills").unwrap()["inputSchema"];
    assert_eq!(schema["properties"]["skillName"]["maxLength"], 256);
    assert_eq!(
        schema["dependentRequired"],
        json!({"fromDate":["toDate"],"toDate":["fromDate"]})
    );
    let app = http::router(UsageStore::open_read_only(&path).unwrap(), 4319);
    let (status, legacy) = http_query(app.clone(), "?sourceId=a").await;
    assert_eq!(status, 200);
    assert_eq!(legacy.as_object().unwrap().len(), 4);
    for query in [
        "?sourceId=a&fromDate=2024-02-29",
        "?sourceId=a&fromDate=2023-02-29&toDate=2023-03-01",
        "?sourceId=a&fromDate=2024-01-01&toDate=2025-01-01",
        "?sourceId=a&skillName=",
        "?sourceId=a&skillName=docs&skillName=docs",
        "?sourceId=a&model=private",
    ] {
        assert_eq!(http_query(app.clone(), query).await.0, 400, "{query}");
    }
    let reader = UsageStore::open_read_only(&path).unwrap();
    for invalid in [
        json!({"sourceId":"a","fromDate":"2024-02-29"}),
        json!({"sourceId":"a","skillName":null}),
        json!({"sourceId":"a","fromDate":"2024-01-01","toDate":"2025-01-01"}),
    ] {
        assert_eq!(
            mcp_call(&reader, invalid)["content"][0]["text"],
            "invalid_tool_arguments"
        );
    }
    let mut invalid = args(&path);
    invalid[8] = "2023-02-29".into();
    let (code, out, err) = cli(&invalid).await;
    assert_eq!(code, 1);
    assert!(out.is_empty());
    assert!(err.contains("invalid_input"));
    assert_eq!(snapshot(dir.path()), before);
}

#[tokio::test]
async fn filtered_queries_never_create_or_migrate_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.sqlite");
    let before = snapshot(dir.path());
    assert_eq!(cli(&args(&path)).await.0, 1);
    assert_eq!(snapshot(dir.path()), before);
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA user_version=1; CREATE TABLE fixture(x);")
        .unwrap();
    drop(db);
    let before = snapshot(dir.path());
    let (code, out, err) = cli(&args(&path)).await;
    assert_eq!(code, 1);
    assert!(out.is_empty());
    assert!(err.contains("unsupported_schema"));
    assert_eq!(snapshot(dir.path()), before);
}

#[tokio::test]
async fn leading_hyphen_skill_names_are_literal_across_query_surfaces() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixture.sqlite");
    let mut writer = UsageStore::open(&path).unwrap();
    source(&writer, "a");
    add(
        &writer,
        event(
            "hyphen",
            "requested",
            None,
            "--docs",
            Some("2024-02-29T00:00:00Z"),
        ),
    );
    writer.close().unwrap();
    let mut cli_args = args(&path);
    *cli_args.last_mut().unwrap() = "--docs".into();
    let (code, out, err) = cli(&cli_args).await;
    assert_eq!(code, 0, "{err}");
    let expected: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(expected["totals"]["requested"], "1");
    let reader = UsageStore::open_read_only(&path).unwrap();
    let mut q = query();
    q["skillName"] = json!("--docs");
    let mcp = mcp_call(&reader, q);
    assert_eq!(
        serde_json::from_str::<Value>(mcp["content"][0]["text"].as_str().unwrap()).unwrap(),
        expected
    );
    let (status, http) = http_query(
        http::router(reader, 4319),
        "?sourceId=a&fromDate=2024-02-28&toDate=2024-03-02&skillName=--docs",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(http, expected);
}
