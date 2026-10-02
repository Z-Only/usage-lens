//! Imported per-response evidence, kept separate from account/cumulative usage.
use super::normalize::count;
use super::validation::{
    CoreError, CoreResult, RAW_BYTES, check_size, exact_keys, identifier, nullable_identifier,
    record, timestamp,
};
use serde_json::{Map, Value, json};
use std::collections::HashSet;

pub const TOKEN_SCHEMA: &str = "a75987455a2879ca151cea5e118fa307be868583";
pub const RESPONSE_WARNINGS: [&str; 5] = [
    "Totals sum only imported per-response usage records, not account usage or cumulative turn/thread snapshots.",
    "Cached input and reasoning output are reported components; do not add them to input or output totals.",
    "The reported total is preserved, not reconstructed; token costs and skill attribution are not inferred.",
    "Zero records means no matching observed imported responses, not zero historical activity.",
    "At most 500 directly reported model groups are displayed; overall totals include all matching records.",
];
pub const RESPONSE_TOKEN_KEYS: [&str; 6] = [
    "inputTokens",
    "cachedInputTokens",
    "cacheWriteInputTokens",
    "outputTokens",
    "reasoningOutputTokens",
    "totalTokens",
];
const FIELDS: [(&str, &str); 6] = [
    ("input_tokens", "inputTokens"),
    ("cached_input_tokens", "cachedInputTokens"),
    ("cache_write_input_tokens", "cacheWriteInputTokens"),
    ("output_tokens", "outputTokens"),
    ("reasoning_output_tokens", "reasoningOutputTokens"),
    ("total_tokens", "totalTokens"),
];

pub fn response_coverage() -> Value {
    json!({
        "completeness":"partial","missingResponses":"unknown",
        "dateBasis":"reported_occurrence_else_import_time",
        "cumulativeSnapshots":"excluded","accountTotalRelationship":"not_combined",
    })
}

pub fn token_counts(input: &Value) -> CoreResult<Value> {
    let input = record(input)?;
    let mut output = Map::new();
    for (raw_name, name) in FIELDS {
        let value = if raw_name == "cache_write_input_tokens" && !input.contains_key(raw_name) {
            "0".to_owned()
        } else {
            count(input.get(raw_name).unwrap_or(&Value::Null)).ok_or(CoreError::InvalidInput)?
        };
        // Parsing decimal to i64 is exact, rejects overflow, and never coerces a float.
        value.parse::<i64>().map_err(|_| CoreError::InvalidInput)?;
        output.insert(name.into(), Value::String(value));
    }
    Ok(Value::Object(output))
}

pub fn normalize_response_token(input: &Value) -> CoreResult<Value> {
    exact_keys(
        input,
        &[
            "sourceId",
            "importedAt",
            "occurredAt",
            "model",
            "collectorVersion",
            "raw",
        ],
    )?;
    check_size(input, RAW_BYTES)?;
    let raw = &input["raw"];
    record(raw)?;
    // Pinned schema requires both snapshots. Validate them, deliberately discard them.
    token_counts(&raw["turn_token_usage"])?;
    token_counts(&raw["thread_token_usage"])?;
    Ok(json!({
        "sourceId":input["sourceId"],
        "threadId":identifier(&raw["thread_id"])? ,
        "turnId":identifier(&raw["turn_id"])? ,
        "sessionId":identifier(&raw["session_id"])? ,
        "rootTurnId":identifier(&raw["root_turn_id"])? ,
        "responseId":identifier(&raw["response_id"])? ,
        "usage":token_counts(&raw["usage"])? ,
        "importedAt":timestamp(&input["importedAt"])? ,
        "occurredAt":if input["occurredAt"].is_null() {None} else {Some(timestamp(&input["occurredAt"])?)} ,
        "model":nullable_identifier(&input["model"])? ,
        "provenance":{
            "provider":"codex_rollout","mode":"imported",
            "collectorVersion":identifier(&input["collectorVersion"])? ,
            "schemaBaseline":TOKEN_SCHEMA,"semantics":"best_effort_provider_observed_response",
        },
    }))
}

/// Import time and adapter version are observations, not immutable response identity.
pub fn immutable_response(value: &Value) -> Value {
    Value::Object(
        [
            "threadId",
            "turnId",
            "sessionId",
            "rootTurnId",
            "responseId",
            "usage",
            "occurredAt",
            "model",
        ]
        .into_iter()
        .map(|key| (key.into(), value[key].clone()))
        .collect(),
    )
}

pub fn import_metadata(input: &Value) -> CoreResult<Value> {
    exact_keys(
        input,
        &[
            "fingerprint",
            "adapterVersion",
            "sourceVersion",
            "importedAt",
            "warningCodes",
        ],
    )?;
    let fingerprint = input["fingerprint"]
        .as_str()
        .ok_or(CoreError::InvalidInput)?;
    if fingerprint.len() != 64
        || !fingerprint
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || input["sourceVersion"] != TOKEN_SCHEMA
    {
        return Err(CoreError::InvalidInput);
    }
    let warnings = input["warningCodes"]
        .as_array()
        .ok_or(CoreError::InvalidInput)?;
    if warnings.len() > 100 {
        return Err(CoreError::InvalidInput);
    }
    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for warning in warnings {
        let code = warning.as_str().ok_or(CoreError::InvalidInput)?;
        if code.is_empty()
            || code.len() > 128
            || !code.as_bytes()[0].is_ascii_lowercase()
            || !code
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(CoreError::InvalidInput);
        }
        if seen.insert(code) {
            unique.push(code);
        }
    }
    Ok(json!({
        "fingerprint":fingerprint,"adapterVersion":identifier(&input["adapterVersion"])? ,
        "sourceVersion":TOKEN_SCHEMA,"importedAt":timestamp(&input["importedAt"])? ,
        "warningCodes":unique,
    }))
}
