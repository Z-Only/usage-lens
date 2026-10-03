//! Synthetic incremental-parser tests. No real session histories are read.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use usage_lens::adapters::rollout::{
    INCREMENTAL_ADAPTER_VERSION, MAX_BYTES, MAX_DEPTH, MAX_EVENTS, MAX_LINE_BYTES, MAX_LINES,
    MAX_RESPONSE_TOKENS, ROLLOUT_ADAPTER_VERSION, ROLLOUT_SOURCE_VERSION,
    parse_incremental_rollout, parse_rollout,
};

const NOW: &str = "2026-10-03T06:00:00.000Z";
const PRIVATE: &str = "PRIVATE_SYNTHETIC_CONTENT";
fn options() -> Value {
    json!({"sourceId":"synthetic","streamId":"stream-a","observedAt":NOW,
        "captureContent":false,"sourceVersion":ROLLOUT_SOURCE_VERSION})
}
fn row(kind: &str, payload: Value) -> Value {
    json!({"timestamp":NOW,"type":kind,"payload":payload})
}
fn bytes(records: &[Value]) -> Vec<u8> {
    records
        .iter()
        .flat_map(|row| (row.to_string() + "\n").into_bytes())
        .collect()
}
fn parse(records: &[Value]) -> Value {
    parse_incremental_rollout(&bytes(records), &options()).unwrap()
}
fn meta() -> Value {
    row(
        "session_meta",
        json!({"id":"thread","session_id":"session"}),
    )
}
fn context() -> Value {
    row("turn_context", json!({"turn_id":"turn","model":"model-a"}))
}
fn message(id: Option<&str>, body: &str) -> Value {
    let mut value = row(
        "response_item",
        json!({"type":"message","role":"user",
        "content":[{"type":"input_text","text":body}]}),
    );
    if let Some(id) = id {
        value["payload"]["id"] = json!(id);
    }
    value
}
fn typed(id: Option<&str>, body: &str) -> Value {
    let mut value = message(
        id,
        &format!(
            "<skill>\n<name>Synthetic</name>\n<path>/never/read/SKILL.md</path>\n{body}\n</skill>"
        ),
    );
    value["payload"]["internal_chat_message_metadata_passthrough"] =
        json!({"content_item_kinds":["skills.selected_skill_instructions"]});
    value
}
fn call(id: &str, skills: bool) -> Value {
    row(
        "response_item",
        json!({"type":"function_call","call_id":id,
        "namespace":if skills {"skills"} else {"synthetic"},"name":"read",
        "arguments":json!({"package":"synthetic"}).to_string()}),
    )
}
fn output(id: &str, body: &str) -> Value {
    row(
        "response_item",
        json!({"type":"function_call_output","call_id":id,
        "output":json!({"resource":"main","contents":body,"next_cursor":null}).to_string()}),
    )
}
fn usage() -> Value {
    json!({"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,
        "reasoning_output_tokens":1,"total_tokens":13})
}
fn token(id: &str) -> Value {
    row(
        "token_usage_record",
        json!({"thread_id":"thread","turn_id":"turn",
        "session_id":"session","root_turn_id":"root","response_id":id,
        "usage":usage(),"turn_token_usage":usage(),"thread_token_usage":usage()}),
    )
}
fn events(value: &Value) -> &[Value] {
    value["events"].as_array().unwrap()
}
fn replay<'a>(value: &'a Value, kind: &str, identity: &str) -> &'a Value {
    value["replayRecords"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["kind"] == kind && record["identity"] == identity)
        .unwrap()
}
fn event_replay<'a>(value: &'a Value, event: &Value) -> &'a Value {
    replay(value, "event", event["eventId"].as_str().unwrap())
}
fn sha256(input: &[u8]) -> String {
    Sha256::digest(input)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn error(records: &[Value], expected: &str) {
    assert_eq!(
        parse_incremental_rollout(&bytes(records), &options())
            .unwrap_err()
            .0,
        expected
    );
}

#[test]
fn fallback_messages_injections_and_unknown_thread_calls_are_append_stable() {
    let first = vec![
        message(None, "first"),
        typed(None, PRIVATE),
        call("call", false),
    ];
    let before = parse(&first);
    let mut appended = first;
    appended.push(message(None, "second"));
    let after = parse(&appended);
    assert_ne!(before["fingerprint"], after["fingerprint"]);
    for event in events(&before) {
        let matching = events(&after)
            .iter()
            .find(|next| next["eventId"] == event["eventId"])
            .unwrap();
        assert_eq!(event, matching);
        assert_eq!(event_replay(&before, event), event_replay(&after, matching));
    }
    let mut other_options = options();
    other_options["streamId"] = json!("stream-b");
    let other = parse_incremental_rollout(&bytes(&appended), &other_options).unwrap();
    assert!(
        events(&after)
            .iter()
            .zip(events(&other))
            .all(|(a, b)| a["eventId"] != b["eventId"])
    );
}

#[test]
fn fallback_identity_uses_physical_line_even_when_envelope_ordinals_repeat_or_change() {
    let mut first = message(None, "same");
    first["ordinal"] = json!(7);
    let mut second = first.clone();
    second["ordinal"] = json!(8);
    assert_eq!(
        events(&parse(&[first.clone()]))[0]["eventId"],
        events(&parse(&[second]))[0]["eventId"]
    );
    let both = parse(&[first.clone(), first]);
    assert_ne!(events(&both)[0]["eventId"], events(&both)[1]["eventId"]);
}

#[test]
fn explicit_identities_deduplicate_copied_streams_and_keep_earliest_positions() {
    let records = vec![
        meta(),
        context(),
        message(Some("item"), PRIVATE),
        typed(Some("typed"), PRIVATE),
        call("call", true),
        output("call", PRIVATE),
        token("response"),
    ];
    let original = parse(&records);
    let mut copied = vec![row("future", json!({}))];
    copied.extend(records.clone());
    copied.extend(records);
    let mut opts = options();
    opts["streamId"] = json!("copy");
    let duplicate = parse_incremental_rollout(&bytes(&copied), &opts).unwrap();
    assert_eq!(events(&original), events(&duplicate));
    assert_eq!(original["responseTokens"], duplicate["responseTokens"]);
    assert_eq!(duplicate["eventPositions"], json!([4, 5, 6, 7]));
    assert_eq!(duplicate["responsePositions"], json!([8]));
    for record in original["replayRecords"].as_array().unwrap() {
        let next = replay(
            &duplicate,
            record["kind"].as_str().unwrap(),
            record["identity"].as_str().unwrap(),
        );
        assert_eq!(record["digest"], next["digest"]);
        assert_eq!(
            next["line"].as_u64().unwrap(),
            record["line"].as_u64().unwrap() + 1
        );
    }
}

#[test]
fn appended_skill_output_has_separate_proof_at_output_line() {
    let before = parse(&[meta(), context(), call("call", true)]);
    let after = parse(&[
        meta(),
        context(),
        call("call", true),
        output("call", PRIVATE),
    ]);
    assert_eq!(before["eventPositions"], json!([3]));
    assert_eq!(after["eventPositions"], json!([3, 4]));
    assert_eq!(
        event_replay(&before, &events(&before)[0]),
        event_replay(&after, &events(&after)[0])
    );
    assert_eq!(events(&after)[1]["evidenceType"], "successful_skill_read");
    let new: Vec<_> = events(&after)
        .iter()
        .zip(after["eventPositions"].as_array().unwrap())
        .filter(|(_, line)| line.as_u64().unwrap() > before["completeLines"].as_u64().unwrap())
        .collect();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].0["eventType"], "skill_loaded");
}

