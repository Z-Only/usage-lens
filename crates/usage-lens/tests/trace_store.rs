//! Synthetic-only store contracts; never reads client history or configuration.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use usage_lens::{
    adapters::{incremental::import_incremental_rollout, rollout::ROLLOUT_SOURCE_VERSION},
    core::{
        UsageStore,
        trace::{TRACE_ADAPTER_VERSION, TRACE_SOURCE_VERSION},
    },
};
const NOW: &str = "2026-10-03T00:00:00.000Z";
fn source(store: &UsageStore, id: &str, mode: &str) {
    store.create_source(&json!({"id":id,"displayName":"Synthetic","mode":mode,"provider":"synthetic","coverageDescription":"Synthetic fixtures only"})).unwrap();
}
fn store() -> UsageStore {
    let store = UsageStore::in_memory().unwrap();
    source(&store, "synthetic", "imported");
    store
}
fn reported(value: &str) -> Value {
    json!({"state":"reported","value":value})
}
fn absent(state: &str) -> Value {
    json!({"state":state,"value":null})
}
fn attempt(id: &str) -> Value {
    json!({"attemptId":id,"threadId":"thread","turnId":"turn","inferenceId":id,"startedAt":NOW,"completedAt":NOW,"status":"completed","request":{"model":reported("future-model"),"reasoningEffort":reported("future-effort"),"serviceTier":reported("future-tier")},"observed":{"model":absent("omitted"),"serviceTier":absent("not_reported")},"responseId":format!("response-{id}"),"upstreamRequestId":format!("upstream-{id}"),"tokens":{"inputTokens":reported("9223372036854775807"),"cachedInputTokens":reported("0"),"cacheWriteInputTokens":absent("omitted"),"outputTokens":reported("2"),"reasoningOutputTokens":absent("invalid"),"totalTokens":absent("not_reported")},"requestProjection":{"messages":[{"role":"user","text":"VISIBLE SENTINEL api_key=synthetic-secret"}],"projection":"visible_text_only"},"responseProjection":{"messages":[{"role":"assistant","text":"RESPONSE SENTINEL"}],"projection":"visible_text_only"},"evidence":"prepared_request"})
}
fn bundle(id: &str, attempts: Vec<Value>) -> Value {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(id.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    json!({"sourceId":"synthetic","fingerprint":hash,"adapterVersion":TRACE_ADAPTER_VERSION,"sourceVersion":TRACE_SOURCE_VERSION,"importedAt":NOW,"bundleId":id,"attempts":attempts,"warningCodes":["prepared_request_not_delivery_proof"]})
}
fn all(store: &UsageStore) -> Value {
    store
        .get_trace_attempts(&json!({"sourceId":"synthetic"}))
        .unwrap()
}
fn summary(store: &UsageStore) -> Value {
    store
        .get_trace_summary(&json!({"sourceId":"synthetic"}))
        .unwrap()
}
fn detail(store: &UsageStore, id: &str) -> Value {
    store
        .get_local_trace_detail(&json!({"sourceId":"synthetic","attemptId":id}))
        .unwrap()
}
fn version(store: &UsageStore) -> Value {
    store.get_status().unwrap()["schemaVersion"].clone()
}
#[test]
fn legacy_queries_do_not_migrate_and_old_and_new_read_only_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store, "synthetic", "imported");
    assert_eq!(version(&store), 2);
    assert_eq!(all(&store)["attempts"], json!([]));
    assert_eq!(summary(&store)["attemptCount"], "0");
    assert_eq!(summary(&store)["totals"]["inputTokens"], Value::Null);
    assert_eq!(all(&store)["coverage"]["capture"], "not_captured");
    assert_eq!(
        store
            .get_local_trace_detail(&json!({"sourceId":"synthetic","attemptId":"missing"}))
            .unwrap_err()
            .code(),
        "trace_attempt_not_found"
    );
    let before = std::fs::read(&path).unwrap();
    let ro = UsageStore::open_read_only(&path).unwrap();
    assert_eq!(summary(&ro)["attemptCount"], "0");
    drop(ro);
    assert_eq!(before, std::fs::read(&path).unwrap());
    assert_eq!(version(&store), 2);
    import_incremental_rollout(&store,b"",&json!({"sourceId":"synthetic","streamId":"empty","observedAt":NOW,"sourceVersion":ROLLOUT_SOURCE_VERSION})).unwrap();
    assert_eq!(version(&store), 3);
    let ro = UsageStore::open_read_only(&path).unwrap();
    assert_eq!(all(&ro)["attempts"], json!([]));
    drop(ro);
    assert_eq!(version(&store), 3);
    store
        .import_trace_bundle(&bundle("bundle", vec![attempt("one")]))
        .unwrap();
    assert_eq!(version(&store), 4);
    let before = std::fs::read(&path).unwrap();
    let ro = UsageStore::open_read_only(&path).unwrap();
    assert_eq!(summary(&ro)["attemptCount"], "1");
    assert_eq!(detail(&ro, "one")["contentRetained"], false);
    assert!(ro.import_trace_bundle(&bundle("write", vec![])).is_err());
    drop(ro);
    assert_eq!(before, std::fs::read(&path).unwrap());
    // Version four continues to support explicit incremental checkpoints.
    import_incremental_rollout(&store,b"",&json!({"sourceId":"synthetic","streamId":"after-trace","observedAt":NOW,"sourceVersion":ROLLOUT_SOURCE_VERSION})).unwrap();
    assert!(
        store
            .get_rollout_checkpoint(&json!({"sourceId":"synthetic","streamId":"after-trace"}))
            .unwrap()
            .is_object()
    );
    drop(store);
    assert_eq!(version(&UsageStore::open(&path).unwrap()), 4);
}
#[test]
fn migration_is_atomic_and_only_successful_explicit_imports_upgrade() {
    let store = store();
    let mut invalid = bundle("bundle", vec![attempt("one")]);
    invalid["sourceVersion"] = json!("unsupported");
    assert!(store.import_trace_bundle(&invalid).is_err());
    assert_eq!(version(&store), 2);
    store
        .update_settings(&json!({"capturePaused":true}))
        .unwrap();
    assert_eq!(
        store
            .import_trace_bundle(&bundle("bundle", vec![]))
            .unwrap_err()
            .code(),
        "capture_paused"
    );
    assert_eq!(version(&store), 2);
    store
        .update_settings(&json!({"capturePaused":false}))
        .unwrap();
    // The second record conflicts after migration and the first record insert; everything rolls back.
    let mut two = attempt("two");
    two["responseId"] = json!("response-one");
    assert_eq!(
        store
            .import_trace_bundle(&bundle("conflict", vec![attempt("one"), two]))
            .unwrap_err()
            .code(),
        "trace_identity_conflict"
    );
    assert_eq!(version(&store), 2);
    assert_eq!(summary(&store)["attemptCount"], "0");
    source(&store, "demo", "demo");
    let mut demo = bundle("bundle", vec![]);
    demo["sourceId"] = json!("demo");
    assert_eq!(
        store.import_trace_bundle(&demo).unwrap_err().code(),
        "imported_source_required"
    );
    assert_eq!(version(&store), 2);
    let mut missing = demo;
    missing["sourceId"] = json!("missing");
    assert_eq!(
        store.import_trace_bundle(&missing).unwrap_err().code(),
        "source_not_found"
    );
    store.import_trace_bundle(&bundle("empty", vec![])).unwrap();
    assert_eq!(version(&store), 4);
    assert_eq!(summary(&store)["coverage"]["capture"], "available");
}
#[test]
fn metadata_only_is_default_and_cannot_be_backfilled() {
    let store = store();
    let input = bundle("bundle", vec![attempt("one")]);
    let result = store.import_trace_bundle(&input).unwrap();
    assert_eq!(result["attemptsInserted"], "1");
    assert_eq!(result["contentsRetained"], "0");
    assert_eq!(all(&store)["attempts"][0]["contentRetained"], false);
    assert_eq!(detail(&store, "one")["content"], Value::Null);
    let list = all(&store).to_string();
    assert!(!list.contains("VISIBLE SENTINEL"));
    assert!(!list.contains("requestProjection"));
    assert!(!list.contains("synthetic-secret"));
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    let mut replay = input.clone();
    replay["importedAt"] = json!("2026-10-04T00:00:00.000Z");
    assert_eq!(
        store.import_trace_bundle(&replay).unwrap()["importAlreadyPresent"],
        true
    );
    assert_eq!(
        store
            .import_trace_bundle(&bundle("new-bundle", vec![attempt("one")]))
            .unwrap()["attemptsInserted"],
        "0"
    );
    assert_eq!(detail(&store, "one")["contentRetained"], false);
}
#[test]
fn opt_in_content_is_redacted_and_content_deletion_never_resurrects() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixture.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store, "synthetic", "imported");
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    let input = bundle("bundle", vec![attempt("one")]);
    assert_eq!(
        store.import_trace_bundle(&input).unwrap()["contentsRetained"],
        "1"
    );
    let local = detail(&store, "one");
    assert_eq!(local["contentRetained"], true);
    assert!(local.to_string().contains("VISIBLE SENTINEL"));
    assert!(local.to_string().contains("[REDACTED]"));
    assert!(!local.to_string().contains("synthetic-secret"));
    assert!(!String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains("synthetic-secret"));
    assert_eq!(all(&store)["attempts"][0]["contentRetained"], true);
    assert_eq!(
        store
            .clear_local_content(&json!({"sourceId":"synthetic"}))
            .unwrap()["contentsDeleted"],
        "1"
    );
    store.import_trace_bundle(&input).unwrap();
    store
        .import_trace_bundle(&bundle("replay", vec![attempt("one")]))
        .unwrap();
    assert_eq!(detail(&store, "one")["contentRetained"], false);
    assert!(!String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains("VISIBLE SENTINEL"));
}
#[test]
fn clear_and_retention_preserve_trace_tombstones_and_source_scope() {
    let store = store();
    store
        .update_settings(&json!({"contentCaptureEnabled":true,"retentionDays":1}))
        .unwrap();
    source(&store, "other", "imported");
    let input = bundle("bundle", vec![attempt("one")]);
    store.import_trace_bundle(&input).unwrap();
    let mut other = input.clone();
    other["sourceId"] = json!("other");
    store.import_trace_bundle(&other).unwrap();
    assert_eq!(
        store.clear_data(&json!({"sourceId":"synthetic"})).unwrap()["traceAttemptsDeleted"],
        "1"
    );
    assert_eq!(summary(&store)["attemptCount"], "0");
    assert_eq!(
        store
            .get_trace_summary(&json!({"sourceId":"other"}))
            .unwrap()["attemptCount"],
        "1"
    );
    assert_eq!(
        store.import_trace_bundle(&input).unwrap()["importAlreadyPresent"],
        true
    );
    assert_eq!(
        store
            .import_trace_bundle(&bundle("replay", vec![attempt("one")]))
            .unwrap()["attemptsInserted"],
        "0"
    );
    store
        .import_trace_bundle(&bundle("new", vec![attempt("two")]))
        .unwrap();
    assert_eq!(
        store
            .apply_retention(&json!({"now":"2026-10-05T00:00:00.000Z"}))
            .unwrap()["traceAttemptsDeleted"],
        "2"
    );
    assert_eq!(
        store
            .import_trace_bundle(&bundle("new-replay", vec![attempt("two")]))
            .unwrap()["attemptsInserted"],
        "0"
    );
    let mut changed = attempt("two");
    changed["request"]["model"] = reported("different");
    assert_eq!(
        store
            .import_trace_bundle(&bundle("changed", vec![changed]))
            .unwrap_err()
            .code(),
        "trace_identity_conflict"
    );
    assert_eq!(
        store.clear_local_content(&json!({})).unwrap()["contentsDeleted"],
        "0"
    );
    store.clear_data(&json!({})).unwrap();
    assert_eq!(
        store
            .import_trace_bundle(&bundle("third-replay", vec![attempt("two")]))
            .unwrap()["attemptsInserted"],
        "0"
    );
    // Deleting a source intentionally forgets that local namespace, consistent with existing stores.
    store.delete_source("synthetic").unwrap();
    source(&store, "synthetic", "imported");
    assert_eq!(
        store.import_trace_bundle(&input).unwrap()["attemptsInserted"],
        "1"
    );
}
#[test]
fn immutable_bundle_attempt_and_response_identities_are_transactional() {
    let store = store();
    let input = bundle("bundle", vec![attempt("one")]);
    store.import_trace_bundle(&input).unwrap();
    for changed in [
        {
            let mut v = input.clone();
            v["fingerprint"] = json!("f".repeat(64));
            v
        },
        {
            let mut v = input.clone();
            v["bundleId"] = json!("alias");
            v
        },
        {
            let mut v = input.clone();
            v["attempts"][0]["request"]["model"] = reported("different");
            v
        },
    ] {
        assert_eq!(
            store.import_trace_bundle(&changed).unwrap_err().code(),
            "trace_identity_conflict"
        );
    }
    let mut changed = attempt("one");
    changed["tokens"]["inputTokens"] = reported("1");
    assert_eq!(
        store
            .import_trace_bundle(&bundle("conflict", vec![attempt("new"), changed]))
            .unwrap_err()
            .code(),
        "trace_identity_conflict"
    );
    assert_eq!(summary(&store)["attemptCount"], "1");
    let mut alias = attempt("alias");
    alias["inferenceId"] = json!("one");
    assert_eq!(
        store
            .import_trace_bundle(&bundle("alias", vec![alias]))
            .unwrap_err()
            .code(),
        "trace_identity_conflict"
    );
    let mut same_id = attempt("one");
    same_id["inferenceId"] = json!("different");
    assert_eq!(
        store
            .import_trace_bundle(&bundle("same-id", vec![same_id]))
            .unwrap_err()
            .code(),
        "trace_identity_conflict"
    );
    let mut response_alias = attempt("two");
    response_alias["responseId"] = json!("response-one");
    assert_eq!(
        store
            .import_trace_bundle(&bundle("same-response", vec![response_alias]))
            .unwrap_err()
            .code(),
        "trace_identity_conflict"
    );
    assert_eq!(
        store
            .import_trace_bundle(&bundle("duplicate", vec![attempt("one"), attempt("one")]))
            .unwrap()["attemptsInserted"],
        "0"
    );
}
#[test]
fn summaries_keep_requested_observed_unknowns_and_exact_token_evidence_separate() {
    let store = store();
    let mut two = attempt("two");
    two["observed"]["model"] = reported("served-model");
    two["observed"]["serviceTier"] = reported("actual-tier");
    two["request"]["serviceTier"] = absent("invalid");
    let mut three = attempt("three");
    three["status"] = json!("failed");
    three["tokens"] = Value::Null;
    three["responseId"] = Value::Null;
    three["request"]["model"] = absent("not_reported");
    let mut four = attempt("four");
    four["status"] = json!("incomplete");
    four["completedAt"] = Value::Null;
    four["tokens"] = Value::Null;
    four["responseId"] = Value::Null;
    four["request"]["model"] = absent("omitted");
    let mut five = attempt("five");
    five["status"] = json!("cancelled");
    five["tokens"] = Value::Null;
    five["request"]["model"] = absent("invalid");
    store
        .import_trace_bundle(&bundle(
            "bundle",
            vec![attempt("one"), two, three, four, five],
        ))
        .unwrap();
    let summary = summary(&store);
    assert_eq!(summary["attemptCount"], "5");
    assert_eq!(summary["groupsTruncated"], false);
    assert_eq!(summary["tokenAttemptCount"], "2");
    assert_eq!(summary["totals"]["inputTokens"], "18446744073709551614");
    assert_eq!(summary["totals"]["cachedInputTokens"], "0");
    assert_eq!(summary["totals"]["cacheWriteInputTokens"], Value::Null);
    assert_eq!(
        summary["tokenCoverage"]["inputTokens"]["reportedCount"],
        "2"
    );
    assert_eq!(
        summary["tokenCoverage"]["inputTokens"]["notReportedCount"],
        "3"
    );
    assert_eq!(
        summary["tokenCoverage"]["reasoningOutputTokens"]["invalidCount"],
        "2"
    );
    assert_eq!(summary["byRequestedModel"].as_array().unwrap().len(), 4);
    let reported = summary["byRequestedModel"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["state"] == "reported")
        .unwrap();
    assert_eq!(reported["value"], "future-model");
    assert_eq!(reported["totals"]["inputTokens"], "18446744073709551614");
    assert_ne!(summary["byRequestedModel"], summary["byObservedModel"]);
    assert!(summary["threadId"].is_null());
    let text = summary.to_string();
    // Thread identifiers are intentionally exposed by the local-only thread breakdown.
    assert_eq!(summary["byThread"][0]["threadId"], "thread");
    for forbidden in [
        "sourceId",
        "turnId",
        "inferenceId",
        "attemptId",
        "bundleId",
        "responseId",
        "upstreamRequestId",
        "fingerprint",
        "VISIBLE SENTINEL",
        "RESPONSE SENTINEL",
        "requestProjection",
        "synthetic-secret",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
    assert_eq!(
        store
            .get_response_token_usage(&json!({"sourceId":"synthetic"}))
            .unwrap()["responseCount"],
        "0"
    );
}
#[test]
fn stable_pagination_dates_and_source_bound_cursors() {
    let store = store();
    source(&store, "other", "imported");
    let mut old = attempt("old");
    old["startedAt"] = json!("2026-10-02T01:00:00Z");
    old["completedAt"] = json!("2026-10-02T02:00:00Z");
    store
        .import_trace_bundle(&bundle("bundle", vec![old, attempt("a"), attempt("b")]))
        .unwrap();
    let first = store
        .get_trace_attempts(&json!({"sourceId":"synthetic","limit":1}))
        .unwrap();
    assert_eq!(first["attempts"][0]["attemptId"], "b");
    let second = store
        .get_trace_attempts(&json!({"sourceId":"synthetic","limit":1,"cursor":first["nextCursor"]}))
        .unwrap();
    assert_eq!(second["attempts"][0]["attemptId"], "a");
    let third = store
        .get_trace_attempts(
            &json!({"sourceId":"synthetic","limit":1,"cursor":second["nextCursor"]}),
        )
        .unwrap();
    assert_eq!(third["attempts"][0]["attemptId"], "old");
    assert_eq!(third["nextCursor"], Value::Null);
    assert!(
        store
            .get_trace_attempts(&json!({"sourceId":"other","cursor":first["nextCursor"]}))
            .is_err()
    );
    let filtered = store
        .get_trace_attempts(
            &json!({"sourceId":"synthetic","fromDate":"2026-10-02","toDate":"2026-10-02"}),
        )
        .unwrap();
    assert_eq!(filtered["attempts"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["fromDate"], "2026-10-02");
    assert_eq!(
        store
            .get_trace_summary(
                &json!({"sourceId":"synthetic","fromDate":"2026-10-03","toDate":"2026-10-03"})
            )
            .unwrap()["attemptCount"],
        "2"
    );
    assert_eq!(
        store
            .get_trace_summary(
                &json!({"sourceId":"synthetic","fromDate":"2026-10-04","toDate":"2026-10-04"})
            )
            .unwrap()["attemptCount"],
        "0"
    );
    let cursor = URL_SAFE_NO_PAD
        .encode(json!({"sourceId":"synthetic","kind":"events","at":NOW,"key":"one"}).to_string());
    assert!(
        store
            .get_trace_attempts(&json!({"sourceId":"synthetic","cursor":cursor}))
            .is_err()
    );
}
#[test]
fn malformed_inputs_are_bounded_rejected_and_do_not_migrate() {
    let store = store();
    let good = bundle("bundle", vec![attempt("one")]);
    let changes: Vec<(&str, Value)> = vec![
        ("fingerprint", json!("X".repeat(64))),
        ("bundleId", json!("../invalid")),
        ("adapterVersion", json!("other")),
        ("importedAt", json!("invalid")),
        ("sourceId", Value::Null),
        ("warningCodes", json!(["bad-code"])),
        ("attempts", json!({})),
        ("extra", json!(true)),
    ];
    for (key, value) in changes {
        let mut changed = good.clone();
        changed[key] = value;
        assert!(store.import_trace_bundle(&changed).is_err(), "{key}");
    }
    for key in ["request", "observed"] {
        let mut v = good.clone();
        v["attempts"][0][key] = json!({});
        assert!(store.import_trace_bundle(&v).is_err());
    }
    for value in [
        json!({"state":"reported","value":""}),
        json!({"state":"unknown","value":null}),
        json!({"state":"omitted","value":"leak"}),
        json!({"state":"reported","value":42}),
        json!({"state":"invalid"}),
        json!({"state":"reported","value":"model","extra":true}),
    ] {
        let mut v = good.clone();
        v["attempts"][0]["request"]["model"] = value;
        assert!(store.import_trace_bundle(&v).is_err());
    }
    for value in [
        json!("-1"),
        json!("01"),
        json!("1.0"),
        json!("1e2"),
        json!("9223372036854775808"),
        json!(1),
        json!("9".repeat(30)),
    ] {
        let mut v = good.clone();
        v["attempts"][0]["tokens"]["inputTokens"]["value"] = value;
        assert!(store.import_trace_bundle(&v).is_err());
    }
    let attempt_changes: Vec<(&str, Value)> = vec![
        ("startedAt", json!("bad")),
        ("completedAt", Value::Null),
        ("status", json!("unknown")),
        ("status", json!("failed")),
        ("responseId", Value::Null),
        ("evidence", json!("sent")),
        (
            "requestProjection",
            json!({"messages":[],"projection":"raw"}),
        ),
        (
            "responseProjection",
            json!({"messages":[{"role":"system","text":"hidden"}],"projection":"visible_text_only"}),
        ),
        (
            "requestProjection",
            json!({"messages":[{"role":"user","text":10}],"projection":"visible_text_only"}),
        ),
        (
            "requestProjection",
            json!({"messages":{},"projection":"visible_text_only"}),
        ),
        ("tokens", json!({})),
        ("extra", json!(true)),
    ];
    for (key, value) in attempt_changes {
        let mut v = good.clone();
        v["attempts"][0][key] = value;
        assert!(store.import_trace_bundle(&v).is_err(), "{key}");
    }
    for input in [
        json!({"sourceId":"synthetic","limit":0}),
        json!({"sourceId":"synthetic","limit":501}),
        json!({"sourceId":"synthetic","cursor":"!"}),
        json!({"sourceId":"synthetic","cursor":null}),
        json!({"sourceId":"synthetic","fromDate":"2026-10-03"}),
        json!({"sourceId":"synthetic","fromDate":null,"toDate":null}),
        json!({"sourceId":"synthetic","fromDate":"2026-10-04","toDate":"2026-10-03"}),
        json!({"sourceId":"synthetic","extra":true}),
    ] {
        assert!(store.get_trace_attempts(&input).is_err());
    }
    assert!(
        store
            .get_trace_summary(&json!({"sourceId":"synthetic","extra":true}))
            .is_err()
    );
    assert!(
        store
            .get_local_trace_detail(&json!({"sourceId":"synthetic","attemptId":"bad id"}))
            .is_err()
    );
    assert!(
        store
            .get_local_trace_detail(&json!({"sourceId":"synthetic","attemptId":"one","extra":true}))
            .is_err()
    );
    assert_eq!(version(&store), 2);
}
#[test]
fn input_and_projection_bounds_are_enforced_before_persistence() {
    let store = store();
    let good = bundle("bundle", vec![attempt("one")]);
    let mut v = good.clone();
    v["attempts"] = json!(vec![attempt("same"); 1001]);
    assert!(store.import_trace_bundle(&v).is_err());
    let mut v = good.clone();
    v["warningCodes"] = json!(vec!["warning"; 101]);
    assert!(store.import_trace_bundle(&v).is_err());
    let mut v = good.clone();
    v["attempts"][0]["requestProjection"]["messages"][0]["text"] = json!("x".repeat(512 * 1024));
    assert!(store.import_trace_bundle(&v).is_err());
    let mut v = good.clone();
    v["attempts"][0]["requestProjection"]["messages"] =
        json!(vec![json!({"role":"user","text":"x"}); 1001]);
    assert!(store.import_trace_bundle(&v).is_err());
    let mut v = good.clone();
    v["attempts"][0]["requestProjection"]["messages"][0]["text"] =
        json!("x".repeat(2 * 1024 * 1024));
    assert!(store.import_trace_bundle(&v).is_err());
    assert_eq!(version(&store), 2);
}
#[test]
fn null_projections_closed_store_and_source_errors_are_safe() {
    let mut store = store();
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    let mut a = attempt("one");
    a["requestProjection"] = Value::Null;
    a["responseProjection"] = Value::Null;
    store
        .import_trace_bundle(&bundle("bundle", vec![a]))
        .unwrap();
    assert_eq!(detail(&store, "one")["contentRetained"], false);
    assert_eq!(
        store
            .get_local_trace_detail(&json!({"sourceId":"synthetic","attemptId":"missing"}))
            .unwrap_err()
            .code(),
        "trace_attempt_not_found"
    );
    assert_eq!(
        store
            .get_trace_summary(&json!({"sourceId":"missing"}))
            .unwrap_err()
            .code(),
        "source_not_found"
    );
    assert_eq!(
        store
            .get_trace_attempts(&json!({"sourceId":"missing"}))
            .unwrap_err()
            .code(),
        "source_not_found"
    );
    store.close().unwrap();
    assert_eq!(
        store
            .get_trace_summary(&json!({"sourceId":"synthetic"}))
            .unwrap_err()
            .code(),
        "store_closed"
    );
}

#[test]
fn wall_clock_regression_is_preserved_and_metadata_secrets_are_invalid() {
    let store = store();
    let mut value = attempt("one");
    value["completedAt"] = json!("2026-10-02T23:59:59.000Z");
    value["request"]["model"] = reported("unknown future model");
    value["request"]["serviceTier"] = reported("api_key=secret-value");
    value["observed"]["model"] = reported("sk-proj-abcdefghijklmnop");
    store
        .import_trace_bundle(&bundle("regression", vec![value]))
        .unwrap();
    let result = detail(&store, "one");
    assert_eq!(result["attempt"]["completedAt"], "2026-10-02T23:59:59.000Z");
    assert_eq!(
        result["attempt"]["request"]["model"]["value"],
        "unknown future model"
    );
    assert_eq!(
        result["attempt"]["request"]["serviceTier"],
        absent("invalid")
    );
    assert_eq!(result["attempt"]["observed"]["model"], absent("invalid"));
    assert!(!result.to_string().contains("secret-value"));
    assert!(
        !summary(&store)
            .to_string()
            .contains("sk-proj-abcdefghijklmnop")
    );
}

#[test]
fn metadata_group_cap_does_not_truncate_exact_overall_totals() {
    let store = store();
    let records = (0..501)
        .map(|index| {
            let mut value = attempt(&format!("attempt-{index}"));
            value["request"]["model"] = reported(&format!("model-{index}"));
            value["tokens"]["inputTokens"] = reported("1");
            value
        })
        .collect();
    store
        .import_trace_bundle(&bundle("many-models", records))
        .unwrap();
    let result = summary(&store);
    assert_eq!(result["byRequestedModel"].as_array().unwrap().len(), 500);
    assert_eq!(result["attemptCount"], "501");
    assert_eq!(result["groupsTruncated"], true);
    assert_eq!(result["totals"]["inputTokens"], "501");
    assert!(result["warnings"].to_string().contains("500 groups"));
}

#[test]
fn import_warning_metadata_survives_read_only_retention_and_data_deletion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("warnings.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store, "synthetic", "imported");
    source(&store, "other", "imported");
    assert_eq!(
        summary(&store)["importWarnings"],
        json!({"scope":"all_retained_source","codes":[],"truncated":false})
    );
    let mut input = bundle("warnings", vec![attempt("one")]);
    input["warningCodes"] = json!([
        "trace_sequence_gap",
        "future_safe_warning",
        "trace_sequence_gap",
        "trace_clock_regression"
    ]);
    store.import_trace_bundle(&input).unwrap();
    let expected = json!({"scope":"all_retained_source","codes":["trace_sequence_gap","future_safe_warning","trace_clock_regression"],"truncated":false});
    for result in [all(&store), detail(&store, "one"), summary(&store)] {
        assert_eq!(result["importWarnings"], expected);
        assert!(
            result["warnings"]
                .to_string()
                .contains("Import coverage: trace_sequence_gap")
        );
        assert!(result["warnings"].to_string().contains("source-wide"));
    }
    assert_eq!(
        store
            .get_trace_summary(&json!({"sourceId":"other"}))
            .unwrap()["importWarnings"]["codes"],
        json!([])
    );
    let filtered = store
        .get_trace_summary(
            &json!({"sourceId":"synthetic","fromDate":"2026-11-01","toDate":"2026-11-01"}),
        )
        .unwrap();
    assert_eq!(filtered["attemptCount"], "0");
    assert_eq!(filtered["importWarnings"], expected);
    let before = std::fs::read(&path).unwrap();
    let ro = UsageStore::open_read_only(&path).unwrap();
    assert_eq!(summary(&ro)["importWarnings"], expected);
    assert_eq!(detail(&ro, "one")["importWarnings"], expected);
    assert_eq!(all(&ro)["importWarnings"], expected);
    drop(ro);
    assert_eq!(before, std::fs::read(&path).unwrap());
    store.clear_local_content(&json!({})).unwrap();
    assert_eq!(detail(&store, "one")["importWarnings"], expected);
    store
        .apply_retention(&json!({"now":"2026-12-31T00:00:00Z"}))
        .unwrap();
    assert_eq!(summary(&store)["attemptCount"], "0");
    assert_eq!(summary(&store)["importWarnings"], expected);
    store.clear_data(&json!({})).unwrap();
    assert_eq!(all(&store)["importWarnings"], expected);
    assert_eq!(
        store.import_trace_bundle(&input).unwrap()["attemptsInserted"],
        "0"
    );
    assert_eq!(summary(&store)["importWarnings"], expected);
    assert!(summary(&store)["threadId"].is_null());
    let text = summary(&store).to_string();
    assert!(!text.contains("\"thread\""));
    for forbidden in [
        "warnings.sqlite",
        "fingerprint",
        "bundleId",
        "requestProjection",
        "VISIBLE SENTINEL",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
    store.delete_source("synthetic").unwrap();
    source(&store, "synthetic", "imported");
    assert_eq!(summary(&store)["importWarnings"]["codes"], json!([]));
}

#[test]
fn import_warning_projection_bounds_imports_and_distinct_codes_with_explicit_truncation() {
    let store = store();
    for index in 0..101 {
        let mut value = bundle(&format!("bundle-{index:03}"), vec![]);
        value["warningCodes"] = json!([format!("warning_{index:03}")]);
        store.import_trace_bundle(&value).unwrap();
    }
    let result = summary(&store);
    let warnings = &result["importWarnings"];
    assert_eq!(warnings["truncated"], true);
    assert_eq!(warnings["codes"].as_array().unwrap().len(), 100);
    assert_eq!(warnings["codes"][0], "warning_100");
    assert!(
        !warnings["codes"]
            .as_array()
            .unwrap()
            .contains(&json!("warning_000"))
    );
    assert!(
        result["warnings"]
            .to_string()
            .contains("truncated to the latest 100 imports")
    );
    source(&store, "distinct", "imported");
    let mut first = bundle("first", vec![]);
    first["sourceId"] = json!("distinct");
    first["warningCodes"] = json!((0..100).map(|i| format!("code_{i:03}")).collect::<Vec<_>>());
    store.import_trace_bundle(&first).unwrap();
    let mut second = bundle("second", vec![]);
    second["sourceId"] = json!("distinct");
    second["warningCodes"] = json!(["newest_code"]);
    second["importedAt"] = json!("2026-10-04T00:00:00Z");
    store.import_trace_bundle(&second).unwrap();
    let result = store
        .get_trace_summary(&json!({"sourceId":"distinct"}))
        .unwrap();
    assert_eq!(result["importWarnings"]["truncated"], true);
    assert_eq!(
        result["importWarnings"]["codes"].as_array().unwrap().len(),
        100
    );
    assert_eq!(result["importWarnings"]["codes"][0], "newest_code");
    assert!(
        !result["importWarnings"]["codes"]
            .as_array()
            .unwrap()
            .contains(&json!("code_099"))
    );
    source(&store, "empty", "imported");
    let mut empty = bundle("empty", vec![]);
    empty["sourceId"] = json!("empty");
    empty["warningCodes"] = json!([]);
    store.import_trace_bundle(&empty).unwrap();
    assert_eq!(
        store
            .get_trace_summary(&json!({"sourceId":"empty"}))
            .unwrap()["importWarnings"],
        json!({"scope":"all_retained_source","codes":[],"truncated":false})
    );
}

#[test]
fn corrupt_warning_metadata_fails_closed_without_echoing_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupt-warnings.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store, "synthetic", "imported");
    store
        .import_trace_bundle(&bundle("bundle", vec![attempt("one")]))
        .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    for invalid in [
        "not-json".to_owned(),
        json!({"body":"secret body"}).to_string(),
        json!(vec!["warning"; 101]).to_string(),
        json!(["api_key=secret-value"]).to_string(),
        json!([42]).to_string(),
    ] {
        db.execute("UPDATE trace_imports SET warning_codes=?", [invalid])
            .unwrap();
        assert_eq!(
            store
                .get_trace_summary(&json!({"sourceId":"synthetic"}))
                .unwrap_err()
                .code(),
            "storage_error"
        );
        assert_eq!(
            store
                .get_trace_attempts(&json!({"sourceId":"synthetic"}))
                .unwrap_err()
                .code(),
            "storage_error"
        );
        assert_eq!(
            store
                .get_local_trace_detail(&json!({"sourceId":"synthetic","attemptId":"one"}))
                .unwrap_err()
                .code(),
            "storage_error"
        );
    }
    db.execute("UPDATE trace_imports SET warning_codes='[]'", [])
        .unwrap();
    assert_eq!(summary(&store)["importWarnings"]["codes"], json!([]));
}

#[test]
fn bounded_group_streaming_preserves_counts_for_retained_keys() {
    let store = store();
    let records = (0..500)
        .map(|index| {
            let mut value = attempt(&format!("first-{index:03}"));
            value["request"]["model"] = reported(&format!("model-{index:03}"));
            value["tokens"]["inputTokens"] = reported("1");
            value
        })
        .collect();
    store
        .import_trace_bundle(&bundle("first", records))
        .unwrap();
    assert_eq!(summary(&store)["groupsTruncated"], false);
    let records = [
        ("aaa", "aaa"),
        ("zzz", "zzz"),
        ("retained", "model-000"),
        ("evicted", "model-499"),
    ]
    .into_iter()
    .map(|(id, model)| {
        let mut value = attempt(id);
        value["request"]["model"] = reported(model);
        value["tokens"]["inputTokens"] = reported("5");
        value
    })
    .collect();
    store
        .import_trace_bundle(&bundle("second", records))
        .unwrap();
    let result = summary(&store);
    let groups = result["byRequestedModel"].as_array().unwrap();
    assert_eq!(groups.len(), 500);
    assert_eq!(result["groupsTruncated"], true);
    assert_eq!(result["attemptCount"], "504");
    assert_eq!(result["totals"]["inputTokens"], "520");
    assert!(
        groups
            .iter()
            .all(|group| group["value"] != "zzz" && group["value"] != "model-499")
    );
    let retained = groups
        .iter()
        .find(|group| group["value"] == "model-000")
        .unwrap();
    assert_eq!(retained["count"], "2");
    assert_eq!(retained["totals"]["inputTokens"], "6");
    assert_eq!(groups[0]["value"], "aaa");
    assert_eq!(groups[499]["value"], "model-498");
}

// The shared core budget includes structural overhead; it is not serialized
// UTF-8 length. Find a valid ASCII boundary and prove one more byte exceeds it.
fn fill_to_budget(value: &mut Value, pointer: &str, budget: usize) {
    use usage_lens::core::validation::check_size;
    let (mut low, mut high) = (0, budget);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        *value.pointer_mut(pointer).unwrap() = json!("x".repeat(middle));
        if check_size(value, budget).is_ok() {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    *value.pointer_mut(pointer).unwrap() = json!("x".repeat(low + 1));
    assert_eq!(
        check_size(value, budget).unwrap_err().code(),
        "input_too_large"
    );
    *value.pointer_mut(pointer).unwrap() = json!("x".repeat(low));
    check_size(value, budget).unwrap();
}
fn sized_projection(size: usize) -> Value {
    let mut value =
        json!({"projection":"visible_text_only","messages":[{"role":"user","text":""}]});
    fill_to_budget(&mut value, "/messages/0/text", size);
    value
}
fn sized_import(size: usize) -> Value {
    use usage_lens::core::validation::{CONTENT_BYTES, check_size};
    let attempts = (0..4)
        .map(|index| {
            let mut value = attempt(&format!("normalized-{index}"));
            value["requestProjection"] = if index < 3 {
                sized_projection(CONTENT_BYTES)
            } else {
                sized_projection(128)
            };
            value["responseProjection"] = Value::Null;
            value
        })
        .collect::<Vec<_>>();
    let mut result = bundle("normalized-size", attempts);
    fill_to_budget(
        &mut result,
        "/attempts/3/requestProjection/messages/0/text",
        size,
    );
    for attempt in result["attempts"].as_array().unwrap() {
        check_size(&attempt["requestProjection"], CONTENT_BYTES).unwrap();
    }
    result
}

#[test]
fn every_projection_limit_rejects_atomically_even_with_capture_off() {
    use usage_lens::core::validation::{BATCH_EVENTS, CONTENT_BYTES, RAW_BYTES};
    for capture in [false, true] {
        for schema in [2, 3, 4] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("bounded.sqlite");
            let store = UsageStore::open(&path).unwrap();
            source(&store, "synthetic", "imported");
            store
                .update_settings(&json!({"contentCaptureEnabled":capture}))
                .unwrap();
            if schema == 3 {
                import_incremental_rollout(&store,b"",&json!({"sourceId":"synthetic","streamId":"empty","observedAt":NOW,"sourceVersion":ROLLOUT_SOURCE_VERSION})).unwrap();
            } else if schema == 4 {
                store
                    .import_trace_bundle(&bundle("seed", vec![attempt("seed")]))
                    .unwrap();
            }
            let before = std::fs::read(&path).unwrap();
            let before_summary = summary(&store);
            let mut large = attempt("oversize-projection");
            large["requestProjection"] = sized_projection(CONTENT_BYTES + 1);
            let mut many = attempt("too-many-messages");
            many["requestProjection"]["messages"] =
                json!(vec![json!({"role":"user","text":"x"}); BATCH_EVENTS + 1]);
            for (input, code) in [
                (bundle("large-projection", vec![large]), "input_too_large"),
                (bundle("many-messages", vec![many]), "invalid_input"),
                (sized_import(RAW_BYTES + 1), "input_too_large"),
            ] {
                assert_eq!(
                    store.import_trace_bundle(&input).unwrap_err().code(),
                    code,
                    "capture={capture},schema={schema}"
                );
                assert_eq!(version(&store), schema);
                assert_eq!(summary(&store), before_summary);
                assert_eq!(
                    std::fs::read(&path).unwrap(),
                    before,
                    "Rejected import changed persisted state"
                );
            }
            // A rejection creates no replay marker: the same bundle/attempt identity can
            // subsequently be accepted when independently supplied within all bounds.
            let mut bounded = attempt("oversize-projection");
            bounded["requestProjection"] = sized_projection(CONTENT_BYTES);
            assert_eq!(
                store
                    .import_trace_bundle(&bundle("large-projection", vec![bounded]))
                    .unwrap()["attemptsInserted"],
                "1"
            );
            assert_eq!(
                detail(&store, "oversize-projection")["contentRetained"],
                capture
            );
        }
    }
}
#[test]
fn exact_normalized_and_message_count_boundaries_are_supported_for_both_capture_modes() {
    use usage_lens::core::validation::{BATCH_EVENTS, RAW_BYTES};
    for capture in [false, true] {
        let store = store();
        store
            .update_settings(&json!({"contentCaptureEnabled":capture}))
            .unwrap();
        let input = sized_import(RAW_BYTES);
        assert_eq!(
            store.import_trace_bundle(&input).unwrap()["attemptsInserted"],
            "4"
        );
        let mut value = attempt("message-boundary");
        value["requestProjection"]["messages"] =
            json!(vec![json!({"role":"user","text":"x"}); BATCH_EVENTS]);
        assert_eq!(
            store
                .import_trace_bundle(&bundle("messages", vec![value]))
                .unwrap()["attemptsInserted"],
            "1"
        );
        assert_eq!(summary(&store)["attemptCount"], "5");
        assert_eq!(
            detail(&store, "message-boundary")["contentRetained"],
            capture
        );
    }
}

#[test]
fn preflight_predicts_shared_acceptance_without_migration_or_content_exposure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic-preview.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store, "synthetic", "imported");
    let input = bundle(
        "preview-bundle",
        vec![attempt("preview-one"), attempt("preview-two")],
    );
    for schema in [2, 3, 4] {
        if schema == 3 {
            import_incremental_rollout(&store,b"",&json!({"sourceId":"synthetic","streamId":"empty","observedAt":NOW,"sourceVersion":ROLLOUT_SOURCE_VERSION})).unwrap();
        }
        if schema == 4 {
            store
                .import_trace_bundle(&bundle("empty-upgrade", vec![]))
                .unwrap();
        }
        for capture in [false, true] {
            store
                .update_settings(&json!({"contentCaptureEnabled":capture}))
                .unwrap();
            let before = std::fs::read(&path).unwrap();
            let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
            let ro = UsageStore::open_read_only(&path).unwrap();
            let preview = ro.preflight_trace_bundle(&input).unwrap();
            assert_eq!(preview["dryRun"], true);
            assert_eq!(preview["operation"], "trace_import_preflight");
            assert_eq!(preview["status"], "ready");
            assert_eq!(preview["attemptsInBundle"], "2");
            assert_eq!(preview["attemptsWouldInsert"], "2");
            assert_eq!(preview["attemptsAlreadyPresent"], "0");
            assert_eq!(
                preview["contentsWouldRetain"],
                if capture { "2" } else { "0" }
            );
            assert_eq!(preview["database"]["schemaVersion"], schema);
            assert_eq!(preview["database"]["wouldUpgrade"], schema < 4);
            assert_eq!(preview["database"]["accessMode"], "read_only");
            for secret in [
                "VISIBLE SENTINEL",
                "RESPONSE SENTINEL",
                "preview-one",
                "preview-bundle",
                "threadId",
                "responseId",
                "fingerprint",
                "synthetic-preview.sqlite",
            ] {
                assert!(!preview.to_string().contains(secret), "{secret}");
            }
            drop(ro);
            assert_eq!(before, std::fs::read(&path).unwrap());
            assert_eq!(
                modified,
                std::fs::metadata(&path).unwrap().modified().unwrap()
            );
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
            assert_eq!(version(&store), schema);
            assert_eq!(all(&store)["attempts"], json!([]));
        }
    }
    let preview = store.preflight_trace_bundle(&input).unwrap();
    let accepted = store.import_trace_bundle(&input).unwrap();
    assert_eq!(preview["attemptsWouldInsert"], accepted["attemptsInserted"]);
    assert_eq!(preview["contentsWouldRetain"], accepted["contentsRetained"]);
    let duplicate = store.preflight_trace_bundle(&input).unwrap();
    assert_eq!(duplicate["importAlreadyPresent"], true);
    assert_eq!(duplicate["attemptsWouldInsert"], "0");
    assert_eq!(duplicate["attemptsAlreadyPresent"], "2");
    assert_eq!(duplicate["contentsWouldRetain"], "0");
    store.clear_data(&json!({"sourceId":"synthetic"})).unwrap();
    assert_eq!(store.preflight_trace_bundle(&input).unwrap(), duplicate);
    let overlap = bundle(
        "overlapping-bundle",
        vec![attempt("preview-one"), attempt("new-one")],
    );
    let preview = store.preflight_trace_bundle(&overlap).unwrap();
    assert_eq!(preview["importAlreadyPresent"], false);
    assert_eq!(preview["attemptsAlreadyPresent"], "1");
    assert_eq!(preview["attemptsWouldInsert"], "1");
    assert_eq!(
        store.import_trace_bundle(&overlap).unwrap()["attemptsInserted"],
        "1"
    );
}

