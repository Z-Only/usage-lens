use rusqlite::Connection;
use serde_json::{Value, json};
use std::{collections::BTreeMap, ffi::OsString, path::Path};
use usage_lens::{adapters::demo::seed_demo, cli::run_main, core::UsageStore};

const QUERIES: &[&str] = &[
    "status",
    "health",
    "overview",
    "daily",
    "quota",
    "tools",
    "skills",
    "skill-summary",
    "response-tokens",
    "history",
    "events",
    "detail",
    "settings",
    "mcp",
];

fn snapshot(directory: &Path) -> BTreeMap<OsString, Vec<u8>> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), std::fs::read(entry.path()).unwrap())
        })
        .collect()
}

fn synthetic_store(path: &Path) {
    let mut store = UsageStore::open(path).unwrap();
    seed_demo(&store, 1790920800000).unwrap();
    store.close().unwrap();
}

fn query_args(command: &str, path: &Path) -> Vec<String> {
    let mut args = vec![command.into(), "--db".into(), path.to_str().unwrap().into()];
    if !["status", "settings", "mcp"].contains(&command) {
        args.extend(["--source".into(), "demo".into()]);
    }
    if command == "daily" {
        args.extend(["--from", "2026-09-01", "--to", "2026-10-02"].map(String::from));
    }
    if command == "detail" {
        args.extend(["--event".into(), "demo-event-0".into()]);
    }
    args
}

async fn cli(args: &[String]) -> (i32, String, String) {
    let input = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"0"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"usage_status","arguments":{}}}),
    ]
    .map(|value| format!("{value}\n"))
    .concat();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run_main(args, input.as_bytes(), &mut out, &mut err).await;
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[tokio::test]
async fn all_persisted_queries_and_mcp_preserve_existing_store_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    synthetic_store(&path);
    let before = snapshot(dir.path());
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    for command in QUERIES {
        let (code, out, err) = cli(&query_args(command, &path)).await;
        assert_eq!(code, 0, "{command}: {err}");
        assert!(err.is_empty());
        if *command == "mcp" {
            let messages: Vec<Value> = out
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(messages.len(), 2);
            assert_eq!(
                messages[0]["result"]["serverInfo"]["version"],
                env!("CARGO_PKG_VERSION")
            );
            assert!(
                messages[1]["result"]["content"][0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("synthetic")
            );
        } else {
            assert!(serde_json::from_str::<Value>(&out).is_ok(), "{command}");
        }
        assert_eq!(snapshot(dir.path()), before, "{command}");
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
    }
}

