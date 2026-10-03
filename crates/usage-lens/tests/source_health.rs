use rusqlite::Connection;
use serde_json::{Value, json};
use std::{collections::BTreeMap, ffi::OsString, path::Path};
use usage_lens::core::{CoreError, UsageStore, response_tokens::TOKEN_SCHEMA};

const NOW: &str = "2026-10-03T12:00:00.000Z";
const OLD: &str = "2026-10-01T01:00:00.000Z";
const RECENT: &str = "2026-10-03T11:59:00.000Z";
const FUTURE: &str = "2026-10-03T12:01:00.000Z";

fn ms(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .unwrap()
        .timestamp_millis()
}
fn source(store: &UsageStore, id: &str) {
    store
        .create_source(&json!({
            "id":id,"displayName":"/private/SOURCE_NAME_CANARY",
            "mode":"imported","provider":"user_import",
            "coverageDescription":"SOURCE_DESCRIPTION_CANARY"
        }))
        .unwrap();
}
fn memory() -> UsageStore {
    let store = UsageStore::with_clock_ms(":memory:", ms(NOW)).unwrap();
    source(&store, "a");
    source(&store, "b");
    store
}
fn query() -> Value {
    json!({"sourceId":"a"})
}
fn observation(store: &UsageStore, method: &str, at: &str, source_as_of: Option<&str>) {
    store.ingest_observation(&json!({
        "sourceId":"a","method":method,"observedAt":at,"sourceAsOf":source_as_of,
        "adapterVersion":"synthetic-1","schemaBaseline":"synthetic-schema",
        "raw": if method=="account/read" {json!({"account":{"type":"chatgpt","email":"EMAIL_CANARY"}})} else {json!({})}
    })).unwrap();
}
fn failure(store: &UsageStore, method: &str, at: &str, code: &str) {
    store
        .record_failure(&json!({
            "sourceId":"a","method":method,"attemptedAt":at,"errorCode":code
        }))
        .unwrap();
}
fn event(id: &str, at: &str, occurred_at: Option<&str>) -> Value {
    json!({
        "sourceId":"a","eventId":id,"sourceEventId":"SOURCE_EVENT_CANARY".to_owned()+id,
        "eventType":"user_prompt","evidenceType":"explicit_user_message",
        "observedAt":at,"occurredAt":occurred_at,"collectorVersion":"synthetic-1",
        "sessionId":"SESSION_CANARY","turnId":"TURN_CANARY","retryOfEventId":"RETRY_CANARY",
        "model":"MODEL_CANARY","content":{"body":"/private/BODY_CANARY","toolArguments":{"v":"ARG_CANARY"},"toolResult":"RESULT_CANARY"}
    })
}
fn response(id: &str, at: &str, occurred_at: Option<&str>) -> Value {
    let usage = json!({"input_tokens":"5","cached_input_tokens":"1","output_tokens":"3","reasoning_output_tokens":"1","total_tokens":"8"});
    json!({
        "sourceId":"a","importedAt":at,"occurredAt":occurred_at,
        "model":"MODEL_CANARY","collectorVersion":"synthetic-1",
        "raw":{"thread_id":"THREAD_CANARY","turn_id":"TURN_CANARY","session_id":"SESSION_CANARY",
            "root_turn_id":"ROOT_CANARY","response_id":id,"usage":usage,
            "turn_token_usage":usage,"thread_token_usage":usage}
    })
}
fn metadata(index: usize, at: &str, warnings: Value) -> Value {
    json!({"fingerprint":format!("{index:064x}"),"adapterVersion":"synthetic-1",
        "sourceVersion":TOKEN_SCHEMA,"importedAt":at,"warningCodes":warnings})
}
fn import(store: &UsageStore, source: &str, index: usize, at: &str, warnings: Value) -> Value {
    store
        .import_data(&json!({"sourceId":source,"observations":[],"events":[],
        "importMetadata":metadata(index,at,warnings)}))
        .unwrap()
}
fn error(result: Result<Value, CoreError>, expected: CoreError) {
    assert_eq!(result.unwrap_err(), expected);
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

#[test]
fn empty_source_is_unknown_not_zero_history_or_complete() {
    let store = memory();
    let result = store.get_health(&query()).unwrap();
    assert_eq!(result["schemaVersion"], 1);
    assert_eq!(result["checkedAt"], NOW);
    assert_eq!(result["maxAgeMs"], 900000);
    assert_eq!(
        result["source"],
        json!({"id":"a","mode":"imported","provider":"user_import","accountBinding":"unverified_local_namespace"})
    );
    assert_eq!(
        result["settings"],
        json!({"capturePaused":false,"contentCaptureEnabled":false,"retentionDays":30,"scope":"all_sources"})
    );
    assert_eq!(
        result["coverage"],
        json!({"completeness":"partial","missingRecords":"unknown","preCollectionHistory":"unknown"})
    );
    assert_eq!(
        result["provenance"],
        json!({"basis":"stored_records","measurement":"measured","estimated":false})
    );
    for (index, method) in [
        "account/read",
        "account/usage/read",
        "account/rateLimits/read",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(
            result["observations"][index],
            json!({
                "method":method,"availability":"missing","capability":"unknown",
                "freshness":{"state":"unknown","observedAt":null,"sourceAsOf":null,"sourceAsOfStatus":"not_provided","ageMs":null,"maxAgeMs":900000,"basis":"collection_time"},
                "lastFailure":null,"provenance":null
            })
        );
    }
    for kind in ["events", "skills", "responseTokens"] {
        assert_eq!(
            result["stored"][kind],
            json!({"count":"0","firstCapturedAt":null,"lastCapturedAt":null,"firstOccurredAt":null,"lastOccurredAt":null,"unknownOccurredAtCount":"0"})
        );
    }
    assert_eq!(
        result["stored"]["imports"],
        json!({"count":"0","firstImportedAt":null,"lastImportedAt":null,"warningCodes":[],"warningCodesTruncated":false,"importsExamined":"0"})
    );
    let text = result.to_string();
    assert!(text.contains("not zero historical activity"));
    assert!(text.contains("Measured means locally retained"));
    assert!(!text.contains("CANARY"));
    assert!(!text.contains("completenessPercentage"));
}

#[test]
fn availability_freshness_and_failures_are_independent() {
    let store = memory();
    observation(&store, "account/read", RECENT, Some(OLD));
    observation(&store, "account/usage/read", OLD, None);
    observation(&store, "account/rateLimits/read", FUTURE, None);
    failure(&store, "account/read", OLD, "unsupported_method");
    failure(&store, "account/usage/read", RECENT, "rpc_error");
    failure(
        &store,
        "account/rateLimits/read",
        FUTURE,
        "unsupported_method",
    );
    let result = store
        .get_health(&json!({"sourceId":"a","maxAgeMs":60000}))
        .unwrap();
    let account = &result["observations"][0];
    assert_eq!(account["availability"], "available");
    assert_eq!(account["capability"], "observed");
    assert_eq!(account["freshness"]["state"], "fresh");
    assert_eq!(account["freshness"]["ageMs"], 60000);
    assert_eq!(account["freshness"]["sourceAsOf"], OLD);
    assert_eq!(account["freshness"]["sourceAsOfStatus"], "reported");
    assert_eq!(account["lastFailure"]["atOrAfterLatestObservation"], false);
    assert_eq!(account["lastFailure"]["state"], "stale");
    assert_eq!(
        account["provenance"],
        json!({"sourceId":"a","provider":"codex_app_server","method":"account/read","adapterVersion":"synthetic-1","schemaBaseline":"synthetic-schema","mode":"imported"})
    );
    let usage = &result["observations"][1];
    assert_eq!(usage["availability"], "available");
    assert_eq!(usage["capability"], "observed");
    assert_eq!(usage["freshness"]["state"], "stale");
    assert_eq!(usage["freshness"]["observedAt"], OLD);
    assert_eq!(
        usage["lastFailure"],
        json!({"errorCode":"rpc_error","attemptedAt":RECENT,"state":"recent","ageMs":60000,"atOrAfterLatestObservation":true})
    );
    let quota = &result["observations"][2];
    assert_eq!(quota["availability"], "available");
    assert_eq!(quota["capability"], "unsupported");
    assert_eq!(quota["freshness"]["state"], "future");
    assert!(quota["freshness"]["ageMs"].is_null());
    assert_eq!(quota["lastFailure"]["state"], "future");
    assert!(quota["lastFailure"]["ageMs"].is_null());
    assert_eq!(quota["lastFailure"]["atOrAfterLatestObservation"], true);
    let text = result.to_string();
    assert!(!text.contains("EMAIL_CANARY"));
    assert!(!text.contains("\"data\""));
    assert!(!text.contains("\"raw\""));
}

#[test]
fn failed_without_observations_stays_missing_and_new_success_supersedes_failure() {
    let store = memory();
    failure(&store, "account/read", RECENT, "unsupported_method");
    failure(&store, "account/usage/read", RECENT, "request_timeout");
    let result = store.get_health(&query()).unwrap();
    for item in result["observations"].as_array().unwrap() {
        assert_eq!(item["availability"], "missing");
        assert_eq!(item["freshness"]["state"], "unknown");
    }
    assert_eq!(result["observations"][0]["capability"], "unsupported");
    assert_eq!(result["observations"][1]["capability"], "unknown");
    assert_eq!(
        result["observations"][1]["lastFailure"]["errorCode"],
        "request_timeout"
    );
    assert_eq!(
        result["observations"][1]["lastFailure"]["atOrAfterLatestObservation"],
        true
    );
    observation(&store, "account/read", NOW, None);
    let result = store
        .get_health(&json!({"sourceId":"a","maxAgeMs":0}))
        .unwrap();
    assert_eq!(result["observations"][0]["availability"], "available");
    assert_eq!(result["observations"][0]["capability"], "observed");
    assert_eq!(result["observations"][0]["freshness"]["state"], "fresh");
    assert_eq!(result["observations"][0]["freshness"]["ageMs"], 0);
    assert_eq!(
        result["observations"][0]["lastFailure"]["atOrAfterLatestObservation"],
        false
    );
    store.set_clock_ms(ms(NOW) + 1).unwrap();
    assert_eq!(
        store
            .get_health(&json!({"sourceId":"a","maxAgeMs":0}))
            .unwrap()["observations"][0]["freshness"]["state"],
        "stale"
    );
}

#[test]
fn records_are_source_scoped_and_preserve_capture_occurrence_unknowns() {
    let store = memory();
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    store
        .ingest_event(&event("EVENT_CANARY_1", OLD, Some(RECENT)))
        .unwrap();
    let mut skill = event("EVENT_CANARY_2", RECENT, None);
    skill["eventType"] = json!("skill_loaded");
    skill["evidenceType"] = json!("successful_skill_read");
    skill["skillName"] = json!("/private/SKILL_CANARY");
    skill["skillEvidenceKind"] = json!("main_read");
    store.ingest_event(&skill).unwrap();
    let mut other = event("EVENT_CANARY_3", FUTURE, Some(OLD));
    other["sourceId"] = json!("b");
    store.ingest_event(&other).unwrap();
    store
        .ingest_response_token(&response("RESPONSE_CANARY_1", RECENT, Some(OLD)))
        .unwrap();
    store
        .ingest_response_token(&response("RESPONSE_CANARY_2", NOW, None))
        .unwrap();
    let mut other = response("RESPONSE_CANARY_3", FUTURE, Some(FUTURE));
    other["sourceId"] = json!("b");
    store.ingest_response_token(&other).unwrap();
    import(&store, "a", 0, OLD, json!(["synthetic_partial"]));
    import(
        &store,
        "a",
        1,
        RECENT,
        json!(["synthetic_partial", "unknown_time"]),
    );
    import(&store, "b", 0, FUTURE, json!(["other_source_warning"]));
    let duplicate = import(&store, "a", 0, NOW, json!(["duplicate_not_retained"]));
    assert_eq!(duplicate["importAlreadyPresent"], true);
    store
        .update_settings(
            &json!({"capturePaused":true,"contentCaptureEnabled":false,"retentionDays":7}),
        )
        .unwrap();
    let result = store.get_health(&query()).unwrap();
    assert_eq!(
        result["stored"]["events"],
        json!({"count":"2","firstCapturedAt":OLD,"lastCapturedAt":RECENT,"firstOccurredAt":RECENT,"lastOccurredAt":RECENT,"unknownOccurredAtCount":"1"})
    );
    assert_eq!(
        result["stored"]["skills"],
        json!({"count":"1","firstCapturedAt":RECENT,"lastCapturedAt":RECENT,"firstOccurredAt":null,"lastOccurredAt":null,"unknownOccurredAtCount":"1"})
    );
    assert_eq!(
        result["stored"]["responseTokens"],
        json!({"count":"2","firstCapturedAt":RECENT,"lastCapturedAt":NOW,"firstOccurredAt":OLD,"lastOccurredAt":OLD,"unknownOccurredAtCount":"1"})
    );
    assert_eq!(
        result["stored"]["imports"],
        json!({"count":"2","firstImportedAt":OLD,"lastImportedAt":RECENT,"warningCodes":["synthetic_partial","unknown_time"],"warningCodesTruncated":false,"importsExamined":"2"})
    );
    assert_eq!(
        result["settings"],
        json!({"capturePaused":true,"contentCaptureEnabled":false,"retentionDays":7,"scope":"all_sources"})
    );
    let text = result.to_string();
    for forbidden in [
        "CANARY",
        "/private/",
        "other_source_warning",
        "duplicate_not_retained",
        "fingerprint",
        "threadId",
        "eventId",
        "sessionId",
        "toolArguments",
    ] {
        assert!(!text.contains(forbidden), "{forbidden} leaked");
    }
    assert!(text.contains("Import coverage: synthetic_partial"));
    let b = store.get_health(&json!({"sourceId":"b"})).unwrap();
    assert_eq!(b["stored"]["events"]["count"], "1");
    assert_eq!(b["stored"]["skills"]["count"], "0");
    assert_eq!(b["stored"]["responseTokens"]["count"], "1");
    assert_eq!(
        b["stored"]["imports"]["warningCodes"],
        json!(["other_source_warning"])
    );
    assert!(!b.to_string().contains("synthetic_partial"));
}

#[test]
fn import_warning_projection_is_bounded_and_discloses_truncation() {
    let store = memory();
    let codes: Vec<String> = (0..100)
        .map(|index| format!("warning_{index:03}"))
        .collect();
    import(&store, "a", 0, OLD, json!(codes));
    import(
        &store,
        "a",
        1,
        NOW,
        json!(["extra_warning", "extra_warning"]),
    );
    let first = store.get_health(&query()).unwrap();
    assert_eq!(first["stored"]["imports"]["count"], "2");
    assert_eq!(first["stored"]["imports"]["importsExamined"], "2");
    assert_eq!(
        first["stored"]["imports"]["warningCodes"]
            .as_array()
            .unwrap()
            .len(),
        100
    );
    assert_eq!(first["stored"]["imports"]["warningCodesTruncated"], true);
    assert!(
        first
            .to_string()
            .contains("additional warnings may be unknown")
    );
    for index in 2..102 {
        import(&store, "a", index, RECENT, json!(["recent_only"]));
    }
    let next = store.get_health(&query()).unwrap();
    assert_eq!(next["stored"]["imports"]["count"], "102");
    assert_eq!(next["stored"]["imports"]["firstImportedAt"], OLD);
    assert_eq!(next["stored"]["imports"]["lastImportedAt"], NOW);
    assert_eq!(next["stored"]["imports"]["importsExamined"], "100");
    assert_eq!(
        next["stored"]["imports"]["warningCodes"],
        json!(["extra_warning", "recent_only"])
    );
    assert_eq!(next["stored"]["imports"]["warningCodesTruncated"], true);
}

#[test]
fn health_rejects_unknown_keys_invalid_freshness_and_missing_sources() {
    let mut store = memory();
    for input in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"sourceId":"a","unknown":true}),
        json!({"sourceId":"a","maxAgeMs":null}),
        json!({"sourceId":"a","maxAgeMs":-1}),
        json!({"sourceId":"a","maxAgeMs":2592000001_u64}),
        json!({"sourceId":"a","maxAgeMs":1.5}),
        json!({"sourceId":"a","maxAgeMs":"100"}),
        json!({"sourceId":"a","maxAgeMs":true}),
        json!({"sourceId":"/private/bad"}),
        json!({"sourceId":null}),
        json!({"sourceId":"a","fromDate":"2026-01-01"}),
    ] {
        error(store.get_health(&input), CoreError::InvalidInput);
    }
    assert!(
        store
            .get_health(&json!({"sourceId":"a","maxAgeMs":2592000000_u64}))
            .is_ok()
    );
    error(
        store.get_health(&json!({"sourceId":"missing"})),
        CoreError::SourceNotFound,
    );
    store.close().unwrap();
    error(store.get_health(&query()), CoreError::StoreClosed);
}

