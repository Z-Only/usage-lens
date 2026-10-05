//! Synthetic local trace insights; no client history, credentials, or recording.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, NaiveDate};
use serde_json::{Value, json};
use usage_lens::{
    adapters::{incremental::import_incremental_rollout, rollout::ROLLOUT_SOURCE_VERSION},
    core::{
        UsageStore,
        trace::{TRACE_ADAPTER_VERSION, TRACE_SOURCE_VERSION},
    },
};
const NOW: &str = "2026-10-03T00:00:00.000Z";
fn source(store: &UsageStore, id: &str) {
    store.create_source(&json!({"id":id,"displayName":"Synthetic","mode":"imported","provider":"synthetic","coverageDescription":"Synthetic only"})).unwrap();
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
fn absent(state: &str) -> Value {
    json!({"state":state,"value":null})
}
fn attempt(id: &str) -> Value {
    json!({"attemptId":id,"threadId":"thread-A","turnId":"private-turn","inferenceId":id,"startedAt":NOW,"completedAt":NOW,"status":"completed","request":{"model":reported("model-A"),"reasoningEffort":reported("high"),"serviceTier":reported("priority")},"observed":{"model":reported("observed-model"),"serviceTier":reported("observed-tier")},"responseId":format!("private-response-{id}"),"upstreamRequestId":null,"tokens":{"inputTokens":reported("9007199254740993"),"cachedInputTokens":reported("0"),"cacheWriteInputTokens":absent("omitted"),"outputTokens":reported("2"),"reasoningOutputTokens":absent("invalid"),"totalTokens":absent("not_reported")},"requestProjection":{"messages":[{"role":"user","text":"PRIVATE CONTENT SENTINEL"}],"projection":"visible_text_only"},"responseProjection":null,"evidence":"prepared_request"})
}
fn import(store: &UsageStore, source: &str, id: &str, attempts: Vec<Value>) {
    use sha2::{Digest, Sha256};
    let fingerprint = Sha256::digest(id.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    store.import_trace_bundle(&json!({"sourceId":source,"fingerprint":fingerprint,"adapterVersion":TRACE_ADAPTER_VERSION,"sourceVersion":TRACE_SOURCE_VERSION,"importedAt":"2026-10-05T01:02:03Z","bundleId":id,"attempts":attempts,"warningCodes":["synthetic_warning"]})).unwrap();
}
fn query(extra: Value) -> Value {
    let mut input = json!({"sourceId":"selected"});
    input
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    input
}
fn summary(store: &UsageStore, extra: Value) -> Value {
    store.get_trace_summary(&query(extra)).unwrap()
}
fn ids(list: &Value) -> Vec<String> {
    list["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["attemptId"].as_str().unwrap().to_owned())
        .collect()
}
fn row<'a>(summary: &'a Value, group: &str, key: &str, value: &str) -> &'a Value {
    summary[group]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[key] == value)
        .unwrap()
}
fn assert_total(value: &Value, count: &str, tokens: &str, input: Value, anomalies: &str) {
    assert_eq!(value["count"], count);
    assert_eq!(value["tokenAttemptCount"], tokens);
    assert_eq!(value["totals"]["inputTokens"], input);
    assert_eq!(value["timestampAnomalyCount"], anomalies);
}

