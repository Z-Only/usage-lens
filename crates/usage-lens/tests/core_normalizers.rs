use serde_json::{Value, json};
use usage_lens::core::{
    normalize::{cell, count, normalize_account, normalize_rate_limits, normalize_usage},
    redact::{CONTENT_WARNING, redact_text, sanitize_content},
    response_tokens::{
        TOKEN_SCHEMA, immutable_response, import_metadata, normalize_response_token,
        response_coverage, token_counts,
    },
    validation::{
        CONTENT_BYTES, CoreError, RAW_BYTES, bounded_text, check_size, date_label, date_range,
        exact_keys, identifier, integer, source_id, timestamp,
    },
};

fn counts(total: Value) -> Value {
    json!({"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":10,"total_tokens":total})
}
fn token() -> Value {
    json!({
        "sourceId":"imported", "importedAt":"2026-10-02T00:00:00Z", "collectorVersion":"fixture-1",
        "raw":{
            "thread_id":"thread-1","turn_id":"turn-1","session_id":"session-1",
            "root_turn_id":"root-turn","response_id":"response-1","usage":counts(json!(130)),
            "turn_token_usage":counts(json!(1000)),"thread_token_usage":counts(json!(10000)),
        }
    })
}

#[test]
fn errors_only_expose_static_codes() {
    for code in [
        "invalid_input",
        "input_too_large",
        "source_exists",
        "source_not_found",
        "event_not_found",
        "capture_paused",
        "unsupported_schema",
        "store_closed",
        "response_token_conflict",
        "imported_source_required",
        "storage_error",
    ] {
        assert_eq!(CoreError::new(code).code(), code);
        assert_eq!(CoreError::new(code).to_string(), code);
    }
    assert_eq!(
        CoreError::new("UNTRUSTED_PROVIDER_CANARY").to_string(),
        "storage_error"
    );
}

#[test]
fn validates_identifiers_and_text_without_coercion() {
    assert_eq!(
        identifier(&json!("model-v1:a/b@c+d_1.2")).unwrap(),
        "model-v1:a/b@c+d_1.2"
    );
    for bad in [
        json!(null),
        json!(1),
        json!(""),
        json!("a\nb"),
        json!("trailing\n"),
        json!("a' OR 1=1 --"),
        json!("x".repeat(161)),
    ] {
        assert_eq!(identifier(&bad).unwrap_err(), CoreError::InvalidInput);
    }
    assert!(source_id(&json!("x:y_1-z.2")).is_ok());
    for bad in ["x/y", "x@y", "x+y"] {
        assert!(source_id(&json!(bad)).is_err());
    }
    assert!(source_id(&json!("x".repeat(81))).is_err());
    assert!(bounded_text(&json!("😀"), 2).is_ok());
    assert!(bounded_text(&json!("😀"), 1).is_err());
    assert!(bounded_text(&json!("a\u{7f}"), 256).is_err());
    assert!(exact_keys(&json!({"unknown":1}), &["known"]).is_err());
    assert!(exact_keys(&json!([]), &[]).is_err());
}

#[test]
fn validates_calendar_dates_and_canonical_millisecond_timestamps() {
    for bad in ["2026-02-30", "26-01-01", "2026-13-01", "2026-01-01\n"] {
        assert!(date_label(&json!(bad)).is_err(), "{bad}");
    }
    assert!(date_label(&json!("2024-02-29")).is_ok());
    for bad in [
        "2026-02-30T00:00:00Z",
        "2026-10-01T25:00:00Z",
        "2026-10-01T00:00:00+01:00",
        "2026-10-01T00:00:60Z",
        "2026-10-01T00:00:00.1234Z",
    ] {
        assert!(timestamp(&json!(bad)).is_err(), "{bad}");
    }
    for (input, expected) in [
        ("2026-10-01T00:00:00Z", "2026-10-01T00:00:00.000Z"),
        ("2026-10-01T00:00:00.1Z", "2026-10-01T00:00:00.100Z"),
        ("2026-10-01T00:00:00.12Z", "2026-10-01T00:00:00.120Z"),
    ] {
        assert_eq!(timestamp(&json!(input)).unwrap(), expected);
    }
    assert_eq!(
        date_range(&json!("2026-10-01"), &json!("2026-10-01")).unwrap(),
        ("2026-10-01".into(), "2026-10-01".into())
    );
    assert!(date_range(&json!("2026-10-02"), &json!("2026-10-01")).is_err());
    assert!(date_range(&json!("2000-01-01"), &json!("2026-10-01")).is_err());
}