#[test]
fn tool_output_capture_cannot_change_immutable_call_fingerprint() {
    let mut opts = options();
    opts["captureContent"] = json!(true);
    let before =
        parse_incremental_rollout(&bytes(&[meta(), call("ordinary", false)]), &opts).unwrap();
    let after = parse_incremental_rollout(
        &bytes(&[
            meta(),
            call("ordinary", false),
            output("ordinary", "result"),
        ]),
        &opts,
    )
    .unwrap();
    assert_ne!(events(&before)[0]["content"], events(&after)[0]["content"]);
    assert_eq!(
        event_replay(&before, &events(&before)[0]),
        event_replay(&after, &events(&after)[0])
    );
}

#[test]
fn out_of_order_output_replay_is_stable_when_later_duplicate_proves_skill_read() {
    let before = parse(&[meta(), output("call", PRIVATE), call("call", true)]);
    assert_eq!(events(&before).len(), 1);
    let mut later = output("call", PRIVATE);
    later["timestamp"] = json!("2026-10-03T07:00:00Z");
    let after = parse(&[meta(), output("call", PRIVATE), call("call", true), later]);
    assert_eq!(after["eventPositions"], json!([3, 4]));
    assert_eq!(events(&after)[1]["occurredAt"], "2026-10-03T07:00:00.000Z");
    let output_replay = before["replayRecords"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "tool_output")
        .unwrap();
    assert_eq!(
        output_replay,
        replay(
            &after,
            "tool_output",
            output_replay["identity"].as_str().unwrap()
        )
    );
    assert_eq!(output_replay["line"], 2);
    assert_eq!(after["replayRecords"].as_array().unwrap().len(), 3);
}

