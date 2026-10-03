//! Pure, bounded projection of an explicitly selected, unverified JSONL buffer.
//! This module never discovers files, follows embedded paths, or executes content.
use super::AdapterError;
use crate::core::{normalize::count, validation};
use chrono::{DateTime, Datelike, Timelike, Utc};
use regex::Regex;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::LazyLock};

pub const ROLLOUT_SOURCE_VERSION: &str = "a75987455a2879ca151cea5e118fa307be868583";
pub const ROLLOUT_ADAPTER_VERSION: &str = "usage-lens-rollout/0.1.0";
pub const INCREMENTAL_ADAPTER_VERSION: &str = "usage-lens-rollout-incremental/0.1.0";
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_LINE_BYTES: usize = 256 * 1024;
pub const MAX_LINES: usize = 20000;
pub const MAX_DEPTH: usize = 32;
pub const MAX_EVENTS: usize = 1000;
pub const MAX_RESPONSE_TOKENS: usize = 1000;
const LIMITS: [(&str, usize); 6] = [
    ("maxBytes", MAX_BYTES),
    ("maxLineBytes", MAX_LINE_BYTES),
    ("maxLines", MAX_LINES),
    ("maxDepth", MAX_DEPTH),
    ("maxEvents", MAX_EVENTS),
    ("maxResponseTokens", MAX_RESPONSE_TOKENS),
];
const TOKEN_KEYS: [&str; 6] = [
    "input_tokens",
    "cached_input_tokens",
    "cache_write_input_tokens",
    "output_tokens",
    "reasoning_output_tokens",
    "total_tokens",
];
const WARNING_TEXT: [(&str, &str); 19] = [
    (
        "partial_skill_read",
        "A successful main skill read returned a continuation cursor; it proves a main-read observation, not that the entire skill was loaded.",
    ),
    (
        "out_of_order_tool_output",
        "A tool output preceded its matching call and was not used as success or result evidence.",
    ),
    (
        "imported_unverified_partial",
        "Imported local records are unverified, user-editable evidence with partial coverage; they do not prove service-verified usage or successful tasks.",
    ),
    (
        "skill_evidence_may_overlap",
        "Skill read and instruction-injection evidence are separate observations that can overlap; their sum is not a unique skill-load or invocation count.",
    ),
    (
        "copied_histories_may_overlap",
        "Copied or forked histories may overlap. Missing stable identities are scoped to this file, not globally deduplicated.",
    ),
    (
        "unterminated_final_line",
        "The final line has no newline terminator; only a complete valid JSON record is accepted.",
    ),
    (
        "forked_history",
        "This file declares forked or inherited history; overlapping observations may remain across different thread identities.",
    ),
    (
        "invalid_model_metadata",
        "Invalid or missing direct turn model metadata was left unknown.",
    ),
    (
        "unsupported_legacy_token_count",
        "Legacy token_count snapshots are unsupported for response totals and are never summed or mixed with response-token records.",
    ),
    (
        "unknown_record_type",
        "Unknown rollout record types were skipped; coverage may be incomplete.",
    ),
    (
        "file_scoped_tool_identity",
        "Tool records without preceding session metadata use file-scoped identities.",
    ),
    (
        "invalid_typed_content_alignment",
        "Message content with invalid typed-content alignment was excluded.",
    ),
    (
        "invalid_skill_injection",
        "Malformed typed skill instructions were excluded without claiming a skill load.",
    ),
    (
        "malformed_tool_arguments",
        "A tool call has malformed JSON arguments; no skill evidence was inferred.",
    ),
    (
        "unmatched_tool_call",
        "Unmatched tool calls were retained without claiming success or a skill load.",
    ),
    (
        "invalid_skill_read_arguments",
        "Invalid skills.read arguments did not establish a skill load.",
    ),
    (
        "unverified_skill_read_result",
        "A skills.read call had no matching schema-valid successful result; no skill load was claimed.",
    ),
    (
        "non_main_skill_read",
        "Resource and continuation reads are tool evidence only; they do not add main skill-load observations.",
    ),
    (
        "unmatched_tool_output",
        "Unmatched tool outputs were excluded without inferring calls or skill loads.",
    ),
];

#[derive(Clone)]
struct Position {
    line: usize,
    time: String,
    thread: String,
    session: Option<String>,
    turn: Option<String>,
    model: Option<String>,
    identity: String,
}
struct Call {
    position: Position,
    payload: Value,
    signature: String,
}
struct Output {
    line: usize,
    time: String,
    value: Option<Value>,
    signature: String,
}

