//! Explicit bounded Rollout Trace bundles. Prepared payloads are not delivery proof.
//! No discovery, subprocesses, network calls, or execution of imported data.
use super::AdapterError;
use crate::core::{redact::redact_text, validation};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::{Component, Path},
};

pub const TRACE_SOURCE_VERSION: &str = "a956835d020762cb2b570053af06f643a11c0ecc";
pub const TRACE_ADAPTER_VERSION: &str = "usage-lens-trace/0.1.0";
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const MAX_TRACE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_LINE_BYTES: usize = 256 * 1024;
pub const MAX_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_BUNDLE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_EVENTS: usize = 20_000;
pub const MAX_ATTEMPTS: usize = 1_000;
pub const MAX_PAYLOADS: usize = 2_000;
pub const MAX_DEPTH: usize = 32;
const TOKEN_KEYS: [(&str, &str); 6] = [
    ("input_tokens", "inputTokens"),
    ("cached_input_tokens", "cachedInputTokens"),
    ("cache_write_input_tokens", "cacheWriteInputTokens"),
    ("output_tokens", "outputTokens"),
    ("reasoning_output_tokens", "reasoningOutputTokens"),
    ("total_tokens", "totalTokens"),
];
type Result<T> = std::result::Result<T, AdapterError>;
fn bad(code: &'static str) -> AdapterError {
    AdapterError(code)
}
fn ensure(ok: bool, code: &'static str) -> Result<()> {
    if ok { Ok(()) } else { Err(bad(code)) }
}
fn ident(value: &Value) -> Result<String> {
    validation::identifier(value).map_err(|_| bad("trace_invalid_identity"))
}
fn timestamp(value: &Value) -> Result<String> {
    let ms = value.as_i64().ok_or(bad("trace_invalid_timestamp"))?;
    let text = DateTime::<Utc>::from_timestamp_millis(ms)
        .ok_or(bad("trace_invalid_timestamp"))?
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    validation::timestamp(&json!(text)).map_err(|_| bad("trace_invalid_timestamp"))
}
fn digest(bytes: impl AsRef<[u8]>) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn parse(bytes: &[u8]) -> Result<Value> {
    let (mut depth, mut quoted, mut escaped) = (0usize, false, false);
    for byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                quoted = false;
            }
        } else {
            match *byte {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    depth += 1;
                    ensure(depth <= MAX_DEPTH, "trace_depth_limit")?;
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    serde_json::from_slice(bytes).map_err(|_| bad("trace_invalid_json"))
}
fn metadata(parent: &Value, key: &str) -> Value {
    match parent.get(key) {
        None => json!({"state":"omitted","value":null}),
        Some(Value::Null) => json!({"state":"not_reported","value":null}),
        Some(value)
            if validation::bounded_text(value, 128).is_ok()
                && value.as_str().is_some_and(|s| redact_text(s) == s) =>
        {
            json!({"state":"reported","value":value})
        }
        _ => json!({"state":"invalid","value":null}),
    }
}
fn effort(request: &Value) -> Value {
    match request.get("reasoning") {
        Some(Value::Null) => json!({"state":"not_reported","value":null}),
        Some(v) if !v.is_object() => json!({"state":"invalid","value":null}),
        _ => metadata(&request["reasoning"], "effort"),
    }
}
fn token_cells(value: &Value) -> Value {
    let mut result = json!({});
    for (raw, key) in TOKEN_KEYS {
        result[key] = match value.get(raw) {
            None => json!({"state":"omitted","value":null}),
            Some(Value::Null) => json!({"state":"not_reported","value":null}),
            Some(value) if value.as_i64().is_some_and(|n| n >= 0) => {
                json!({"state":"reported","value":value.to_string()})
            }
            _ => json!({"state":"invalid","value":null}),
        };
    }
    result
}
/// Strict visible text allowlist. Typed/injected instructions, reasoning, tool data,
/// attachments, instructions, headers and all unknown body fields are excluded.
fn projection(items: &Value) -> Value {
    let messages: Vec<Value> = items
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let role = item["role"].as_str()?;
            if item["type"] != "message"
                || !["user", "assistant"].contains(&role)
                || item
                    .get("channel")
                    .is_some_and(|v| !v.is_null() && v != "final" && v != "commentary")
            {
                return None;
            }
            if role == "assistant"
                && item
                    .get("phase")
                    .is_some_and(|v| !v.is_null() && v != "commentary" && v != "final_answer")
            {
                return None;
            }
            let parts = item["content"].as_array()?;
            let kinds =
                item["internal_chat_message_metadata_passthrough"]["content_item_kinds"].as_array();
            if role == "assistant"
                && item
                    .get("internal_chat_message_metadata_passthrough")
                    .is_some_and(|v| {
                        !v.is_null()
                            && v.get("content_item_kinds").is_some_and(|k| {
                                !k.is_null() && k.as_array().is_none_or(|k| !k.is_empty())
                            })
                    })
            {
                return None;
            }
            if role == "user" && kinds.is_none_or(|k| k.len() != parts.len()) {
                return None;
            }
            let text = parts
                .iter()
                .enumerate()
                .filter(|(index, v)| {
                    (v["type"] == "input_text" || v["type"] == "output_text")
                        && (role == "assistant" || kinds.is_some_and(|k| k[*index] == "user.text"))
                })
                .filter_map(|(_, v)| v["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() {
                None
            } else {
                Some(json!({"role":role,"text":redact_text(&text)}))
            }
        })
        .collect();
    json!({"projection":"visible_text_only","messages":messages})
}