#[test]
fn replay_digests_ignore_import_time_and_capture_setting_but_not_private_evidence() {
    let records = [
        meta(),
        context(),
        message(Some("message"), PRIVATE),
        typed(Some("typed"), PRIVATE),
        call("call", true),
        output("call", PRIVATE),
        token("response"),
    ];
    let uncaptured = parse(&records);
    assert!(!uncaptured.to_string().contains(PRIVATE));
    let mut opts = options();
    opts["captureContent"] = json!(true);
    opts["observedAt"] = json!("2026-10-04T00:00:00Z");
    let captured = parse_incremental_rollout(&bytes(&records), &opts).unwrap();
    assert_eq!(uncaptured["replayRecords"], captured["replayRecords"]);
    assert!(!captured["replayRecords"].to_string().contains(PRIVATE));
    assert!(!captured["replayRecords"].to_string().contains("never/read"));
    for entry in uncaptured["replayRecords"].as_array().unwrap() {
        let digest = entry["digest"].as_str().unwrap();
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
        assert_eq!(entry.as_object().unwrap().len(), 4);
    }
    for changed_index in [2, 3, 5] {
        let mut changed = records.clone();
        let serialized = changed[changed_index]
            .to_string()
            .replace(PRIVATE, "CHANGED_PRIVATE");
        changed[changed_index] = serde_json::from_str(&serialized).unwrap();
        let changed = parse(&changed);
        assert_ne!(uncaptured["replayRecords"], changed["replayRecords"]);
    }
}

#[test]
fn conflicting_explicit_messages_and_typed_bodies_fail_even_without_capture() {
    error(
        &[
            meta(),
            message(Some("same"), "one"),
            message(Some("same"), "two"),
        ],
        "rollout_conflicting_event_identity",
    );
    error(
        &[
            meta(),
            typed(Some("same"), "one"),
            typed(Some("same"), "two"),
        ],
        "rollout_conflicting_event_identity",
    );
    let mut later = message(Some("same"), "one");
    later["timestamp"] = json!("2026-10-03T07:00:00Z");
    error(
        &[meta(), message(Some("same"), "one"), later],
        "rollout_conflicting_event_identity",
    );
}

#[test]
fn conflicting_calls_outputs_tokens_and_context_are_atomic_errors() {
    let mut changed = call("same", true);
    changed["payload"]["arguments"] = json!("{}");
    error(
        &[meta(), call("same", true), changed],
        "rollout_conflicting_call_identity",
    );
    error(
        &[
            meta(),
            call("same", true),
            output("same", "one"),
            output("same", "two"),
        ],
        "rollout_conflicting_output_identity",
    );
    error(
        &[output("same", "one"), output("same", "two")],
        "rollout_conflicting_output_identity",
    );
    let mut changed = token("same");
    changed["payload"]["usage"]["output_tokens"] = json!(99);
    error(
        &[token("same"), changed],
        "rollout_conflicting_response_identity",
    );
    let mut changed_context = context();
    changed_context["payload"]["model"] = json!("model-b");
    error(
        &[
            meta(),
            context(),
            call("same", true),
            changed_context.clone(),
            call("same", true),
        ],
        "rollout_conflicting_call_identity",
    );
    error(
        &[
            meta(),
            context(),
            message(Some("same"), "body"),
            changed_context,
            message(Some("same"), "body"),
        ],
        "rollout_conflicting_event_identity",
    );
}

