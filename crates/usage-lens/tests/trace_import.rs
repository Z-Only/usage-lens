//! Synthetic bounded bundle tests; never inspect installed or user trace records.
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
use usage_lens::{
    adapters::{AdapterError, trace::*},
    cli::{parse_arguments, run_main},
    core::UsageStore,
};
const NOW: &str = "2026-10-03T14:00:00.000Z";
fn options() -> Value {
    json!({"sourceId":"local","importedAt":NOW,"sourceVersion":TRACE_SOURCE_VERSION})
}
fn manifest() -> Value {
    json!({"schema_version":1,"trace_id":"trace-one","rollout_id":"rollout-one","root_thread_id":"thread","started_at_unix_ms":1791036000000i64,"raw_event_log":"trace.jsonl","payloads_dir":"payloads"})
}
fn reference(n: usize, kind: &str) -> Value {
    json!({"raw_payload_id":format!("raw_payload:{n}"),"kind":{"type":kind},"path":format!("payloads/{n}.json")})
}
fn event(seq: u64, payload: Value) -> Value {
    json!({"schema_version":1,"seq":seq,"wall_time_unix_ms":1791036000000i64+seq as i64,"rollout_id":"rollout-one","thread_id":"thread","codex_turn_id":"turn","payload":payload})
}
fn started() -> Value {
    event(
        1,
        json!({"type":"inference_started","inference_call_id":"infer","thread_id":"thread","codex_turn_id":"turn","model":"model-a","provider_name":"synthetic","request_payload":reference(1,"inference_request")}),
    )
}
fn completed() -> Value {
    event(
        2,
        json!({"type":"inference_completed","inference_call_id":"infer","response_id":"response","upstream_request_id":"upstream","response_payload":reference(2,"inference_response")}),
    )
}
fn message(role: &str, text: &str) -> Value {
    let mut result = json!({"type":"message","role":role,"content":[{"type":if role=="user"{"input_text"}else{"output_text"},"text":text}]});
    if role == "user" {
        result["internal_chat_message_metadata_passthrough"] =
            json!({"content_item_kinds":["user.text"]});
    }
    result
}

