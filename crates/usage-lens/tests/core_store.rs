use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use usage_lens::core::UsageStore;
use usage_lens::core::response_tokens::TOKEN_SCHEMA;
const NOW: &str = "2026-10-02T12:00:00Z";
fn at_ms(v: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(v)
        .unwrap()
        .timestamp_millis()
}
fn source(store: &UsageStore, id: &str, mode: &str) {
    store.create_source(&json!({"id":id,"displayName":"Synthetic source","mode":mode,"provider":"synthetic","coverageDescription":"Synthetic tests only"})).unwrap();
}
fn memory() -> UsageStore {
    let db = UsageStore::with_clock_ms(":memory:", at_ms(NOW)).unwrap();
    source(&db, "a", "demo");
    db
}
fn event(id: &str) -> Value {
    json!({"sourceId":"a","eventId":id,"eventType":"user_prompt","evidenceType":"explicit_user_message","observedAt":NOW,"collectorVersion":"test-1"})
}
fn obs(raw: Value) -> Value {
    json!({"sourceId":"a","method":"account/usage/read","observedAt":NOW,"adapterVersion":"test-1","schemaBaseline":"synthetic-1","raw":raw})
}
fn query() -> Value {
    json!({"sourceId":"a"})
}
fn daily() -> Value {
    json!({"sourceId":"a","fromDate":"2026-10-01","toDate":"2026-10-02"})
}
fn failure(method: &str, code: &str, at: &str) -> Value {
    json!({"sourceId":"a","method":method,"attemptedAt":at,"errorCode":code})
}
fn strip(mut v: Value) -> Value {
    v.as_object_mut().unwrap().remove("sourceId");
    v
}
fn usage(n: &str) -> Value {
    json!({"input_tokens":n,"cached_input_tokens":"2","output_tokens":"4","reasoning_output_tokens":"1","total_tokens":"11"})
}
fn token(id: &str) -> Value {
    json!({"sourceId":"imported","importedAt":NOW,"collectorVersion":"test-1","raw":{"thread_id":"thread-1","turn_id":"turn-1","session_id":"session-1","root_turn_id":"root-1","response_id":id,"usage":usage("7"),"turn_token_usage":usage("777"),"thread_token_usage":usage("7777")}})
}
fn metadata() -> Value {
    json!({"fingerprint":"a".repeat(64),"adapterVersion":"test-1","sourceVersion":TOKEN_SCHEMA,"importedAt":NOW,"warningCodes":["synthetic_partial"]})
}
fn err<T>(result: Result<T, usage_lens::core::CoreError>, code: &str) {
    match result {
        Ok(_) => panic!("expected {code}"),
        Err(e) => assert_eq!(e.code(), code),
    }
}

#[test]
fn sources_settings_closed_and_limits() {
    let mut db = memory();
    assert_eq!(
        db.get_settings().unwrap(),
        json!({"capturePaused":false,"contentCaptureEnabled":false,"retentionDays":30})
    );
    source(&db, "b", "imported");
    source(&db, "c", "live");
    assert_eq!(db.list_sources().unwrap().as_array().unwrap().len(), 3);
    let duplicate = json!({"id":"a","displayName":"a","mode":"demo","provider":"synthetic","coverageDescription":"a"});
    err(db.create_source(&duplicate), "source_exists");
    err(
        db.get_overview(&json!({"sourceId":"missing"})),
        "source_not_found",
    );
    for input in [
        json!({"capturePaused":null}),
        json!({"contentCaptureEnabled":1}),
        json!({"retentionDays":0}),
        json!({"retentionDays":3651}),
        json!({"unknown":true}),
    ] {
        err(db.update_settings(&input), "invalid_input");
    }
    db.set_settings(&json!({"contentCaptureEnabled":true,"retentionDays":1}))
        .unwrap();
    let mut e = event("retained");
    e["content"] = json!({"body":"retained"});
    db.ingest_event(&e).unwrap();
    db.update_settings(&json!({"capturePaused":true,"contentCaptureEnabled":false}))
        .unwrap();
    err(db.ingest_event(&event("next")), "capture_paused");
    err(db.ingest_observation(&obs(json!({}))), "capture_paused");
    err(
        db.record_failure(&failure("account/read", "rpc_error", NOW)),
        "capture_paused",
    );
    assert_eq!(
        db.get_local_event_detail(&json!({"sourceId":"a","eventId":"retained"}))
            .unwrap()["content"]["body"],
        "retained"
    );
    db.update_settings(&json!({"capturePaused":false})).unwrap();
    db.set_clock_ms(at_ms("2026-10-03T12:00:00Z")).unwrap();
    assert_eq!(db.get_settings().unwrap()["retentionDays"], 1);
    db.close().unwrap();
    db.close().unwrap();
    err(db.get_status(), "store_closed");
    err(db.list_sources(), "store_closed");
    err(UsageStore::open(""), "invalid_input");
    err(UsageStore::open("\0bad"), "invalid_input");
}