#[test]
fn integer_validation_uses_exact_decimal_arithmetic() {
    for text in ["1", "1.0", "1e0", "100e-2"] {
        assert_eq!(
            integer(&serde_json::from_str(text).unwrap(), 0, 10).unwrap(),
            1
        );
    }
    assert_eq!(integer(&json!(-2), -2, 10).unwrap(), -2);
    for bad in [
        json!("1"),
        json!(1.5),
        json!(false),
        json!(9007199254740992u64),
        json!(-3),
    ] {
        assert!(integer(&bad, -2, 10).is_err());
    }
}

#[test]
fn bounds_the_whole_untrusted_tree_including_discarded_fields() {
    assert_eq!(
        normalize_usage(&Value::Null).unwrap_err(),
        CoreError::InvalidInput
    );
    assert_eq!(
        normalize_usage(&json!({"ignored":"x".repeat(RAW_BYTES + 1)})).unwrap_err(),
        CoreError::InputTooLarge
    );
    let mut deep = Value::Null;
    for _ in 0..35 {
        deep = json!({"deep":deep});
    }
    assert_eq!(
        check_size(&deep, RAW_BYTES).unwrap_err(),
        CoreError::InputTooLarge
    );
    assert_eq!(
        check_size(&json!(vec![0; 100001]), RAW_BYTES).unwrap_err(),
        CoreError::InputTooLarge
    );
    assert!(check_size(&json!({"small":true}), 100).is_ok());
}

#[test]
fn counts_are_exact_decimal_strings_and_never_lossy_floats() {
    for bad in [
        json!(-1),
        json!(1.2),
        json!(true),
        json!("01"),
        json!("-1"),
        json!("1e3"),
        json!(""),
        json!("1".repeat(129)),
        Value::Null,
    ] {
        assert_eq!(count(&bad), None, "{bad}");
    }
    for zero in [
        json!(0),
        json!(0.0),
        json!("0"),
        serde_json::from_str("-0.0").unwrap(),
    ] {
        assert_eq!(count(&zero), Some("0".into()));
    }
    for spelling in ["1.0", "1e3", "100e-2"] {
        assert!(count(&serde_json::from_str(spelling).unwrap()).is_some());
    }
    let huge = "123456789012345678901234567890";
    assert_eq!(
        count(&serde_json::from_str(huge).unwrap()),
        Some(huge.into())
    );
    assert_eq!(count(&json!("9".repeat(128))), Some("9".repeat(128)));
    assert_eq!(
        count(&serde_json::from_str::<Value>(&"9".repeat(129)).unwrap()),
        None
    );
    for spelling in ["1e1000000", "1.1e-1", "9007199254740992.0"] {
        assert_eq!(count(&serde_json::from_str(spelling).unwrap()), None);
    }
}

#[test]
fn retains_zero_null_omitted_and_invalid_as_distinct_cells() {
    let result = normalize_usage(&json!({"summary":{
        "lifetimeTokens":"123456789012345678901234567890", "peakDailyTokens":0,
        "currentStreakDays":null,"longestStreakDays":"01",
    },"dailyUsageBuckets":[{"startDate":"2026-10-01","tokens":"999999999999999999999"}]}))
    .unwrap();
    assert_eq!(
        result["summary"]["lifetimeTokens"],
        json!({"status":"reported","value":"123456789012345678901234567890"})
    );
    assert_eq!(
        result["summary"]["peakDailyTokens"],
        json!({"status":"reported","value":"0"})
    );
    assert_eq!(
        result["summary"]["currentStreakDays"]["status"],
        "not_reported"
    );
    assert_eq!(
        result["summary"]["longestRunningTurnSec"]["status"],
        "omitted"
    );
    assert_eq!(result["summary"]["longestStreakDays"]["status"], "invalid");
    assert_eq!(
        result["dailyUsageBuckets"]["value"][0]["tokens"]["value"],
        "999999999999999999999"
    );
    assert_eq!(
        normalize_usage(&json!({"summary":null})).unwrap()["summaryState"],
        "not_reported"
    );
    assert_eq!(
        normalize_usage(&json!({})).unwrap()["summaryState"],
        "omitted"
    );
    assert_eq!(
        normalize_usage(&json!({"summary":42})).unwrap()["summaryState"],
        "invalid"
    );
    assert_eq!(
        cell(&Value::Null, "a", |_| None),
        json!({"status":"omitted","value":null})
    );
}

