use super::AdapterError;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn field(value: Option<&Value>, required: bool) -> Result<Option<&str>, AdapterError> {
    if value.is_none_or(Value::is_null) {
        return if required {
            Err(AdapterError("invalid_hook_payload"))
        } else {
            Ok(None)
        };
    }
    let text = value
        .and_then(Value::as_str)
        .ok_or(AdapterError("invalid_hook_payload"))?;
    if text.is_empty()
        || text.len() > 160
        || !text.as_bytes()[0].is_ascii_alphanumeric()
        || !text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:@/+-".contains(&b))
    {
        return Err(AdapterError("invalid_hook_payload"));
    }
    Ok(Some(text))
}
fn hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Only documented fields are used. Paths are never opened and skill names are never inferred.
pub fn normalize_hook(
    raw: &Value,
    source_id: &str,
    observed_at: &str,
    capture_content: bool,
) -> Result<Value, AdapterError> {
    let raw = raw
        .as_object()
        .ok_or(AdapterError("invalid_hook_payload"))?;
    let session =
        field(raw.get("session_id"), true)?.ok_or(AdapterError("invalid_hook_payload"))?;
    let turn = field(raw.get("turn_id"), true)?.ok_or(AdapterError("invalid_hook_payload"))?;
    let model = field(raw.get("model"), false)?;
    let mut event = json!({"sourceId":source_id,"observedAt":observed_at,"occurredAt":null,
        "collectorVersion":"usage-lens-hooks/0.1.0","sessionId":session,"turnId":turn,"model":model});
    let name = raw
        .get("hook_event_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let identity = match name {
        "PostToolUse" => {
            let tool =
                field(raw.get("tool_name"), true)?.ok_or(AdapterError("invalid_hook_payload"))?;
            let id =
                field(raw.get("tool_use_id"), true)?.ok_or(AdapterError("invalid_hook_payload"))?;
            event["eventType"] = json!("tool_call");
            event["evidenceType"] = json!("explicit_tool_call");
            event["toolName"] = json!(tool);
            event["sourceEventId"] = json!(format!(
                "hook-call-{}",
                hash(&json!([session, turn, id]).to_string())
            ));
            event["status"] = json!("unknown");
            if capture_content {
                let mut content = json!({});
                for (input, output) in [
                    ("tool_input", "toolArguments"),
                    ("tool_response", "toolResult"),
                ] {
                    if let Some(value) = raw.get(input) {
                        content[output] = value.clone();
                    }
                }
                event["content"] = content;
            }
            id.to_owned()
        }
        "UserPromptSubmit" => {
            let prompt = raw
                .get("prompt")
                .and_then(Value::as_str)
                .ok_or(AdapterError("invalid_hook_payload"))?;
            event["eventType"] = json!("user_prompt");
            event["evidenceType"] = json!("explicit_user_message");
            if capture_content {
                event["content"] = json!({"body":prompt});
            }
            "prompt".to_owned()
        }
        "Stop" => {
            let body = raw
                .get("last_assistant_message")
                .and_then(Value::as_str)
                .ok_or(AdapterError("unsupported_hook_payload"))?;
            event["eventType"] = json!("assistant_visible_message");
            event["evidenceType"] = json!("explicit_assistant_visible_message");
            if capture_content {
                event["content"] = json!({"body":body});
            }
            hash(body)
        }
        _ => return Err(AdapterError("unsupported_hook_event")),
    };
    event["eventId"] = json!(format!(
        "hook-{}",
        hash(&json!([session, turn, name, identity]).to_string())
    ));
    Ok(event)
}