#[test]
fn thread_settings_days_preserve_exact_totals_statuses_and_partial_evidence() {
    let store = store();
    let mut first = attempt("a");
    first["startedAt"] = json!("2026-10-02T23:59:59.9Z");
    first["completedAt"] = json!("2026-10-03T00:00:00Z");
    let mut second = attempt("b");
    second["startedAt"] = json!("2026-10-03T00:00:00.1Z");
    // A completed response before its recorded start is an anomaly, not latency.
    second["completedAt"] = json!("2026-10-03T00:00:00.09Z");
    let mut rows = vec![second, first];
    for (id, status, completed) in [
        ("failed", "failed", json!(NOW)),
        ("cancelled", "cancelled", json!(NOW)),
        ("incomplete", "incomplete", Value::Null),
    ] {
        let mut value = attempt(id);
        value["status"] = json!(status);
        value["completedAt"] = completed;
        value["tokens"] = Value::Null;
        rows.push(value);
    }
    let mut other_thread = attempt("different");
    other_thread["threadId"] = json!("thread-B");
    other_thread["startedAt"] = json!("2026-10-05T00:00:00Z");
    other_thread["completedAt"] = other_thread["startedAt"].clone();
    other_thread["request"]["reasoningEffort"] = reported("low");
    rows.push(other_thread);
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    import(&store, "selected", "groups", rows);
    import(&store, "other", "isolated", vec![attempt("isolation")]);
    let result = summary(&store, json!({}));
    assert_eq!(result["source"]["id"], "selected");
    assert_eq!(result["attemptCount"], "6");
    assert_eq!(result["timestampAnomalyCount"], "1");
    assert_eq!(result["totals"]["inputTokens"], "27021597764222979");
    let thread = row(&result, "byThread", "threadId", "thread-A");
    assert_total(thread, "5", "2", json!("18014398509481986"), "1");
    assert_eq!(thread["firstStartedAt"], "2026-10-02T23:59:59.900Z");
    assert_eq!(thread["lastStartedAt"], "2026-10-03T00:00:00.100Z");
    assert_eq!(
        thread["statusCounts"],
        json!({"completed":"2","failed":"1","cancelled":"1","incomplete":"1"})
    );
    assert_eq!(
        thread["tokenCoverage"]["inputTokens"],
        json!({"reportedCount":"2","omittedCount":"0","notReportedCount":"3","invalidCount":"0"})
    );
    assert_eq!(
        thread["tokenCoverage"]["reasoningOutputTokens"]["invalidCount"],
        "2"
    );
    assert!(thread["totals"]["reasoningOutputTokens"].is_null());
    assert_eq!(thread["totals"]["cachedInputTokens"], "0");
    assert_eq!(
        result["byDay"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["date"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["2026-10-02", "2026-10-03", "2026-10-05"]
    );
    assert_total(
        &result["byDay"][1],
        "4",
        "1",
        json!("9007199254740993"),
        "1",
    );
    assert_total(
        &result["byRequestedSettings"][0],
        "5",
        "2",
        json!("18014398509481986"),
        "1",
    );
    assert_eq!(
        result["byRequestedSettings"][0]["request"]["reasoningEffort"],
        reported("high")
    );
    for name in [
        "byStatus",
        "byRequestedModel",
        "byRequestedReasoningEffort",
        "byRequestedServiceTier",
        "byObservedModel",
        "byObservedServiceTier",
    ] {
        assert_eq!(
            result[name][0]["timestampAnomalyCount"],
            if name == "byStatus" { "0" } else { "1" }
        );
    }
    for flag in [
        "groupsTruncated",
        "threadsTruncated",
        "requestedSettingsTruncated",
        "daysTruncated",
    ] {
        assert_eq!(result[flag], false);
    }
    assert_eq!(result["coverage"]["completeness"], "partial");
    assert_eq!(result["coverage"]["missingAttempts"], "unknown");
    assert_eq!(result["coverage"]["dateBasis"], "attempt_start_utc");
    let text = result.to_string();
    for secret in [
        "PRIVATE CONTENT SENTINEL",
        "private-turn",
        "private-response",
        "isolation",
        "attemptId",
        "inferenceId",
        "requestProjection",
    ] {
        assert!(!text.contains(secret), "{secret}");
    }
    assert!(result["warnings"].to_string().contains("not causal"));
    assert!(result["warnings"].to_string().contains("not latency"));
    let list = store
        .get_trace_attempts(&query(json!({"order":"oldest_first"})))
        .unwrap();
    for value in list["attempts"].as_array().unwrap() {
        assert_eq!(value["timestampAnomaly"], value["attemptId"] == "b");
        let detail = store
            .get_local_trace_detail(&query(json!({"attemptId":value["attemptId"]})))
            .unwrap();
        assert_eq!(
            detail["attempt"]["timestampAnomaly"],
            value["timestampAnomaly"]
        );
    }
}

#[test]
fn every_filter_limits_thread_extents_and_all_new_breakdowns() {
    let store = store();
    let selected = attempt("selected");
    let mut values = vec![selected];
    for (id, key, value) in [
        ("model", "model", "model-B"),
        ("effort", "reasoningEffort", "low"),
        ("tier", "serviceTier", "default"),
    ] {
        let mut value_row = attempt(id);
        value_row["request"][key] = reported(value);
        values.push(value_row);
    }
    let mut failed = attempt("failed");
    failed["status"] = json!("failed");
    failed["tokens"] = Value::Null;
    values.push(failed);
    let mut thread = attempt("thread");
    thread["threadId"] = json!("thread-B");
    values.push(thread);
    let mut day = attempt("day");
    day["startedAt"] = json!("2026-10-02T23:59:59.999Z");
    values.push(day);
    import(&store, "selected", "filters", values);
    let result = summary(
        &store,
        json!({"threadId":"thread-A","status":"completed","requestedModel":"model-A","requestedReasoningEffort":"high","requestedServiceTier":"priority","fromDate":"2026-10-03","toDate":"2026-10-03"}),
    );
    assert_eq!(result["attemptCount"], "1");
    for group in ["byThread", "byRequestedSettings", "byDay"] {
        assert_eq!(result[group].as_array().unwrap().len(), 1);
        assert_total(&result[group][0], "1", "1", json!("9007199254740993"), "0");
    }
    assert_eq!(result["byThread"][0]["firstStartedAt"], NOW);
    assert_eq!(result["byThread"][0]["lastStartedAt"], NOW);
    assert_eq!(
        result["byThread"][0]["statusCounts"],
        json!({"completed":"1","failed":"0","cancelled":"0","incomplete":"0"})
    );
    let empty = summary(&store, json!({"threadId":"missing"}));
    for group in ["byThread", "byRequestedSettings", "byDay"] {
        assert_eq!(empty[group], json!([]));
    }
    assert_eq!(empty["timestampAnomalyCount"], "0");
    assert!(empty["totals"]["inputTokens"].is_null());
}

#[test]
fn joint_settings_keep_each_state_value_and_requested_observed_evidence_distinct() {
    let store = store();
    let mut rows = Vec::new();
    let states = ["reported", "omitted", "not_reported", "invalid"];
    for (i, state) in states.iter().enumerate() {
        let mut value = attempt(&format!("state-{i}"));
        for key in ["model", "reasoningEffort", "serviceTier"] {
            value["request"][key] = if *state == "reported" {
                reported("future value")
            } else {
                absent(state)
            };
        }
        rows.push(value);
    }
    let mut model_case = attempt("case");
    model_case["request"]["model"] = reported("Model-A");
    rows.push(model_case);
    let mut effort = attempt("effort");
    effort["request"]["reasoningEffort"] = reported("low");
    rows.push(effort);
    let mut tier = attempt("tier");
    tier["request"]["serviceTier"] = reported("default");
    rows.push(tier);
    rows.push(attempt("baseline"));
    import(&store, "selected", "states", rows);
    let result = summary(&store, json!({}));
    let groups = result["byRequestedSettings"].as_array().unwrap();
    assert_eq!(groups.len(), 8);
    for (index, state) in ["invalid", "not_reported", "omitted"].iter().enumerate() {
        for key in ["model", "reasoningEffort", "serviceTier"] {
            assert_eq!(groups[index]["request"][key], absent(state));
        }
    }
    assert_eq!(groups[3]["request"]["model"], reported("Model-A"));
    assert_eq!(groups[4]["request"]["model"], reported("future value"));
    assert_eq!(groups[5]["request"]["serviceTier"], reported("default"));
    assert_eq!(groups[6]["request"]["serviceTier"], reported("priority"));
    assert_eq!(groups[7]["request"]["reasoningEffort"], reported("low"));
    assert!(
        !groups
            .iter()
            .any(|row| row["request"]["model"] == reported("observed-model"))
    );
    let filtered = summary(&store, json!({"requestedModel":"model-A"}));
    assert_eq!(filtered["byRequestedSettings"].as_array().unwrap().len(), 3);
}

fn day(index: i64) -> String {
    (NaiveDate::from_ymd_opt(2024, 1, 2).unwrap() + Duration::days(index))
        .format("%Y-%m-%d")
        .to_string()
}
fn grouped_attempt(id: &str, index: i64, dimension: &str) -> Value {
    let mut value = attempt(id);
    match dimension {
        "thread" => value["threadId"] = json!(format!("thread-{index:04}")),
        "settings" => value["request"]["model"] = reported(&format!("model-{index:04}")),
        "day" => {
            value["startedAt"] = json!(format!("{}T00:00:00Z", day(index)));
            value["completedAt"] = value["startedAt"].clone();
        }
        _ => panic!("invalid test dimension"),
    }
    value["tokens"]["inputTokens"] = reported("1");
    value
}
#[test]
fn each_group_cap_keeps_smallest_keys_full_counts_and_independent_truncation() {
    for (dimension, group, flag, key) in [
        ("thread", "byThread", "threadsTruncated", "threadId"),
        (
            "settings",
            "byRequestedSettings",
            "requestedSettingsTruncated",
            "request",
        ),
        ("day", "byDay", "daysTruncated", "date"),
    ] {
        let store = store();
        // Reversed keys make replacements unavoidable, independently of planner scan order.
        let first = (1..=500)
            .rev()
            .enumerate()
            .map(|(id, index)| grouped_attempt(&format!("a-{id:04}"), index, dimension))
            .collect();
        import(&store, "selected", "first", first);
        assert_eq!(summary(&store, json!({}))[flag], false);
        let rows = [0, 501, 1, 500, 0]
            .into_iter()
            .enumerate()
            .map(|(id, index)| {
                let mut value = grouped_attempt(&format!("z-{id:04}"), index, dimension);
                value["tokens"]["inputTokens"] = reported("9007199254740993");
                value
            })
            .collect();
        import(&store, "selected", "later", rows);
        let result = summary(&store, json!({}));
        let groups = result[group].as_array().unwrap();
        assert_eq!(groups.len(), 500);
        assert_eq!(result[flag], true);
        for other in [
            "threadsTruncated",
            "requestedSettingsTruncated",
            "daysTruncated",
        ] {
            if other != flag {
                assert_eq!(result[other], false);
            }
        }
        assert_eq!(result["attemptCount"], "505");
        assert_eq!(result["totals"]["inputTokens"], "45035996273705465");
        assert_total(&groups[0], "2", "2", json!("18014398509481986"), "0");
        assert_total(&groups[1], "2", "2", json!("9007199254740994"), "0");
        let expected_first = match dimension {
            "thread" => json!("thread-0000"),
            "settings" => grouped_attempt("x", 0, dimension)["request"].clone(),
            _ => json!(day(0)),
        };
        let expected_last = match dimension {
            "thread" => json!("thread-0499"),
            "settings" => grouped_attempt("x", 499, dimension)["request"].clone(),
            _ => json!(day(499)),
        };
        assert_eq!(groups[0][key], expected_first);
        assert_eq!(groups[499][key], expected_last);
    }
}

#[test]
fn chronological_pagination_orders_timestamp_then_id_and_binds_direction() {
    let store = store();
    let mut rows = vec![attempt("a"), attempt("b"), attempt("c")];
    let mut earlier = attempt("z-early");
    earlier["startedAt"] = json!("2026-10-02T23:59:59.999Z");
    rows.push(earlier);
    let mut later = attempt("a-late");
    later["startedAt"] = json!("2026-10-03T00:00:00.001Z");
    later["completedAt"] = later["startedAt"].clone();
    rows.push(later);
    let mut excluded = attempt("excluded");
    excluded["threadId"] = json!("thread-B");
    rows.push(excluded);
    import(&store, "selected", "pages", rows);
    for (order, expected) in [
        ("oldest_first", vec!["z-early", "a", "b", "c", "a-late"]),
        ("newest_first", vec!["a-late", "c", "b", "a", "z-early"]),
    ] {
        let mut input = query(json!({"threadId":"thread-A","order":order,"limit":2}));
        let first = store.get_trace_attempts(&input).unwrap();
        assert_eq!(first["order"], order);
        let cursor = first["nextCursor"].clone();
        let mut found = ids(&first);
        input["cursor"] = cursor.clone();
        loop {
            let next = store.get_trace_attempts(&input).unwrap();
            found.extend(ids(&next));
            if next["nextCursor"].is_null() {
                break;
            }
            input["cursor"] = next["nextCursor"].clone();
            input["limit"] = json!(1);
        }
        assert_eq!(found, expected);
        input["cursor"] = cursor.clone();
        input["order"] = json!(if order == "oldest_first" {
            "newest_first"
        } else {
            "oldest_first"
        });
        assert_eq!(
            store.get_trace_attempts(&input).unwrap_err().code(),
            "invalid_input"
        );
        let decoded: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(cursor.as_str().unwrap()).unwrap())
                .unwrap();
        assert_eq!(decoded["order"], order);
        for changed in [Value::Null, json!("unknown"), json!(true)] {
            let mut value = decoded.clone();
            value["order"] = changed;
            input["order"] = json!(order);
            input["cursor"] = json!(URL_SAFE_NO_PAD.encode(value.to_string()));
            assert!(store.get_trace_attempts(&input).is_err());
        }
        let mut legacy = decoded;
        legacy.as_object_mut().unwrap().remove("order");
        input["cursor"] = json!(URL_SAFE_NO_PAD.encode(legacy.to_string()));
        assert!(store.get_trace_attempts(&input).is_err());
    }
    let default = store
        .get_trace_attempts(&query(json!({"limit":2})))
        .unwrap();
    let explicit = store
        .get_trace_attempts(&query(json!({"limit":2,"order":"newest_first"})))
        .unwrap();
    assert_eq!(default, explicit);
    let next = store
        .get_trace_attempts(&query(json!({"cursor":explicit["nextCursor"]})))
        .unwrap();
    assert!(!ids(&next).is_empty());
}

#[test]
fn invalid_orders_are_rejected_on_empty_legacy_schema_and_summary_remains_order_free() {
    let store = store();
    for value in [
        Value::Null,
        json!(true),
        json!(1),
        json!([]),
        json!({}),
        json!(""),
        json!("ASC"),
        json!("oldest"),
        json!("oldest_first "),
        json!("newest_first; DROP TABLE sources"),
    ] {
        assert_eq!(
            store
                .get_trace_attempts(&query(json!({"order":value})))
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }
    for order in ["oldest_first", "newest_first"] {
        let list = store
            .get_trace_attempts(&query(json!({"order":order})))
            .unwrap();
        assert_eq!(list["order"], order);
        assert_eq!(list["attempts"], json!([]));
        assert_eq!(
            store
                .get_trace_summary(&query(json!({"order":order})))
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }
    assert_eq!(store.get_status().unwrap()["schemaVersion"], 2);
}

#[test]
fn all_supported_schemas_are_read_only_and_missing_history_is_not_zero_filled() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("insights.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store, "selected");
    for schema in [2, 3, 4] {
        if schema == 3 {
            import_incremental_rollout(&store,b"",&json!({"sourceId":"selected","streamId":"empty","observedAt":NOW,"sourceVersion":ROLLOUT_SOURCE_VERSION})).unwrap();
        }
        if schema == 4 {
            import(&store, "selected", "read-only", vec![attempt("a")]);
        }
        assert_eq!(store.get_status().unwrap()["schemaVersion"], schema);
        let before = std::fs::read(&path).unwrap();
        let readonly = UsageStore::open_read_only(&path).unwrap();
        let result = summary(
            &readonly,
            json!({"fromDate":"2026-10-01","toDate":"2026-10-05"}),
        );
        for group in ["byThread", "byRequestedSettings", "byDay"] {
            assert_eq!(
                result[group].as_array().unwrap().len(),
                usize::from(schema == 4)
            );
        }
        if schema == 4 {
            assert_eq!(result["byDay"][0]["date"], "2026-10-03");
        }
        for order in ["oldest_first", "newest_first"] {
            assert_eq!(
                readonly
                    .get_trace_attempts(&query(json!({"order":order})))
                    .unwrap()["order"],
                order
            );
        }
        assert_eq!(readonly.get_status().unwrap()["schemaVersion"], schema);
        drop(readonly);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn incomplete_attempts_never_infer_completion_or_anomaly_and_start_is_mandatory() {
    let store = store();
    let mut value = attempt("incomplete");
    value["status"] = json!("incomplete");
    value["completedAt"] = Value::Null;
    value["tokens"] = Value::Null;
    import(&store, "selected", "incomplete", vec![value.clone()]);
    let result = summary(&store, json!({}));
    assert_total(&result["byThread"][0], "1", "0", Value::Null, "0");
    assert_eq!(result["byDay"][0]["date"], "2026-10-03");
    let list = store.get_trace_attempts(&query(json!({}))).unwrap();
    assert_eq!(list["attempts"][0]["timestampAnomaly"], false);
    for start in [
        Value::Null,
        json!("2026-10-02T23:00:00-01:00"),
        json!("bad timestamp"),
    ] {
        value["startedAt"] = start;
        let input = json!({"sourceId":"selected","fingerprint":"a".repeat(64),"adapterVersion":TRACE_ADAPTER_VERSION,"sourceVersion":TRACE_SOURCE_VERSION,"importedAt":NOW,"bundleId":"invalid","attempts":[value],"warningCodes":[]});
        assert_eq!(
            store.import_trace_bundle(&input).unwrap_err().code(),
            "invalid_input"
        );
    }
}