#[tokio::test]
async fn skill_summary_matches_mcp_aggregates_without_individual_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    synthetic_store(&path);
    let mut writer = UsageStore::open(&path).unwrap();
    for index in 0..2 {
        writer.ingest_event(&json!({
            "sourceId":"demo", "eventId":format!("EVENT_ID_CANARY_{index}"),
            "sourceEventId":format!("SOURCE_EVENT_ID_CANARY_{index}"),
            "eventType":"skill_loaded", "evidenceType":"successful_skill_read",
            "skillName":"spreadsheets", "skillEvidenceKind":"main_read",
            "skillVersion":"SKILL_VERSION_CANARY", "model":"MODEL_CANARY",
            "sessionId":"SESSION_ID_CANARY", "turnId":"TURN_ID_CANARY",
            "retryOfEventId":"RETRY_ID_CANARY", "collectorVersion":"COLLECTOR_VERSION_CANARY",
            "observedAt":"2024-01-02T03:04:05.006Z", "occurredAt":"2023-01-02T03:04:05.006Z",
            "content":{"body":"BODY_CANARY", "toolArguments":{"value":"ARGUMENTS_CANARY"}, "toolResult":"RESULT_CANARY"}
        })).unwrap();
    }
    writer.close().unwrap();
    let before = snapshot(dir.path());
    let mut args = query_args("skill-summary", &path);
    args.extend(["--max-age-ms".into(), "0".into()]);
    let (code, out, err) = cli(&args).await;
    assert_eq!(code, 0, "{err}");
    assert!(err.is_empty());
    let summary: Value = serde_json::from_str(&out).unwrap();
    let reader = UsageStore::open_read_only(&path).unwrap();
    let params = json!({"sourceId":"demo","maxAgeMs":0});
    assert_eq!(summary, reader.get_skill_summary(&params).unwrap());
    let overview = reader.get_overview(&params).unwrap();
    assert_eq!(
        summary,
        json!({
            "source":overview["source"], "skills":overview["events"]["skills"],
            "coverage":overview["events"]["coverage"], "warnings":overview["warnings"]
        })
    );
    let groups = summary["skills"].as_array().unwrap();
    assert_eq!(groups.len(), 4);
    for (kind, evidence, count) in [
        ("requested", Value::Null, "8"),
        ("loaded", Value::Null, "8"),
        ("invoked", Value::Null, "8"),
        ("loaded", json!("main_read"), "2"),
    ] {
        assert!(groups.contains(
            &json!({"kind":kind,"evidenceKind":evidence,"name":"spreadsheets","count":count})
        ));
    }
    assert_eq!(summary["source"]["id"], "demo");
    assert_eq!(summary["source"]["provider"], "synthetic");
    assert_eq!(
        summary["source"]["accountBinding"],
        "unverified_local_namespace"
    );
    assert_eq!(summary["coverage"]["completeness"], "partial");
    assert_eq!(summary["coverage"]["missingEvents"], "unknown");
    assert!(!summary["warnings"].as_array().unwrap().is_empty());

    let input = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"0"}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"usage_skills","arguments":params}}),
    ].map(|value| format!("{value}\n")).concat();
    let mut mcp_out = Vec::new();
    let mut mcp_err = Vec::new();
    assert_eq!(
        run_main(
            &query_args("mcp", &path),
            input.as_bytes(),
            &mut mcp_out,
            &mut mcp_err
        )
        .await,
        0
    );
    assert!(mcp_err.is_empty());
    let mcp_text = String::from_utf8(mcp_out).unwrap();
    let response: Value = serde_json::from_str(mcp_text.lines().nth(1).unwrap()).unwrap();
    assert!(response["result"]["isError"].is_null());
    let mcp_summary: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(summary, mcp_summary);
    for forbidden in [
        "events",
        "eventId",
        "sourceEventId",
        "sessionId",
        "turnId",
        "observedAt",
        "occurredAt",
        "nextCursor",
        "content",
        "toolArguments",
        "toolResult",
        "EVENT_ID_CANARY",
        "SOURCE_EVENT_ID_CANARY",
        "SESSION_ID_CANARY",
        "TURN_ID_CANARY",
        "RETRY_ID_CANARY",
        "COLLECTOR_VERSION_CANARY",
        "SKILL_VERSION_CANARY",
        "MODEL_CANARY",
        "BODY_CANARY",
        "ARGUMENTS_CANARY",
        "RESULT_CANARY",
        "2024-01-02T03:04:05.006Z",
        "2023-01-02T03:04:05.006Z",
    ] {
        // Look for structural keys exactly: explanatory warnings can mention events.
        assert!(!out.contains(&format!("\"{forbidden}\":")), "{forbidden}");
        if forbidden.contains("CANARY") || forbidden.starts_with("202") {
            assert!(!out.contains(forbidden), "{forbidden}");
            assert!(!mcp_text.contains(forbidden), "{forbidden}");
        }
    }
    // The old local-only evidence command keeps its event metadata contract.
    let (code, evidence, err) = cli(&query_args("skills", &path)).await;
    assert_eq!(code, 0, "{err}");
    assert!(evidence.contains("EVENT_ID_CANARY_0"));
    assert!(evidence.contains("SESSION_ID_CANARY"));
    assert_eq!(
        reader
            .get_local_event_detail(&json!({"sourceId":"demo","eventId":"EVENT_ID_CANARY_0"}))
            .unwrap()["content"]["body"],
        "BODY_CANARY"
    );
    assert_eq!(snapshot(dir.path()), before);
}

#[tokio::test]
async fn skill_summary_rejects_raw_evidence_filters_and_unknown_sources() {
    for flag in [
        "--kind", "--from", "--to", "--model", "--limit", "--cursor", "--event",
    ] {
        let args = ["skill-summary", "--demo", flag, "private"].map(String::from);
        let (code, out, err) = cli(&args).await;
        assert_eq!(code, 1, "{flag}");
        assert!(out.is_empty());
        assert_eq!(err, "Usage Lens: invalid_argument\n");
    }
    let store = UsageStore::in_memory().unwrap();
    seed_demo(&store, 1790920800000).unwrap();
    for key in [
        "kind", "fromDate", "toDate", "model", "limit", "cursor", "eventId",
    ] {
        let mut params = json!({"sourceId":"demo"});
        params[key] = json!("private");
        assert_eq!(
            store.get_skill_summary(&params).unwrap_err().code(),
            "invalid_input"
        );
    }
    assert_eq!(
        store
            .get_skill_summary(&json!({"sourceId":"missing"}))
            .unwrap_err()
            .code(),
        "source_not_found"
    );
}

#[tokio::test]
async fn all_persisted_queries_and_mcp_reject_missing_or_old_schema_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("SYNTHETIC_PRIVATE_PATH.sqlite");
    for command in QUERIES {
        let (code, out, err) = cli(&query_args(command, &missing)).await;
        assert_eq!(code, 1, "{command}");
        assert!(out.is_empty());
        assert_eq!(err, "Usage Lens: storage_error\n");
        assert!(snapshot(dir.path()).is_empty());
    }
    for version in [0, 1, 99] {
        let path = dir.path().join(format!("schema-{version}.sqlite"));
        synthetic_store(&path);
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(&format!(
                "DROP TABLE imports; DROP TABLE response_tokens; PRAGMA user_version={version};"
            ))
            .unwrap();
        drop(connection);
        let before = snapshot(dir.path());
        for command in QUERIES {
            let (code, out, err) = cli(&query_args(command, &path)).await;
            assert_eq!(code, 1, "{command}, schema {version}");
            assert!(out.is_empty());
            assert_eq!(err, "Usage Lens: unsupported_schema\n");
            assert_eq!(snapshot(dir.path()), before, "{command}, schema {version}");
        }
    }
}