fn request() -> Value {
    json!({"model":"model-a","reasoning":{"effort":"high"},"service_tier":"priority","input":[message("user","Visible question"),message("assistant","Visible history")]})
}
fn response() -> Value {
    json!({"response_id":"response","upstream_request_id":"upstream","token_usage":{"input_tokens":9007199254740993i64,"cached_input_tokens":0,"cache_write_input_tokens":0,"output_tokens":7,"reasoning_output_tokens":3,"total_tokens":9007199254741000i64},"output_items":[message("assistant","Visible answer")]})
}
fn bytes(value: &Value) -> Vec<u8> {
    value.to_string().into_bytes()
}
fn lines(events: &[Value]) -> Vec<u8> {
    events
        .iter()
        .flat_map(|v| (v.to_string() + "\n").into_bytes())
        .collect()
}
fn payloads() -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("payloads/1.json".into(), bytes(&request())),
        ("payloads/2.json".into(), bytes(&response())),
    ])
}
fn parse(
    m: &Value,
    events: &[Value],
    p: &BTreeMap<String, Vec<u8>>,
) -> Result<Value, AdapterError> {
    parse_trace_bundle(
        &bytes(m),
        &lines(events),
        |path| {
            p.get(path)
                .cloned()
                .ok_or(AdapterError("synthetic_missing_payload"))
        },
        &options(),
    )
}
fn normal() -> Value {
    parse(&manifest(), &[started(), completed()], &payloads()).unwrap()
}
#[test]
fn preserves_prepared_request_direct_response_tokens_and_unknown_observed_metadata() {
    let parsed = normal();
    let a = &parsed["attempts"][0];
    assert_eq!(parsed["adapterVersion"], TRACE_ADAPTER_VERSION);
    assert_eq!(parsed["sourceVersion"], TRACE_SOURCE_VERSION);
    assert_eq!(parsed["fingerprint"].as_str().unwrap().len(), 64);
    assert_eq!(a["request"]["model"]["value"], "model-a");
    assert_eq!(a["request"]["reasoningEffort"]["value"], "high");
    assert_eq!(a["request"]["serviceTier"]["value"], "priority");
    assert_eq!(a["observed"]["model"]["state"], "omitted");
    assert_eq!(a["observed"]["serviceTier"]["state"], "omitted");
    assert_eq!(a["evidence"], "prepared_request");
    assert_eq!(a["status"], "completed");
    assert_eq!(a["responseId"], "response");
    assert_eq!(a["tokens"]["inputTokens"]["value"], "9007199254740993");
    assert_eq!(a["tokens"]["cachedInputTokens"]["value"], "0");
    assert_eq!(
        a["requestProjection"]["messages"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        a["responseProjection"]["messages"][0]["text"],
        "Visible answer"
    );
    assert_eq!(parsed, normal());
}
#[test]
fn metadata_states_and_tokens_never_infer_standard_or_copy_requested_values() {
    for (value, state) in [
        (Value::Null, "not_reported"),
        (json!(42), "invalid"),
        (json!(""), "invalid"),
        (json!("future-model"), "reported"),
    ] {
        let mut req = request();
        req["model"] = value.clone();
        req["reasoning"] = value.clone();
        req["service_tier"] = value;
        let mut p = payloads();
        p.insert("payloads/1.json".into(), bytes(&req));
        let result = parse(&manifest(), &[started(), completed()], &p).unwrap();
        let a = &result["attempts"][0];
        assert_eq!(a["request"]["model"]["state"], state);
        assert_eq!(a["request"]["serviceTier"]["state"], state);
        assert_eq!(
            a["request"]["reasoningEffort"]["state"],
            if state == "not_reported" {
                "not_reported"
            } else {
                "invalid"
            }
        );
        assert_eq!(a["observed"]["serviceTier"]["state"], "omitted");
    }
    let mut req = request();
    for k in ["model", "reasoning", "service_tier"] {
        req.as_object_mut().unwrap().remove(k);
    }
    let mut p = payloads();
    p.insert("payloads/1.json".into(), bytes(&req));
    let r = parse(&manifest(), &[started(), completed()], &p).unwrap();
    assert_eq!(
        r["attempts"][0]["request"]["reasoningEffort"]["state"],
        "omitted"
    );
    assert_eq!(
        r["attempts"][0]["request"]["serviceTier"]["state"],
        "omitted"
    );
    for (value, state) in [
        (Value::Null, "not_reported"),
        (json!(-1), "invalid"),
        (json!(1.5), "invalid"),
        (json!("12"), "invalid"),
        (json!(i64::MAX), "reported"),
    ] {
        let mut resp = response();
        resp["token_usage"]["input_tokens"] = value;
        p.insert("payloads/2.json".into(), bytes(&resp));
        let r = parse(&manifest(), &[started(), completed()], &p).unwrap();
        assert_eq!(r["attempts"][0]["tokens"]["inputTokens"]["state"], state);
    }
    let mut resp = response();
    resp["token_usage"] = json!({});
    p.insert("payloads/2.json".into(), bytes(&resp));
    assert_eq!(
        parse(&manifest(), &[started(), completed()], &p).unwrap()["attempts"][0]["tokens"]["inputTokens"]
            ["state"],
        "omitted"
    );
    resp["token_usage"] = Value::Null;
    p.insert("payloads/2.json".into(), bytes(&resp));
    assert!(
        parse(&manifest(), &[started(), completed()], &p).unwrap()["attempts"][0]["tokens"]
            .is_null()
    );
    let mut done = completed();
    done["payload"]["response_id"] = Value::Null;
    resp["response_id"] = Value::Null;
    resp["token_usage"] = response()["token_usage"].clone();
    p.insert("payloads/2.json".into(), bytes(&resp));
    assert!(parse(&manifest(), &[started(), done], &p).unwrap()["attempts"][0]["tokens"].is_null());
}
#[test]
fn protects_instructions_reasoning_unclassified_user_text_and_known_secrets() {
    let mut unclassified = message("user", "UNCLASSIFIED_PRIVATE");
    unclassified
        .as_object_mut()
        .unwrap()
        .remove("internal_chat_message_metadata_passthrough");
    let mut injected = message("user", "INJECTED_PRIVATE");
    injected["internal_chat_message_metadata_passthrough"]["content_item_kinds"] =
        json!(["skills.selected_skill_instructions"]);
    let mut misaligned = message("user", "MISALIGNED_PRIVATE");
    misaligned["internal_chat_message_metadata_passthrough"]["content_item_kinds"] = json!([]);
    let mut analysis = message("assistant", "ANALYSIS_PRIVATE");
    analysis["channel"] = json!("analysis");
    let mut empty = message("assistant", "");
    empty["content"] = json!([{"type":"input_image","image_url":"IMAGE_PRIVATE"}]);
    let mut req = request();
    req["instructions"] = json!("INSTRUCTIONS_PRIVATE");
    req["authorization"] = json!("Bearer CREDENTIAL_PRIVATE");
    req["input"] = json!([message("system","SYSTEM_PRIVATE"),message("developer","DEVELOPER_PRIVATE"),unclassified,injected,misaligned,analysis,empty,{"type":"reasoning","text":"REASONING_PRIVATE"},{"type":"message","role":"assistant","content":null},message("assistant","api_key=secretvalue sk-proj-abcdefghijklmnop"),message("user","normal \"quoted\" [bracket] \\ slash")]);
    let mut p = payloads();
    p.insert("payloads/1.json".into(), bytes(&req));
    let r = parse(&manifest(), &[started(), completed()], &p).unwrap();
    let text = r.to_string();
    for secret in [
        "UNCLASSIFIED_PRIVATE",
        "INJECTED_PRIVATE",
        "MISALIGNED_PRIVATE",
        "ANALYSIS_PRIVATE",
        "IMAGE_PRIVATE",
        "INSTRUCTIONS_PRIVATE",
        "CREDENTIAL_PRIVATE",
        "SYSTEM_PRIVATE",
        "DEVELOPER_PRIVATE",
        "REASONING_PRIVATE",
        "secretvalue",
        "sk-proj-abcdefghijklmnop",
    ] {
        assert!(!text.contains(secret), "{secret}");
    }
    assert!(text.contains("REDACTED"));
    assert_eq!(
        r["attempts"][0]["requestProjection"]["messages"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn cancelled_failed_and_incomplete_are_distinct_and_non_inference_events_are_not_counted() {
    for kind in ["inference_failed", "inference_cancelled"] {
        for partial in [false, true] {
            let mut terminal = event(
                4,
                json!({"type":kind,"inference_call_id":"infer","upstream_request_id":"upstream","partial_response_payload":null,"error":"SECRET_ERROR","reason":"SECRET_REASON"}),
            );
            let mut p = payloads();
            if partial {
                terminal["payload"]["partial_response_payload"] =
                    reference(2, "inference_response");
                let mut resp = response();
                resp["response_id"] = Value::Null;
                resp["token_usage"] = Value::Null;
                p.insert("payloads/2.json".into(), bytes(&resp));
            }
            let r = parse(
                &manifest(),
                &[
                    started(),
                    event(2, json!({"type":"codex_turn_ended"})),
                    terminal,
                ],
                &p,
            )
            .unwrap();
            let a = &r["attempts"][0];
            assert_eq!(a["status"], kind.strip_prefix("inference_").unwrap());
            assert!(a["tokens"].is_null());
            assert!(!r.to_string().contains("SECRET_"));
            assert!(
                r["warningCodes"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("trace_sequence_gap"))
            );
        }
    }
    let r = parse(&manifest(), &[started()], &payloads()).unwrap();
    assert_eq!(r["attempts"][0]["status"], "incomplete");
    assert!(
        r["warningCodes"]
            .as_array()
            .unwrap()
            .contains(&json!("trace_incomplete_inference"))
    );
    let r = parse_trace_bundle(
        &bytes(&manifest()),
        started().to_string().as_bytes(),
        |path| Ok(payloads()[path].clone()),
        &options(),
    )
    .unwrap();
    assert!(
        r["warningCodes"]
            .as_array()
            .unwrap()
            .contains(&json!("trace_unterminated_final_line"))
    );
}
#[test]
fn rejects_bad_manifest_source_event_identity_sequence_and_lifecycle() {
    for (key, value) in [
        ("schema_version", json!(2)),
        ("raw_event_log", json!("../other")),
        ("payloads_dir", json!("elsewhere")),
        ("trace_id", json!("bad secret")),
        ("started_at_unix_ms", json!("0")),
        ("started_at_unix_ms", json!(i64::MAX)),
    ] {
        let mut m = manifest();
        m[key] = value;
        assert!(parse(&m, &[started()], &payloads()).is_err(), "{key}");
    }
    for (key, value) in [
        ("schema_version", json!(2)),
        ("rollout_id", json!("other")),
        ("seq", json!(0)),
        ("seq", json!("1")),
        ("wall_time_unix_ms", json!(null)),
        ("thread_id", json!("other")),
        ("thread_id", json!("bad id")),
        ("codex_turn_id", Value::Null),
    ] {
        let mut e = started();
        e[key] = value;
        assert!(parse(&manifest(), &[e], &payloads()).is_err(), "{key}");
    }
    for events in [
        vec![completed()],
        vec![started(), started()],
        vec![
            started(),
            completed(),
            event(3, completed()["payload"].clone()),
        ],
        vec![started(), event(2, started()["payload"].clone())],
    ] {
        assert!(parse(&manifest(), &events, &payloads()).is_err());
    }
    let mut e = started();
    e["payload"]["type"] = Value::Null;
    assert!(parse(&manifest(), &[e], &payloads()).is_err());
    let mut done = completed();
    done["thread_id"] = json!("other");
    assert!(parse(&manifest(), &[started(), done], &payloads()).is_err());
    let mut done = completed();
    done["payload"]["response_id"] = json!("bad id");
    assert!(parse(&manifest(), &[started(), done], &payloads()).is_err());
    let mut done = completed();
    done["payload"]["response_payload"] = Value::Null;
    assert!(parse(&manifest(), &[started(), done], &payloads()).is_err());
    for opts in [
        Value::Null,
        json!({}),
        json!({"sourceId":"local","importedAt":NOW,"sourceVersion":"wrong"}),
        json!({"sourceId":"local","importedAt":NOW,"sourceVersion":TRACE_SOURCE_VERSION,"extra":true}),
        json!({"sourceId":"bad source","importedAt":NOW,"sourceVersion":TRACE_SOURCE_VERSION}),
    ] {
        assert!(parse_trace_bundle(&bytes(&manifest()), &[], |_| Ok(vec![]), &opts).is_err());
    }
}
#[test]
fn fails_closed_on_unsafe_or_duplicate_references_and_inconsistent_response() {
    for path in [
        "/tmp/secret",
        "../secret",
        "payloads/../secret",
        "payloads/0.json",
        "payloads/01.json",
        "payloads/-1.json",
        "payloads/a.json",
        "payloads/1/2.json",
        "payloads\\1.json",
        "payloads/123456789012345678901.json",
    ] {
        let mut e = started();
        e["payload"]["request_payload"]["path"] = json!(path);
        assert_eq!(
            parse(&manifest(), &[e], &payloads()).unwrap_err().0,
            "trace_unsafe_path"
        );
    }
    for (key, value) in [
        ("kind", json!("inference_request")),
        ("kind", json!({"type":"other"})),
        ("raw_payload_id", json!("raw_payload:2")),
        ("path", Value::Null),
    ] {
        let mut e = started();
        e["payload"]["request_payload"][key] = value;
        assert!(parse(&manifest(), &[e], &payloads()).is_err());
    }
    let mut second = started();
    second["seq"] = json!(2);
    second["payload"]["inference_call_id"] = json!("other");
    assert_eq!(
        parse(&manifest(), &[started(), second], &payloads())
            .unwrap_err()
            .0,
        "trace_duplicate_payload_reference"
    );
    for (key, value) in [
        ("response_id", json!("other")),
        ("upstream_request_id", json!("other")),
        ("output_items", Value::Null),
    ] {
        let mut p = payloads();
        let mut r = response();
        r[key] = value;
        p.insert("payloads/2.json".into(), bytes(&r));
        assert!(parse(&manifest(), &[started(), completed()], &p).is_err());
    }
    for request in [Value::Null, json!([]), json!({"input":null})] {
        let mut p = payloads();
        p.insert("payloads/1.json".into(), bytes(&request));
        assert!(parse(&manifest(), &[started()], &p).is_err());
    }
    let mut p = payloads();
    p.insert("payloads/1.json".into(), b"not JSON".to_vec());
    assert_eq!(
        parse(&manifest(), &[started()], &p).unwrap_err().0,
        "trace_invalid_json"
    );
    assert_eq!(
        parse(&manifest(), &[started()], &BTreeMap::new())
            .unwrap_err()
            .0,
        "synthetic_missing_payload"
    );
}
#[test]
fn enforces_byte_line_depth_and_event_limits() {
    assert_eq!(
        parse_trace_bundle(
            &vec![b' '; MAX_MANIFEST_BYTES + 1],
            &[],
            |_| Ok(vec![]),
            &options()
        )
        .unwrap_err()
        .0,
        "trace_too_large"
    );
    assert_eq!(
        parse_trace_bundle(
            &bytes(&manifest()),
            &vec![b' '; MAX_TRACE_BYTES + 1],
            |_| Ok(vec![]),
            &options()
        )
        .unwrap_err()
        .0,
        "trace_too_large"
    );
    assert_eq!(
        parse_trace_bundle(
            &bytes(&manifest()),
            &vec![b'x'; MAX_LINE_BYTES + 1],
            |_| Ok(vec![]),
            &options()
        )
        .unwrap_err()
        .0,
        "trace_line_limit"
    );
    let mut p = payloads();
    p.insert("payloads/1.json".into(), vec![b'x'; MAX_PAYLOAD_BYTES + 1]);
    assert_eq!(
        parse(&manifest(), &[started()], &p).unwrap_err().0,
        "trace_too_large"
    );
    p.insert(
        "payloads/1.json".into(),
        format!(
            "{}0{}",
            "[".repeat(MAX_DEPTH + 1),
            "]".repeat(MAX_DEPTH + 1)
        )
        .into_bytes(),
    );
    assert_eq!(
        parse(&manifest(), &[started()], &p).unwrap_err().0,
        "trace_depth_limit"
    );
    let events = (1..=MAX_EVENTS + 1)
        .map(|i| event(i as u64, json!({"type":"ignored"})))
        .collect::<Vec<_>>();
    assert_eq!(
        parse(&manifest(), &events, &payloads()).unwrap_err().0,
        "trace_event_limit"
    );
    let mut events = Vec::new();
    let mut p = BTreeMap::new();
    for i in 1..=MAX_ATTEMPTS + 1 {
        let mut e = started();
        e["seq"] = json!(i);
        e["payload"]["inference_call_id"] = json!(format!("infer-{i}"));
        e["payload"]["request_payload"] = reference(i, "inference_request");
        events.push(e);
        p.insert(format!("payloads/{i}.json"), bytes(&request()));
    }
    assert_eq!(
        parse(&manifest(), &events, &p).unwrap_err().0,
        "trace_attempt_limit"
    );
    let huge = json!({"input":[],"unused":"x".repeat(1024*1024)});
    for i in 1..=40 {
        p.insert(format!("payloads/{i}.json"), bytes(&huge));
    }
    assert_eq!(
        parse(&manifest(), &events[..40], &p).unwrap_err().0,
        "trace_too_large"
    );
}
fn materialize(path: &Path) {
    std::fs::create_dir_all(path.join("payloads")).unwrap();
    std::fs::write(path.join("manifest.json"), bytes(&manifest())).unwrap();
    std::fs::write(path.join("trace.jsonl"), lines(&[started(), completed()])).unwrap();
    for (name, data) in payloads() {
        std::fs::write(path.join(name), data).unwrap();
    }
}
#[test]
fn reads_only_explicit_regular_confined_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    materialize(&root);
    assert_eq!(read_trace_bundle(&root, &options()).unwrap(), normal());
    assert_eq!(
        read_trace_bundle(Path::new("relative"), &options())
            .unwrap_err()
            .0,
        "absolute_import_path_required"
    );
    assert!(read_trace_bundle(&root.join("missing"), &options()).is_err());
    std::fs::remove_file(root.join("payloads/1.json")).unwrap();
    std::fs::create_dir(root.join("payloads/1.json")).unwrap();
    assert_eq!(
        read_trace_bundle(&root, &options()).unwrap_err().0,
        "trace_regular_file_required"
    );
    std::fs::remove_dir(root.join("payloads/1.json")).unwrap();
    std::fs::write(
        root.join("payloads/1.json"),
        vec![b' '; MAX_PAYLOAD_BYTES + 1],
    )
    .unwrap();
    assert_eq!(
        read_trace_bundle(&root, &options()).unwrap_err().0,
        "trace_too_large"
    );
}
#[cfg(unix)]
#[test]
fn rejects_symlinks_in_root_ancestors_payload_directory_and_files() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let root = base.join("bundle");
    materialize(&root);
    symlink(&root, base.join("alias")).unwrap();
    assert_eq!(
        read_trace_bundle(&base.join("alias"), &options())
            .unwrap_err()
            .0,
        "trace_unsafe_path"
    );
    assert!(read_trace_bundle(&root.join(".."), &options()).is_err());
    std::fs::rename(root.join("payloads"), base.join("payloads-outside")).unwrap();
    symlink(base.join("payloads-outside"), root.join("payloads")).unwrap();
    assert!(read_trace_bundle(&root, &options()).is_err());
    std::fs::remove_file(root.join("payloads")).unwrap();
    std::fs::rename(base.join("payloads-outside"), root.join("payloads")).unwrap();
    std::fs::remove_file(root.join("payloads/1.json")).unwrap();
    symlink(root.join("payloads/2.json"), root.join("payloads/1.json")).unwrap();
    assert!(read_trace_bundle(&root, &options()).is_err());
    std::fs::remove_file(root.join("payloads/1.json")).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(root.join("payloads/1.json"))
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        read_trace_bundle(&root, &options()).unwrap_err().0,
        "trace_regular_file_required"
    );
}
async fn cli(args: &[&str]) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run_main(
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        &b""[..],
        &mut out,
        &mut err,
    )
    .await;
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}
#[tokio::test]
async fn cli_import_query_and_read_only_compatibility() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let bundle = root.join("bundle");
    materialize(&bundle);
    let db = root.join("db.sqlite");
    let dbs = db.to_str().unwrap();
    let bs = bundle.to_str().unwrap();
    assert_eq!(
        cli(&[
            "source", "--db", dbs, "--source", "local", "--mode", "imported"
        ])
        .await
        .0,
        0
    );
    assert_eq!(
        cli(&["settings", "--db", dbs, "--content", "true"]).await.0,
        0
    );
    let result = cli(&[
        "import-trace-bundle",
        "--db",
        dbs,
        "--source",
        "local",
        "--directory",
        bs,
        "--source-version",
        TRACE_SOURCE_VERSION,
    ])
    .await;
    assert_eq!(result.0, 0, "{}", result.2);
    let attempts = cli(&["trace-attempts", "--db", dbs, "--source", "local"]).await;
    assert_eq!(attempts.0, 0, "{}", attempts.2);
    let value: Value = serde_json::from_str(&attempts.1).unwrap();
    let id = value["attempts"][0]["attemptId"].as_str().unwrap();
    assert_eq!(
        cli(&[
            "trace-detail",
            "--db",
            dbs,
            "--source",
            "local",
            "--attempt",
            id
        ])
        .await
        .0,
        0
    );
    let summary = cli(&[
        "trace-summary",
        "--db",
        dbs,
        "--source",
        "local",
        "--from",
        "2026-10-03",
        "--to",
        "2026-10-03",
    ])
    .await;
    assert_eq!(summary.0, 0, "{}", summary.2);
    assert!(!summary.1.contains("Visible question"));
    let store = UsageStore::open_read_only(&db).unwrap();
    assert_eq!(
        store
            .get_trace_summary(&json!({"sourceId":"local"}))
            .unwrap()["attemptCount"],
        "1"
    );
    let bad = cli(&[
        "import-trace-bundle",
        "--demo",
        "--source",
        "local",
        "--directory",
        bs,
        "--source-version",
        TRACE_SOURCE_VERSION,
    ])
    .await;
    assert_eq!(bad.0, 1);
    assert!(parse_arguments(&["trace-detail".into(), "--event".into(), "event".into()]).is_err());
}