fn hash(bytes: impl AsRef<[u8]>) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn id(value: &Value) -> Result<String, AdapterError> {
    validation::identifier(value).map_err(|_| AdapterError("rollout_invalid_identifier"))
}
fn text(value: &Value, max: usize) -> bool {
    validation::bounded_text(value, max).is_ok()
}
fn warn(warnings: &mut Vec<&'static str>, code: &'static str) {
    if !warnings.contains(&code) {
        warnings.push(code);
    }
}

// The reference serializes large integer lexemes as decimal strings for stable identities.
// Map keys are sorted explicitly: serde_json also supports preserve_order elsewhere.
fn stable(value: &Value) -> String {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut entries: Vec<_> = map.iter().collect();
                entries.sort_by_key(|(a, _)| *a);
                Value::Object(
                    entries
                        .into_iter()
                        .map(|(k, v)| (k.clone(), sorted(v)))
                        .collect(),
                )
            }
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            Value::Number(n)
                if n.as_f64().is_some_and(|n| {
                    n.abs() > validation::MAX_SAFE_INTEGER as f64 && n.fract() == 0.0
                }) =>
            {
                Value::String(n.to_string())
            }
            Value::Number(n) => {
                let number = n.as_f64().expect("validated finite JSON number");
                if number.fract() == 0.0 {
                    json!(number as i64)
                } else {
                    json!(number)
                }
            }
            _ => value.clone(),
        }
    }
    sorted(value).to_string()
}
fn time(value: &Value) -> Result<String, AdapterError> {
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\A[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]{1,9})?(?:Z|[+-][0-9]{2}:[0-9]{2})\z").unwrap()
    });
    let value = value
        .as_str()
        .filter(|s| PATTERN.is_match(s))
        .ok_or(AdapterError("rollout_invalid_timestamp"))?;
    if &value[11..13] > "23" || &value[14..16] > "59" || &value[17..19] > "59" {
        return Err(AdapterError("rollout_invalid_timestamp"));
    }
    let date = DateTime::parse_from_rfc3339(value)
        .map_err(|_| AdapterError("rollout_invalid_timestamp"))?
        .with_timezone(&Utc);
    let year = if (0..=9999).contains(&date.year()) {
        format!("{:04}", date.year())
    } else {
        format!("{:+07}", date.year())
    };
    Ok(format!(
        "{year}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        date.month(),
        date.day(),
        date.hour(),
        date.minute(),
        date.second(),
        date.timestamp_subsec_millis()
    ))
}
/// Bound nesting before parsing, including nested tool JSON. Embedded paths stay inert.
fn check_depth(input: &[u8], max_depth: usize) -> Result<(), AdapterError> {
    let (mut depth, mut quoted, mut escaped) = (0_i32, false, false);
    for &byte in input {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else if byte == b'"' {
            quoted = true;
        } else if byte == b'{' || byte == b'[' {
            depth += 1;
            if depth > max_depth as i32 {
                return Err(AdapterError("rollout_depth_limit"));
            }
        } else if byte == b'}' || byte == b']' {
            depth -= 1;
        }
    }
    Ok(())
}