#[test]
fn existing_rollback_store_health_queries_preserve_bytes_mtime_and_directory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let mut writer = UsageStore::with_clock_ms(&path, ms(NOW)).unwrap();
    source(&writer, "a");
    writer
        .ingest_event(&event("EVENT_CANARY", OLD, None))
        .unwrap();
    observation(&writer, "account/usage/read", OLD, None);
    failure(&writer, "account/usage/read", RECENT, "request_timeout");
    writer
        .ingest_response_token(&response("RESPONSE_CANARY", NOW, None))
        .unwrap();
    import(&writer, "a", 0, RECENT, json!(["synthetic_partial"]));
    writer.close().unwrap();
    let before = snapshot(dir.path());
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let mut reader = UsageStore::open_read_only(&path).unwrap();
    reader.set_clock_ms(ms(NOW)).unwrap();
    for age in [0, 900000, 2592000000_i64] {
        let result = reader
            .get_health(&json!({"sourceId":"a","maxAgeMs":age}))
            .unwrap();
        assert_eq!(result["stored"]["events"]["count"], "1");
        assert_eq!(result["stored"]["responseTokens"]["count"], "1");
        assert_eq!(
            result["observations"][1]["lastFailure"]["errorCode"],
            "request_timeout"
        );
        assert_eq!(snapshot(dir.path()), before);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
    }
    reader.close().unwrap();
    assert_eq!(snapshot(dir.path()), before);
    let db = Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "delete"
    );
}