#[test]
fn complete_prefix_hash_and_positions_include_blank_lines_crlf_and_original_bom() {
    let mut input = vec![0xef, 0xbb, 0xbf];
    input.extend_from_slice(b"\r\n\n");
    input.extend(bytes(&[message(None, "body")]));
    let complete = input.clone();
    input.extend_from_slice(b"{\"unfinished\":");
    let result = parse_incremental_rollout(&input, &options()).unwrap();
    assert_eq!(result["completeBytes"], complete.len());
    assert_eq!(result["completeLines"], 3);
    assert_eq!(result["eventPositions"], json!([3]));
    assert_eq!(result["recordsSeen"], 1);
    assert_eq!(result["fingerprint"], sha256(&complete));
    assert_eq!(result["deferredBytes"], input.len() - complete.len());
    assert!(
        !result["warningCodes"]
            .as_array()
            .unwrap()
            .contains(&json!("unterminated_final_line"))
    );
}

#[test]
fn empty_whitespace_and_unterminated_valid_or_partial_records_are_noops() {
    let valid = message(None, "body").to_string();
    for input in [
        b"".as_slice(),
        b"\n \n",
        valid.as_bytes(),
        b"{\"unfinished\":",
        b"\xef\xbb",
    ] {
        let result = parse_incremental_rollout(input, &options()).unwrap();
        assert_eq!(result["recordsSeen"], 0);
        assert!(events(&result).is_empty());
        assert_eq!(result["replayRecords"], json!([]));
        assert_eq!(result["eventPositions"], json!([]));
        assert_eq!(result["responsePositions"], json!([]));
    }
    let mut terminated = valid.into_bytes();
    terminated.push(b'\n');
    assert_eq!(
        parse_incremental_rollout(&terminated, &options()).unwrap()["recordsSeen"],
        1
    );
}

#[test]
fn incomplete_utf8_tail_is_deferred_but_complete_invalid_utf8_or_json_is_rejected() {
    let mut input = bytes(&[message(None, "body")]);
    let complete_len = input.len();
    input.extend_from_slice(b"{\"text\":\"\xf0\x9f");
    let result = parse_incremental_rollout(&input, &options()).unwrap();
    assert_eq!(result["completeBytes"], complete_len);
    assert_eq!(result["recordsSeen"], 1);
    input.push(b'\n');
    assert_eq!(
        parse_incremental_rollout(&input, &options()).unwrap_err().0,
        "rollout_invalid_utf8"
    );
    let mut input = bytes(&[message(None, "body")]);
    input.extend_from_slice(b"{invalid}\n");
    assert_eq!(
        parse_incremental_rollout(&input, &options()).unwrap_err().0,
        "rollout_invalid_json"
    );
}