#[test]
fn excludes_ambiguous_duplicate_and_malformed_dates() {
    let result = normalize_usage(&json!({"dailyUsageBuckets":[
        {"startDate":"2026-02-30","tokens":1},null,
        {"startDate":"2026-10-01","tokens":2},{"startDate":"2026-10-01","tokens":3},
        {"startDate":1,"tokens":-1},{"startDate":"2026-10-02","tokens":0},
    ]}))
    .unwrap();
    let statuses: Vec<_> = result["dailyUsageBuckets"]["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["startDate"]["status"].as_str().unwrap())
        .collect();
    assert_eq!(
        statuses,
        vec![
            "invalid", "omitted", "invalid", "invalid", "invalid", "reported"
        ]
    );
    assert_eq!(
        result["dailyUsageBuckets"]["value"][5]["timezoneStatus"],
        "not_documented"
    );
    assert_eq!(
        normalize_usage(&json!({"dailyUsageBuckets":vec![0;10001]})).unwrap()["dailyUsageBuckets"]
            ["status"],
        "invalid"
    );
}

#[test]
fn account_projection_drops_identity_credentials_and_unknown_fields() {
    let result = normalize_account(&json!({"account":{
        "type":"chatgpt","planType":"pro","email":"fixture@example.invalid",
        "accountId":"synthetic-account","secret":"DO_NOT_PERSIST",
    },"requiresOpenaiAuth":false,"rawBody":"DO_NOT_PERSIST"}))
    .unwrap();
    assert_eq!(
        result,
        json!({"accountState":"reported","type":{"status":"reported","value":"chatgpt"},"planType":{"status":"reported","value":"pro"},"requiresOpenaiAuth":{"status":"reported","value":false}})
    );
    assert!(!result.to_string().contains("DO_NOT_PERSIST"));
    assert_eq!(
        normalize_account(&json!({"account":{"type":"\n","planType":"x".repeat(257)}})).unwrap()["type"]
            ["status"],
        "invalid"
    );
    assert_eq!(
        normalize_account(&json!({"account":{"type":""}})).unwrap()["type"]["status"],
        "reported"
    );
}

#[test]
fn quota_multi_map_is_canonical_and_never_additive() {
    let empty = normalize_rate_limits(
        &json!({"rateLimitsByLimitId":{},"rateLimits":{"primary":{"usedPercent":30}}}),
    )
    .unwrap();
    assert_eq!(empty["bucketView"], "multi");
    assert_eq!(empty["buckets"], json!([]));
    assert_eq!(
        normalize_rate_limits(&json!({"rateLimitsByLimitId":[],"rateLimits":{}})).unwrap()["bucketView"],
        "unavailable"
    );
    assert_eq!(
        normalize_rate_limits(&json!({"rateLimitsByLimitId":null,"rateLimits":{}})).unwrap()["bucketView"],
        "legacy_fallback"
    );
    let result = normalize_rate_limits(&json!({
        "rateLimitsByLimitId":{
            "one":{"limitId":"other","primary":{"usedPercent":125.5,"windowDurationMins":300,"resetsAt":1800000000},"secondary":null,"credits":{"hasCredits":true,"unlimited":false,"balance":"12.30"},"spendControlReached":false},
            "two":{"primary":{"usedPercent":-1},"secondary":true,"credits":[]},"invalid":null,
        },"ordinaryUsageAllowed":null,"rateLimitResetCredits":{"availableCount":2,"credits":[{"id":"DROP_ME"}]},
    })).unwrap();
    let buckets = result["buckets"].as_array().unwrap();
    let one = buckets.iter().find(|b| b["bucketKey"] == "one").unwrap();
    let two = buckets.iter().find(|b| b["bucketKey"] == "two").unwrap();
    assert_eq!(one["keyMatchesLimitId"], false);
    assert_eq!(one["primary"]["value"]["usedPercent"]["value"], 125.5);
    assert_eq!(one["secondary"]["status"], "not_reported");
    assert_eq!(two["primary"]["value"]["usedPercent"]["status"], "invalid");
    assert_eq!(
        result["rateLimitResetCredits"]["value"],
        json!({"availableCount":{"status":"reported","value":"2"},"detailsState":"reported","detailRowsReturned":"1"})
    );
    assert!(!result.to_string().contains("DROP_ME"));
    assert!(result.get("total").is_none());
}

#[test]
fn redacts_known_secrets_and_excludes_nonvisible_structured_content() {
    let input = json!({
        "body":"sk-proj-abcdefghijklmnopqrst password=canary Bearer abc.def.ghi\nCookie: session=canary\n-----BEGIN PRIVATE KEY-----\ncanary\n-----END PRIVATE KEY-----",
        "toolArguments":{
            "password":"canary","api_key":"canary","nested":[
                {"authorization":"canary","value":"ghp_abcdefghijklmnopqrstuvwxyz","system_prompt":"do not store","hidden_reasoning":"do not store","reasoning":"do not store"},
                {"role":"system","content":"do not store"},{"type":"reasoning","content":"do not store"},
                {"eventType":"developer","content":"do not store"},
            ],"safe":true,"amount":serde_json::from_str::<Value>("99999999999999999999").unwrap(),
        },"toolResult":{"refresh_token":"canary","good":"visible"},
        "files":[{"name":"fixture.txt","content":"access_token=canary"}],
    });
    let result = sanitize_content(&input).unwrap();
    let serialized = result.to_string();
    for secret in [
        "canary",
        "do not store",
        "abcdefghijklmnopqrst",
        "ghp_abcdefghijklmnopqrstuvwxyz",
    ] {
        assert!(!serialized.contains(secret), "{secret}");
    }
    assert!(serialized.contains("[REDACTED"));
    assert_eq!(result["toolArguments"]["amount"], "99999999999999999999");
    assert!(CONTENT_WARNING.contains("best-effort") && CONTENT_WARNING.contains("may remain"));
    assert!(
        redact_text(
            "AKIAABCDEFGHIJKLMNOP eyJabcdefghijk.abcdef.abcdef secret=value\nAuthorization: secret"
        )
        .contains("[REDACTED")
    );
}

#[test]
fn redacts_quoted_json_strings_without_claiming_complete_secret_detection() {
    let input = r#"{"password":"SYNTHETIC_PASSWORD_CANARY","api_key":"SYNTHETIC_API_KEY_CANARY","OPENAI_API_KEY":"SYNTHETIC_OTHER_CANARY","nested":{"access_token":"SYNTHETIC_TOKEN_CANARY"},"safe":"visible"}"#;
    let result =
        sanitize_content(&json!({"body":input,"toolArguments":input,"toolResult":input})).unwrap();
    assert!(!result.to_string().contains("SYNTHETIC_"));
    assert!(result.to_string().contains("visible"));
    assert_eq!(
        redact_text(r#"{"password":"with\"escaped\\secret"}"#),
        r#"{"password":"[REDACTED]"}"#
    );
    assert_eq!(
        redact_text("unknown sensitivity may remain"),
        "unknown sensitivity may remain"
    );
}

#[test]
fn rejects_malformed_oversized_or_unexpected_local_content() {
    for bad in [
        json!({"body":1}),
        json!({"body":null}),
        json!({"files":[{"name":"x","content":1}]}),
        json!({"files":[{"name":"x","content":"a","path":"/forbidden"}]}),
        json!({"pathToRead":"/forbidden/path"}),
        json!({"files":vec![json!({"name":"a","content":"b"});21]}),
    ] {
        assert_eq!(sanitize_content(&bad).unwrap_err(), CoreError::InvalidInput);
    }
    assert!(sanitize_content(&json!({"files":[]})).is_ok());
    assert_eq!(
        sanitize_content(&json!({"body":"x".repeat(CONTENT_BYTES+1)})).unwrap_err(),
        CoreError::InputTooLarge
    );
}

#[test]
fn response_records_preserve_reported_totals_and_discard_cumulative_snapshots() {
    let mut input = token();
    input["raw"]["usage"]["total_tokens"] = json!(120);
    input["raw"]["unknown_secret"] = json!("DO_NOT_PERSIST");
    let result = normalize_response_token(&input).unwrap();
    assert_eq!(result["usage"]["totalTokens"], "120");
    assert_eq!(result["usage"]["inputTokens"], "100");
    assert_eq!(result["usage"]["cacheWriteInputTokens"], "0");
    assert_eq!(result["model"], Value::Null);
    assert_eq!(result["importedAt"], "2026-10-02T00:00:00.000Z");
    assert!(!result.to_string().contains("DO_NOT_PERSIST"));
    assert!(!result.to_string().contains("thread_token_usage"));
    assert_eq!(
        response_coverage()["accountTotalRelationship"],
        "not_combined"
    );
    input["importedAt"] = json!("2026-10-03T00:00:00Z");
    input["collectorVersion"] = json!("v2");
    assert_eq!(
        immutable_response(&result),
        immutable_response(&normalize_response_token(&input).unwrap())
    );
    input["model"] = json!("model-a");
    assert_ne!(
        immutable_response(&result),
        immutable_response(&normalize_response_token(&input).unwrap())
    );
}

#[test]
fn response_validation_enforces_required_fields_and_i64_count_bounds() {
    for key in [
        "thread_id",
        "turn_id",
        "session_id",
        "root_turn_id",
        "response_id",
        "usage",
        "turn_token_usage",
        "thread_token_usage",
    ] {
        let mut input = token();
        input["raw"].as_object_mut().unwrap().remove(key);
        assert_eq!(
            normalize_response_token(&input).unwrap_err(),
            CoreError::InvalidInput,
            "{key}"
        );
    }
    let mut input = token();
    input["secretExtra"] = json!("never saved");
    assert!(normalize_response_token(&input).is_err());
    assert_eq!(
        token_counts(&counts(json!("9223372036854775807"))).unwrap()["totalTokens"],
        "9223372036854775807"
    );
    for bad in [
        json!(-1),
        json!(null),
        json!("9223372036854775808"),
        json!("9".repeat(128)),
    ] {
        assert!(token_counts(&counts(bad)).is_err());
    }
    let mut null_cache = counts(json!(0));
    null_cache["cache_write_input_tokens"] = Value::Null;
    assert!(token_counts(&null_cache).is_err());
}

#[test]
fn import_metadata_is_strict_and_deduplicates_warning_codes_in_order() {
    let base = json!({"fingerprint":"a".repeat(64),"adapterVersion":"fixture-1","sourceVersion":TOKEN_SCHEMA,"importedAt":"2026-10-02T00:00:00Z","warningCodes":["z_warning","a_warning","z_warning"]});
    let result = import_metadata(&base).unwrap();
    assert_eq!(result["warningCodes"], json!(["z_warning", "a_warning"]));
    assert_eq!(result["importedAt"], "2026-10-02T00:00:00.000Z");
    for (key, bad) in [
        ("fingerprint", json!("A".repeat(64))),
        ("sourceVersion", json!("other")),
        ("warningCodes", json!(["private content!"])),
        ("warningCodes", json!(vec!["a"; 101])),
        ("extra", json!(true)),
    ] {
        let mut input = base.clone();
        input[key] = bad;
        assert_eq!(
            import_metadata(&input).unwrap_err(),
            CoreError::InvalidInput
        );
    }
}

#[test]
fn local_content_preserves_non_secret_numeric_values_exactly() {
    let input: Value = serde_json::from_str(r#"{"toolArguments":{"small":7,"zero":0,"negative":-12,"fraction":1.25,"huge":9007199254740993}}"#).unwrap();
    let output = sanitize_content(&input).unwrap();
    assert_eq!(output["toolArguments"]["small"], json!(7));
    assert_eq!(output["toolArguments"]["zero"], json!(0));
    assert_eq!(output["toolArguments"]["negative"], json!(-12));
    assert_eq!(output["toolArguments"]["fraction"], json!(1.25));
    assert_eq!(output["toolArguments"]["huge"], json!("9007199254740993"));
}

#[test]
fn serialized_tool_json_obeys_structured_exclusions_and_header_redaction() {
    let input = json!({"toolResult":r#"{"role":"developer","content":"SYNTHETIC_EXCLUDED_CANARY"}"#,
        "toolArguments":{"headers":[["Authorization","Basic SYNTHETIC_BASIC_CANARY"],["Accept","application/json"]]}});
    let output = sanitize_content(&input).unwrap();
    let encoded = output.to_string();
    assert!(!encoded.contains("SYNTHETIC_EXCLUDED_CANARY"));
    assert!(!encoded.contains("SYNTHETIC_BASIC_CANARY"));
    assert!(encoded.contains("application/json"));
    assert!(output["toolResult"].is_string());
    let malformed = json!({"toolResult":"{ordinary non-JSON output"});
    assert_eq!(sanitize_content(&malformed).unwrap(), malformed);
    let mut nested = json!("safe");
    for _ in 0..40 {
        nested = json!({"child":nested});
    }
    let output = sanitize_content(&json!({"toolResult":nested.to_string()})).unwrap();
    assert!(output.to_string().contains("EXCLUDED_NESTED_CONTENT"));
}

#[test]
fn serialized_nesting_limit_cannot_fall_back_to_opaque_sensitive_text() {
    let text = format!(
        "{}{{\"role\":\"developer\",\"content\":\"DEPTH_CANARY\"}}{}",
        "[".repeat(130),
        "]".repeat(130)
    );
    let output = sanitize_content(&json!({"toolResult":text})).unwrap();
    assert!(!output.to_string().contains("DEPTH_CANARY"));
    let ordinary = json!({"body":"[]{} \"", "path":"a\\b"}).to_string();
    let output = sanitize_content(&json!({"toolResult":ordinary})).unwrap();
    let parsed: Value = serde_json::from_str(output["toolResult"].as_str().unwrap()).unwrap();
    assert_eq!(parsed["body"], "[]{} \"");
    assert_eq!(parsed["path"], "a\\b");
}