#[test]
fn sequence_order_preserves_wall_clock_regression_and_does_not_infer_duration() {
    let mut done = completed();
    done["wall_time_unix_ms"] = json!(1791035000000i64);
    let result = parse(&manifest(), &[started(), done], &payloads()).unwrap();
    assert!(
        result["warningCodes"]
            .as_array()
            .unwrap()
            .contains(&json!("trace_clock_regression"))
    );
    assert_eq!(result["attempts"][0]["status"], "completed");
    assert!(result["attempts"][0].get("durationMs").is_none());
    let store = UsageStore::in_memory().unwrap();
    store.create_source(&json!({"id":"local","mode":"imported","displayName":"local","provider":"synthetic","coverageDescription":"Synthetic only"})).unwrap();
    store.import_trace_bundle(&result).unwrap();
    assert_eq!(
        store
            .get_trace_summary(&json!({"sourceId":"local"}))
            .unwrap()["attemptCount"],
        "1"
    );
}

#[test]
fn malformed_token_parent_and_nonpartial_failure_summaries_fail_closed() {
    for value in [json!(42), json!([]), json!("tokens")] {
        let mut p = payloads();
        let mut r = response();
        r["token_usage"] = value;
        p.insert("payloads/2.json".into(), bytes(&r));
        assert_eq!(
            parse(&manifest(), &[started(), completed()], &p)
                .unwrap_err()
                .0,
            "trace_invalid_response"
        );
    }
    let done = event(
        2,
        json!({"type":"inference_failed","inference_call_id":"infer","upstream_request_id":"upstream","partial_response_payload":reference(2,"inference_response")}),
    );
    assert_eq!(
        parse(&manifest(), &[started(), done], &payloads())
            .unwrap_err()
            .0,
        "trace_invalid_partial_response"
    );
}