#[test]
fn preflight_rejects_same_conflicts_as_import_and_rechecks_changed_settings() {
    let store = store();
    let input = bundle("bundle", vec![attempt("one")]);
    store.preflight_trace_bundle(&input).unwrap();
    store
        .update_settings(&json!({"capturePaused":true}))
        .unwrap();
    for preview in [true, false] {
        let error = if preview {
            store.preflight_trace_bundle(&input)
        } else {
            store.import_trace_bundle(&input)
        }
        .unwrap_err();
        assert_eq!(error.code(), "capture_paused");
        assert_eq!(version(&store), 2);
    }
    store
        .update_settings(&json!({"capturePaused":false}))
        .unwrap();
    let mut cases = Vec::new();
    let mut duplicate_id = attempt("one");
    duplicate_id["request"]["model"] = reported("changed");
    cases.push(vec![attempt("one"), duplicate_id]);
    let mut duplicate_identity = attempt("two");
    duplicate_identity["inferenceId"] = json!("one");
    cases.push(vec![attempt("one"), duplicate_identity]);
    let mut duplicate_response = attempt("two");
    duplicate_response["responseId"] = json!("response-one");
    cases.push(vec![attempt("one"), duplicate_response]);
    for attempts in cases {
        let conflict = bundle("conflict", attempts);
        assert_eq!(
            store.preflight_trace_bundle(&conflict).unwrap_err().code(),
            "trace_identity_conflict"
        );
        assert_eq!(
            store.import_trace_bundle(&conflict).unwrap_err().code(),
            "trace_identity_conflict"
        );
        assert_eq!(version(&store), 2);
    }
    let repeated = bundle("exact-duplicate", vec![attempt("one"), attempt("one")]);
    let preview = store.preflight_trace_bundle(&repeated).unwrap();
    assert_eq!(preview["attemptsInBundle"], "2");
    assert_eq!(preview["attemptsWouldInsert"], "1");
    assert_eq!(preview["attemptsAlreadyPresent"], "1");
    assert_eq!(
        store.import_trace_bundle(&repeated).unwrap()["attemptsInserted"],
        "1"
    );
    let mut conflict = repeated.clone();
    conflict["fingerprint"] = json!("f".repeat(64));
    assert_eq!(
        store.preflight_trace_bundle(&conflict).unwrap_err().code(),
        "trace_identity_conflict"
    );
    let mut stale = attempt("one");
    stale["request"]["model"] = reported("changed");
    assert_eq!(
        store
            .preflight_trace_bundle(&bundle("changed-attempt", vec![stale]))
            .unwrap_err()
            .code(),
        "trace_identity_conflict"
    );
    let mut claimed_response = attempt("three");
    claimed_response["responseId"] = json!("response-one");
    assert_eq!(
        store
            .preflight_trace_bundle(&bundle("changed-response", vec![claimed_response]))
            .unwrap_err()
            .code(),
        "trace_identity_conflict"
    );
    let mut missing = input.clone();
    missing["sourceId"] = json!("missing");
    assert_eq!(
        store.preflight_trace_bundle(&missing).unwrap_err().code(),
        "source_not_found"
    );
    source(&store, "live", "live");
    missing["sourceId"] = json!("live");
    assert_eq!(
        store.preflight_trace_bundle(&missing).unwrap_err().code(),
        "imported_source_required"
    );
    let mut invalid = input;
    invalid["unknown"] = json!(true);
    assert_eq!(
        store.preflight_trace_bundle(&invalid).unwrap_err().code(),
        "invalid_input"
    );
}

#[test]
fn preflight_does_not_reserve_acceptance_or_restore_deleted_content() {
    let store = store();
    let input = bundle("later", vec![attempt("one")]);
    assert_eq!(
        store.preflight_trace_bundle(&input).unwrap()["attemptsWouldInsert"],
        "1"
    );
    let mut competing = attempt("one");
    competing["request"]["model"] = reported("changed-since-preview");
    store
        .import_trace_bundle(&bundle("first", vec![competing]))
        .unwrap();
    let retained = all(&store);
    assert_eq!(
        store.import_trace_bundle(&input).unwrap_err().code(),
        "trace_identity_conflict"
    );
    assert_eq!(all(&store), retained);
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    let input = bundle("captured", vec![attempt("two")]);
    store.import_trace_bundle(&input).unwrap();
    store
        .clear_local_content(&json!({"sourceId":"synthetic"}))
        .unwrap();
    let preview = store.preflight_trace_bundle(&input).unwrap();
    assert_eq!(preview["contentCaptureEnabled"], true);
    assert_eq!(preview["contentsWouldRetain"], "0");
    assert_eq!(preview["attemptsWouldInsert"], "0");
    assert_eq!(detail(&store, "two")["contentRetained"], false);
}
