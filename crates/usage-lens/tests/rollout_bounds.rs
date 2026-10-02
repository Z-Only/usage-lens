//! Synthetic end-to-end regressions for parser/store projection budget consistency.
use serde_json::{Value, json};
use usage_lens::{
    adapters::rollout::{ROLLOUT_SOURCE_VERSION, parse_rollout},
    core::{
        CoreError, UsageStore,
        validation::{
            CONTENT_BYTES, RAW_BYTES, check_event_batch_size, check_import_size, check_size,
            preflight_rollout_import,
        },
    },
};

const NOW: &str = "2026-10-02T06:00:00.000Z";
fn options(capture: bool) -> Value {
    json!({"sourceId":"sample","observedAt":NOW,"captureContent":capture,"sourceVersion":ROLLOUT_SOURCE_VERSION})
}
fn envelope(kind: &str, payload: Value) -> Value {
    json!({"timestamp":NOW,"type":kind,"payload":payload})
}
fn source_rows() -> Vec<Value> {
    vec![envelope(
        "session_meta",
        json!({"id":"thread","session_id":"session"}),
    )]
}
fn encode(rows: &[Value]) -> Vec<u8> {
    (rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n")
        .into_bytes()
}
fn nested_arrays(depth: usize) -> Value {
    let mut value = json!(0);
    for _ in 0..depth {
        value = Value::Array(vec![value]);
    }
    value
}
fn call(id: usize, args: Value) -> Value {
    envelope(
        "response_item",
        json!({"type":"function_call","call_id":format!("call-{id}"),"namespace":"synthetic","name":"tool","arguments":args.to_string()}),
    )
}
fn message(id: usize, size: usize) -> Value {
    envelope(
        "response_item",
        json!({"type":"message","id":format!("item-{id}"),"role":"user","content":[{"type":"input_text","text":"x".repeat(size)}]}),
    )
}
fn store(capture: bool) -> UsageStore {
    let store = UsageStore::in_memory().unwrap();
    store.create_source(&json!({"id":"sample","displayName":"Synthetic projection bounds","mode":"imported","provider":"synthetic","coverageDescription":"Synthetic only"})).unwrap();
    store
        .update_settings(&json!({"contentCaptureEnabled":capture}))
        .unwrap();
    store
}
fn import_input(mut parsed: Value) -> Value {
    parsed.as_object_mut().unwrap().remove("warnings");
    parsed.as_object_mut().unwrap().remove("recordsSeen");
    parsed["importedAt"] = json!(NOW);
    parsed
}
fn event(id: usize, content: Value) -> Value {
    json!({"sourceId":"sample","eventId":format!("event-{id}"),"eventType":"tool_call","evidenceType":"explicit_tool_call","toolName":"synthetic.tool","observedAt":NOW,"collectorVersion":"synthetic-1","content":content})
}
fn projection(events: Vec<Value>) -> Value {
    json!({"sourceId":"sample","fingerprint":"a".repeat(64),"sourceVersion":ROLLOUT_SOURCE_VERSION,"adapterVersion":"synthetic-1","importedAt":NOW,"warningCodes":[],"events":events,"responseTokens":[]})
}
fn bundle(input: &Value) -> Value {
    let mut events = input["events"].clone();
    for event in events.as_array_mut().unwrap() {
        event.as_object_mut().unwrap().remove("sourceId");
    }
    json!({"sourceId":"sample","observations":[],"events":events,"responseTokens":[],"importMetadata":{"fingerprint":input["fingerprint"],"sourceVersion":input["sourceVersion"],"adapterVersion":input["adapterVersion"],"importedAt":NOW,"warningCodes":[]}})
}
fn assert_empty(store: &UsageStore) {
    let status = store.get_status().unwrap();
    for key in ["eventCount", "observationCount", "responseTokenCount"] {
        assert_eq!(status[key], "0", "{key}");
    }
}
fn assert_rejected_every_path(input: &Value) {
    for path in 0..3 {
        let store = store(true);
        let result = match path {
            0 => store.import_rollout(input),
            1 => store.import_data(&bundle(input)),
            _ => store.ingest_events(&input["events"]),
        };
        assert_eq!(result.unwrap_err(), CoreError::InputTooLarge, "path {path}");
        assert_empty(&store);
        // A failed projection must not leave an import-ledger entry behind.
        let small = projection(vec![event(999, json!({"body":"small"}))]);
        assert_eq!(
            store.import_rollout(&small).unwrap()["importAlreadyPresent"],
            false
        );
    }
}

#[test]
fn moderate_source_capture_off_imports_but_capture_on_fails_in_parser_before_storage() {
    let mut rows = source_rows();
    rows.extend((0..16).map(|id| message(id, 150 * 1024)));
    let bytes = encode(&rows);
    assert!(bytes.len() > RAW_BYTES && bytes.len() < 8 * 1024 * 1024);
    let parsed = parse_rollout(&bytes, &options(false)).unwrap();
    let db = store(false);
    assert_eq!(
        db.import_rollout(&import_input(parsed)).unwrap()["eventsInserted"],
        "16"
    );
    let db = store(true);
    assert_eq!(
        parse_rollout(&bytes, &options(true)).unwrap_err().0,
        "rollout_projection_limit"
    );
    assert_empty(&db);
}

#[test]
fn content_depth_29_to_31_passes_every_import_layer_without_truncation() {
    for depth in 29..=31 {
        let mut rows = source_rows();
        rows.push(call(depth, nested_arrays(depth)));
        let parsed = parse_rollout(&encode(&rows), &options(true)).unwrap();
        let imported = import_input(parsed);
        for path in 0..3 {
            let db = store(true);
            match path {
                0 => {
                    db.import_rollout(&imported).unwrap();
                }
                1 => {
                    db.import_data(&bundle(&imported)).unwrap();
                }
                _ => {
                    db.ingest_events(&imported["events"]).unwrap();
                }
            }
            let detail = db
                .get_local_event_detail(
                    &json!({"sourceId":"sample","eventId":imported["events"][0]["eventId"]}),
                )
                .unwrap();
            assert_eq!(
                detail["content"]["toolArguments"],
                nested_arrays(depth),
                "depth {depth}, path {path}"
            );
            assert!(!detail.to_string().contains("EXCLUDED_NESTED_CONTENT"));
        }
    }
}

#[test]
fn true_content_depth_32_rejects_early_with_capture_and_leaves_no_partial_rows() {
    let mut rows = source_rows();
    rows.push(message(0, 10));
    rows.push(call(32, nested_arrays(32)));
    let bytes = encode(&rows);
    assert_eq!(
        parse_rollout(&bytes, &options(true)).unwrap_err().0,
        "rollout_projection_limit"
    );
    let without_content = import_input(parse_rollout(&bytes, &options(false)).unwrap());
    assert_eq!(
        store(false).import_rollout(&without_content).unwrap()["eventsInserted"],
        "2"
    );
    let input = projection(vec![
        event(0, json!({"body":"valid first record"})),
        event(32, json!({"toolArguments":nested_arrays(32)})),
    ]);
    assert_rejected_every_path(&input);
}

#[test]
fn total_byte_budget_still_includes_content_across_all_paths() {
    let input = projection(
        (0..16)
            .map(|id| event(id, json!({"body":"x".repeat(150*1024)})))
            .collect(),
    );
    for event in input["events"].as_array().unwrap() {
        check_size(&event["content"], CONTENT_BYTES).unwrap();
    }
    assert_rejected_every_path(&input);
}

#[test]
fn per_content_byte_limit_and_one_byte_over_are_enforced_in_every_path() {
    // check_size's stable budget for {"body":<string>} is string bytes + 10.
    for size in [CONTENT_BYTES - 10, CONTENT_BYTES - 9] {
        let content = json!({"body":"x".repeat(size)});
        let input = projection(vec![event(0, content)]);
        if size == CONTENT_BYTES - 9 {
            assert_rejected_every_path(&input);
        } else {
            preflight_rollout_import(&input).unwrap();
            for path in 0..3 {
                let db = store(true);
                match path {
                    0 => {
                        db.import_rollout(&input).unwrap();
                    }
                    1 => {
                        db.import_data(&bundle(&input)).unwrap();
                    }
                    _ => {
                        db.ingest_events(&input["events"]).unwrap();
                    }
                }
                assert_eq!(db.get_status().unwrap()["eventCount"], "1");
            }
        }
    }
    // An expanded structured argument can exceed its content budget while its JSONL line fits.
    let mut rows = source_rows();
    rows.push(call(0, json!(vec![0; 50_000])));
    let bytes = encode(&rows);
    assert!(bytes.len() < 256 * 1024);
    assert_eq!(
        parse_rollout(&bytes, &options(true)).unwrap_err().0,
        "rollout_projection_limit"
    );
}

#[test]
fn node_budget_is_global_across_individually_bounded_content_objects() {
    let content = json!({"toolArguments":vec![0;35_000]});
    check_size(&content, CONTENT_BYTES).unwrap();
    let accepted = projection(vec![event(0, content.clone()), event(1, content.clone())]);
    store(true).import_rollout(&accepted).unwrap();
    let rejected = projection(vec![
        event(0, content.clone()),
        event(1, content.clone()),
        event(2, content),
    ]);
    assert!(rejected.to_string().len() < RAW_BYTES);
    assert_rejected_every_path(&rejected);
    let mut rows = source_rows();
    rows.extend((0..3).map(|id| call(id, json!(vec![0; 35_000]))));
    assert_eq!(
        parse_rollout(&encode(&rows), &options(true)).unwrap_err().0,
        "rollout_projection_limit"
    );
}

#[test]
fn depth_reset_is_limited_to_event_content_and_does_not_hide_nested_content_keys() {
    let nested = json!({"content":nested_arrays(32)});
    let input = projection(vec![event(0, json!({"toolArguments":nested}))]);
    assert_rejected_every_path(&input);
    let ordinary = json!({"events":[{"other":nested_arrays(32)}]});
    assert_eq!(
        check_import_size(&ordinary, RAW_BYTES).unwrap_err(),
        CoreError::InputTooLarge
    );
    let unscoped = json!({"content":nested_arrays(32)});
    assert_eq!(
        check_size(&unscoped, RAW_BYTES).unwrap_err(),
        CoreError::InputTooLarge
    );
    assert_eq!(
        check_event_batch_size(&json!([{"other":nested_arrays(32)}]), RAW_BYTES).unwrap_err(),
        CoreError::InputTooLarge
    );
}

#[test]
fn near_total_byte_boundary_parser_success_implies_the_same_store_preflight_succeeds() {
    // Locate the largest accepted content body to test the actual normalized threshold,
    // including sourceId removal/reinjection and the importMetadata wrapper.
    let (mut low, mut high) = (100 * 1024, 150 * 1024);
    while low + 1 < high {
        let mid = (low + high) / 2;
        let mut rows = source_rows();
        rows.extend((0..16).map(|id| message(id, mid)));
        if parse_rollout(&encode(&rows), &options(true)).is_ok() {
            low = mid;
        } else {
            high = mid;
        }
    }
    let mut rows = source_rows();
    rows.extend((0..16).map(|id| message(id, low)));
    let input = import_input(parse_rollout(&encode(&rows), &options(true)).unwrap());
    assert_eq!(
        store(true).import_rollout(&input).unwrap()["eventsInserted"],
        "16"
    );
    let mut over = source_rows();
    over.extend((0..16).map(|id| message(id, high)));
    assert_eq!(
        parse_rollout(&encode(&over), &options(true)).unwrap_err().0,
        "rollout_projection_limit"
    );
}

#[test]
fn native_cli_reports_projection_limit_before_any_database_rows_are_inserted() {
    use std::{fs, process::Command};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let file = dir.path().join("synthetic.jsonl");
    let db = UsageStore::open(&path).unwrap();
    db.create_source(&json!({"id":"sample","displayName":"Synthetic CLI bounds","mode":"imported","provider":"synthetic","coverageDescription":"Synthetic only"})).unwrap();
    db.update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    let mut rows = source_rows();
    rows.extend((0..16).map(|id| message(id, 150 * 1024)));
    fs::write(&file, encode(&rows)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_usage-lens"))
        .args([
            "import-rollout",
            "--db",
            path.to_str().unwrap(),
            "--source",
            "sample",
            "--file",
            file.to_str().unwrap(),
            "--source-version",
            ROLLOUT_SOURCE_VERSION,
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "Usage Lens: rollout_projection_limit"
    );
    assert_empty(&db);
}

#[test]
fn shared_preflight_keeps_required_shapes_source_scope_and_record_caps() {
    let base = projection(vec![event(0, json!({"body":"small"}))]);
    for (field, value) in [
        ("events", Value::Null),
        ("events", json!([null])),
        ("events", json!(vec![event(0, json!({})); 1001])),
        (
            "responseTokens",
            json!(vec![json!({"sourceId":"sample"}); 1001]),
        ),
    ] {
        let mut input = base.clone();
        input[field] = value;
        assert_eq!(
            preflight_rollout_import(&input).unwrap_err(),
            CoreError::InvalidInput
        );
    }
    let mut wrong_source = base.clone();
    wrong_source["events"][0]["sourceId"] = json!("another");
    assert_eq!(
        preflight_rollout_import(&wrong_source).unwrap_err(),
        CoreError::InvalidInput
    );
    let mut extra = base;
    extra["unknown"] = json!(true);
    assert_eq!(
        preflight_rollout_import(&extra).unwrap_err(),
        CoreError::InvalidInput
    );
}