/// Open every path component relative to an already opened directory with no-follow.
/// This prevents intermediate-directory replacement/symlink races on Unix.
#[cfg(unix)]
fn confined_file(root: &Path, relative: &Path) -> Result<File> {
    use rustix::fs::{Mode, OFlags, open, openat};
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut dir = open("/", flags, Mode::empty()).map_err(|_| bad("trace_unsafe_path"))?;
    let all: Vec<_> = root.components().chain(relative.components()).collect();
    for part in &all[..all.len() - 1] {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                dir = openat(&dir, *name, flags, Mode::empty())
                    .map_err(|_| bad("trace_unsafe_path"))?
            }
            _ => return Err(bad("trace_unsafe_path")),
        }
    }
    let Some(Component::Normal(name)) = all.last() else {
        return Err(bad("trace_unsafe_path"));
    };
    let fd = openat(
        &dir,
        *name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| bad("trace_unsafe_path"))?;
    Ok(File::from(fd))
}
/// Hold non-share-delete directory handles while reading on Windows so checked
/// ancestors cannot be replaced. Reject all reparse points, including junctions.
#[cfg(windows)]
fn confined_file(root: &Path, relative: &Path) -> Result<File> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    ensure(
        matches!(root.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_))),
        "trace_unsafe_path",
    )?;
    let full = root.join(relative);
    let mut path = std::path::PathBuf::new();
    let mut guards = Vec::new();
    let parts: Vec<_> = full.components().collect();
    for (index, part) in parts.iter().enumerate() {
        ensure(
            matches!(
                part,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            ),
            "trace_unsafe_path",
        )?;
        path.push(part.as_os_str());
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x02200000)
            .open(&path)
            .map_err(|_| bad("trace_unsafe_path"))?;
        let meta = file.metadata().map_err(|_| bad("trace_unsafe_path"))?;
        ensure(meta.file_attributes() & 0x400 == 0, "trace_unsafe_path")?;
        if index == parts.len() - 1 {
            return Ok(file);
        }
        ensure(meta.is_dir(), "trace_unsafe_path")?;
        guards.push(file);
    }
    Err(bad("trace_unsafe_path"))
}
fn read_file(root: &Path, path: &str, max: usize) -> Result<Vec<u8>> {
    let file = confined_file(root, Path::new(path))?;
    let meta = file.metadata().map_err(|_| bad("trace_read_failed"))?;
    ensure(meta.is_file(), "trace_regular_file_required")?;
    ensure(meta.len() <= max as u64, "trace_too_large")?;
    let mut bytes = Vec::new();
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| bad("trace_read_failed"))?;
    ensure(bytes.len() <= max, "trace_too_large")?;
    Ok(bytes)
}
struct Payloads<F> {
    read: F,
    hashes: BTreeMap<String, String>,
    ids: BTreeMap<String, String>,
    bytes: usize,
}
impl<F: FnMut(&str) -> Result<Vec<u8>>> Payloads<F> {
    fn load(&mut self, reference: &Value, expected: &str) -> Result<Value> {
        ensure(
            reference.is_object() && reference["kind"]["type"] == expected,
            "trace_invalid_payload_reference",
        )?;
        let path = reference["path"]
            .as_str()
            .ok_or(bad("trace_invalid_payload_reference"))?;
        let ordinal = path
            .strip_prefix("payloads/")
            .and_then(|v| v.strip_suffix(".json"))
            .ok_or(bad("trace_unsafe_path"))?;
        ensure(
            !ordinal.is_empty()
                && ordinal.len() <= 20
                && !ordinal.starts_with('0')
                && ordinal.bytes().all(|b| b.is_ascii_digit()),
            "trace_unsafe_path",
        )?;
        ensure(
            reference["raw_payload_id"] == format!("raw_payload:{ordinal}"),
            "trace_invalid_payload_reference",
        )?;
        let id = format!("raw_payload:{ordinal}");
        ensure(
            !self.ids.contains_key(&id),
            "trace_duplicate_payload_reference",
        )?;
        ensure(self.ids.len() < MAX_PAYLOADS, "trace_payload_limit")?;
        let bytes = (self.read)(path)?;
        ensure(bytes.len() <= MAX_PAYLOAD_BYTES, "trace_too_large")?;
        self.bytes += bytes.len();
        ensure(self.bytes <= MAX_BUNDLE_BYTES, "trace_too_large")?;
        self.hashes.insert(path.into(), digest(&bytes));
        self.ids.insert(id, path.into());
        let value = parse(&bytes)?;
        ensure(value.is_object(), "trace_invalid_payload")?;
        Ok(value)
    }
}