#[test]
fn exact_latest_observations_and_daily_unknowns() {
    let db = memory();
    assert_eq!(
        db.get_daily_usage(&daily()).unwrap()["sumOfReturnedBuckets"]["status"],
        "omitted"
    );
    let original = obs(
        json!({"summary":{"lifetimeTokens":"10000000000000000000000000001"},"dailyUsageBuckets":[{"startDate":"2026-10-01","tokens":"9007199254740993"},{"startDate":"2026-10-02","tokens":"9007199254740995"}]}),
    );
    assert_eq!(db.ingest_observation(&original).unwrap()["inserted"], true);
    assert_eq!(db.ingest_observation(&original).unwrap()["inserted"], false);
    let mut old = obs(
        json!({"summary":{"lifetimeTokens":1},"dailyUsageBuckets":[{"startDate":"2026-10-01","tokens":1}]}),
    );
    old["observedAt"] = json!("2026-01-01T00:00:00Z");
    db.ingest_observation(&old).unwrap();
    let result = db.get_daily_usage(&daily()).unwrap();
    assert_eq!(result["buckets"].as_array().unwrap().len(), 2);
    assert_eq!(result["sumOfReturnedBuckets"]["value"], "18014398509481988");
    assert_eq!(result["sourceTimezone"], Value::Null);
    assert_eq!(
        db.get_overview(&query()).unwrap()["usage"]["data"]["summary"]["lifetimeTokens"]["value"],
        "10000000000000000000000000001"
    );
    for (i, raw, status) in [
        (1, json!({"dailyUsageBuckets":null}), "not_reported"),
        (2, json!({"dailyUsageBuckets":[]}), "reported"),
        (
            3,
            json!({"dailyUsageBuckets":[{"startDate":"2026-10-01","tokens":null},{"startDate":"2026-10-02","tokens":0},{"startDate":"2026-10-03","tokens":10},{"startDate":"invalid","tokens":999}]}),
            "not_reported",
        ),
        (4, json!({"dailyUsageBuckets":"wrong"}), "invalid"),
        (5, json!({}), "omitted"),
    ] {
        let mut value = obs(raw);
        value["observedAt"] = json!(format!("2026-10-02T12:00:0{i}Z"));
        db.ingest_observation(&value).unwrap();
        let result = db.get_daily_usage(&daily()).unwrap();
        assert_eq!(result["sumOfReturnedBuckets"]["status"], status);
        if i == 2 {
            assert_eq!(result["sumOfReturnedBuckets"]["value"], "0");
        }
        if i == 3 {
            assert_eq!(result["unknownTokenBucketCount"], "1");
        }
    }
}
#[test]
fn failed_attempts_freshness_capabilities_and_future_clock() {
    let db = memory();
    db.record_failure(&failure("account/read", "unsupported_method", NOW))
        .unwrap();
    db.record_failure(&failure("account/read", "unsupported_method", NOW))
        .unwrap();
    assert_eq!(
        db.get_overview(&query()).unwrap()["account"]["lastFailure"]["errorCode"],
        "unsupported_method"
    );
    let mut input = obs(json!({"summary":{"lifetimeTokens":3}}));
    input["observedAt"] = json!("2026-10-02T10:00:00Z");
    input["sourceAsOf"] = json!("2026-10-01T00:00:00Z");
    db.ingest_observation(&input).unwrap();
    db.record_failure(&failure(
        "account/usage/read",
        "request_timeout",
        "2026-10-02T11:00:00Z",
    ))
    .unwrap();
    let result = db
        .get_overview(&json!({"sourceId":"a","maxAgeMs":1000}))
        .unwrap();
    assert_eq!(result["usage"]["freshness"]["ageMs"], 7200000);
    assert_eq!(result["usage"]["freshness"]["stale"], true);
    assert_eq!(result["usage"]["freshness"]["sourceAsOfStatus"], "reported");
    assert_eq!(
        result["usage"]["lastFailure"]["errorCode"],
        "request_timeout"
    );
    let capabilities = db.get_status().unwrap()["capabilities"].clone();
    assert!(
        capabilities
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["method"] == "account/read" && v["state"] == "unsupported")
    );
    assert!(
        capabilities
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["method"] == "account/usage/read" && v["state"] == "partial")
    );
    input["observedAt"] = json!("2026-10-03T00:00:00Z");
    db.ingest_observation(&input).unwrap();
    let result = db.get_overview(&query()).unwrap();
    assert_eq!(result["usage"]["freshness"]["ageMs"], 0);
    assert!(result["usage"]["lastFailure"].is_null());
    assert!(result["usage"]["warnings"].to_string().contains("future"));
    let mut account = obs(json!({"account":{"type":"apiKey"},"requiresOpenaiAuth":false}));
    account["method"] = json!("account/read");
    account["observedAt"] = json!("2026-10-03T00:00:00Z");
    db.ingest_observation(&account).unwrap();
    assert_eq!(
        db.get_overview(&query()).unwrap()["account"]["data"]["type"]["value"],
        "apiKey"
    );
}