#[test]
fn read_only_store_enforces_sqlite_writes_and_sees_normal_concurrent_updates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    synthetic_store(&path);
    let before = snapshot(dir.path());
    let mut reader = UsageStore::open_read_only(&path).unwrap();
    assert_eq!(reader.get_status().unwrap()["eventCount"], "56");
    for result in [
        reader.update_settings(&json!({"capturePaused":true})),
        reader.clear_data(&json!({})),
        reader.clear_local_content(&json!({})),
        reader.create_source(&json!({"id":"new","displayName":"Synthetic","mode":"imported","provider":"synthetic","coverageDescription":"Synthetic"})),
        reader.ingest_event(&json!({"sourceId":"demo","eventId":"new","eventType":"user_prompt","evidenceType":"explicit_user_message","observedAt":"2026-10-02T06:00:00Z","collectorVersion":"test-1"})),
    ] {
        assert_eq!(result.unwrap_err().code(), "storage_error");
    }
    assert_eq!(
        reader.delete_source("demo").unwrap_err().code(),
        "storage_error"
    );
    assert_eq!(snapshot(dir.path()), before);
    // Keep the reader open while the normal writer changes the database. A long-lived
    // MCP reader must observe later commits and must not use immutable snapshots.
    let mut writer = UsageStore::open(&path).unwrap();
    writer
        .update_settings(&json!({"capturePaused":true}))
        .unwrap();
    assert_eq!(reader.get_settings().unwrap()["capturePaused"], true);
    writer
        .update_settings(&json!({"capturePaused":false}))
        .unwrap();
    assert_eq!(reader.get_settings().unwrap()["capturePaused"], false);
    writer.close().unwrap();
    reader.close().unwrap();
}

#[tokio::test]
async fn read_only_rejects_external_wal_before_creating_or_changing_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    synthetic_store(&path);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("PRAGMA journal_mode=WAL;")
        .unwrap();
    drop(connection);
    // A clean, closed WAL database has no sidecars. A normal SQLite read-only open
    // could create them; the read-only store's header check must reject it first.
    assert_eq!(snapshot(dir.path()).len(), 1);
    let before = snapshot(dir.path());
    for command in QUERIES {
        let (code, out, err) = cli(&query_args(command, &path)).await;
        assert_eq!(code, 1, "{command}");
        assert!(out.is_empty());
        assert_eq!(err, "Usage Lens: storage_error\n");
        assert_eq!(snapshot(dir.path()), before);
    }
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("UPDATE settings SET payload=payload;")
        .unwrap();
    let before = snapshot(dir.path());
    assert_eq!(before.len(), 3);
    assert_eq!(
        UsageStore::open_read_only(&path).err().unwrap().code(),
        "storage_error"
    );
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn read_only_paths_and_corrupt_headers_have_static_errors() {
    for invalid in ["", ":memory:", "\0private", &"x".repeat(4097)] {
        assert_eq!(
            UsageStore::open_read_only(invalid).err().unwrap().code(),
            "invalid_input"
        );
    }
    let dir = tempfile::tempdir().unwrap();
    for path in [
        dir.path().to_owned(),
        dir.path().join("missing-parent/missing.sqlite"),
    ] {
        assert_eq!(
            UsageStore::open_read_only(path).err().unwrap().code(),
            "storage_error"
        );
    }
    for (name, bytes) in [
        ("empty", Vec::new()),
        ("short", b"private".to_vec()),
        ("corrupt", vec![b'x'; 100]),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            UsageStore::open_read_only(&path).err().unwrap().code(),
            "storage_error"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[tokio::test]
async fn explicit_setup_and_settings_updates_still_create_and_migrate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let args = [
        "source",
        "--db",
        path.to_str().unwrap(),
        "--source",
        "local",
        "--mode",
        "imported",
    ]
    .map(String::from);
    let (code, _, err) = cli(&args).await;
    assert_eq!(code, 0, "{err}");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("DROP TABLE imports; DROP TABLE response_tokens; PRAGMA user_version=1;")
        .unwrap();
    drop(connection);
    let mut args = query_args("settings", &path);
    args.extend(["--pause".into(), "true".into()]);
    let (code, _, err) = cli(&args).await;
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        UsageStore::open_read_only(&path)
            .unwrap()
            .get_settings()
            .unwrap()["capturePaused"],
        true
    );
}