fn parse_json(input: &str, max_depth: usize) -> Result<Value, AdapterError> {
    check_depth(input.as_bytes(), max_depth)?;
    let value: Value =
        serde_json::from_str(input).map_err(|_| AdapterError("rollout_invalid_json"))?;
    fn numbers(value: &Value) -> bool {
        match value {
            Value::Number(n) => {
                let spelling = n.to_string();
                n.as_f64().is_some_and(|v| {
                    v.is_finite()
                        && (v.fract() != 0.0
                            || v.abs() <= validation::MAX_SAFE_INTEGER as f64
                            || !spelling.contains(['.', 'e', 'E']))
                })
            }
            Value::Array(items) => items.iter().all(numbers),
            Value::Object(map) => map.values().all(numbers),
            _ => true,
        }
    }
    if !numbers(&value) {
        return Err(AdapterError("rollout_invalid_json"));
    }
    Ok(value)
}
fn counts(value: &Value) -> Result<Value, AdapterError> {
    let map = value
        .as_object()
        .ok_or(AdapterError("rollout_invalid_token_usage"))?;
    let mut result = Map::new();
    for key in TOKEN_KEYS {
        let number = if key == "cache_write_input_tokens" && !map.contains_key(key) {
            Some("0".to_owned())
        } else {
            count(&value[key])
        };
        let number = number
            .filter(|n| n.parse::<i64>().is_ok())
            .ok_or(AdapterError("rollout_invalid_token_usage"))?;
        result.insert(key.into(), Value::String(number));
    }
    Ok(Value::Object(result))
}
fn read_result(value: &Value) -> bool {
    value.as_object().is_some_and(|m| {
        m.keys()
            .all(|k| ["resource", "contents", "next_cursor", "skill_root"].contains(&k.as_str()))
    }) && value["resource"].is_string()
        && value["contents"].is_string()
        && value
            .get("next_cursor")
            .is_some_and(|v| v.is_null() || v.is_string())
        && value.get("skill_root").is_none_or(Value::is_string)
}
/// Only aligned typed instructions are inspected; their body is never retained.
fn injection(value: &str) -> Option<String> {
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"\A<skill>\n<name>([^<>\r\n]+)</name>\n<path>([^<>\r\n]+)</path>(?:\n|</skill>)",
        )
        .unwrap()
    });
    let captures = PATTERN.captures(value)?;
    let name = captures.get(1)?.as_str();
    let path = captures.get(2)?.as_str();
    (value.ends_with("</skill>") && text(&json!(name), 256) && text(&json!(path), 4096))
        .then(|| name.to_owned())
}
fn common(position: &Position, source: &str, observed_at: &str) -> Value {
    json!({"sourceId":source,"observedAt":observed_at,"occurredAt":position.time,
        "collectorVersion":ROLLOUT_ADAPTER_VERSION,"sessionId":position.session,"turnId":position.turn,"model":position.model})
}
fn add_event(
    events: &mut Vec<Value>,
    indexes: &mut HashMap<String, usize>,
    event: Value,
    limit: usize,
) -> Result<(), AdapterError> {
    let key = event["eventId"]
        .as_str()
        .expect("constructed event ID")
        .to_owned();
    if let Some(index) = indexes.get(&key) {
        events[*index] = event;
    } else {
        indexes.insert(key, events.len());
        events.push(event);
    }
    if events.len() > limit {
        return Err(AdapterError("rollout_event_limit"));
    }
    Ok(())
}
fn extend(mut value: Value, fields: Value) -> Value {
    value
        .as_object_mut()
        .expect("constructed event")
        .extend(fields.as_object().expect("constructed fields").clone());
    value
}

/// Incremental evidence contains only bounded identities, hashes, and physical line numbers.
/// Content and import-time settings never become part of a replay identity.
#[derive(Default)]
struct IncrementalEvidence {
    records: Vec<Value>,
    indexes: HashMap<(String, String), usize>,
    event_lines: HashMap<String, usize>,
    response_lines: Vec<usize>,
}
impl IncrementalEvidence {
    fn record(
        &mut self,
        kind: &str,
        identity: &str,
        immutable: &Value,
        line: usize,
        conflict: &'static str,
    ) -> Result<bool, AdapterError> {
        let key = (kind.to_owned(), identity.to_owned());
        let digest = hash(stable(immutable));
        if let Some(index) = self.indexes.get(&key) {
            if self.records[*index]["digest"] != digest {
                return Err(AdapterError(conflict));
            }
            return Ok(false);
        }
        if self.records.len() >= MAX_LINES {
            return Err(AdapterError("rollout_replay_limit"));
        }
        self.indexes.insert(key, self.records.len());
        self.records
            .push(json!({"kind":kind,"identity":identity,"digest":digest,"line":line}));
        Ok(true)
    }
}

fn immutable_position(position: &Position) -> Value {
    json!({"thread":position.thread,"occurredAt":position.time,
        "sessionId":position.session,"turnId":position.turn,"model":position.model})
}

// Snapshot parsing intentionally retains its existing last-wins behavior. Incremental
// replay instead rejects changed evidence and keeps the first equivalent observation.
fn add_projected_event(
    events: &mut Vec<Value>,
    indexes: &mut HashMap<String, usize>,
    event: Value,
    limit: usize,
    incremental: &mut Option<IncrementalEvidence>,
    evidence: (Value, usize),
) -> Result<(), AdapterError> {
    if let Some(incremental) = incremental {
        let identity = event["eventId"].as_str().expect("constructed event ID");
        if !incremental.record(
            "event",
            identity,
            &evidence.0,
            evidence.1,
            "rollout_conflicting_event_identity",
        )? {
            return Ok(());
        }
        incremental
            .event_lines
            .insert(identity.to_owned(), evidence.1);
    }
    add_event(events, indexes, event, limit)
}

