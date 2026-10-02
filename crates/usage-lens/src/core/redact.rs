//! Local detail only: known-secret redaction is deliberately best effort.
use super::validation::{
    CONTENT_BYTES, CoreError, CoreResult, MAX_SAFE_INTEGER, bounded_text, check_size, exact_keys,
    record,
};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::sync::LazyLock;

pub const CONTENT_WARNING: &str = "Local content redaction is best-effort; unknown secrets and sensitive information may remain. Content is excluded from aggregate/plugin queries.";

static SECRET_KEYS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r"(?i)\A(?:authorization|proxy-authorization|cookie|set-cookie|password|passwd|secret|client[_-]?secret|access[_-]?token|refresh[_-]?token|id[_-]?token|api[_-]?key|private[_-]?key|token|authToken|sessionToken|credential|credentials|.*[_-](?:secret|token|api[_-]?key|password))\z"
).unwrap()
});
static EXCLUDED_KEYS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r"(?i)\A(?:system|developer|system[_-]?prompt|developer[_-]?prompt|hidden[_-]?reasoning|chain[_-]?of[_-]?thought|reasoning|analysis|reasoning[_-]?content|reasoning[_-]?details)\z"
).unwrap()
});

static TEXT_RULES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
    (Regex::new(r"(?s)-----BEGIN (?:[A-Z ]+ )?PRIVATE KEY-----.*?-----END (?:[A-Z ]+ )?PRIVATE KEY-----").unwrap(), "[REDACTED_PRIVATE_KEY]"),
    (Regex::new(r"\b(?:sk-(?:proj-|svcacct-)?[A-Za-z0-9_-]{12,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[A-Z0-9]{16})\b").unwrap(), "[REDACTED_KEY]"),
    (Regex::new(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b").unwrap(), "[REDACTED_TOKEN]"),
    (Regex::new(r#"(?i)("(?:[^"\\]*[_-])?(?:password|passwd|secret|api[_-]?key|access[_-]?token|refresh[_-]?token|authToken|sessionToken|token|cookie|authorization)"\s*:\s*)"(?:[^"\\]|\\.)*""#).unwrap(), "${1}\"[REDACTED]\""),
    (Regex::new(r"(?i)\b(Bearer\s+)[A-Za-z0-9._~+/-]+=*").unwrap(), "${1}[REDACTED]"),
    (Regex::new(r"(?i)\b((?:[A-Z][A-Z0-9_]*_API_KEY|api[_-]?key|access[_-]?token|refresh[_-]?token|password|secret)\s*[=:]\s*)[^\s,;]+").unwrap(), "${1}[REDACTED]"),
    (Regex::new(r"(?im)^(\s*(?:cookie|set-cookie|authorization)\s*:\s*).*$").unwrap(), "${1}[REDACTED]"),
]
});

pub fn redact_text(input: &str) -> String {
    let mut value = input.to_owned();
    for (pattern, replacement) in TEXT_RULES.iter() {
        value = pattern.replace_all(&value, *replacement).into_owned();
    }
    value
}

fn serialized_nesting_exceeds_limit(text: &str) -> bool {
    let (mut nesting, mut quoted, mut escaped) = (0usize, false, false);
    for byte in text.bytes() {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    nesting += 1;
                    if nesting > 32 {
                        return true;
                    }
                }
                b'}' | b']' => nesting = nesting.saturating_sub(1),
                _ => {}
            }
        }
    }
    false
}

fn sanitize(value: &Value) -> Value {
    sanitize_at(value, 0)
}

fn sanitize_at(value: &Value, depth: usize) -> Value {
    if depth >= 32 {
        return Value::String("[EXCLUDED_NESTED_CONTENT]".to_owned());
    }
    match value {
        Value::String(text) => {
            let trimmed = text.trim_start();
            if (trimmed.starts_with('{') || trimmed.starts_with('['))
                && serialized_nesting_exceeds_limit(text)
            {
                return Value::String("[EXCLUDED_NESTED_CONTENT]".to_owned());
            }
            if (trimmed.starts_with('{') || trimmed.starts_with('['))
                && let Ok(parsed) = serde_json::from_str::<Value>(text)
            {
                return Value::String(sanitize_at(&parsed, depth + 1).to_string());
            }
            Value::String(redact_text(text))
        }
        Value::Array(items) => {
            if items.len() == 2
                && items[0]
                    .as_str()
                    .is_some_and(|key| SECRET_KEYS.is_match(key))
            {
                return json!([items[0], "[REDACTED]"]);
            }
            Value::Array(
                items
                    .iter()
                    .map(|item| sanitize_at(item, depth + 1))
                    .collect(),
            )
        }
        Value::Object(items) => {
            if ["role", "type", "eventType"].iter().any(|key| {
                items.get(*key).and_then(Value::as_str).is_some_and(|kind| {
                    matches!(
                        kind,
                        "system" | "developer" | "analysis" | "reasoning" | "hidden_reasoning"
                    )
                })
            }) {
                return Value::String("[EXCLUDED_NON_VISIBLE_CONTENT]".to_owned());
            }
            Value::Object(
                items
                    .iter()
                    .filter(|(key, _)| !EXCLUDED_KEYS.is_match(key))
                    .map(|(key, value)| {
                        (
                            key.clone(),
                            if SECRET_KEYS.is_match(key) {
                                Value::String("[REDACTED]".to_owned())
                            } else {
                                sanitize_at(value, depth + 1)
                            },
                        )
                    })
                    .collect(),
            )
        }
        // Large exact integer JSON values take the reference BigInt-to-string path.
        Value::Number(number) => {
            let text = number.to_string();
            if !text.contains(['.', 'e', 'E'])
                && number
                    .as_i64()
                    .is_none_or(|n| !(-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&n))
            {
                Value::String(text)
            } else {
                value.clone()
            }
        }
        _ => value.clone(),
    }
}

pub fn sanitize_content(input: &Value) -> CoreResult<Value> {
    let fields = record(input)?;
    exact_keys(input, &["body", "toolArguments", "toolResult", "files"])?;
    check_size(input, CONTENT_BYTES)?;
    let mut output = Map::new();
    if let Some(body) = fields.get("body") {
        let body = body.as_str().ok_or(CoreError::InvalidInput)?;
        output.insert("body".into(), Value::String(redact_text(body)));
    }
    for key in ["toolArguments", "toolResult"] {
        if let Some(value) = fields.get(key) {
            output.insert(key.into(), sanitize(value));
        }
    }
    if let Some(files) = fields.get("files") {
        let files = files.as_array().ok_or(CoreError::InvalidInput)?;
        if files.len() > 20 {
            return Err(CoreError::InvalidInput);
        }
        let result: CoreResult<Vec<Value>> = files
            .iter()
            .map(|file| {
                exact_keys(file, &["name", "content"])?;
                let name = bounded_text(&file["name"], 256)?;
                let content = file["content"].as_str().ok_or(CoreError::InvalidInput)?;
                Ok(json!({"name":redact_text(&name),"content":redact_text(content)}))
            })
            .collect();
        output.insert("files".into(), Value::Array(result?));
    }
    Ok(Value::Object(output))
}