#[test]
fn assistant_typed_instructions_and_unknown_phases_are_excluded() {
    for field in ["internal_chat_message_metadata_passthrough", "phase"] {
        let mut item = message("assistant", "ASSISTANT_PRIVATE");
        item[field] = if field == "phase" {
            json!("analysis")
        } else {
            json!({"content_item_kinds":["skills.selected_skill_instructions"]})
        };
        let mut req = request();
        req["input"] = json!([item]);
        let mut p = payloads();
        p.insert("payloads/1.json".into(), bytes(&req));
        assert!(
            !parse(&manifest(), &[started(), completed()], &p)
                .unwrap()
                .to_string()
                .contains("ASSISTANT_PRIVATE")
        );
    }
    for phase in [Value::Null, json!("commentary"), json!("final_answer")] {
        let mut item = message("assistant", "visible");
        item["phase"] = phase;
        item["internal_chat_message_metadata_passthrough"] = json!({"content_item_kinds":[]});
        let mut req = request();
        req["input"] = json!([item]);
        let mut p = payloads();
        p.insert("payloads/1.json".into(), bytes(&req));
        assert_eq!(
            parse(&manifest(), &[started(), completed()], &p).unwrap()["attempts"][0]["requestProjection"]
                ["messages"][0]["text"],
            "visible"
        );
    }
}