#[test]
fn byte_line_and_depth_limits_include_the_deferred_tail() {
    let mut opts = options();
    opts["limits"] = json!({"maxBytes":3});
    assert_eq!(
        parse_incremental_rollout(b"1234", &opts).unwrap_err().0,
        "rollout_byte_limit"
    );
    opts["limits"] = json!({"maxLineBytes":3});
    assert_eq!(
        parse_incremental_rollout(b"\n1234", &opts).unwrap_err().0,
        "rollout_line_byte_limit"
    );
    opts["limits"] = json!({"maxLines":1});
    assert_eq!(
        parse_incremental_rollout(b"\ntail", &opts).unwrap_err().0,
        "rollout_line_count_limit"
    );
    assert!(parse_incremental_rollout(b"\n", &opts).is_ok());
    opts["limits"] = json!({"maxDepth":2});
    assert_eq!(
        parse_incremental_rollout(b"[[[", &opts).unwrap_err().0,
        "rollout_depth_limit"
    );
    assert!(parse_incremental_rollout(b"\"[[[[[[", &opts).is_ok());
    assert_eq!(
        parse_incremental_rollout(&vec![b'x'; MAX_BYTES + 1], &options())
            .unwrap_err()
            .0,
        "rollout_byte_limit"
    );
    assert_eq!(
        parse_incremental_rollout(&vec![b'x'; MAX_LINE_BYTES + 1], &options())
            .unwrap_err()
            .0,
        "rollout_line_byte_limit"
    );
    assert_eq!(
        parse_incremental_rollout(&vec![b'\n'; MAX_LINES + 1], &options())
            .unwrap_err()
            .0,
        "rollout_line_count_limit"
    );
    assert_eq!(
        parse_incremental_rollout(&[b'['; MAX_DEPTH + 1], &options())
            .unwrap_err()
            .0,
        "rollout_depth_limit"
    );
}

#[test]
fn limits_only_tighten_and_event_response_caps_remain_enforced() {
    for (name, max) in [
        ("maxBytes", MAX_BYTES),
        ("maxLineBytes", MAX_LINE_BYTES),
        ("maxLines", MAX_LINES),
        ("maxDepth", MAX_DEPTH),
        ("maxEvents", MAX_EVENTS),
        ("maxResponseTokens", MAX_RESPONSE_TOKENS),
    ] {
        for value in [json!(0), json!(max + 1), json!(1.5), json!("1")] {
            let mut opts = options();
            opts["limits"] = json!({name:value});
            assert_eq!(
                parse_incremental_rollout(b"", &opts).unwrap_err().0,
                "rollout_invalid_limits",
                "{name}"
            );
        }
    }
    let mut opts = options();
    opts["limits"] = json!({"maxEvents":1});
    assert_eq!(
        parse_incremental_rollout(&bytes(&[message(None, "one"), message(None, "two")]), &opts)
            .unwrap_err()
            .0,
        "rollout_event_limit"
    );
    opts["limits"] = json!({"maxResponseTokens":1});
    assert_eq!(
        parse_incremental_rollout(&bytes(&[token("one"), token("two")]), &opts)
            .unwrap_err()
            .0,
        "rollout_response_limit"
    );
}

#[test]
fn projection_size_and_content_depth_bounds_are_preserved() {
    let large: Vec<_> = (0..16)
        .map(|i| message(Some(&format!("item-{i}")), &"x".repeat(150 * 1024)))
        .collect();
    assert!(parse_incremental_rollout(&bytes(&large), &options()).is_ok());
    let mut opts = options();
    opts["captureContent"] = json!(true);
    assert_eq!(
        parse_incremental_rollout(&bytes(&large), &opts)
            .unwrap_err()
            .0,
        "rollout_projection_limit"
    );
    let mut deep = call("deep", false);
    deep["payload"]["arguments"] = json!("[".repeat(32) + "0" + &"]".repeat(32));
    assert_eq!(
        parse_incremental_rollout(&bytes(&[deep]), &opts)
            .unwrap_err()
            .0,
        "rollout_projection_limit"
    );
}

#[test]
fn replay_ledger_cannot_exceed_the_fixed_twenty_thousand_record_bound() {
    let mut records: Vec<_> = (0..MAX_LINES - 1)
        .map(|i| output(&format!("orphan-{i}"), "body"))
        .collect();
    let mut two = typed(Some("mixed"), PRIVATE);
    two["payload"]["content"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"input_text","text":"ordinary"}));
    two["payload"]["internal_chat_message_metadata_passthrough"]["content_item_kinds"] =
        json!(["skills.selected_skill_instructions", null]);
    records.push(two);
    let input = bytes(&records);
    assert!(input.len() < MAX_BYTES);
    assert_eq!(
        parse_incremental_rollout(&input, &options()).unwrap_err().0,
        "rollout_replay_limit"
    );
}