#[test]
fn quota_independent_windows_and_stable_history() {
    let db = memory();
    for i in 0..3 {
        let mut input = obs(
            json!({"rateLimitsByLimitId":{"a":{"primary":{"usedPercent":i*50}},"b":{"primary":{"usedPercent":20}}},"rateLimits":{"primary":{"usedPercent":100}}}),
        );
        input["method"] = json!("account/rateLimits/read");
        input["observedAt"] = json!(format!("2026-10-02T1{i}:00:00Z"));
        db.ingest_observation(&input).unwrap();
    }
    assert_eq!(
        db.get_quota(&query()).unwrap()["observation"]["data"]["buckets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let page = db
        .get_quota_history(&json!({"sourceId":"a","limit":2}))
        .unwrap();
    assert_eq!(page["observations"].as_array().unwrap().len(), 2);
    let next = db
        .get_quota_history(&json!({"sourceId":"a","limit":2,"cursor":page["nextCursor"]}))
        .unwrap();
    assert_eq!(next["observations"].as_array().unwrap().len(), 1);
    assert!(next["nextCursor"].is_null());
    assert_eq!(next["coverage"]["betweenSamples"], "unknown");
    assert_eq!(
        db.get_quota_history(&query()).unwrap()["observations"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn metadata_events_skills_filters_and_counts() {
    let db = memory();
    source(&db, "other", "demo");
    let mut events = vec![];
    for (id, kind, evidence, name, ek) in [
        ("prompt", "user_prompt", "explicit_user_message", None, None),
        (
            "assistant",
            "assistant_visible_message",
            "explicit_assistant_visible_message",
            None,
            None,
        ),
        (
            "tool",
            "tool_call",
            "explicit_tool_call",
            Some("shell"),
            Some("resource_read"),
        ),
        (
            "tool2",
            "tool_call",
            "explicit_tool_call",
            Some("shell"),
            Some("continuation_read"),
        ),
        (
            "requested",
            "skill_requested",
            "explicit_skill_input",
            Some("imagegen"),
            None,
        ),
        (
            "loaded",
            "skill_loaded",
            "successful_skill_read",
            Some("imagegen"),
            Some("main_read"),
        ),
        (
            "injected",
            "skill_loaded",
            "typed_skill_injection",
            Some("imagegen"),
            Some("instruction_injection"),
        ),
        (
            "invoked",
            "skill_invoked",
            "explicit_execution_record",
            Some("imagegen"),
            None,
        ),
    ] {
        let mut e = event(id);
        e["eventType"] = json!(kind);
        e["evidenceType"] = json!(evidence);
        if let Some(name) = name {
            e[if kind == "tool_call" {
                "toolName"
            } else {
                "skillName"
            }] = json!(name);
        }
        if let Some(ek) = ek {
            e["skillEvidenceKind"] = json!(ek);
        }
        if kind == "tool_call" {
            e["model"] = json!("model-a");
            e["occurredAt"] = json!("2026-10-01T10:00:00Z");
        }
        events.push(e);
    }
    db.ingest_events(&json!(events)).unwrap();
    let overview = db.get_overview(&query()).unwrap();
    assert_eq!(overview["events"]["total"], "8");
    assert_eq!(overview["events"]["unknownModelCount"], "6");
    assert_eq!(
        overview["events"]["tools"],
        json!([{"name":"shell","count":"2"}])
    );
    assert_eq!(overview["events"]["skills"].as_array().unwrap().len(), 4);
    assert_eq!(
        db.get_tool_usage(
            &json!({"sourceId":"a","fromDate":"2026-10-01","toDate":"2026-10-01","model":"model-a"})
        )
        .unwrap()["tools"][0]["count"],
        "2"
    );
    assert_eq!(
        db.get_skill_evidence(&json!({"sourceId":"a","kind":"loaded"}))
            .unwrap()["events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        db.get_skill_evidence(&query()).unwrap()["status"],
        "available"
    );
    assert_eq!(
        db.get_skill_evidence(&json!({"sourceId":"other"})).unwrap()["status"],
        "unavailable"
    );
    assert_eq!(db.get_events(&json!({"sourceId":"a","eventType":"tool_call","model":"model-a","fromDate":"2026-10-01","toDate":"2026-10-01"})).unwrap()["events"].as_array().unwrap().len(),2);
    assert!(
        db.get_status().unwrap()["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["sourceId"] == "a" && v["method"] == "skills" && v["state"] == "partial")
    );
}

#[test]
fn cursor_validation_pagination_and_input_allowlist() {
    let db = memory();
    for id in ["a", "b", "c"] {
        db.ingest_event(&event(id)).unwrap();
    }
    let page = db.get_events(&json!({"sourceId":"a","limit":2})).unwrap();
    assert_eq!(page["events"][0]["eventId"], "c");
    let next = db
        .get_events(&json!({"sourceId":"a","limit":2,"cursor":page["nextCursor"]}))
        .unwrap();
    assert_eq!(next["events"][0]["eventId"], "a");
    assert!(next["nextCursor"].is_null());
    for input in [
        json!({"sourceId":"a","cursor":null}),
        json!({"sourceId":"a","cursor":"%%%"}),
        json!({"sourceId":"a","cursor":"eA"}),
        json!({"sourceId":"a","limit":0}),
        json!({"sourceId":"a","limit":501}),
        json!({"sourceId":"a","fromDate":"2026-10-02"}),
        json!({"sourceId":"a","model":null}),
        json!({"sourceId":"a","path":"private"}),
    ] {
        err(db.get_events(&input), "invalid_input");
    }
    for value in [
        json!({"sourceId":"other","kind":"events","at":NOW,"key":"a"}),
        json!({"sourceId":"a","kind":"quota","at":NOW,"key":"a"}),
        json!({"sourceId":"a","kind":"events","at":"bad","key":"a"}),
        json!({"sourceId":"a","kind":"events","at":NOW,"key":"a","extra":true}),
    ] {
        let cursor = URL_SAFE_NO_PAD.encode(value.to_string());
        err(
            db.get_events(&json!({"sourceId":"a","cursor":cursor})),
            "invalid_input",
        );
    }
    let bad = URL_SAFE_NO_PAD.encode(
        json!({"sourceId":"a","kind":"quota","at":NOW,"key":"9007199254740992"}).to_string(),
    );
    err(
        db.get_quota_history(&json!({"sourceId":"a","cursor":bad})),
        "invalid_input",
    );
    err(
        db.get_overview(&json!({"sourceId":"a","maxAgeMs":-1})),
        "invalid_input",
    );
    err(
        db.get_skill_evidence(&json!({"sourceId":"a","kind":"invented"})),
        "invalid_input",
    );
    err(
        db.get_tool_usage(&json!({"sourceId":"a","limit":1})),
        "invalid_input",
    );
}

#[test]
fn content_privacy_redaction_search_and_no_duplicate_upgrade() {
    let db = memory();
    let mut disabled = event("disabled");
    disabled["sourceEventId"] = json!("external-1");
    disabled["content"] = json!({"body":"never-retained private-canary"});
    let result = db.ingest_event(&disabled).unwrap();
    assert_eq!(result["contentRetained"], false);
    assert!(result["warnings"].to_string().contains("disabled"));
    assert!(
        db.get_local_event_detail(&json!({"sourceId":"a","eventId":"disabled"}))
            .unwrap()["content"]
            .is_null()
    );
    db.update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    assert_eq!(
        db.ingest_event(&disabled).unwrap()["contentRetained"],
        false
    );
    disabled["eventId"] = json!("different");
    assert_eq!(db.ingest_event(&disabled).unwrap()["inserted"], false);
    let mut retained = event("retained");
    retained["content"] = json!({"body":"SYNTHETIC_LOCAL_CANARY password=hidden","toolArguments":{"q":"SYNTHETIC_LOCAL_CANARY","system_prompt":"MUST_NOT_REMAIN","hidden_reasoning":"MUST_NOT_REMAIN","nested":[{"role":"developer","content":"MUST_NOT_REMAIN"},{"authorization":"hidden"}]},"toolResult":"{\"password\":\"SYNTHETIC_PASSWORD_CANARY\"}","files":[{"name":"fixture.txt","content":"SYNTHETIC_LOCAL_CANARY"}]});
    db.ingest_event(&retained).unwrap();
    assert_eq!(
        db.ingest_event(&event("retained")).unwrap()["contentRetained"],
        true
    );
    let detail = db
        .get_local_event_detail(&json!({"sourceId":"a","eventId":"retained"}))
        .unwrap();
    assert!(detail.to_string().contains("SYNTHETIC_LOCAL_CANARY"));
    for secret in ["MUST_NOT_REMAIN", "hidden", "SYNTHETIC_PASSWORD_CANARY"] {
        assert!(!detail.to_string().contains(secret), "{secret}");
    }
    assert!(detail["warnings"].to_string().contains("best-effort"));
    let results = vec![
        db.get_status().unwrap(),
        db.get_overview(&query()).unwrap(),
        db.get_daily_usage(&daily()).unwrap(),
        db.get_quota(&query()).unwrap(),
        db.get_quota_history(&query()).unwrap(),
        db.get_events(&query()).unwrap(),
        db.get_skill_evidence(&query()).unwrap(),
        db.get_tool_usage(&query()).unwrap(),
        db.get_response_token_records(&query()).unwrap(),
        db.get_response_token_usage(&query()).unwrap(),
    ];
    let serialized = json!(results).to_string();
    assert!(!serialized.contains("SYNTHETIC_LOCAL_CANARY"));
    assert!(!serialized.contains("fixture.txt"));
    let search = db
        .search_local_details(&json!({"sourceId":"a","query":"SYNTHETIC_LOCAL_CANARY"}))
        .unwrap();
    assert_eq!(search["events"].as_array().unwrap().len(), 1);
    assert!(!search.to_string().contains("SYNTHETIC_LOCAL_CANARY"));
    assert_eq!(
        db.search_local_details(&json!({"sourceId":"a","query":"%","limit":1}))
            .unwrap()["events"],
        json!([])
    );
    err(
        db.get_local_event_detail(&json!({"sourceId":"a","eventId":"missing"})),
        "event_not_found",
    );
    let mut invalid = event("bad");
    invalid["rawBody"] = json!("SYNTHETIC_LOCAL_CANARY");
    let e = db.ingest_event(&invalid).unwrap_err();
    assert!(!e.to_string().contains("CANARY"));
}

#[test]
fn event_validation_and_batch_atomicity() {
    let db = memory();
    let bad_fields = [
        ("eventType", json!("system")),
        ("eventType", json!("developer")),
        ("eventType", json!("reasoning")),
        ("eventType", json!("hidden_reasoning")),
        ("evidenceType", json!("explicit_tool_call")),
        ("toolName", json!("shell")),
        ("skillName", json!("skill")),
        ("skillVersion", json!("1")),
        ("durationMs", json!(1.5)),
        ("status", Value::Null),
        ("content", json!({"body":1})),
        ("skillEvidenceKind", json!("main_read")),
        ("skillEvidenceKind", json!("instruction_injection")),
        ("skillEvidenceKind", json!("resource_read")),
        ("skillEvidenceKind", json!("continuation_read")),
    ];
    for (key, v) in bad_fields {
        let mut e = event("invalid");
        e[key] = v;
        err(db.ingest_event(&e), "invalid_input");
    }
    let mut missing = event("missing-tool");
    missing["eventType"] = json!("tool_call");
    missing["evidenceType"] = json!("explicit_tool_call");
    err(db.ingest_event(&missing), "invalid_input");
    let mut bad = event("bad");
    bad["evidenceType"] = json!("wrong");
    err(
        db.ingest_events(&json!([event("valid"), bad])),
        "invalid_input",
    );
    assert_eq!(db.get_status().unwrap()["eventCount"], "0");
    err(db.ingest_events(&json!({})), "invalid_input");
    err(
        db.ingest_events(&json!(vec![event("too-many"); 1001])),
        "invalid_input",
    );
    let mut e = event("optional");
    for (key, value) in [
        ("durationMs", json!("100000000000000000000")),
        ("sessionId", json!("s1")),
        ("turnId", json!("t1")),
        ("status", json!("error")),
        ("reasoningEffort", json!("high")),
        ("retryOfEventId", json!("prior")),
        ("model", json!("model-a")),
        ("occurredAt", json!("2026-10-01T05:00:00Z")),
    ] {
        e[key] = value;
    }
    db.ingest_event(&e).unwrap();
    let v = db.get_events(&query()).unwrap()["events"][0].clone();
    assert_eq!(v["durationMs"], "100000000000000000000");
    assert_eq!(v["occurredAt"], "2026-10-01T05:00:00.000Z");
    assert_eq!(v["confidence"], "direct");
    let mut big = event("huge");
    big["content"] = json!({"body":"x".repeat(524289)});
    err(db.ingest_event(&big), "input_too_large");
}

#[test]
fn import_atomicity_dedupe_and_paused_state() {
    let db = memory();
    let input = json!({"sourceId":"a","observations":[strip(obs(json!({"summary":{"lifetimeTokens":10}})))],"events":[strip(event("e1"))]});
    assert_eq!(
        db.import_data(&input).unwrap(),
        json!({"observationsInserted":"1","eventsInserted":"1"})
    );
    assert_eq!(
        db.import_data(&input).unwrap(),
        json!({"observationsInserted":"0","eventsInserted":"0"})
    );
    let mut bad = input.clone();
    bad["observations"][0]["observedAt"] = json!("2026-10-03T00:00:00Z");
    bad["events"][0]["evidenceType"] = json!("wrong");
    err(db.import_data(&bad), "invalid_input");
    assert_eq!(db.get_status().unwrap()["observationCount"], "1");
    assert_eq!(db.get_status().unwrap()["eventCount"], "1");
    let mut invalid = input.clone();
    invalid["events"][0]["sourceId"] = json!("a");
    err(db.import_data(&invalid), "invalid_input");
    invalid = input.clone();
    invalid["observations"][0]["sourceId"] = json!("a");
    err(db.import_data(&invalid), "invalid_input");
    db.update_settings(&json!({"capturePaused":true})).unwrap();
    err(db.import_data(&input), "capture_paused");
}

#[test]
fn responses_exact_sum_dedupe_identity_and_import_ledger() {
    let db = memory();
    source(&db, "imported", "imported");
    let q = json!({"sourceId":"imported"});
    assert_eq!(
        db.get_response_token_usage(&q).unwrap()["totals"]["totalTokens"],
        "0"
    );
    let first = token("r1");
    assert_eq!(db.ingest_response_token(&first).unwrap()["inserted"], true);
    let mut duplicate = first.clone();
    duplicate["importedAt"] = json!("2026-10-03T00:00:00Z");
    duplicate["collectorVersion"] = json!("test-2");
    assert_eq!(
        db.ingest_response_token(&duplicate).unwrap()["inserted"],
        false
    );
    let mut conflict = first.clone();
    conflict["raw"]["usage"]["input_tokens"] = json!("8");
    err(
        db.ingest_response_token(&conflict),
        "response_token_conflict",
    );
    let mut other = first.clone();
    other["raw"]["session_id"] = json!("session-2");
    other["model"] = json!("model-a");
    other["occurredAt"] = json!("2026-10-01T00:00:00Z");
    assert_eq!(db.ingest_response_token(&other).unwrap()["inserted"], true);
    let mut huge = token("r-big");
    huge["raw"]["usage"] = json!({"input_tokens":"9223372036854775807","cached_input_tokens":"2","cache_write_input_tokens":"3","output_tokens":"4","reasoning_output_tokens":"1","total_tokens":"9223372036854775807"});
    huge["model"] = json!("model-a");
    db.ingest_response_token(&huge).unwrap();
    let totals = db.get_response_token_usage(&q).unwrap();
    assert_eq!(totals["responseCount"], "3");
    assert_eq!(totals["totals"]["inputTokens"], "9223372036854775821");
    assert_eq!(totals["totals"]["cachedInputTokens"], "6");
    assert_eq!(totals["totals"]["totalTokens"], "9223372036854775829");
    assert_eq!(totals["byModel"][0]["model"], "model-a");
    assert_eq!(totals["coverage"]["cumulativeSnapshots"], "excluded");
    assert_eq!(db.get_response_token_usage(&json!({"sourceId":"imported","fromDate":"2026-10-01","toDate":"2026-10-01","model":"model-a"})).unwrap()["responseCount"],"1");
    let page = db
        .get_response_token_records(&json!({"sourceId":"imported","limit":2}))
        .unwrap();
    assert_eq!(page["records"].as_array().unwrap().len(), 2);
    let next = db
        .get_response_token_records(
            &json!({"sourceId":"imported","limit":2,"cursor":page["nextCursor"]}),
        )
        .unwrap();
    assert_eq!(next["records"].as_array().unwrap().len(), 1);
    assert!(next["nextCursor"].is_null());
    let mut wrong = first.clone();
    wrong["sourceId"] = json!("a");
    err(db.ingest_response_token(&wrong), "imported_source_required");
    let mut e = event("import-event");
    e["sourceId"] = json!("imported");
    let batch = json!({"sourceId":"imported","observations":[],"events":[strip(e)],"responseTokens":[strip(token("r-import"))],"importMetadata":metadata()});
    assert_eq!(
        db.import_data(&batch).unwrap()["responseTokensInserted"],
        "1"
    );
    assert_eq!(
        db.import_data(&batch).unwrap()["importAlreadyPresent"],
        true
    );
    for result in [
        db.get_status().unwrap(),
        db.get_overview(&q).unwrap(),
        db.get_events(&q).unwrap(),
        db.get_skill_evidence(&q).unwrap(),
        db.get_tool_usage(&q).unwrap(),
        db.get_response_token_usage(&q).unwrap(),
        db.get_response_token_records(&q).unwrap(),
    ] {
        assert!(result.to_string().contains("synthetic_partial"));
    }
    let mut failed = batch.clone();
    failed["importMetadata"]["fingerprint"] = json!("b".repeat(64));
    failed["events"][0]["eventId"] = json!("must-roll-back");
    failed["responseTokens"][0] = strip(conflict);
    err(db.import_data(&failed), "response_token_conflict");
    assert!(
        !db.get_events(&q)
            .unwrap()
            .to_string()
            .contains("must-roll-back")
    );
    failed["responseTokens"][0] = strip(token("recovered"));
    assert_eq!(
        db.import_data(&failed).unwrap()["importAlreadyPresent"],
        false
    );
    let mut rollout = metadata();
    rollout["sourceId"] = json!("imported");
    rollout["fingerprint"] = json!("c".repeat(64));
    let mut re = event("rollout");
    re["sourceId"] = json!("imported");
    rollout["events"] = json!([re]);
    rollout["responseTokens"] = json!([token("rollout-response")]);
    assert_eq!(db.import_rollout(&rollout).unwrap()["eventsInserted"], "1");
    rollout["events"][0]["sourceId"] = json!("a");
    err(db.import_rollout(&rollout), "invalid_input");
}

#[test]
fn retention_cutoff_inclusive_and_cascading_deletion() {
    let db = memory();
    source(&db, "other", "demo");
    source(&db, "imported", "imported");
    db.update_settings(&json!({"contentCaptureEnabled":true,"retentionDays":1}))
        .unwrap();
    for (id, at) in [
        ("older", "2026-10-01T11:59:59.999Z"),
        ("boundary", "2026-10-01T12:00:00Z"),
        ("current", NOW),
    ] {
        let mut e = event(id);
        e["observedAt"] = json!(at);
        e["content"] = json!({"body":"synthetic"});
        db.ingest_event(&e).unwrap();
    }
    let mut other = event("other");
    other["sourceId"] = json!("other");
    other["content"] = json!({"body":"other"});
    db.ingest_event(&other).unwrap();
    let mut old = obs(json!({}));
    old["observedAt"] = json!("2026-09-01T00:00:00Z");
    db.ingest_observation(&old).unwrap();
    db.record_failure(&failure(
        "account/read",
        "rpc_error",
        "2026-09-01T00:00:00Z",
    ))
    .unwrap();
    let mut t = token("old-response");
    t["importedAt"] = json!("2026-09-01T00:00:00Z");
    let mut m = metadata();
    m["importedAt"] = t["importedAt"].clone();
    db.import_data(&json!({"sourceId":"imported","observations":[],"events":[],"responseTokens":[strip(t)],"importMetadata":m})).unwrap();
    assert_eq!(
        db.apply_retention(&json!({"now":NOW})).unwrap(),
        json!({"observationsDeleted":"1","eventsDeleted":"1","attemptsDeleted":"1","responseTokensDeleted":"1","importsDeleted":"1"})
    );
    err(
        db.get_local_event_detail(&json!({"sourceId":"a","eventId":"older"})),
        "event_not_found",
    );
    assert_eq!(
        db.clear_local_content(&query()).unwrap()["contentsDeleted"],
        "2"
    );
    assert_eq!(
        db.clear_local_content(&json!({})).unwrap()["contentsDeleted"],
        "1"
    );
    assert_eq!(db.clear_data(&query()).unwrap()["eventsDeleted"], "2");
    assert_eq!(db.get_status().unwrap()["eventCount"], "1");
    db.delete_source("other").unwrap();
    assert_eq!(db.get_status().unwrap()["eventCount"], "0");
    assert_eq!(db.clear_data(&json!({})).unwrap()["eventsDeleted"], "0");
    assert_eq!(
        db.apply_retention(&json!({})).unwrap()["eventsDeleted"],
        "0"
    );
}

#[test]
fn on_disk_private_persistence_secure_delete_and_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let mut db = UsageStore::with_clock_ms(&path, at_ms(NOW)).unwrap();
    source(&db, "a", "demo");
    db.update_settings(&json!({"contentCaptureEnabled":true,"retentionDays":1}))
        .unwrap();
    let canary = "SYNTHETIC_ERASURE_CANARY_923fafdd";
    let mut e = event("old");
    e["observedAt"] = json!("2020-01-01T00:00:00Z");
    e["content"] = json!({"body":canary.repeat(3000)});
    db.ingest_event(&e).unwrap();
    db.close().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let mut db = UsageStore::open(&path).unwrap();
    assert_eq!(db.get_status().unwrap()["eventCount"], "1");
    assert_eq!(db.get_settings().unwrap()["retentionDays"], 1);
    assert!(String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains(canary));
    db.clear_local_content(&json!({})).unwrap();
    db.close().unwrap();
    assert!(!String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains(canary));
    for suffix in ["-journal", "-wal", "-shm"] {
        assert!(!std::path::PathBuf::from(format!("{}{suffix}", path.display())).exists());
    }
    let future = dir.path().join("future.sqlite");
    let conn = rusqlite::Connection::open(&future).unwrap();
    conn.execute_batch("PRAGMA user_version=99").unwrap();
    drop(conn);
    err(UsageStore::open(&future), "unsupported_schema");
    err(UsageStore::open(dir.path()), "storage_error");
}

#[test]
fn source_cap_migrations_and_safe_corrupt_store_errors() {
    let db = UsageStore::in_memory().unwrap();
    for i in 0..100 {
        source(&db, &format!("source-{i:03}"), "demo");
    }
    err(db.create_source(&json!({"id":"excess","displayName":"Synthetic","mode":"demo","provider":"synthetic","coverageDescription":"Synthetic"})),"input_too_large");
    let mut import =
        json!({"sourceId":"source-000","observations":[],"events":[],"importMetadata":metadata()});
    err(db.import_data(&import), "imported_source_required");
    import.as_object_mut().unwrap().remove("importMetadata");
    import["responseTokens"] = json!([strip(token("not-imported"))]);
    err(db.import_data(&import), "imported_source_required");
    err(
        UsageStore::with_clock_ms(":memory:", i64::MAX),
        "invalid_input",
    );
    err(db.set_clock_ms(i64::MIN), "invalid_input");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("migrate.sqlite");
    let mut original = UsageStore::open(&path).unwrap();
    source(&original, "a", "demo");
    original.ingest_event(&event("keep")).unwrap();
    original.close().unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DROP TABLE imports;DROP TABLE response_tokens;PRAGMA user_version=1;")
        .unwrap();
    drop(conn);
    let mut migrated = UsageStore::open(&path).unwrap();
    assert_eq!(migrated.get_status().unwrap()["eventCount"], "1");
    assert_eq!(migrated.get_status().unwrap()["responseTokenCount"], "0");
    migrated.close().unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "UPDATE sources SET payload=?",
        ["{broken /SYNTHETIC_PRIVATE_PATH"],
    )
    .unwrap();
    drop(conn);
    let db = UsageStore::open(&path).unwrap();
    let error = db.list_sources().unwrap_err();
    assert_eq!(error.to_string(), "storage_error");
}

#[test]
fn exact_totals_include_all_models_after_group_cap() {
    let db = memory();
    source(&db, "imported", "imported");
    let mut records = vec![];
    for i in 0..503 {
        let mut value = token(&format!("r-{i:03}"));
        value["model"] = json!(format!("model-{i:03}"));
        records.push(strip(value));
    }
    let result = db
        .import_data(
            &json!({"sourceId":"imported","observations":[],"events":[],"responseTokens":records}),
        )
        .unwrap();
    assert_eq!(result["responseTokensInserted"], "503");
    let mut extra = token("priority-1");
    extra["model"] = json!("model-502");
    db.ingest_response_token(&extra).unwrap();
    let result = db
        .get_response_token_usage(&json!({"sourceId":"imported"}))
        .unwrap();
    assert_eq!(result["responseCount"], "504");
    assert_eq!(result["totals"]["totalTokens"], "5544");
    assert_eq!(result["byModel"].as_array().unwrap().len(), 500);
    assert_eq!(result["byModel"][0]["model"], "model-502");
    assert_eq!(result["byModel"][0]["responseCount"], "2");
    assert_eq!(result["byModel"][1]["model"], "model-000");
}

#[test]
fn synthetic_bounded_batch_throughput_preserves_exact_count() {
    let db = memory();
    let started = std::time::Instant::now();
    for batch in 0..10 {
        let events = (0..1000)
            .map(|i| {
                let mut e = event(&format!("b{batch}-e{i}"));
                e["model"] = json!(format!("model-{}", i % 3));
                e
            })
            .collect::<Vec<_>>();
        db.ingest_events(&json!(events)).unwrap();
    }
    let import_elapsed = started.elapsed();
    let start = std::time::Instant::now();
    let result = db.get_overview(&query()).unwrap();
    let query_elapsed = start.elapsed();
    assert_eq!(result["events"]["total"], "10000");
    assert_eq!(
        db.get_events(&json!({"sourceId":"a","limit":500})).unwrap()["events"]
            .as_array()
            .unwrap()
            .len(),
        500
    );
    eprintln!(
        "Synthetic 10,000 metadata events: import={import_elapsed:?}; overview={query_elapsed:?}; bounded 1,000-record input batches, 500-record page"
    );
}

#[test]
fn failed_persisted_writes_are_sanitized_and_import_is_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("failure.sqlite");
    let db = UsageStore::with_clock_ms(&path, at_ms(NOW)).unwrap();
    source(&db, "a", "imported");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_event BEFORE INSERT ON events WHEN NEW.event_id='reject' BEGIN SELECT RAISE(ABORT,'SYNTHETIC_PRIVATE_DATABASE_PATH'); END;
      CREATE TRIGGER fail_settings BEFORE UPDATE ON settings BEGIN SELECT RAISE(ABORT,'SYNTHETIC_SECRET'); END;
      CREATE TRIGGER fail_source BEFORE INSERT ON sources BEGIN SELECT RAISE(ABORT,'SYNTHETIC_SECRET'); END;").unwrap();
    err(
        db.update_settings(&json!({"capturePaused":true})),
        "storage_error",
    );
    assert_eq!(db.get_settings().unwrap()["capturePaused"], false);
    err(db.create_source(&json!({"id":"blocked","displayName":"Synthetic","mode":"imported","provider":"synthetic","coverageDescription":"Synthetic"})), "storage_error");
    assert_eq!(db.list_sources().unwrap().as_array().unwrap().len(), 1);
    err(db.import_data(&json!({"sourceId":"a","observations":[],"events":[strip(event("first")),strip(event("reject"))]})), "storage_error");
    assert_eq!(db.get_status().unwrap()["eventCount"], "0");
    conn.execute_batch(
        "DROP TRIGGER fail_event; DROP TRIGGER fail_settings; DROP TRIGGER fail_source;",
    )
    .unwrap();
    db.ingest_event(&event("after-recovery")).unwrap();
    assert_eq!(db.get_status().unwrap()["eventCount"], "1");
}
