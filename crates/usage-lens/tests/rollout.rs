//! Synthetic regression port of tests/adapters/rollout.test.ts. No real logs or paths are read.
use serde_json::{Value, json};
use usage_lens::adapters::rollout::{
    MAX_BYTES, ROLLOUT_ADAPTER_VERSION, ROLLOUT_SOURCE_VERSION, parse_rollout,
};
const NOW: &str = "2026-10-02T06:00:00.000Z";
const WRAPPED: &str = "<skill>\n<name>Example Skill</name>\n<path>/never/read/SKILL.md</path>\n<resource_access>{\"package\":\"skill://synthetic/example\",\"main_resource\":\"skill://synthetic/example/SKILL.md\"}</resource_access>\nPRIVATE INJECTED INSTRUCTIONS\n</skill>";
fn opts() -> Value {
    json!({"sourceId":"local","observedAt":NOW,"captureContent":false,"sourceVersion":ROLLOUT_SOURCE_VERSION})
}
fn row(kind: &str, payload: Value) -> Value {
    json!({"timestamp":NOW,"type":kind,"payload":payload})
}
fn lines(records: &[Value]) -> Vec<u8> {
    let mut out = records
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out.into_bytes()
}
fn meta() -> Value {
    row(
        "session_meta",
        json!({"id":"thread","session_id":"session","cwd":"/never/open","transcript_path":"/never/open"}),
    )
}
fn context() -> Value {
    row("turn_context", json!({"turn_id":"turn","model":"model-a"}))
}
fn call_named(call_id: &str, args: Value, namespace: Value, name: &str) -> Value {
    row(
        "response_item",
        json!({"type":"function_call","namespace":namespace,"name":name,"call_id":call_id,"arguments":args.to_string()}),
    )
}
fn call_args(call_id: &str, args: Value) -> Value {
    call_named(call_id, args, json!("skills"), "read")
}
fn call() -> Value {
    call_args("read1", json!({"package":"skill://synthetic/example"}))
}
fn result_value(call_id: &str, value: Value) -> Value {
    row(
        "response_item",
        json!({"type":"function_call_output","call_id":call_id,"output":value.to_string()}),
    )
}
fn result_named(call_id: &str) -> Value {
    result_value(
        call_id,
        json!({"resource":"skill://synthetic/example/SKILL.md","contents":"PRIVATE SKILL INSTRUCTIONS","next_cursor":null}),
    )
}
fn result() -> Value {
    result_named("read1")
}
fn message(role: &str, text: &str) -> Value {
    row(
        "response_item",
        json!({"type":"message","role":role,"content":[{"type":if role=="assistant" {"output_text"} else {"input_text"},"text":text}]}),
    )
}
fn typed_kind(text: &str, kind: Value) -> Value {
    let mut record = message("user", text);
    record["payload"]["internal_chat_message_metadata_passthrough"] =
        json!({"content_item_kinds":[kind]});
    record
}
fn typed(text: &str) -> Value {
    typed_kind(text, json!("skills.selected_skill_instructions"))
}
fn usage() -> Value {
    json!({"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1,"total_tokens":13})
}
fn token() -> Value {
    row(
        "token_usage_record",
        json!({"thread_id":"thread","turn_id":"turn","session_id":"session","root_turn_id":"root","response_id":"response","usage":usage(),"turn_token_usage":usage(),"thread_token_usage":usage()}),
    )
}
fn parse(records: &[Value]) -> Value {
    parse_rollout(&lines(records), &opts()).unwrap()
}
fn capture(records: &[Value]) -> Value {
    let mut options = opts();
    options["captureContent"] = json!(true);
    parse_rollout(&lines(records), &options).unwrap()
}
fn events(parsed: &Value) -> &[Value] {
    parsed["events"].as_array().unwrap()
}
fn skills(parsed: &Value) -> Vec<&Value> {
    events(parsed)
        .iter()
        .filter(|v| v["eventType"] == "skill_loaded")
        .collect()
}
fn warning(parsed: &Value, code: &str) -> bool {
    parsed["warningCodes"]
        .as_array()
        .unwrap()
        .contains(&json!(code))
}
fn error(records: &[Value], code: &str) {
    assert_eq!(parse_rollout(&lines(records), &opts()).unwrap_err().0, code);
}

#[test]
fn matches_exact_skill_calls_and_separates_main_reads_from_injections() {
    let parsed = capture(&[meta(), context(), call(), result(), typed(WRAPPED)]);
    assert_eq!(parsed["sourceId"], "local");
    assert_eq!(parsed["adapterVersion"], ROLLOUT_ADAPTER_VERSION);
    assert_eq!(parsed["sourceVersion"], ROLLOUT_SOURCE_VERSION);
    assert_eq!(parsed["recordsSeen"], 5);
    let fingerprint = parsed["fingerprint"].as_str().unwrap();
    assert_eq!(fingerprint.len(), 64);
    assert!(
        fingerprint
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let evidence = skills(&parsed);
    assert_eq!(evidence.len(), 2);
    let read = evidence
        .iter()
        .find(|v| v["evidenceType"] == "successful_skill_read")
        .unwrap();
    for (key, value) in [
        ("skillEvidenceKind", "main_read"),
        ("skillName", "skill://synthetic/example"),
        ("status", "success"),
        ("sessionId", "session"),
        ("turnId", "turn"),
        ("model", "model-a"),
    ] {
        assert_eq!(read[key], value);
    }
    let injected = evidence
        .iter()
        .find(|v| v["evidenceType"] == "typed_skill_injection")
        .unwrap();
    assert_eq!(injected["skillEvidenceKind"], "instruction_injection");
    assert_eq!(injected["skillName"], "Example Skill");
    assert_eq!(injected["status"], "unknown");
    let serialized = parsed.to_string();
    for excluded in ["PRIVATE", "never/read", "never/open"] {
        assert!(!serialized.contains(excluded));
    }
    assert!(
        events(&parsed)
            .iter()
            .all(|v| v["eventType"] != "skill_invoked")
    );
    assert!(warning(&parsed, "skill_evidence_may_overlap"));
    assert_eq!(
        parsed["warnings"].as_array().unwrap().len(),
        parsed["warningCodes"].as_array().unwrap().len()
    );
    for code in parsed["warningCodes"].as_array().unwrap() {
        let code = code.as_str().unwrap();
        assert!(code.as_bytes()[0].is_ascii_lowercase());
        assert!(
            code.len() <= 128
                && code
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        );
    }
}

#[test]
fn mentions_catalog_lookalikes_system_developer_and_reasoning_never_prove_loads() {
    let mut hidden = message("assistant", "PRIVATE REASONING");
    hidden["payload"]["channel"] = json!("analysis");
    let parsed = capture(&[
        meta(),
        message("user", "skills.read loaded Example Skill"),
        message("assistant", "I invoked Example Skill"),
        typed_kind(WRAPPED, json!("skills.catalog")),
        message("system", WRAPPED),
        message("developer", WRAPPED),
        hidden,
        row(
            "response_item",
            json!({"type":"reasoning","summary":"PRIVATE REASONING"}),
        ),
        call_named(
            "fake",
            json!({"package":"example"}),
            json!("functions"),
            "read",
        ),
        result_value("fake", json!({"value":"ordinary output"})),
    ]);
    assert_eq!(
        events(&parsed)
            .iter()
            .map(|v| &v["eventType"])
            .collect::<Vec<_>>(),
        vec![
            &json!("user_prompt"),
            &json!("assistant_visible_message"),
            &json!("tool_call")
        ]
    );
    assert!(skills(&parsed).is_empty());
    assert!(!parsed.to_string().contains("PRIVATE"));
}

#[test]
fn cursor_and_explicit_resource_reads_are_not_main_loads() {
    let parsed = parse(&[
        meta(),
        call_args("cursor", json!({"package":"example","cursor":"page2"})),
        result_named("cursor"),
        call_args(
            "resource",
            json!({"package":"example","resource":"skill://example/SKILL.md"}),
        ),
        result_named("resource"),
        call_args(
            "null",
            json!({"package":"example","resource":null,"cursor":null}),
        ),
        result_value(
            "null",
            json!({"resource":"main","contents":"","next_cursor":"next","skill_root":"/never/follow"}),
        ),
    ]);
    assert_eq!(
        events(&parsed)
            .iter()
            .filter(|v| v["eventType"] == "tool_call")
            .count(),
        3
    );
    assert_eq!(skills(&parsed).len(), 1);
    assert!(warning(&parsed, "non_main_skill_read"));
    assert!(warning(&parsed, "partial_skill_read"));
}

#[test]
fn successful_read_requires_the_exact_matching_result_schema() {
    let cases = [
        json!("error"),
        json!({"error":"denied"}),
        json!({"resource":"main","contents":"body"}),
        json!({"resource":1,"contents":"body","next_cursor":null}),
        json!({"resource":"main","contents":false,"next_cursor":null}),
        json!({"resource":"main","contents":"body","next_cursor":null,"success":true}),
        json!({"resource":"main","contents":"body","next_cursor":null,"skill_root":1}),
        json!({"resource":"main","contents":"body","next_cursor":false}),
        Value::Null,
        json!([]),
    ];
    for value in cases {
        let parsed = parse(&[meta(), call(), result_value("read1", value)]);
        assert!(skills(&parsed).is_empty());
        assert!(warning(&parsed, "unverified_skill_read_result"));
    }
    let unmatched = parse(&[meta(), call()]);
    assert!(skills(&unmatched).is_empty());
    assert!(warning(&unmatched, "unverified_skill_read_result"));
    let mut structured = result();
    structured["payload"]["output"] =
        json!({"resource":"main","contents":"body","next_cursor":null});
    assert!(skills(&parse(&[call(), structured])).is_empty());
}

#[test]
fn malformed_read_arguments_do_not_infer_success() {
    let cases = [
        Value::Null,
        json!([]),
        json!({}),
        json!({"package":""}),
        json!({"package":2}),
        json!({"package":"example","cursor":2}),
        json!({"package":"example","resource":""}),
        json!({"package":"example","extra":true}),
        json!({"package":"x\ny"}),
        json!({"package":"x".repeat(161)}),
        json!({"package":"example","cursor":"x".repeat(1025)}),
    ];
    for args in cases {
        let parsed = parse(&[meta(), call_args("read1", args), result()]);
        assert!(warning(&parsed, "invalid_skill_read_arguments"));
        assert!(skills(&parsed).is_empty());
    }
}

#[test]
fn captures_only_opted_in_visible_content_and_excludes_all_skills_output_bodies() {
    let mut assistant = message("assistant", "visible answer");
    assistant["payload"]["channel"] = json!("final");
    let records = [
        meta(),
        message("user", "visible prompt"),
        assistant,
        call_named(
            "tool",
            json!({"path":"/never/open"}),
            Value::Null,
            "read_file",
        ),
        result_value("tool", json!({"value":"local visible result"})),
        call_named("catalog", json!({}), json!("skills"), "list"),
        result_value("catalog", json!({"body":"PRIVATE CATALOG"})),
    ];
    assert!(
        events(&parse(&records))
            .iter()
            .all(|v| v.get("content").is_none())
    );
    let parsed = capture(&records);
    let tool = events(&parsed)
        .iter()
        .find(|v| v["toolName"] == "read_file")
        .unwrap();
    assert_eq!(
        tool["content"],
        json!({"toolArguments":{"path":"/never/open"},"toolResult":{"value":"local visible result"}})
    );
    let catalog = events(&parsed)
        .iter()
        .find(|v| v["toolName"] == "skills.list")
        .unwrap();
    assert_eq!(catalog["content"], json!({"toolArguments":{}}));
    assert_eq!(events(&parsed)[0]["content"]["body"], "visible prompt");
    assert!(!parsed.to_string().contains("PRIVATE"));
}

#[test]
fn generic_string_structured_and_null_outputs_never_interpret_success() {
    let mut plain = result_named("plain");
    plain["payload"]["output"] = json!("plain text");
    let mut structured = result_named("structured");
    structured["payload"]["output"] = json!({"value":"object result"});
    let parsed = capture(&[
        meta(),
        call_named("plain", json!({}), Value::Null, "tool"),
        plain,
        call_named("structured", json!({}), json!("fs"), "read"),
        structured,
        call_named("missing", json!({}), Value::Null, "tool"),
        call_named("null", Value::Null, Value::Null, "tool"),
        result_value("null", Value::Null),
    ]);
    assert_eq!(events(&parsed)[0]["content"]["toolResult"], "plain text");
    assert_eq!(
        events(&parsed)[1]["content"]["toolResult"],
        json!({"value":"object result"})
    );
    assert_eq!(
        events(&parsed)[3]["content"],
        json!({"toolArguments":null,"toolResult":"null"})
    );
    assert!(events(&parsed).iter().all(|v| v["status"] == "unknown"));
    assert!(warning(&parsed, "unmatched_tool_call"));
}

#[test]
fn malformed_arguments_and_orphan_output_bodies_are_never_retained() {
    let mut bad = call();
    bad["payload"]["arguments"] = json!("{PRIVATE");
    let parsed = capture(&[bad, result(), result_named("orphan")]);
    for code in [
        "malformed_tool_arguments",
        "invalid_skill_read_arguments",
        "unmatched_tool_output",
        "file_scoped_tool_identity",
    ] {
        assert!(warning(&parsed, code));
    }
    assert_eq!(events(&parsed).len(), 1);
    assert_eq!(events(&parsed)[0]["content"], json!({}));
    assert!(!parsed.to_string().contains("PRIVATE"));
    let mut deeply_nested = call();
    deeply_nested["payload"]["arguments"] = json!("[".repeat(33) + &"]".repeat(33));
    assert!(warning(
        &parse(&[deeply_nested]),
        "malformed_tool_arguments"
    ));
}

#[test]
fn call_ids_deduplicate_per_thread_ignoring_item_id_and_accept_subsequent_duplicate_output() {
    let mut copy = call();
    copy["payload"]["id"] = json!("copy");
    let mut copy_output = result();
    copy_output["payload"]["id"] = json!("copy-output");
    assert_eq!(
        events(&parse(&[meta(), result(), call(), copy, copy_output])).len(),
        2
    );
    assert_eq!(
        events(&parse(&[
            meta(),
            call(),
            result(),
            row("session_meta", json!({"id":"thread2"})),
            call(),
            result()
        ]))
        .len(),
        4
    );
    let a = parse(&[meta(), call(), result()]);
    let b = parse(&[meta(), call(), result()]);
    assert_eq!(a, b);
    // Unlike a file-scoped message identity, tool identity does not depend on file bytes.
    let padded = parse(&[meta(), call(), result(), row("future_type", json!({}))]);
    assert_ne!(a["fingerprint"], padded["fingerprint"]);
    assert_eq!(events(&a), events(&padded));
}

#[test]
fn outputs_before_calls_cannot_establish_success_or_retained_result() {
    let parsed = capture(&[meta(), result(), call()]);
    assert_eq!(events(&parsed).len(), 1);
    assert!(warning(&parsed, "out_of_order_tool_output"));
    assert!(warning(&parsed, "unmatched_tool_call"));
    assert_eq!(events(&parsed)[0]["status"], "unknown");
    assert!(events(&parsed)[0]["content"].get("toolResult").is_none());
}

#[test]
fn successful_main_read_uses_first_subsequent_output_timestamp_across_midnight() {
    let mut started = call();
    started["timestamp"] = json!("2026-10-01T23:59:59Z");
    let mut ended = result();
    ended["timestamp"] = json!("2026-10-02T00:00:01Z");
    let mut duplicate = result();
    duplicate["timestamp"] = json!("2026-10-02T01:00:00Z");
    let parsed = parse(&[meta(), started, ended, duplicate]);
    assert_eq!(events(&parsed)[0]["occurredAt"], "2026-10-01T23:59:59.000Z");
    assert_eq!(skills(&parsed)[0]["occurredAt"], "2026-10-02T00:00:01.000Z");
}

#[test]
fn conflicting_tool_ids_atomically_reject_the_whole_batch() {
    error(
        &[
            meta(),
            call(),
            call_args("read1", json!({"package":"different"})),
        ],
        "rollout_conflicting_call_identity",
    );
    error(
        &[
            meta(),
            call(),
            result(),
            result_value(
                "read1",
                json!({"resource":"main","contents":"different","next_cursor":null}),
            ),
        ],
        "rollout_conflicting_output_identity",
    );
    // An error result is never replaced by a later incompatible success.
    error(
        &[
            call(),
            result_value("read1", json!({"error":"denied"})),
            result(),
        ],
        "rollout_conflicting_output_identity",
    );
}

#[test]
fn stable_item_id_and_content_index_deduplicate_injections_and_isolate_mixed_content() {
    let mut mixed = message("user", "not used");
    mixed["payload"]["id"] = json!("item");
    mixed["payload"]["content"] = json!([{"type":"input_text","text":WRAPPED},{"type":"input_text","text":"ordinary user text"},{"type":"input_text","text":"PRIVATE CATALOG"}]);
    mixed["payload"]["internal_chat_message_metadata_passthrough"] =
        json!({"content_item_kinds":["skills.selected_skill_instructions",null,"skills.catalog"]});
    let parsed = capture(&[meta(), mixed.clone(), mixed]);
    assert_eq!(events(&parsed).len(), 2);
    assert_eq!(events(&parsed)[1]["content"]["body"], "ordinary user text");
    assert!(!parsed.to_string().contains("PRIVATE"));
    assert_eq!(events(&parse(&[typed(WRAPPED), typed(WRAPPED)])).len(), 2);
    assert_eq!(
        events(&parse(&[typed(&WRAPPED.replace("Example Skill", "Name"))]))[0]["skillName"],
        "Name"
    );
}

#[test]
fn incorrect_typed_content_alignment_fails_closed() {
    for metadata in [
        json!({}),
        json!({"content_item_kinds":[]}),
        json!("bad"),
        json!({"content_item_kinds":"bad"}),
    ] {
        let mut record = message("user", WRAPPED);
        record["payload"]["internal_chat_message_metadata_passthrough"] = metadata;
        let parsed = capture(&[record]);
        assert!(events(&parsed).is_empty());
        assert!(warning(&parsed, "invalid_typed_content_alignment"));
        assert!(!parsed.to_string().contains("PRIVATE"));
    }
}

#[test]
fn malformed_typed_wrappers_are_excluded_without_claiming_a_load() {
    for wrapper in [
        WRAPPED.replace("<name>", "prefix<name>"),
        WRAPPED.replace("</skill>", ""),
        "<skill>\n<name>Example</name>\n<path></path>\n</skill>".into(),
        WRAPPED.replace("Example Skill", &"x".repeat(257)),
        WRAPPED.replace("/never/read/SKILL.md", &"x".repeat(4097)),
        WRAPPED.replace("Example Skill", "bad\tname"),
    ] {
        let parsed = capture(&[typed(&wrapper)]);
        assert!(events(&parsed).is_empty());
        assert!(warning(&parsed, "invalid_skill_injection"));
    }
    // The generated header may be followed directly by the close tag.
    assert_eq!(
        skills(&parse(&[typed(
            "<skill>\n<name>Header</name>\n<path>/inert</path></skill>"
        )]))
        .len(),
        1
    );
}

#[test]
fn assistant_typed_annotations_and_wrong_content_part_kinds_are_not_visible_messages() {
    let mut assistant = message("assistant", WRAPPED);
    assistant["payload"]["internal_chat_message_metadata_passthrough"] =
        json!({"content_item_kinds":["skills.selected_skill_instructions"]});
    assert!(events(&capture(&[assistant])).is_empty());
    let mut image = typed(WRAPPED);
    image["payload"]["content"][0]["type"] = json!("image");
    assert!(warning(
        &parse(&[typed(WRAPPED), image]),
        "invalid_skill_injection"
    ));
    let mut empty = message("user", "");
    empty["payload"]["content"] = json!([{"type":"image"},null,{"type":"input_text","text":123}]);
    assert!(events(&parse(&[empty])).is_empty());
    let mut no_text = typed(WRAPPED);
    no_text["payload"]["content"][0]["text"] = Value::Null;
    assert!(warning(&parse(&[no_text]), "invalid_skill_injection"));
}

#[test]
fn per_response_counts_are_lossless_and_cumulative_duplicate_changes_do_not_recount() {
    let mut huge = token();
    huge["payload"]["usage"]["input_tokens"] = serde_json::from_str("9007199254740993").unwrap();
    huge["payload"]["usage"]["total_tokens"] = serde_json::from_str("9007199254740996").unwrap();
    let parsed = parse(&[meta(), context(), huge]);
    let token = &parsed["responseTokens"][0];
    assert_eq!(token["model"], "model-a");
    assert_eq!(token["importedAt"], NOW);
    assert_eq!(token["occurredAt"], NOW);
    assert_eq!(token["raw"]["usage"]["input_tokens"], "9007199254740993");
    assert_eq!(token["raw"]["usage"]["total_tokens"], "9007199254740996");
    assert_eq!(token["raw"]["usage"]["reasoning_output_tokens"], "1");
    assert_eq!(token["raw"]["usage"]["cache_write_input_tokens"], "0");
    let mut duplicate = self::token();
    duplicate["payload"]["thread_token_usage"]["total_tokens"] = json!(999);
    assert_eq!(
        parse(&[self::token(), duplicate])["responseTokens"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for field in ["turn_id", "thread_id"] {
        let mut different = self::token();
        different["payload"][field] = json!("different");
        assert!(parse(&[meta(), context(), different])["responseTokens"][0]["model"].is_null());
    }
    assert!(parse(&[self::token()])["responseTokens"][0]["model"].is_null());
}

#[test]
fn response_identity_requires_every_field_and_rejects_conflicting_immutable_values() {
    for field in [
        "thread_id",
        "turn_id",
        "session_id",
        "root_turn_id",
        "response_id",
        "usage",
        "turn_token_usage",
        "thread_token_usage",
    ] {
        let mut missing = token();
        missing["payload"].as_object_mut().unwrap().remove(field);
        assert!(
            parse_rollout(&lines(&[missing]), &opts()).is_err(),
            "{field}"
        );
    }
    let mut conflict = token();
    conflict["payload"]["usage"]["output_tokens"] = json!(4);
    conflict["payload"]["usage"]["total_tokens"] = json!(14);
    error(
        &[token(), conflict],
        "rollout_conflicting_response_identity",
    );
    for field in ["turn_id", "root_turn_id"] {
        let mut conflict = token();
        conflict["payload"][field] = json!("changed");
        error(
            &[token(), conflict],
            "rollout_conflicting_response_identity",
        );
    }
    let mut new_time = token();
    new_time["timestamp"] = json!("2026-10-02T07:00:00Z");
    error(
        &[token(), new_time],
        "rollout_conflicting_response_identity",
    );
    let mut another_session = token();
    another_session["payload"]["session_id"] = json!("other");
    assert_eq!(
        parse(&[token(), another_session])["responseTokens"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn token_counts_reject_negative_fractional_noncanonical_missing_and_i64_overflow() {
    for value in [
        json!(-1),
        json!(0.5),
        json!("01"),
        json!("90071992547409999999999"),
        Value::Null,
        json!(true),
        json!({}),
        json!("9223372036854775808"),
    ] {
        let mut record = token();
        record["payload"]["usage"]["input_tokens"] = value;
        error(&[record], "rollout_invalid_token_usage");
    }
    for field in [
        "input_tokens",
        "cached_input_tokens",
        "output_tokens",
        "reasoning_output_tokens",
        "total_tokens",
    ] {
        let mut record = token();
        record["payload"]["usage"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        error(&[record], "rollout_invalid_token_usage");
    }
    let mut max = token();
    max["payload"]["usage"]["input_tokens"] = serde_json::from_str("9223372036854775807").unwrap();
    assert_eq!(
        parse(&[max])["responseTokens"][0]["raw"]["usage"]["input_tokens"],
        "9223372036854775807"
    );
    let mut bad_default = token();
    bad_default["payload"]["usage"]["cache_write_input_tokens"] = Value::Null;
    error(&[bad_default], "rollout_invalid_token_usage");
}

#[test]
fn legacy_snapshots_and_unknown_records_never_invent_response_usage() {
    let parsed = capture(&[
        row(
            "event_msg",
            json!({"type":"token_count","info":{"total_token_usage":usage(),"last_token_usage":usage()}}),
        ),
        row("event_msg", json!({"type":"other"})),
        row("event_msg", Value::Null),
        row("future_type", json!({"contents":"PRIVATE"})),
    ]);
    assert!(parsed["responseTokens"].as_array().unwrap().is_empty());
    assert!(events(&parsed).is_empty());
    assert!(warning(&parsed, "unsupported_legacy_token_count"));
    assert!(warning(&parsed, "unknown_record_type"));
    assert!(!parsed.to_string().contains("PRIVATE"));
}

#[test]
fn model_attribution_is_direct_and_session_fallback_is_metadata_only() {
    let parsed = parse(&[
        row(
            "session_meta",
            json!({"id":"thread","forked_from_id":"parent"}),
        ),
        row("turn_context", json!({"model":"model-a"})),
        message("user", "visible"),
    ]);
    assert_eq!(events(&parsed)[0]["sessionId"], "thread");
    assert_eq!(events(&parsed)[0]["model"], "model-a");
    assert!(events(&parsed)[0]["turnId"].is_null());
    assert!(warning(&parsed, "forked_history"));
    assert!(warning(
        &parse(&[
            meta(),
            row(
                "turn_context",
                json!({"turn_id":"turn","model":"bad\nmodel"})
            ),
            token()
        ]),
        "invalid_model_metadata"
    ));
    assert!(
        events(&parse(&[
            meta(),
            row("turn_context", json!({"turn_id":null})),
            message("user", "visible")
        ]))[0]["model"]
            .is_null()
    );
    let reset = parse(&[
        meta(),
        context(),
        row("session_meta", json!({"id":"new-thread"})),
        message("user", "visible"),
    ]);
    assert!(events(&reset)[0]["model"].is_null());
    assert!(events(&reset)[0]["turnId"].is_null());
    for key in ["parent_thread_id", "forked_from_ordinal_exclusive"] {
        let mut record = meta();
        record["payload"][key] = json!(0);
        assert!(warning(&parse(&[record]), "forked_history"));
    }
}

#[test]
fn byte_input_crlf_whitespace_ordinals_and_fractional_rfc3339_are_supported() {
    let mut record = message("user", "a \\\"quoted\\\" [body]");
    record["ordinal"] = json!(1);
    record["timestamp"] = json!("2026-10-02T07:00:00.123456789+01:00");
    let input = "\n".to_owned()
        + &String::from_utf8(lines(&[record]))
            .unwrap()
            .replace('\n', "\r\n");
    let parsed = parse_rollout(input.as_bytes(), &opts()).unwrap();
    assert_eq!(parsed["recordsSeen"], 1);
    assert_eq!(events(&parsed)[0]["occurredAt"], "2026-10-02T06:00:00.123Z");
    let mut input = lines(&[message("user", "one")]);
    input.pop();
    assert!(warning(
        &parse_rollout(&input, &opts()).unwrap(),
        "unterminated_final_line"
    ));
    let input = [
        b"\xef\xbb\xbf".as_slice(),
        &lines(&[message("user", "BOM")]),
    ]
    .concat();
    assert_eq!(parse_rollout(&input, &opts()).unwrap()["recordsSeen"], 1);
    let mut offset = message("user", "offset");
    offset["timestamp"] = json!("2026-10-02T01:00:00-05:00");
    assert_eq!(events(&parse(&[offset]))[0]["occurredAt"], NOW);
}

#[test]
fn malformed_or_truncated_envelopes_fail_atomically() {
    let cases = [
        ("", "rollout_empty_input"),
        ("\n  \n", "rollout_empty_input"),
        ("{\"truncated\":", "rollout_invalid_json"),
        ("{}", "rollout_invalid_envelope"),
        ("[]", "rollout_invalid_envelope"),
        ("null", "rollout_invalid_envelope"),
    ];
    for (input, code) in cases {
        assert_eq!(
            parse_rollout(input.as_bytes(), &opts()).unwrap_err().0,
            code
        );
    }
    for (record, code) in [
        (
            json!({"type":"response_item","timestamp":NOW}),
            "rollout_invalid_envelope",
        ),
        (
            row("response_item", Value::Null),
            "rollout_invalid_response_item",
        ),
        (
            row("session_meta", Value::Null),
            "rollout_invalid_session_metadata",
        ),
        (
            row("turn_context", json!([])),
            "rollout_invalid_turn_context",
        ),
        (
            row("token_usage_record", Value::Null),
            "rollout_invalid_token_record",
        ),
    ] {
        error(&[record], code);
    }
    let mut content = message("user", "x");
    content["payload"]["content"] = json!({});
    error(&[content], "rollout_invalid_message_content");
    let mut ordinal = row("future", json!({}));
    ordinal["ordinal"] = json!(-1);
    error(&[ordinal], "rollout_invalid_ordinal");
    for timestamp in [
        "bad",
        "2026-02-31T00:00:00Z",
        "2026-10-02T24:00:00Z",
        "2026-10-02T00:60:00Z",
        "2026-10-02T00:00:60Z",
        "2026-10-02T00:00:00+25:00",
        "2026-10-02T00:00:00.1234567890Z",
    ] {
        let mut record = row("future", json!({}));
        record["timestamp"] = json!(timestamp);
        error(&[record], "rollout_invalid_timestamp");
    }
}

#[test]
fn rejects_bad_source_declarations_options_identifiers_and_call_encodings() {
    let mut unsupported = opts();
    unsupported["sourceVersion"] = json!("unknown");
    for options in [unsupported, Value::Null] {
        assert_eq!(
            parse_rollout(&lines(&[meta()]), &options).unwrap_err().0,
            "rollout_unsupported_source_version"
        );
    }
    for (key, value) in [
        ("captureContent", json!("yes")),
        ("limits", json!([])),
        ("limits", Value::Null),
    ] {
        let mut options = opts();
        options[key] = value;
        assert_eq!(
            parse_rollout(&lines(&[meta()]), &options).unwrap_err().0,
            "rollout_invalid_options"
        );
    }
    for (key, value) in [
        ("sourceId", "bad/id"),
        ("observedAt", "2026-10-02T06:00:00+00:00"),
    ] {
        let mut options = opts();
        options[key] = json!(value);
        assert_eq!(
            parse_rollout(&lines(&[meta()]), &options).unwrap_err().0,
            "invalid_input"
        );
    }
    assert_eq!(
        parse_rollout(&[0xc3, 0x28], &opts()).unwrap_err().0,
        "rollout_invalid_utf8"
    );
    error(
        &[row("session_meta", json!({"id":"bad\nid"}))],
        "rollout_invalid_identifier",
    );
    let mut malformed = call();
    malformed["payload"]["arguments"] = json!({});
    error(&[malformed], "rollout_invalid_tool_arguments_encoding");
    for field in ["namespace", "name", "call_id", "id"] {
        let mut record = call();
        record["payload"][field] = json!("bad\n");
        error(&[record], "rollout_invalid_identifier");
    }
    let too_long = call_named("long", json!({}), json!("a".repeat(100)), &"b".repeat(100));
    error(&[too_long], "rollout_invalid_identifier");
}

#[test]
fn hard_limits_only_tighten_and_enforce_every_projection_boundary() {
    let mut second = token();
    second["payload"]["response_id"] = json!("second");
    for (limits, records, code) in [
        (json!({"maxBytes":1}), vec![meta()], "rollout_byte_limit"),
        (
            json!({"maxLineBytes":1}),
            vec![meta()],
            "rollout_line_byte_limit",
        ),
        (
            json!({"maxLines":1}),
            vec![meta(), context()],
            "rollout_line_count_limit",
        ),
        (json!({"maxDepth":1}), vec![meta()], "rollout_depth_limit"),
        (
            json!({"maxEvents":1}),
            vec![meta(), call(), result()],
            "rollout_event_limit",
        ),
        (
            json!({"maxResponseTokens":1}),
            vec![token(), second],
            "rollout_response_limit",
        ),
    ] {
        let mut options = opts();
        options["limits"] = limits;
        assert_eq!(
            parse_rollout(&lines(&records), &options).unwrap_err().0,
            code
        );
    }
    for limits in [
        json!({"maxBytes":MAX_BYTES+1}),
        json!({"maxLines":0}),
        json!({"maxDepth":1.5}),
        json!({"madeUp":1}),
        json!({"maxLines":"1"}),
        json!({"maxDepth":null}),
    ] {
        let mut options = opts();
        options["limits"] = limits;
        assert_eq!(
            parse_rollout(&lines(&[meta()]), &options).unwrap_err().0,
            "rollout_invalid_limits"
        );
    }
    let unfinished = [lines(&[meta()]), b"{\"unfinished\"".to_vec()].concat();
    assert_eq!(
        parse_rollout(&unfinished, &opts()).unwrap_err().0,
        "rollout_invalid_json"
    );
    for number in ["1e999", "9007199254740993.0", "9.007199254740993e15"] {
        let input = format!(
            "{{\"timestamp\":\"{NOW}\",\"type\":\"future\",\"payload\":{{\"value\":{number}}}}}\n"
        );
        assert_eq!(
            parse_rollout(input.as_bytes(), &opts()).unwrap_err().0,
            "rollout_invalid_json"
        );
    }
}

#[test]
fn warning_and_event_order_preserve_the_reference_insertion_order() {
    let parsed = parse(&[
        row("future", json!({})),
        call(),
        message("user", "one"),
        result(),
        typed(WRAPPED),
    ]);
    assert_eq!(
        events(&parsed)
            .iter()
            .map(|v| v["eventType"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["user_prompt", "skill_loaded", "tool_call", "skill_loaded"]
    );
    assert_eq!(
        parsed["warningCodes"],
        json!([
            "imported_unverified_partial",
            "skill_evidence_may_overlap",
            "copied_histories_may_overlap",
            "unknown_record_type",
            "file_scoped_tool_identity"
        ])
    );
}

#[test]
fn record_paths_and_instruction_claims_are_inert_unverified_data() {
    let parsed = parse(&[
        meta(),
        context(),
        typed(WRAPPED),
        row(
            "future",
            json!({"tool":"execute","instructions":"PRIVATE","path":"/synthetic/private/transcripts"}),
        ),
    ]);
    assert_eq!(skills(&parsed).len(), 1);
    assert!(warning(&parsed, "imported_unverified_partial"));
    assert!(!parsed.to_string().contains("/synthetic/private"));
    assert!(!parsed.to_string().contains("PRIVATE"));
    assert_eq!(skills(&parsed)[0]["status"], "unknown");
}

#[test]
fn stable_id_golden_values_match_the_typescript_reference() {
    // These hashes were produced by the source TypeScript parser on synthetic fixtures.
    let mut injection = typed(WRAPPED);
    injection["payload"]["id"] = json!("item");
    let mut answer = message("assistant", "visible");
    answer["payload"]["id"] = json!("answer");
    let parsed = capture(&[meta(), call(), result(), injection, answer]);
    let ids: Vec<_> = events(&parsed)
        .iter()
        .map(|v| v["eventId"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "injection-8f6e5df0769720fa7d44b853a3fec061bea7b5c5b574c780c3faf51a15633715",
            "message-eff77d17749fb58b051980bf3e3820d4b84c980e18b3cdcbb20ce94238fc4e32",
            "tool-858e506d296e2d6f34d37d8698cfdcd3b84cc614c565a39637d60e6809e8cb00",
            "skill-858e506d296e2d6f34d37d8698cfdcd3b84cc614c565a39637d60e6809e8cb00",
        ]
    );
}

#[test]
fn equivalent_structured_outputs_have_stable_signatures_and_missing_output_is_not_retained() {
    let mut first = result();
    first["payload"]["output"] =
        serde_json::from_str(r#"{"b":[1.0,0.1,-0,9007199254740993],"a":null}"#).unwrap();
    let mut copy = result();
    copy["payload"]["output"] =
        serde_json::from_str(r#"{"a":null,"b":[1,0.10,0,9007199254740993]}"#).unwrap();
    let parsed = capture(&[
        call_named("read1", json!({}), Value::Null, "ordinary"),
        first,
        copy,
    ]);
    assert_eq!(events(&parsed).len(), 1);
    assert_eq!(
        events(&parsed)[0]["content"]["toolResult"]["b"][3].to_string(),
        "9007199254740993"
    );
    let mut missing = result();
    missing["payload"].as_object_mut().unwrap().remove("output");
    let parsed = capture(&[
        call_named("read1", json!({}), Value::Null, "ordinary"),
        missing.clone(),
    ]);
    assert_eq!(events(&parsed)[0]["content"], json!({"toolArguments":{}}));
    let mut present_null = missing.clone();
    present_null["payload"]["output"] = Value::Null;
    error(
        &[missing, present_null],
        "rollout_conflicting_output_identity",
    );
}

#[test]
fn timezone_offsets_can_cross_the_four_digit_year_boundary_losslessly() {
    for (timestamp, expected) in [
        ("9999-12-31T23:00:00-01:00", "+010000-01-01T00:00:00.000Z"),
        ("0000-01-01T00:00:00+01:00", "-000001-12-31T23:00:00.000Z"),
    ] {
        let mut record = message("user", "synthetic");
        record["timestamp"] = json!(timestamp);
        assert_eq!(events(&parse(&[record]))[0]["occurredAt"], expected);
    }
}

#[test]
fn parsed_bundle_imports_atomically_once_and_preserves_separate_evidence_categories() {
    use usage_lens::core::UsageStore;
    let now = chrono::DateTime::parse_from_rfc3339(NOW)
        .unwrap()
        .timestamp_millis();
    let store = UsageStore::with_clock_ms(":memory:", now).unwrap();
    store.create_source(&json!({"id":"local","displayName":"Synthetic importer test","mode":"imported","provider":"user_import","coverageDescription":"Synthetic only"})).unwrap();
    let parsed = parse(&[meta(), context(), token(), call(), result(), typed(WRAPPED)]);
    let mut batch = parsed.clone();
    batch.as_object_mut().unwrap().remove("warnings");
    batch.as_object_mut().unwrap().remove("recordsSeen");
    batch["importedAt"] = json!(NOW);
    let inserted = store.import_rollout(&batch).unwrap();
    assert_eq!(inserted["eventsInserted"], "3");
    assert_eq!(inserted["responseTokensInserted"], "1");
    assert_eq!(
        store.import_rollout(&batch).unwrap()["importAlreadyPresent"],
        true
    );
    assert_eq!(store.get_status().unwrap()["eventCount"], "3");
    assert_eq!(
        store
            .get_response_token_usage(&json!({"sourceId":"local"}))
            .unwrap()["responseCount"],
        "1"
    );
    let overview = store.get_overview(&json!({"sourceId":"local"})).unwrap();
    for kind in ["main_read", "instruction_injection"] {
        assert!(
            overview["events"]["skills"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["evidenceKind"] == kind && v["count"] == "1")
        );
    }
    for event in events(&parsed) {
        assert!(
            store
                .get_local_event_detail(&json!({"sourceId":"local","eventId":event["eventId"]}))
                .unwrap()["content"]
                .is_null()
        );
    }
}
