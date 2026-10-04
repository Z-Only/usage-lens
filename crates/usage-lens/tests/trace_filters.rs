//! Exact local trace query scopes. All stores and records are synthetic.
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
const KEYS: [&str; 5] = [
    "threadId",
    "status",
    "requestedModel",
    "requestedReasoningEffort",
    "requestedServiceTier",
];
fn source(store: &UsageStore, id: &str) {
    store.create_source(&json!({"id":id,"displayName":"Synthetic","mode":"imported","provider":"synthetic","coverageDescription":"Synthetic fixtures only"})).unwrap();
}
fn store() -> UsageStore {
    let store = UsageStore::in_memory().unwrap();
    source(&store, "selected");
    source(&store, "other");
    store
}
fn reported(value: &str) -> Value {
    json!({"state":"reported","value":value})
}
fn attempt(id: &str) -> Value {
    json!({"attemptId":id,"threadId":"thread-A","turnId":"turn","inferenceId":id,"startedAt":NOW,"completedAt":NOW,"status":"completed","request":{"model":reported("model-A"),"reasoningEffort":reported("high"),"serviceTier":reported("priority")},"observed":{"model":reported("observed-model"),"serviceTier":reported("observed-tier")},"responseId":format!("response-{id}"),"upstreamRequestId":null,"tokens":{"inputTokens":reported("9007199254740993"),"cachedInputTokens":reported("0"),"cacheWriteInputTokens":{"state":"omitted","value":null},"outputTokens":reported("2"),"reasoningOutputTokens":{"state":"invalid","value":null},"totalTokens":{"state":"not_reported","value":null}},"requestProjection":{"messages":[{"role":"user","text":"LOCAL CONTENT SENTINEL"}],"projection":"visible_text_only"},"responseProjection":null,"evidence":"prepared_request"})
}
fn import(store: &UsageStore, source: &str, id: &str, attempts: Vec<Value>) -> Value {
    use sha2::{Digest, Sha256};
    let fingerprint = Sha256::digest(id.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let input = json!({"sourceId":source,"fingerprint":fingerprint,"adapterVersion":TRACE_ADAPTER_VERSION,"sourceVersion":TRACE_SOURCE_VERSION,"importedAt":NOW,"bundleId":id,"attempts":attempts,"warningCodes":["synthetic_source_warning"]});
    store.import_trace_bundle(&input).unwrap();
    input
}
fn input(filters: Value) -> Value {
    let mut result = json!({"sourceId":"selected"});
    result
        .as_object_mut()
        .unwrap()
        .extend(filters.as_object().unwrap().clone());
    result
}
fn ids(list: &Value) -> Vec<&str> {
    list["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["attemptId"].as_str().unwrap())
        .collect()
}
fn scope(store: &UsageStore, filters: Value, expected: &[&str]) -> (Value, Value) {
    let query = input(filters);
    let list = store.get_trace_attempts(&query).unwrap();
    let summary = store.get_trace_summary(&query).unwrap();
    assert_eq!(ids(&list), expected, "{query}");
    assert_eq!(
        summary["attemptCount"],
        expected.len().to_string(),
        "{query}"
    );
    for key in KEYS.into_iter().chain(["fromDate", "toDate"]) {
        assert_eq!(list[key], query[key], "list echo {key}");
        assert_eq!(summary[key], query[key], "summary echo {key}");
        assert!(list.get(key).is_some());
        assert!(summary.get(key).is_some());
    }
    assert_eq!(list["importWarnings"], summary["importWarnings"]);
    assert!(!summary.to_string().contains("LOCAL CONTENT SENTINEL"));
    (list, summary)
}
fn invalid_both(store: &UsageStore, query: &Value) {
    assert_eq!(
        store.get_trace_attempts(query).unwrap_err().code(),
        "invalid_input",
        "{query}"
    );
    assert_eq!(
        store.get_trace_summary(query).unwrap_err().code(),
        "invalid_input",
        "{query}"
    );
}

#[test]
fn exact_filters_intersect_and_list_summary_have_identical_source_scope() {
    let store = store();
    let mut rows = vec![attempt("a")];
    for (id, field, value) in [("b", "threadId", "thread-B"), ("c", "status", "failed")] {
        let mut row = attempt(id);
        row[field] = json!(value);
        if field == "status" {
            row["tokens"] = Value::Null;
        }
        rows.push(row);
    }
    for (id, key, value) in [
        ("d", "model", "model-B"),
        ("e", "reasoningEffort", "low"),
        ("f", "serviceTier", "default"),
    ] {
        let mut row = attempt(id);
        row["request"][key] = reported(value);
        rows.push(row);
    }
    import(&store, "selected", "selected-bundle", rows);
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    import(&store, "other", "other-bundle", vec![attempt("a")]);
    let (list, summary) = scope(&store, json!({}), &["f", "e", "d", "c", "b", "a"]);
    assert!(
        list["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["contentRetained"] == false)
    );
    assert_eq!(summary["totals"]["inputTokens"], "45035996273704965");
    scope(
        &store,
        json!({"threadId":"thread-A"}),
        &["f", "e", "d", "c", "a"],
    );
    scope(&store, json!({"status":"failed"}), &["c"]);
    scope(&store, json!({"requestedModel":"model-B"}), &["d"]);
    scope(&store, json!({"requestedReasoningEffort":"low"}), &["e"]);
    scope(&store, json!({"requestedServiceTier":"default"}), &["f"]);
    let (_, summary) = scope(
        &store,
        json!({"threadId":"thread-A","status":"completed","requestedModel":"model-A","requestedReasoningEffort":"high","requestedServiceTier":"priority"}),
        &["a"],
    );
    assert_eq!(summary["totals"]["inputTokens"], "9007199254740993");
    assert_eq!(summary["byObservedModel"][0]["value"], "observed-model");
    assert_eq!(
        store
            .get_trace_attempts(&json!({"sourceId":"other","threadId":"thread-A"}))
            .unwrap()["attempts"][0]["contentRetained"],
        true
    );
    for missing in [
        json!({"threadId":"missing"}),
        json!({"status":"cancelled"}),
        json!({"requestedModel":"model"}),
        json!({"requestedReasoningEffort":"High"}),
        json!({"requestedServiceTier":"PRIORITY"}),
    ] {
        let (_, summary) = scope(&store, missing, &[]);
        assert!(summary["totals"]["inputTokens"].is_null());
        assert_eq!(
            summary["importWarnings"]["codes"],
            json!(["synthetic_source_warning"])
        );
        assert_eq!(summary["importWarnings"]["scope"], "all_retained_source");
    }
}

#[test]
fn requested_filters_never_infer_observed_or_missing_values() {
    let store = store();
    let mut rows = Vec::new();
    for (index, state) in ["omitted", "not_reported", "invalid"].iter().enumerate() {
        let mut row = attempt(&format!("absent-{index}"));
        for key in ["model", "reasoningEffort", "serviceTier"] {
            row["request"][key] = json!({"state":state,"value":null});
        }
        row["observed"]["model"] = reported("model-A");
        row["observed"]["serviceTier"] = reported("priority");
        rows.push(row);
    }
    rows.push(attempt("reported"));
    import(&store, "selected", "states", rows);
    for (key, value) in [
        ("requestedModel", "model-A"),
        ("requestedReasoningEffort", "high"),
        ("requestedServiceTier", "priority"),
    ] {
        scope(&store, json!({key:value}), &["reported"]);
        for missing in ["omitted", "not_reported", "invalid", "unknown", "null", ""] {
            if missing.is_empty() {
                invalid_both(&store, &input(json!({key:missing})));
            } else {
                scope(&store, json!({key:missing}), &[]);
            }
        }
    }
    scope(&store, json!({"requestedModel":"observed-model"}), &[]);
    scope(&store, json!({"requestedServiceTier":"observed-tier"}), &[]);
}

#[test]
fn exact_arbitrary_requested_values_are_bound_and_not_trimmed_or_normalized() {
    let store = store();
    let values = [
        "model-A",
        "Model-A",
        " model-A ",
        " ",
        "x' OR 1=1 --",
        "%_",
        "未知/模型",
        "null",
    ];
    let mut rows = Vec::new();
    for (index, value) in values.iter().enumerate() {
        let mut row = attempt(&format!("id-{index}"));
        for key in ["model", "reasoningEffort", "serviceTier"] {
            row["request"][key] = reported(value);
        }
        rows.push(row);
    }
    import(&store, "selected", "arbitrary", rows);
    for (index, value) in values.iter().enumerate() {
        for &key in &KEYS[2..] {
            scope(&store, json!({key:value}), &[&format!("id-{index}")]);
        }
    }
    for &key in &KEYS[2..] {
        scope(&store, json!({key:"MODEL-A"}), &[]);
    }
}

#[test]
fn valid_statuses_dates_and_utc_boundaries_match_the_same_attempts() {
    let store = store();
    let mut rows = Vec::new();
    for (index, status) in ["completed", "failed", "cancelled", "incomplete"]
        .iter()
        .enumerate()
    {
        let mut row = attempt(&format!("status-{index}"));
        row["status"] = json!(status);
        if *status != "completed" {
            row["tokens"] = Value::Null;
        }
        if *status == "incomplete" {
            row["completedAt"] = Value::Null;
        }
        rows.push(row);
    }
    let mut before = attempt("before");
    before["startedAt"] = json!("2026-10-02T23:59:59.999Z");
    let mut last = attempt("last");
    last["startedAt"] = json!("2026-10-03T23:59:59.999Z");
    last["completedAt"] = last["startedAt"].clone();
    let mut after = attempt("after");
    after["startedAt"] = json!("2026-10-04T00:00:00Z");
    after["completedAt"] = after["startedAt"].clone();
    rows.extend([before, last, after]);
    import(&store, "selected", "dates", rows);
    scope(
        &store,
        json!({"threadId":"thread-A","fromDate":"2026-10-03","toDate":"2026-10-03"}),
        &["last", "status-3", "status-2", "status-1", "status-0"],
    );
    for (index, status) in ["failed", "cancelled", "incomplete"].iter().enumerate() {
        let (_, summary) = scope(
            &store,
            json!({"status":status,"fromDate":"2026-10-03","toDate":"2026-10-03"}),
            &[&format!("status-{}", index + 1)],
        );
        assert_eq!(summary["tokenAttemptCount"], "0");
        assert!(summary["totals"]["inputTokens"].is_null());
    }
    scope(
        &store,
        json!({"status":"completed","requestedModel":"model-A","fromDate":"2026-10-03","toDate":"2026-10-03"}),
        &["last", "status-0"],
    );
    let (_, summary) = scope(
        &store,
        json!({"fromDate":"2025-01-01","toDate":"2025-01-01","threadId":"thread-A"}),
        &[],
    );
    assert_eq!(
        summary["importWarnings"]["codes"],
        json!(["synthetic_source_warning"])
    );
}

#[test]
fn malformed_filter_values_are_rejected_even_without_trace_schema() {
    let store = store();
    for key in KEYS {
        for value in [
            Value::Null,
            json!(true),
            json!(2),
            json!([]),
            json!({}),
            json!(""),
        ] {
            invalid_both(&store, &input(json!({key:value})));
        }
    }
    for value in [
        "invalid",
        "COMPLETED",
        " completed",
        "completed ",
        "completed,failed",
    ] {
        invalid_both(&store, &input(json!({"status":value})));
    }
    for value in [
        " space",
        ".leading",
        "../path",
        "with space",
        "with?query",
        "line\nfeed",
        "汉字",
    ] {
        invalid_both(&store, &input(json!({"threadId":value})));
    }
    invalid_both(&store, &input(json!({"threadId":"x".repeat(161)})));
    for &key in &KEYS[2..] {
        for value in [
            "x".repeat(129),
            "🦀".repeat(65),
            "line\nfeed".into(),
            "tab\tstop".into(),
            "delete\u{7f}".into(),
            "nul\0byte".into(),
        ] {
            invalid_both(&store, &input(json!({key:value})));
        }
        scope(&store, json!({key:"x".repeat(128)}), &[]);
        scope(&store, json!({key:"🦀".repeat(64)}), &[]);
    }
    scope(&store, json!({"threadId":"x".repeat(160)}), &[]);
    scope(&store, json!({"threadId":"A._:@/+-9"}), &[]);
    for filters in [
        json!({"fromDate":null,"toDate":null}),
        json!({"fromDate":"2026-10-01"}),
        json!({"toDate":"2026-10-01"}),
        json!({"fromDate":"2026-02-30","toDate":"2026-03-01"}),
        json!({"fromDate":"2026-10-04","toDate":"2026-10-03"}),
        json!({"fromDate":"2000-01-01","toDate":"2026-10-03"}),
        json!({"requestedEffort":"high"}),
        json!({"model":"model-A"}),
        json!({"observedModel":"observed-model"}),
    ] {
        invalid_both(&store, &input(filters));
    }
    assert_eq!(store.get_status().unwrap()["schemaVersion"], 2);
    for query in [
        json!({"sourceId":"missing","threadId":"thread-A"}),
        json!({"sourceId":"missing","status":"completed"}),
    ] {
        assert_eq!(
            store.get_trace_attempts(&query).unwrap_err().code(),
            "source_not_found"
        );
        assert_eq!(
            store.get_trace_summary(&query).unwrap_err().code(),
            "source_not_found"
        );
    }
}

#[test]
fn pagination_is_bounded_deterministic_and_binds_every_scope_field() {
    let store = store();
    let mut rows = Vec::new();
    for id in ["a", "b", "c", "d", "e"] {
        rows.push(attempt(id));
    }
    for id in ["bb", "cc", "dd", "ff"] {
        let mut row = attempt(id);
        row["threadId"] = json!("excluded");
        rows.push(row);
    }
    import(&store, "selected", "pages", rows);
    let filters = json!({"sourceId":"selected","threadId":"thread-A","status":"completed","requestedModel":"model-A","requestedReasoningEffort":"high","requestedServiceTier":"priority","fromDate":"2026-10-03","toDate":"2026-10-03","limit":2});
    let first = store.get_trace_attempts(&filters).unwrap();
    assert_eq!(ids(&first), ["e", "d"]);
    let cursor = first["nextCursor"].clone();
    let mut next = filters.clone();
    next["cursor"] = cursor.clone();
    let second = store.get_trace_attempts(&next).unwrap();
    assert_eq!(ids(&second), ["c", "b"]);
    next["cursor"] = second["nextCursor"].clone();
    next["limit"] = json!(1);
    let last = store.get_trace_attempts(&next).unwrap();
    assert_eq!(ids(&last), ["a"]);
    assert!(last["nextCursor"].is_null());
    for (key, value) in [
        ("sourceId", "other"),
        ("threadId", "thread-B"),
        ("status", "failed"),
        ("requestedModel", "model-B"),
        ("requestedReasoningEffort", "low"),
        ("requestedServiceTier", "default"),
        ("fromDate", "2026-10-02"),
        ("toDate", "2026-10-04"),
    ] {
        let mut changed = filters.clone();
        changed["cursor"] = cursor.clone();
        changed[key] = json!(value);
        assert_eq!(
            store.get_trace_attempts(&changed).unwrap_err().code(),
            "invalid_input",
            "{key}"
        );
    }
    for key in KEYS {
        let mut changed = filters.clone();
        changed["cursor"] = cursor.clone();
        changed.as_object_mut().unwrap().remove(key);
        assert_eq!(
            store.get_trace_attempts(&changed).unwrap_err().code(),
            "invalid_input",
            "removed {key}"
        );
    }
    let mut changed = filters.clone();
    changed["cursor"] = cursor;
    changed.as_object_mut().unwrap().remove("fromDate");
    changed.as_object_mut().unwrap().remove("toDate");
    assert!(store.get_trace_attempts(&changed).is_err());
    let unfiltered = store
        .get_trace_attempts(&json!({"sourceId":"selected","limit":1}))
        .unwrap();
    for filter in [
        json!({"threadId":"thread-A"}),
        json!({"fromDate":"2026-10-03","toDate":"2026-10-03"}),
    ] {
        let mut changed = input(filter);
        changed["cursor"] = unfiltered["nextCursor"].clone();
        assert!(store.get_trace_attempts(&changed).is_err());
    }
    // JSON field order and page size are not part of query scope.
    let mut reordered = json!({"requestedServiceTier":"priority","requestedReasoningEffort":"high","requestedModel":"model-A","status":"completed","threadId":"thread-A","toDate":"2026-10-03","fromDate":"2026-10-03","sourceId":"selected","limit":3});
    reordered["cursor"] = first["nextCursor"].clone();
    assert_eq!(
        ids(&store.get_trace_attempts(&reordered).unwrap()),
        ["c", "b", "a"]
    );
}

#[test]
fn malformed_and_unbound_cursors_fail_closed() {
    let store = store();
    import(
        &store,
        "selected",
        "cursors",
        vec![attempt("a"), attempt("b")],
    );
    let first = store
        .get_trace_attempts(&json!({"sourceId":"selected","limit":1}))
        .unwrap();
    let cursor: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(first["nextCursor"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    for (key, value) in [
        ("sourceId", json!("other")),
        ("kind", json!("events")),
        ("filters", json!("wrong")),
        ("at", json!("not-a-timestamp")),
        ("key", json!("!")),
        ("extra", json!(true)),
    ] {
        let mut changed = cursor.clone();
        changed[key] = value;
        let encoded = URL_SAFE_NO_PAD.encode(changed.to_string());
        assert_eq!(
            store
                .get_trace_attempts(&json!({"sourceId":"selected","cursor":encoded}))
                .unwrap_err()
                .code(),
            "invalid_input",
            "{key}"
        );
    }
    for key in ["sourceId", "kind", "filters", "at", "key"] {
        let mut changed = cursor.clone();
        changed.as_object_mut().unwrap().remove(key);
        assert!(store.get_trace_attempts(&json!({"sourceId":"selected","cursor":URL_SAFE_NO_PAD.encode(changed.to_string())})).is_err());
    }
    for value in [
        Value::Null,
        json!(true),
        json!(""),
        json!("!"),
        json!("a"),
        json!("a".repeat(801)),
        json!(URL_SAFE_NO_PAD.encode("not json")),
        json!(URL_SAFE_NO_PAD.encode("[]")),
    ] {
        assert_eq!(
            store
                .get_trace_attempts(&json!({"sourceId":"selected","cursor":value}))
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }
}

#[test]
fn deletion_preserves_scope_cursor_position_and_source_wide_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store, "selected");
    source(&store, "other");
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    let bundle = import(
        &store,
        "selected",
        "selected-delete",
        vec![attempt("a"), attempt("b"), attempt("c")],
    );
    import(&store, "other", "other-delete", vec![attempt("a")]);
    let first = store
        .get_trace_attempts(&json!({"sourceId":"selected","threadId":"thread-A","limit":1}))
        .unwrap();
    assert_eq!(ids(&first), ["c"]);
    // Deleting the cursor's boundary row does not shift or duplicate later rows.
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("PRAGMA foreign_keys=ON", []).unwrap();
    db.execute(
        "DELETE FROM trace_attempts WHERE source_id=? AND attempt_id=?",
        ["selected", "c"],
    )
    .unwrap();
    drop(db);
    let next = store
        .get_trace_attempts(
            &json!({"sourceId":"selected","threadId":"thread-A","cursor":first["nextCursor"]}),
        )
        .unwrap();
    assert_eq!(ids(&next), ["b", "a"]);
    store
        .clear_local_content(&json!({"sourceId":"selected"}))
        .unwrap();
    let (list, _) = scope(&store, json!({"threadId":"thread-A"}), &["b", "a"]);
    assert!(
        list["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["contentRetained"] == false)
    );
    store.clear_data(&json!({"sourceId":"selected"})).unwrap();
    let (_, summary) = scope(&store, json!({"threadId":"thread-A"}), &[]);
    assert_eq!(
        summary["importWarnings"]["codes"],
        json!(["synthetic_source_warning"])
    );
    assert_eq!(
        store
            .get_trace_attempts(&json!({"sourceId":"other","threadId":"thread-A"}))
            .unwrap()["attempts"][0]["contentRetained"],
        true
    );
    assert_eq!(
        store.import_trace_bundle(&bundle).unwrap()["attemptsInserted"],
        "0"
    );
    scope(&store, json!({"threadId":"thread-A"}), &[]);
    let after_clear = store
        .get_trace_attempts(
            &json!({"sourceId":"selected","threadId":"thread-A","cursor":first["nextCursor"]}),
        )
        .unwrap();
    assert!(ids(&after_clear).is_empty());
}

#[test]
fn all_supported_schema_versions_support_filters_read_only_without_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store, "selected");
    let filters = json!({"threadId":"thread-A","status":"completed","requestedModel":"model-A","requestedReasoningEffort":"high","requestedServiceTier":"priority","fromDate":"2026-10-03","toDate":"2026-10-03"});
    for version in [2, 3, 4] {
        if version == 3 {
            import_incremental_rollout(&store,b"",&json!({"sourceId":"selected","streamId":"empty","observedAt":NOW,"sourceVersion":ROLLOUT_SOURCE_VERSION})).unwrap();
        }
        if version == 4 {
            import(&store, "selected", "readonly", vec![attempt("a")]);
        }
        assert_eq!(store.get_status().unwrap()["schemaVersion"], version);
        let before = std::fs::read(&path).unwrap();
        let readonly = UsageStore::open_read_only(&path).unwrap();
        let expected = if version == 4 { vec!["a"] } else { vec![] };
        scope(&readonly, filters.clone(), &expected);
        assert_eq!(readonly.get_status().unwrap()["schemaVersion"], version);
        drop(readonly);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn maximum_filter_and_identity_lengths_keep_cursors_bounded() {
    let store = store();
    let source_id = "s".repeat(80);
    let thread_id = "t".repeat(160);
    let metadata = "🦀".repeat(64);
    source(&store, &source_id);
    let first_id = "a".repeat(160);
    let second_id = "b".repeat(160);
    let mut rows = Vec::new();
    for id in [&first_id, &second_id] {
        let mut row = attempt(id);
        // responseId is independently bounded; keep it unrelated to the long ID.
        row["responseId"] = json!(format!("response-{}", rows.len()));
        row["threadId"] = json!(thread_id);
        for key in ["model", "reasoningEffort", "serviceTier"] {
            row["request"][key] = reported(&metadata);
        }
        rows.push(row);
    }
    import(&store, &source_id, "maximum", rows);
    let mut query = json!({"sourceId":source_id,"threadId":thread_id,"status":"completed","requestedModel":metadata,"requestedReasoningEffort":metadata,"requestedServiceTier":metadata,"fromDate":"2026-10-03","toDate":"2026-10-03","limit":1});
    let first = store.get_trace_attempts(&query).unwrap();
    assert_eq!(ids(&first), [second_id]);
    assert!(first["nextCursor"].as_str().unwrap().len() <= 800);
    query["cursor"] = first["nextCursor"].clone();
    let next = store.get_trace_attempts(&query).unwrap();
    assert_eq!(ids(&next), [first_id]);
    assert!(next["nextCursor"].is_null());
}

async fn cli_query(
    command: &str,
    path: &std::path::Path,
    flags: &[&str],
) -> (i32, Vec<u8>, String) {
    let mut args = vec![
        command.to_owned(),
        "--db".into(),
        path.to_str().unwrap().to_owned(),
        "--source".into(),
        "selected".into(),
    ];
    args.extend(flags.iter().map(|value| (*value).to_owned()));
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let code = usage_lens::cli::run_main(&args, &b""[..], &mut output, &mut errors).await;
    (code, output, String::from_utf8(errors).unwrap())
}

#[tokio::test]
async fn cli_exact_filters_match_core_readonly_and_keep_dash_prefixed_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cli.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store, "selected");
    source(&store, "other");
    let mut rows = Vec::new();
    for id in ["a", "b"] {
        let mut row = attempt(id);
        row["request"]["model"] = reported("--model");
        row["request"]["reasoningEffort"] = reported("--effort");
        row["request"]["serviceTier"] = reported("--tier");
        rows.push(row);
    }
    import(&store, "other", "other-cli", rows.clone());
    rows.push(attempt("unselected"));
    import(&store, "selected", "cli", rows);
    let query = json!({"sourceId":"selected","threadId":"thread-A","status":"completed","requestedModel":"--model","requestedReasoningEffort":"--effort","requestedServiceTier":"--tier","fromDate":"2026-10-03","toDate":"2026-10-03"});
    let expected_list = store.get_trace_attempts(&query).unwrap();
    let expected_summary = store.get_trace_summary(&query).unwrap();
    drop(store);
    let before = std::fs::read(&path).unwrap();
    let filters = [
        "--thread",
        "thread-A",
        "--status",
        "completed",
        "--requested-model",
        "--model",
        "--requested-effort",
        "--effort",
        "--requested-tier",
        "--tier",
        "--from",
        "2026-10-03",
        "--to",
        "2026-10-03",
    ];
    for (command, expected) in [
        ("trace-attempts", expected_list),
        ("trace-summary", expected_summary),
    ] {
        let (code, output, error) = cli_query(command, &path, &filters).await;
        assert_eq!(code, 0, "{error}");
        assert!(error.is_empty());
        assert_eq!(serde_json::from_slice::<Value>(&output).unwrap(), expected);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    let mut flags = filters.to_vec();
    flags.extend(["--limit", "1"]);
    let (code, output, error) = cli_query("trace-attempts", &path, &flags).await;
    assert_eq!(code, 0, "{error}");
    let first: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(ids(&first), ["b"]);
    flags.extend(["--cursor", first["nextCursor"].as_str().unwrap()]);
    let (code, output, error) = cli_query("trace-attempts", &path, &flags).await;
    assert_eq!(code, 0, "{error}");
    assert_eq!(
        ids(&serde_json::from_slice::<Value>(&output).unwrap()),
        ["a"]
    );
    flags[1] = "other-thread";
    let (code, output, error) = cli_query("trace-attempts", &path, &flags).await;
    assert_eq!(code, 1);
    assert!(output.is_empty());
    assert!(error.contains("invalid_input"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[tokio::test]
async fn cli_rejects_invalid_filters_without_writing_or_expanding_aggregate_allowlists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid-cli.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store, "selected");
    drop(store);
    let before = std::fs::read(&path).unwrap();
    for command in ["trace-attempts", "trace-summary"] {
        for flags in [
            vec!["--status", "unknown"],
            vec!["--thread", "has space"],
            vec!["--requested-model", ""],
            vec!["--requested-effort", "bad\nvalue"],
            vec!["--requested-tier", "bad\u{7f}value"],
            vec!["--thread", "one", "--thread", "two"],
            vec!["--from", "2026-10-03"],
            vec!["--requested-service-tier", "priority"],
            vec!["--requestedReasoningEffort", "high"],
            vec!["--model", "model-A"],
            vec!["--thread"],
            vec!["--status", "--invalid"],
        ] {
            let (code, output, error) = cli_query(command, &path, &flags).await;
            assert_eq!(code, 1, "{command} {flags:?}");
            assert!(output.is_empty());
            assert!(!error.is_empty());
        }
        let too_long = "x".repeat(129);
        let (code, output, error) =
            cli_query(command, &path, &["--requested-model", &too_long]).await;
        assert_eq!(code, 1);
        assert!(output.is_empty());
        assert!(error.contains("invalid_input"));
    }
    for command in [
        "overview",
        "skill-summary",
        "response-tokens",
        "trace-detail",
    ] {
        let (code, output, error) =
            cli_query(command, &path, &["--requested-model", "model-A"]).await;
        assert_eq!(code, 1);
        assert!(output.is_empty());
        assert!(error.contains("invalid_argument"), "{command}: {error}");
    }
    for key in [
        "--thread",
        "--status",
        "--requested-model",
        "--requested-effort",
        "--requested-tier",
    ] {
        let args = ["mcp", "--db", path.to_str().unwrap(), key, "value"].map(str::to_owned);
        assert!(usage_lens::cli::parse_arguments(&args).is_err());
    }
    for flags in [vec!["--limit", "1"], vec!["--cursor", "cursor"]] {
        let (code, output, error) = cli_query("trace-summary", &path, &flags).await;
        assert_eq!(code, 1);
        assert!(output.is_empty());
        assert!(error.contains("invalid_argument"));
    }
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let missing = dir.path().join("must-not-create.sqlite");
    let (code, output, _) = cli_query("trace-attempts", &missing, &["--thread", "thread-A"]).await;
    assert_eq!(code, 1);
    assert!(output.is_empty());
    assert!(!missing.exists());
}

#[tokio::test]
async fn cli_filtered_queries_keep_legacy_schema_two_and_three_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy-cli.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store, "selected");
    for version in [2, 3] {
        if version == 3 {
            import_incremental_rollout(&store,b"",&json!({"sourceId":"selected","streamId":"empty","observedAt":NOW,"sourceVersion":ROLLOUT_SOURCE_VERSION})).unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        for command in ["trace-attempts", "trace-summary"] {
            let (code, output, error) = cli_query(
                command,
                &path,
                &[
                    "--thread",
                    "thread-A",
                    "--status",
                    "completed",
                    "--requested-model",
                    "model-A",
                    "--requested-effort",
                    "high",
                    "--requested-tier",
                    "priority",
                ],
            )
            .await;
            assert_eq!(code, 0, "{error}");
            let value: Value = serde_json::from_slice(&output).unwrap();
            assert_eq!(value["coverage"]["capture"], "not_captured");
            assert_eq!(value["threadId"], "thread-A");
            assert_eq!(value["requestedReasoningEffort"], "high");
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
        assert_eq!(store.get_status().unwrap()["schemaVersion"], version);
    }
}