#[cfg(windows)]
#[test]
fn rejects_windows_network_and_device_roots_before_opening() {
    for root in [
        r"\\server\share\bundle",
        r"\\?\UNC\server\share\bundle",
        r"\\.\pipe\bundle",
    ] {
        assert_eq!(
            read_trace_bundle(Path::new(root), &options())
                .unwrap_err()
                .0,
            "trace_unsafe_path"
        );
    }
}

#[test]
fn raw_payload_budget_does_not_override_projection_budget_when_content_capture_is_off() {
    use usage_lens::core::validation::CONTENT_BYTES;
    let mut req = request();
    req["input"] = json!([message("user", &"x".repeat(CONTENT_BYTES))]);
    let mut supplied = payloads();
    supplied.insert("payloads/1.json".into(), bytes(&req));
    assert!(supplied["payloads/1.json"].len() < MAX_PAYLOAD_BYTES);
    let parsed = parse(&manifest(), &[started(), completed()], &supplied).unwrap();
    assert!(parsed["attempts"][0]["requestProjection"].to_string().len() > CONTENT_BYTES);
    for capture in [false, true] {
        let store = UsageStore::in_memory().unwrap();
        store.create_source(&json!({"id":"local","mode":"imported","displayName":"Local","provider":"synthetic","coverageDescription":"Synthetic only"})).unwrap();
        store
            .update_settings(&json!({"contentCaptureEnabled":capture}))
            .unwrap();
        assert_eq!(
            store.import_trace_bundle(&parsed).unwrap_err().code(),
            "input_too_large"
        );
        assert_eq!(store.get_status().unwrap()["schemaVersion"], 2);
        assert_eq!(
            store
                .get_trace_summary(&json!({"sourceId":"local"}))
                .unwrap()["attemptCount"],
            "0"
        );
    }
}