/// Re-project a bounded, caller-selected stream from its beginning. Only newline-
/// terminated records are eligible; the final partial line is deferred verbatim.
/// Callers retain the complete prefix hash and positions to validate append-only replay.
pub fn parse_incremental_rollout(bytes: &[u8], options: &Value) -> Result<Value, AdapterError> {
    let stream = validation::source_id(&options["streamId"])
        .map_err(|_| AdapterError("rollout_invalid_stream_id"))?;
    parse_projection(bytes, options, Some(&stream))
}

/// Atomically parse one caller-selected byte buffer. No partial batch is returned on error.
/// Limits may be tightened by callers but can never exceed the hard bounds.
pub fn parse_rollout(bytes: &[u8], options: &Value) -> Result<Value, AdapterError> {
    parse_projection(bytes, options, None)
}

fn parse_projection(
    bytes: &[u8],
    options: &Value,
    stream: Option<&str>,
) -> Result<Value, AdapterError> {
    if !options.is_object() || options["sourceVersion"] != ROLLOUT_SOURCE_VERSION {
        return Err(AdapterError("rollout_unsupported_source_version"));
    }
    let source =
        validation::source_id(&options["sourceId"]).map_err(|_| AdapterError("invalid_input"))?;
    let observed_at =
        validation::timestamp(&options["observedAt"]).map_err(|_| AdapterError("invalid_input"))?;
    let capture = options["captureContent"]
        .as_bool()
        .ok_or(AdapterError("rollout_invalid_options"))?;
    let mut limits: HashMap<&str, usize> = LIMITS.into_iter().collect();
    if let Some(custom) = options.get("limits") {
        let custom = custom
            .as_object()
            .ok_or(AdapterError("rollout_invalid_options"))?;
        for (key, value) in custom {
            let max = limits
                .get_mut(key.as_str())
                .ok_or(AdapterError("rollout_invalid_limits"))?;
            let value = validation::integer(value, 1, *max as i64)
                .map_err(|_| AdapterError("rollout_invalid_limits"))?;
            *max = value as usize;
        }
    }
    if bytes.len() > limits["maxBytes"] {
        return Err(AdapterError("rollout_byte_limit"));
    }
    let complete_bytes = if stream.is_some() {
        let physical_lines = bytes.iter().filter(|byte| **byte == b'\n').count()
            + usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
        if physical_lines > limits["maxLines"] {
            return Err(AdapterError("rollout_line_count_limit"));
        }
        for line in bytes.split(|byte| *byte == b'\n').take(physical_lines) {
            if line.len() > limits["maxLineBytes"] {
                return Err(AdapterError("rollout_line_byte_limit"));
            }
            check_depth(line, limits["maxDepth"])?;
        }
        bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1)
    } else {
        bytes.len()
    };
    let complete_lines = bytes[..complete_bytes]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count();
    let decoded = std::str::from_utf8(&bytes[..complete_bytes])
        .map_err(|_| AdapterError("rollout_invalid_utf8"))?;
    // TextDecoder's reference behavior drops only an initial UTF-8 BOM; hashing uses original bytes.
    let decoded = decoded.strip_prefix('\u{feff}').unwrap_or(decoded);
    let fingerprint = hash(&bytes[..complete_bytes]);
    let mut incremental = stream.map(|_| IncrementalEvidence::default());
    let mut warnings = vec![
        "imported_unverified_partial",
        "skill_evidence_may_overlap",
        "copied_histories_may_overlap",
    ];
    let mut events = Vec::new();
    let mut event_indexes = HashMap::new();
    let mut calls: Vec<(String, Call)> = Vec::new();
    let mut call_indexes = HashMap::new();
    let mut outputs: HashMap<String, Output> = HashMap::new();
    let mut tokens = Vec::new();
    let mut token_signatures: HashMap<String, String> = HashMap::new();
    let identity_scope = stream
        .map(|stream| hash(stable(&json!(["stream", stream]))))
        .unwrap_or_else(|| fingerprint.clone());
    let mut thread = format!("unknown-{identity_scope}");
    let (mut session, mut turn, mut model) = (None, None, None);
    let mut records_seen = 0;
    let mut lines: Vec<_> = decoded.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    } else if !decoded.is_empty() {
        warn(&mut warnings, "unterminated_final_line");
    }
    if lines.len() > limits["maxLines"] {
        return Err(AdapterError("rollout_line_count_limit"));
    }
    for (index, line) in lines.into_iter().enumerate() {
        if line.len() > limits["maxLineBytes"] {
            return Err(AdapterError("rollout_line_byte_limit"));
        }
        if line.trim().is_empty() {
            continue;
        }
        let envelope = parse_json(line, limits["maxDepth"])?;
        if !envelope.is_object()
            || !text(&envelope["type"], 128)
            || envelope.get("payload").is_none()
        {
            return Err(AdapterError("rollout_invalid_envelope"));
        }
        let occurred_at = time(&envelope["timestamp"])?;
        let ordinal = match envelope.get("ordinal") {
            Some(value) => count(value).ok_or(AdapterError("rollout_invalid_ordinal"))?,
            None => (index + 1).to_string(),
        };
        records_seen += 1;
        let payload = &envelope["payload"];
        match envelope["type"].as_str().expect("validated type") {
            "session_meta" => {
                if !payload.is_object() {
                    return Err(AdapterError("rollout_invalid_session_metadata"));
                }
                thread = id(&payload["id"])?;
                session = Some(if payload["session_id"].is_null() {
                    thread.clone()
                } else {
                    id(&payload["session_id"])?
                });
                turn = None;
                model = None;
                if [
                    "forked_from_id",
                    "parent_thread_id",
                    "forked_from_ordinal_exclusive",
                ]
                .iter()
                .any(|k| !payload[k].is_null())
                {
                    warn(&mut warnings, "forked_history");
                }
                continue;
            }
            "turn_context" => {
                if !payload.is_object() {
                    return Err(AdapterError("rollout_invalid_turn_context"));
                }
                turn = if payload["turn_id"].is_null() {
                    None
                } else {
                    Some(id(&payload["turn_id"])?)
                };
                model = validation::identifier(&payload["model"]).ok();
                if model.is_none() {
                    warn(&mut warnings, "invalid_model_metadata");
                }
                continue;
            }
            _ => {}
        }
        let mut position = Position {
            line: index + 1,
            time: occurred_at.clone(),
            thread: thread.clone(),
            session: session.clone(),
            turn: turn.clone(),
            model: model.clone(),
            identity: if stream.is_some() {
                format!("{identity_scope}:{}", index + 1)
            } else {
                format!("{fingerprint}:{ordinal}:{}", index + 1)
            },
        };
        match envelope["type"].as_str().expect("validated type") {
            "token_usage_record" => {
                if !payload.is_object() {
                    return Err(AdapterError("rollout_invalid_token_record"));
                }
                let raw = json!({"thread_id":id(&payload["thread_id"])? ,"turn_id":id(&payload["turn_id"])? ,"session_id":id(&payload["session_id"])? ,"root_turn_id":id(&payload["root_turn_id"])? ,"response_id":id(&payload["response_id"])? ,"usage":counts(&payload["usage"])? ,"turn_token_usage":counts(&payload["turn_token_usage"])? ,"thread_token_usage":counts(&payload["thread_token_usage"])? });
                let direct_model =
                    if raw["thread_id"] == thread && raw["turn_id"].as_str() == turn.as_deref() {
                        model.clone()
                    } else {
                        None
                    };
                let identity = stable(&json!([
                    source,
                    raw["thread_id"],
                    raw["session_id"],
                    raw["response_id"]
                ]));
                let signature = stable(
                    &json!({"thread_id":raw["thread_id"],"turn_id":raw["turn_id"],"session_id":raw["session_id"],"root_turn_id":raw["root_turn_id"],"response_id":raw["response_id"],"usage":raw["usage"],"occurredAt":occurred_at,"model":direct_model}),
                );
                if let Some(incremental) = &mut incremental {
                    incremental.record(
                        "response",
                        &format!("response-{}", hash(&identity)),
                        &json!(signature),
                        index + 1,
                        "rollout_conflicting_response_identity",
                    )?;
                }
                if let Some(previous) = token_signatures.get(&identity) {
                    if previous != &signature {
                        return Err(AdapterError("rollout_conflicting_response_identity"));
                    }
                } else {
                    token_signatures.insert(identity, signature);
                    if let Some(incremental) = &mut incremental {
                        incremental.response_lines.push(index + 1);
                    }
                    tokens.push(json!({"sourceId":source,"importedAt":observed_at,"occurredAt":occurred_at,"model":direct_model,"collectorVersion":ROLLOUT_ADAPTER_VERSION,"raw":raw}));
                }
                if tokens.len() > limits["maxResponseTokens"] {
                    return Err(AdapterError("rollout_response_limit"));
                }
                continue;
            }
            "event_msg" => {
                if payload.is_object() && payload["type"] == "token_count" {
                    warn(&mut warnings, "unsupported_legacy_token_count");
                }
                continue;
            }
            "response_item" => {}
            _ => {
                warn(&mut warnings, "unknown_record_type");
                continue;
            }
        }
        if !payload.is_object() {
            return Err(AdapterError("rollout_invalid_response_item"));
        }
        if !payload["id"].is_null() {
            position.identity = id(&payload["id"])?;
        }
        let kind = payload["type"].as_str();
        if kind == Some("function_call") || kind == Some("function_call_output") {
            let key = stable(&json!([thread, id(&payload["call_id"])?]));
            let signature = if kind == Some("function_call") {
                let mut immutable = json!({"namespace":payload["namespace"],"name":payload["name"],"arguments":payload["arguments"]});
                if incremental.is_some() {
                    immutable["position"] = immutable_position(&position);
                }
                stable(&immutable)
            } else {
                let value = payload
                    .get("output")
                    .map(|v| json!({"output":v}))
                    .unwrap_or(json!({}));
                stable(&value)
            };
            if thread.starts_with("unknown-") {
                warn(&mut warnings, "file_scoped_tool_identity");
            }
            if kind == Some("function_call") {
                id(&payload["name"])?;
                if !payload["namespace"].is_null() {
                    id(&payload["namespace"])?;
                }
                if !payload["arguments"].is_string() {
                    return Err(AdapterError("rollout_invalid_tool_arguments_encoding"));
                }
                if let Some(previous) = call_indexes.get(&key).map(|i: &usize| &calls[*i].1) {
                    if previous.signature != signature {
                        return Err(AdapterError("rollout_conflicting_call_identity"));
                    }
                } else {
                    call_indexes.insert(key.clone(), calls.len());
                    calls.push((
                        key,
                        Call {
                            position,
                            payload: payload.clone(),
                            signature,
                        },
                    ));
                }
            } else {
                if let Some(incremental) = &mut incremental {
                    // Output timestamp and position describe where an identical output was
                    // copied, not different result evidence. Keep its earliest occurrence;
                    // the independent skill proof below uses the first eligible output.
                    incremental.record(
                        "tool_output",
                        &format!("output-{}", hash(&key)),
                        &json!(signature),
                        index + 1,
                        "rollout_conflicting_output_identity",
                    )?;
                }
                let previous = outputs.get(&key);
                if previous.is_some_and(|p| p.signature != signature) {
                    return Err(AdapterError("rollout_conflicting_output_identity"));
                }
                let call_line = call_indexes.get(&key).map(|i| calls[*i].1.position.line);
                if previous.is_none()
                    || call_line.is_some_and(|line| {
                        previous.is_some_and(|p| p.line < line) && index + 1 > line
                    })
                {
                    outputs.insert(
                        key,
                        Output {
                            line: index + 1,
                            time: occurred_at,
                            value: payload.get("output").cloned(),
                            signature,
                        },
                    );
                }
            }
            continue;
        }
        let role = payload["role"].as_str();
        if kind != Some("message")
            || ![Some("user"), Some("assistant")].contains(&role)
            || (role == Some("assistant")
                && !payload["channel"].is_null()
                && payload["channel"] != "final")
        {
            continue;
        }
        let content = payload["content"]
            .as_array()
            .ok_or(AdapterError("rollout_invalid_message_content"))?;
        let metadata = &payload["internal_chat_message_metadata_passthrough"];
        let kinds = metadata.get("content_item_kinds").and_then(Value::as_array);
        if !metadata.is_null() && kinds.is_none_or(|k| k.len() != content.len()) {
            warn(&mut warnings, "invalid_typed_content_alignment");
            continue;
        }
        let mut body = Vec::new();
        for (content_index, part) in content.iter().enumerate() {
            let kind = kinds.map(|k| &k[content_index]).unwrap_or(&Value::Null);
            if role == Some("user") && kind == "skills.selected_skill_instructions" {
                let name = if part.is_object() && part["type"] == "input_text" {
                    part["text"].as_str().and_then(injection)
                } else {
                    None
                };
                let Some(name) = name else {
                    warn(&mut warnings, "invalid_skill_injection");
                    continue;
                };
                let identity = format!(
                    "injection-{}",
                    hash(stable(&json!([
                        position.thread,
                        position.identity,
                        content_index
                    ])))
                );
                let event = extend(
                    common(&position, &source, &observed_at),
                    json!({"eventId":identity,"sourceEventId":identity,"eventType":"skill_loaded","evidenceType":"typed_skill_injection","skillEvidenceKind":"instruction_injection","skillName":name,"status":"unknown"}),
                );
                add_projected_event(
                    &mut events,
                    &mut event_indexes,
                    event,
                    limits["maxEvents"],
                    &mut incremental,
                    (
                        json!({"position":immutable_position(&position),
                        "kind":kind,"part":part}),
                        position.line,
                    ),
                )?;
            } else if kind.is_null()
                && part.is_object()
                && part["type"]
                    == if role == Some("user") {
                        "input_text"
                    } else {
                        "output_text"
                    }
                && let Some(text) = part["text"].as_str()
            {
                body.push(text);
            }
        }
        if body.is_empty() {
            continue;
        }
        let identity = format!(
            "message-{}",
            hash(stable(&json!([thread, position.identity, role])))
        );
        let mut event = extend(
            common(&position, &source, &observed_at),
            json!({"eventId":identity,"sourceEventId":identity,"eventType":if role==Some("user") {"user_prompt"} else {"assistant_visible_message"},"evidenceType":if role==Some("user") {"explicit_user_message"} else {"explicit_assistant_visible_message"}}),
        );
        if capture {
            event["content"] = json!({"body":body.join("\n")});
        }
        add_projected_event(
            &mut events,
            &mut event_indexes,
            event,
            limits["maxEvents"],
            &mut incremental,
            (
                json!({"position":immutable_position(&position),
                "role":role,"channel":payload["channel"],"body":body}),
                position.line,
            ),
        )?;
    }
    if records_seen == 0 && stream.is_none() {
        return Err(AdapterError("rollout_empty_input"));
    }
    for (key, call) in &calls {
        let Call {
            payload, position, ..
        } = call;
        let candidate = outputs.get(key);
        let output = candidate.filter(|output| output.line > position.line);
        if candidate.is_some() && output.is_none() {
            warn(&mut warnings, "out_of_order_tool_output");
        }
        let is_skill_read = payload["namespace"] == "skills" && payload["name"] == "read";
        let args = parse_json(
            payload["arguments"].as_str().expect("validated arguments"),
            limits["maxDepth"],
        )
        .ok();
        if args.is_none() {
            warn(&mut warnings, "malformed_tool_arguments");
        }
        let result = output
            .and_then(|o| o.value.as_ref())
            .and_then(Value::as_str)
            .map(|s| parse_json(s, limits["maxDepth"]).unwrap_or_else(|_| json!(s)));
        let tool_name = if payload["namespace"].is_null() {
            payload["name"].as_str().expect("validated name").to_owned()
        } else {
            format!(
                "{}.{}",
                payload["namespace"].as_str().expect("validated namespace"),
                payload["name"].as_str().expect("validated name")
            )
        };
        id(&json!(tool_name))?;
        let identity = format!("tool-{}", hash(key));
        let mut content = Map::new();
        if capture {
            if let Some(args) = &args {
                content.insert("toolArguments".into(), args.clone());
            }
            // Every skills output can contain instructions/catalogs; none is ordinary retained content.
            if payload["namespace"] != "skills"
                && let Some(value) = output.and_then(|o| o.value.as_ref())
            {
                content.insert(
                    "toolResult".into(),
                    result
                        .as_ref()
                        .filter(|v| !v.is_null())
                        .unwrap_or(value)
                        .clone(),
                );
            }
        }
        let mut event = extend(
            common(position, &source, &observed_at),
            json!({"eventId":identity,"sourceEventId":identity,"eventType":"tool_call","evidenceType":"explicit_tool_call","toolName":tool_name,"status":"unknown"}),
        );
        if capture {
            event["content"] = Value::Object(content);
        }
        add_projected_event(
            &mut events,
            &mut event_indexes,
            event,
            limits["maxEvents"],
            &mut incremental,
            (json!({"call":call.signature}), position.line),
        )?;
        if output.is_none() {
            warn(&mut warnings, "unmatched_tool_call");
        }
        if !is_skill_read {
            continue;
        }
        let args = args.as_ref().unwrap_or(&Value::Null);
        if !args.as_object().is_some_and(|a| {
            a.keys()
                .all(|k| ["package", "resource", "cursor"].contains(&k.as_str()))
        }) || !text(&args["package"], 160)
            || (!args["resource"].is_null() && !text(&args["resource"], 1024))
            || (!args["cursor"].is_null() && !text(&args["cursor"], 1024))
        {
            warn(&mut warnings, "invalid_skill_read_arguments");
            continue;
        }
        let result = result.as_ref().unwrap_or(&Value::Null);
        if !read_result(result) {
            warn(&mut warnings, "unverified_skill_read_result");
            continue;
        }
        if !args["resource"].is_null() || !args["cursor"].is_null() {
            warn(&mut warnings, "non_main_skill_read");
            continue;
        }
        if !result["next_cursor"].is_null() {
            warn(&mut warnings, "partial_skill_read");
        }
        let identity = format!("skill-{}", hash(key));
        let event = extend(
            common(position, &source, &observed_at),
            json!({"occurredAt":output.expect("valid result requires output").time,"eventId":identity,"sourceEventId":identity,"eventType":"skill_loaded","evidenceType":"successful_skill_read","skillEvidenceKind":"main_read","skillName":args["package"],"status":"success"}),
        );
        let proof = output.expect("valid result requires output");
        add_projected_event(
            &mut events,
            &mut event_indexes,
            event,
            limits["maxEvents"],
            &mut incremental,
            (
                json!({"call":call.signature,"output":proof.signature,
                "occurredAt":proof.time}),
                proof.line,
            ),
        )?;
    }
    if outputs.keys().any(|k| !call_indexes.contains_key(k)) {
        warn(&mut warnings, "unmatched_tool_output");
    }
    let warning_texts: Vec<_> = warnings
        .iter()
        .map(|code| {
            if stream.is_some() {
                match *code {
                    "copied_histories_may_overlap" => return "Copied or forked histories may overlap. Missing stable identities are scoped to this stream, not globally deduplicated.",
                    "file_scoped_tool_identity" => return "Tool records without preceding session metadata use stream-scoped identities.",
                    _ => {}
                }
            }
            WARNING_TEXT
                .iter()
                .find(|(key, _)| key == code)
                .expect("known warning")
                .1
        })
        .collect();
    let mut projection = json!({"sourceId":source,"fingerprint":fingerprint,
        "adapterVersion":ROLLOUT_ADAPTER_VERSION,"sourceVersion":ROLLOUT_SOURCE_VERSION,
        "importedAt":observed_at,"warningCodes":warnings});
    // Move potentially large content into the projection, then bound it before making batch copies.
    projection["events"] = Value::Array(events);
    projection["responseTokens"] = Value::Array(tokens);
    if incremental.is_some() {
        projection["adapterVersion"] = json!(INCREMENTAL_ADAPTER_VERSION);
        for collection in ["events", "responseTokens"] {
            for record in projection[collection]
                .as_array_mut()
                .expect("constructed records")
            {
                record["collectorVersion"] = json!(INCREMENTAL_ADAPTER_VERSION);
            }
        }
    }
    validation::preflight_rollout_import(&projection)
        .map_err(|_| AdapterError("rollout_projection_limit"))?;
    projection
        .as_object_mut()
        .expect("constructed projection")
        .remove("importedAt");
    projection["warnings"] = json!(warning_texts);
    projection["recordsSeen"] = json!(records_seen);
    if let Some(incremental) = incremental {
        projection["streamId"] = json!(stream.expect("incremental stream"));
        projection["completeBytes"] = json!(complete_bytes);
        projection["completeLines"] = json!(complete_lines);
        projection["deferredBytes"] = json!(bytes.len() - complete_bytes);
        projection["eventPositions"] = json!(
            projection["events"]
                .as_array()
                .expect("constructed events")
                .iter()
                .map(|event| {
                    incremental.event_lines
                        [event["eventId"].as_str().expect("constructed event ID")]
                })
                .collect::<Vec<_>>()
        );
        projection["responsePositions"] = json!(incremental.response_lines);
        projection["replayRecords"] = Value::Array(incremental.records);
        // Replay evidence shares the store's bounded metadata shape, including its
        // global node budget; MAX_LINES is a ceiling, not an exemption from it.
        validation::check_size(&projection["replayRecords"], MAX_BYTES)
            .map_err(|_| AdapterError("rollout_projection_limit"))?;
    }
    Ok(projection)
}