/// Parse only explicitly supplied bytes and referenced payloads supplied by the caller.
/// Returned content is a redacted visible-text projection, never the raw trace body.
pub fn parse_trace_bundle<F: FnMut(&str) -> Result<Vec<u8>>>(
    manifest_bytes: &[u8],
    trace_bytes: &[u8],
    read_payload: F,
    options: &Value,
) -> Result<Value> {
    validation::exact_keys(options, &["sourceId", "importedAt", "sourceVersion"])
        .map_err(|_| bad("invalid_input"))?;
    let source = validation::source_id(&options["sourceId"]).map_err(|_| bad("invalid_input"))?;
    let imported =
        validation::timestamp(&options["importedAt"]).map_err(|_| bad("invalid_input"))?;
    ensure(
        options["sourceVersion"] == TRACE_SOURCE_VERSION,
        "trace_unsupported_source_version",
    )?;
    ensure(
        manifest_bytes.len() <= MAX_MANIFEST_BYTES && trace_bytes.len() <= MAX_TRACE_BYTES,
        "trace_too_large",
    )?;
    let manifest = parse(manifest_bytes)?;
    ensure(
        manifest["schema_version"] == 1
            && manifest["raw_event_log"] == "trace.jsonl"
            && manifest["payloads_dir"] == "payloads",
        "trace_invalid_manifest",
    )?;
    let trace_id = ident(&manifest["trace_id"])?;
    let rollout = ident(&manifest["rollout_id"])?;
    ident(&manifest["root_thread_id"])?;
    timestamp(&manifest["started_at_unix_ms"])?;
    let mut payloads = Payloads {
        read: read_payload,
        hashes: BTreeMap::new(),
        ids: BTreeMap::new(),
        bytes: manifest_bytes.len() + trace_bytes.len(),
    };
    let mut attempts: BTreeMap<String, Value> = BTreeMap::new();
    let mut warnings = BTreeSet::from([
        "imported_trace_unverified_partial",
        "prepared_request_not_delivery_proof",
        "visible_text_projection_only",
        "trace_attempts_not_network_request_count",
    ]);
    let (mut last, mut events) = (0u64, 0usize);
    for line in trace_bytes.split(|b| *b == b'\n') {
        if line.is_empty() {
            continue;
        }
        events += 1;
        ensure(events <= MAX_EVENTS, "trace_event_limit")?;
        ensure(line.len() <= MAX_LINE_BYTES, "trace_line_limit")?;
        let event = parse(line)?;
        ensure(
            event["schema_version"] == 1 && event["rollout_id"] == rollout,
            "trace_event_identity_mismatch",
        )?;
        let seq = event["seq"].as_u64().ok_or(bad("trace_invalid_sequence"))?;
        ensure(seq > last, "trace_invalid_sequence")?;
        if seq != last + 1 {
            warnings.insert("trace_sequence_gap");
        }
        last = seq;
        let at = timestamp(&event["wall_time_unix_ms"])?;
        for key in ["thread_id", "codex_turn_id"] {
            if !event[key].is_null() {
                ident(&event[key])?;
            }
        }
        let p = &event["payload"];
        let kind = p["type"].as_str().ok_or(bad("trace_invalid_event"))?;
        match kind {
            "inference_started" => {
                let inference = ident(&p["inference_call_id"])?;
                let thread = ident(&p["thread_id"])?;
                let turn = ident(&p["codex_turn_id"])?;
                ensure(
                    event["thread_id"] == thread && event["codex_turn_id"] == turn,
                    "trace_event_identity_mismatch",
                )?;
                ensure(
                    !attempts.contains_key(&inference),
                    "trace_duplicate_inference",
                )?;
                ensure(attempts.len() < MAX_ATTEMPTS, "trace_attempt_limit")?;
                let request = payloads.load(&p["request_payload"], "inference_request")?;
                ensure(request["input"].is_array(), "trace_invalid_request")?;
                let model = metadata(&request, "model");
                if p.get("model").is_some_and(|v| v != &request["model"]) {
                    warnings.insert("trace_request_model_mismatch");
                }
                let attempt_id = format!(
                    "trace-{}",
                    digest(json!([trace_id, rollout, inference]).to_string())
                );
                attempts.insert(inference.clone(),json!({"attemptId":attempt_id,"threadId":thread,"turnId":turn,"inferenceId":inference,"startedAt":at,"completedAt":null,"status":"incomplete","request":{"model":model,"reasoningEffort":effort(&request),"serviceTier":metadata(&request,"service_tier")},"observed":{"model":metadata(&Value::Null,"model"),"serviceTier":metadata(&Value::Null,"service_tier")},"responseId":null,"upstreamRequestId":null,"tokens":null,"requestProjection":projection(&request["input"]),"responseProjection":null,"evidence":"prepared_request"}));
            }
            "inference_completed" | "inference_failed" | "inference_cancelled" => {
                let inference = ident(&p["inference_call_id"])?;
                let attempt = attempts
                    .get_mut(&inference)
                    .ok_or(bad("trace_unmatched_terminal"))?;
                ensure(
                    attempt["status"] == "incomplete",
                    "trace_duplicate_terminal",
                )?;
                ensure(
                    event["thread_id"] == attempt["threadId"]
                        && event["codex_turn_id"] == attempt["turnId"],
                    "trace_event_identity_mismatch",
                )?;
                for key in ["response_id", "upstream_request_id"] {
                    if !p[key].is_null() {
                        ident(&p[key])?;
                    }
                }
                if at.as_str() < attempt["startedAt"].as_str().unwrap() {
                    warnings.insert("trace_clock_regression");
                }
                attempt["status"] = json!(kind.strip_prefix("inference_").unwrap());
                attempt["completedAt"] = json!(at);
                attempt["upstreamRequestId"] = p["upstream_request_id"].clone();
                let reference = if kind == "inference_completed" {
                    &p["response_payload"]
                } else {
                    &p["partial_response_payload"]
                };
                if !reference.is_null() {
                    let response = payloads.load(reference, "inference_response")?;
                    ensure(
                        response["output_items"].is_array()
                            && (response["token_usage"].is_null()
                                || response["token_usage"].is_object()),
                        "trace_invalid_response",
                    )?;
                    ensure(
                        response["upstream_request_id"] == p["upstream_request_id"],
                        "trace_response_identity_mismatch",
                    )?;
                    if kind == "inference_completed" {
                        ensure(
                            response["response_id"] == p["response_id"],
                            "trace_response_identity_mismatch",
                        )?;
                        attempt["responseId"] = p["response_id"].clone();
                        if !p["response_id"].is_null() && !response["token_usage"].is_null() {
                            attempt["tokens"] = token_cells(&response["token_usage"]);
                        }
                    }
                    if kind != "inference_completed" {
                        ensure(
                            response["response_id"].is_null() && response["token_usage"].is_null(),
                            "trace_invalid_partial_response",
                        )?;
                    }
                    attempt["responseProjection"] = projection(&response["output_items"]);
                } else {
                    ensure(
                        kind != "inference_completed",
                        "trace_invalid_payload_reference",
                    )?;
                }
            }
            _ => {
                warnings.insert("trace_non_inference_events_excluded");
            }
        }
    }
    if !trace_bytes.is_empty() && !trace_bytes.ends_with(b"\n") {
        warnings.insert("trace_unterminated_final_line");
    }
    if attempts.values().any(|v| v["status"] == "incomplete") {
        warnings.insert("trace_incomplete_inference");
    }
    let fingerprint =
        digest(json!([digest(manifest_bytes), digest(trace_bytes), payloads.hashes]).to_string());
    Ok(
        json!({"sourceId":source,"importedAt":imported,"sourceVersion":TRACE_SOURCE_VERSION,"adapterVersion":TRACE_ADAPTER_VERSION,"bundleId":trace_id,"fingerprint":fingerprint,"attempts":attempts.into_values().collect::<Vec<_>>(),"warningCodes":warnings.into_iter().collect::<Vec<_>>() }),
    )
}
/// Read one selected immutable bundle, never search for bundles or follow source paths.
pub fn read_trace_bundle(directory: &Path, options: &Value) -> Result<Value> {
    ensure(directory.is_absolute(), "absolute_import_path_required")?;
    let manifest = read_file(directory, "manifest.json", MAX_MANIFEST_BYTES)?;
    let trace = read_file(directory, "trace.jsonl", MAX_TRACE_BYTES)?;
    parse_trace_bundle(
        &manifest,
        &trace,
        |path| read_file(directory, path, MAX_PAYLOAD_BYTES),
        options,
    )
}
