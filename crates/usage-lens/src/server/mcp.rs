//! Narrow MCP stdio implementation: initialize, ping, tools/list and eight read-only aggregate tools.
//! Newline JSON-RPC frames are capped at 64 KiB. No detail, paths, SQL, or collection methods exist.
use crate::{adapters::AdapterError, core::UsageStore};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

pub const INSTRUCTIONS: &str = "Read-only aggregates from an existing local store. Never equate missing data with zero, quota percent with remaining tokens, or partial events with complete ChatGPT history. Demo sources contain synthetic data. No collection is triggered by a query.";
const NAMES: [&str; 8] = [
    "usage_status",
    "usage_overview",
    "usage_daily",
    "usage_quota",
    "usage_tools",
    "usage_skills",
    "usage_response_tokens",
    "usage_health",
];
pub fn tools_list() -> Value {
    let descriptions = [
        "Sources, capture settings and collection coverage; no raw content.",
        "Reported account metrics and aggregate event counts with provenance and freshness.",
        "Returned daily source-date token buckets, with unknown gaps preserved.",
        "Service-reported quota windows; does not infer remaining token counts or access recovery.",
        "Aggregate observed tool counts; coverage is partial.",
        "Aggregate counts of direct skill evidence, keeping requested, loaded and invoked separate.",
        "Aggregate imported per-response token evidence; partial and never combined with account usage or quota.",
        "Stored source counts, evidence gaps, collection freshness and failures; no collection or completeness inference.",
    ];
    let tools:Vec<Value>=NAMES.iter().zip(descriptions).map(|(name,description)|{
        let mut properties=json!({});let mut required=Vec::new();
        if *name!="usage_status" {
            properties["sourceId"]=json!({"type":"string","pattern":"^[A-Za-z0-9][A-Za-z0-9._:-]{0,79}$"});required.push("sourceId");
            if ["usage_tools","usage_response_tokens"].contains(name) {
                properties["model"]=json!({"type":"string","maxLength":160});
            } else {properties["maxAgeMs"]=json!({"type":"integer","minimum":0,"maximum":2592000000u64});}
            if ["usage_daily","usage_tools","usage_response_tokens"].contains(name) {
                for key in ["fromDate","toDate"] {properties[key]=json!({"type":"string","pattern":"^\\d{4}-\\d{2}-\\d{2}$"});}
                if *name=="usage_daily" {required.extend(["fromDate","toDate"]);}
            }
        }
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})
    }).collect();
    json!({"tools":tools})
}
fn valid_arguments(name: &str, args: &Value) -> bool {
    let Some(map) = args.as_object() else {
        return false;
    };
    let allowed: &[&str] = match name {
        "usage_status" => &[],
        "usage_daily" => &["sourceId", "maxAgeMs", "fromDate", "toDate"],
        "usage_tools" | "usage_response_tokens" => &["sourceId", "fromDate", "toDate", "model"],
        _ => &["sourceId", "maxAgeMs"],
    };
    if map.keys().any(|k| !allowed.contains(&k.as_str())) {
        return false;
    }
    if name != "usage_status" {
        let Some(source) = map.get("sourceId").and_then(Value::as_str) else {
            return false;
        };
        if source.is_empty()
            || source.len() > 80
            || !source.as_bytes()[0].is_ascii_alphanumeric()
            || !source
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        {
            return false;
        }
    }
    for key in ["fromDate", "toDate"] {
        if let Some(value) = map.get(key) {
            let Some(day) = value.as_str() else {
                return false;
            };
            if day.len() != 10
                || day.as_bytes()[4] != b'-'
                || day.as_bytes()[7] != b'-'
                || !day
                    .bytes()
                    .enumerate()
                    .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
            {
                return false;
            }
        } else if name == "usage_daily" {
            return false;
        }
    }
    if map
        .get("maxAgeMs")
        .is_some_and(|v| crate::core::validation::integer(v, 0, 2592000000).is_err())
    {
        return false;
    }
    if map
        .get("model")
        .is_some_and(|v| v.as_str().is_none_or(|s| s.chars().count() > 160))
    {
        return false;
    }
    true
}
fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
#[derive(Default)]
pub struct McpSession {
    initialized: bool,
}
impl McpSession {
    pub fn handle(&mut self, store: &UsageStore, message: Value) -> Option<Value> {
        let Some(map) = message.as_object() else {
            return Some(error(Value::Null, -32600, "Invalid request"));
        };
        let id = map.get("id").cloned();
        if map.get("jsonrpc") != Some(&json!("2.0"))
            || !map.get("method").is_some_and(Value::is_string)
            || id
                .as_ref()
                .is_some_and(|v| !v.is_string() && !v.is_number() && !v.is_null())
        {
            return Some(error(id.unwrap_or(Value::Null), -32600, "Invalid request"));
        }
        let method = message["method"].as_str().unwrap_or("");
        let id = id?;
        let params = message.get("params").cloned().unwrap_or(json!({}));
        if !params.is_object() {
            return Some(error(id, -32602, "Invalid params"));
        }
        let result = match method {
            "initialize" => {
                if self.initialized
                    || !params.get("protocolVersion").is_some_and(Value::is_string)
                    || !params.get("capabilities").is_some_and(Value::is_object)
                    || !params.get("clientInfo").is_some_and(Value::is_object)
                {
                    return Some(error(id, -32602, "Invalid initialize params"));
                }
                self.initialized = true;
                let requested = params["protocolVersion"].as_str().unwrap_or("");
                let version = if ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"]
                    .contains(&requested)
                {
                    requested
                } else {
                    "2025-11-25"
                };
                json!({"protocolVersion":version,"capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"usage-lens","version":env!("CARGO_PKG_VERSION")},"instructions":INSTRUCTIONS})
            }
            "ping" => json!({}),
            _ if !self.initialized => return Some(error(id, -32000, "Server not initialized")),
            "tools/list" => tools_list(),
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                if !NAMES.contains(&name) {
                    return Some(
                        json!({"jsonrpc":"2.0","id":id,"result":{"isError":true,"content":[{"type":"text","text":"unknown_tool"}]}}),
                    );
                }
                if !valid_arguments(name, &args) {
                    return Some(
                        json!({"jsonrpc":"2.0","id":id,"result":{"isError":true,"content":[{"type":"text","text":"invalid_tool_arguments"}]}}),
                    );
                }
                let value = match name {
                    "usage_status" => store.get_status(),
                    "usage_health" => store.get_health(&args),
                    "usage_overview" => store.get_overview(&args),
                    "usage_daily" => store.get_daily_usage(&args),
                    "usage_quota" => store.get_quota(&args),
                    "usage_tools" => store.get_tool_usage(&args),
                    "usage_response_tokens" => store.get_response_token_usage(&args),
                    _ => store.get_skill_summary(&args),
                };
                match value {
                    Ok(value) => json!({"content":[{"type":"text","text":value.to_string()}]}),
                    Err(_) => {
                        json!({"isError":true,"content":[{"type":"text","text":"{\"error\":{\"code\":\"query_failed\"}}"}]})
                    }
                }
            }
            _ => return Some(error(id, -32601, "Method not found")),
        };
        Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
    }
}
pub async fn serve_stdio<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    store: &UsageStore,
    mut input: R,
    mut output: W,
) -> Result<(), AdapterError> {
    // Type-erasing only stream I/O avoids duplicating the protocol loop for each caller.
    serve_streams(store, &mut input, &mut output).await
}
async fn serve_streams(
    store: &UsageStore,
    input: &mut (dyn AsyncRead + Unpin),
    output: &mut (dyn AsyncWrite + Unpin),
) -> Result<(), AdapterError> {
    let mut input = BufReader::new(input);
    let mut frame = Vec::new();
    let mut session = McpSession::default();
    loop {
        let available = input
            .fill_buf()
            .await
            .map_err(|_| AdapterError("stdio_error"))?;
        if available.is_empty() {
            return Ok(());
        }
        let newline = available.iter().position(|b| *b == b'\n');
        let count = newline.map_or(available.len(), |n| n + 1);
        if frame.len() + count > 65536 {
            return Err(AdapterError("mcp_frame_limit"));
        }
        frame.extend_from_slice(&available[..count]);
        input.consume(count);
        if newline.is_none() {
            continue;
        }
        if frame.iter().all(u8::is_ascii_whitespace) {
            frame.clear();
            continue;
        }
        let response = match serde_json::from_slice(&frame) {
            Ok(message) => session.handle(store, message),
            Err(_) => Some(error(Value::Null, -32700, "Parse error")),
        };
        frame.clear();
        if let Some(response) = response {
            output
                .write_all(format!("{response}\n").as_bytes())
                .await
                .map_err(|_| AdapterError("stdio_error"))?;
            output
                .flush()
                .await
                .map_err(|_| AdapterError("stdio_error"))?;
        }
    }
}