#[test]
fn stream_id_is_required_and_separate_versions_do_not_change_snapshot_behavior() {
    for stream in [
        Value::Null,
        json!(""),
        json!("bad id"),
        json!("x".repeat(81)),
        json!(3),
    ] {
        let mut opts = options();
        opts["streamId"] = stream;
        assert_eq!(
            parse_incremental_rollout(b"", &opts).unwrap_err().0,
            "rollout_invalid_stream_id"
        );
    }
    let records = [meta(), message(Some("item"), "body"), token("response")];
    let incremental = parse(&records);
    assert_eq!(incremental["adapterVersion"], INCREMENTAL_ADAPTER_VERSION);
    assert_eq!(
        incremental["events"][0]["collectorVersion"],
        INCREMENTAL_ADAPTER_VERSION
    );
    assert_eq!(
        incremental["responseTokens"][0]["collectorVersion"],
        INCREMENTAL_ADAPTER_VERSION
    );
    let snapshot = parse_rollout(&bytes(&records), &options()).unwrap();
    assert_eq!(snapshot["adapterVersion"], ROLLOUT_ADAPTER_VERSION);
    assert!(snapshot.get("replayRecords").is_none());
    assert!(snapshot.get("completeBytes").is_none());
    let unterminated = message(None, "body").to_string();
    assert_eq!(
        parse_rollout(unterminated.as_bytes(), &options()).unwrap()["recordsSeen"],
        1
    );
    assert_eq!(
        parse_rollout(b"", &options()).unwrap_err().0,
        "rollout_empty_input"
    );
    let conflicts = bytes(&[
        meta(),
        message(Some("same"), "one"),
        message(Some("same"), "two"),
    ]);
    assert!(parse_rollout(&conflicts, &options()).is_ok());
}

#[test]
fn duplicate_response_snapshots_keep_first_position_and_ignore_cumulative_totals() {
    let original = token("same");
    let mut cumulative = original.clone();
    cumulative["payload"]["turn_token_usage"]["total_tokens"] = json!(100);
    cumulative["payload"]["thread_token_usage"]["total_tokens"] = json!(200);
    let parsed = parse(&[meta(), context(), original.clone(), cumulative]);
    let first = parse(&[meta(), context(), original]);
    assert_eq!(parsed["responseTokens"], first["responseTokens"]);
    assert_eq!(parsed["responsePositions"], json!([3]));
    assert_eq!(parsed["replayRecords"], first["replayRecords"]);
}

#[test]
fn repeated_tool_item_ids_and_equivalent_outputs_keep_first_eligible_proof() {
    let original_call = call("call", true);
    let mut copied_call = original_call.clone();
    copied_call["payload"]["id"] = json!("copied-call");
    let original_output = output("call", PRIVATE);
    let mut copied_output = original_output.clone();
    copied_output["payload"]["id"] = json!("copied-output");
    copied_output["timestamp"] = json!("2026-10-03T07:00:00Z");
    let first = parse(&[meta(), original_call.clone(), original_output.clone()]);
    let repeated = parse(&[
        meta(),
        original_call,
        original_output,
        copied_call,
        copied_output,
    ]);
    assert_eq!(events(&first), events(&repeated));
    assert_eq!(first["eventPositions"], repeated["eventPositions"]);
    assert_eq!(first["replayRecords"], repeated["replayRecords"]);
}

#[test]
fn replay_projection_preserves_shared_node_budget_at_twenty_thousand_records() {
    let mut input = bytes(
        &(0..MAX_LINES - 1)
            .map(|i| output(&format!("orphan-{i}"), "body"))
            .collect::<Vec<_>>(),
    );
    assert!(input.len() < MAX_BYTES);
    let accepted = parse_incremental_rollout(&input, &options()).unwrap();
    assert_eq!(
        accepted["replayRecords"].as_array().unwrap().len(),
        MAX_LINES - 1
    );
    input.extend(bytes(&[output("last-orphan", "body")]));
    assert!(input.len() < MAX_BYTES);
    // Four scalar fields plus their record object add five nodes per replay;
    // the enclosing array makes exactly 20,000 records exceed 100,000 nodes.
    assert_eq!(
        parse_incremental_rollout(&input, &options()).unwrap_err().0,
        "rollout_projection_limit"
    );
}