#[test]
fn malformed_import_metadata_cannot_echo_content_through_warning_codes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let mut writer = UsageStore::with_clock_ms(&path, ms(NOW)).unwrap();
    source(&writer, "a");
    import(&writer, "a", 0, NOW, json!([]));
    writer.close().unwrap();
    let connection = Connection::open(&path).unwrap();
    let invalid = metadata(0, NOW, json!(["/private/BODY_CANARY"]));
    connection
        .execute("UPDATE imports SET payload=?", [invalid.to_string()])
        .unwrap();
    connection.close().unwrap();
    let reader = UsageStore::open_read_only(&path).unwrap();
    error(reader.get_health(&query()), CoreError::StorageError);
}

#[test]
fn null_metrics_are_observed_metadata_and_other_sources_do_not_influence_health() {
    let store = memory();
    observation(&store, "account/usage/read", OLD, None);
    store
        .ingest_observation(&json!({
            "sourceId":"a","method":"account/usage/read","observedAt":NOW,
            "adapterVersion":"synthetic-1","schemaBaseline":"synthetic-schema","raw":{"summary":null,"dailyUsageBuckets":null}
        }))
        .unwrap();
    store
        .ingest_observation(&json!({
            "sourceId":"b","method":"account/read","observedAt":FUTURE,
            "adapterVersion":"OTHER_SOURCE_CANARY","schemaBaseline":"synthetic-schema","raw":{"account":null}
        }))
        .unwrap();
    store
        .record_failure(&json!({
            "sourceId":"b","method":"account/usage/read","attemptedAt":FUTURE,
            "errorCode":"unsupported_method"
        }))
        .unwrap();
    let result = store.get_health(&query()).unwrap();
    assert_eq!(result["observations"][0]["availability"], "missing");
    assert_eq!(result["observations"][0]["freshness"]["state"], "unknown");
    assert_eq!(result["observations"][1]["availability"], "available");
    assert_eq!(result["observations"][1]["freshness"]["observedAt"], NOW);
    assert_eq!(result["observations"][1]["freshness"]["state"], "fresh");
    assert_eq!(result["observations"][1]["capability"], "observed");
    assert!(result["observations"][1]["lastFailure"].is_null());
    assert!(!result.to_string().contains("OTHER_SOURCE_CANARY"));
}
