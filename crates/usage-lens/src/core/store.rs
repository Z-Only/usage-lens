use super::{
    normalize::{count, normalize_account, normalize_rate_limits, normalize_usage},
    redact::{CONTENT_WARNING, sanitize_content},
    response_tokens::{
        RESPONSE_WARNINGS, immutable_response, import_metadata, normalize_response_token,
        response_coverage,
    },
    validation::*,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, SecondsFormat, Utc};
use num_bigint::BigUint;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{cell::Cell, collections::BTreeSet, path::Path};

type RecordFilter = (Value, Option<String>, Option<String>, Option<String>);

const METHODS: &[&str] = &[
    "account/read",
    "account/usage/read",
    "account/rateLimits/read",
];
const EVENT_TYPES: &[&str] = &[
    "user_prompt",
    "assistant_visible_message",
    "tool_call",
    "skill_requested",
    "skill_loaded",
    "skill_invoked",
];
const FAILURE_CODES: &[&str] = &[
    "unsupported_method",
    "rpc_error",
    "request_timeout",
    "lifetime_timeout",
    "auth_handoff_required",
    "server_request_blocked",
    "subprocess_closed",
    "subprocess_error",
    "stdout_limit",
    "stderr_limit",
    "frame_limit",
    "malformed_response",
    "unexpected_response_id",
    "invalid_numeric_encoding",
    "invalid_payload",
    "incompatible_auth_mode",
    "unverified_live_access",
];
const WARNINGS: &[&str] = &[
    "Source IDs are user-assigned local namespaces, not verified account bindings.",
    "Coverage is limited to imported or directly observed records; missing history is unknown.",
];
const TOKEN_KEYS: &[&str] = &[
    "inputTokens",
    "cachedInputTokens",
    "cacheWriteInputTokens",
    "outputTokens",
    "reasoningOutputTokens",
    "totalTokens",
];
fn error(code: &'static str) -> CoreError {
    CoreError::new(code)
}
fn require(yes: bool) -> CoreResult<()> {
    if yes {
        Ok(())
    } else {
        Err(error("invalid_input"))
    }
}
fn safe<T>(value: rusqlite::Result<T>) -> CoreResult<T> {
    value.map_err(|_| error("storage_error"))
}
fn parse(text: String) -> CoreResult<Value> {
    serde_json::from_str(&text).map_err(|_| error("storage_error"))
}
fn s(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
fn optional(value: &Value) -> Option<&str> {
    value.as_str()
}
fn coverage() -> Value {
    json!({"completeness":"partial","missingEvents":"unknown","preCollectionHistory":"unknown","tokenAttribution":"not_provided"})
}
fn millis(value: &str) -> CoreResult<i64> {
    DateTime::parse_from_rfc3339(value)
        .map(|v| v.timestamp_millis())
        .map_err(|_| error("invalid_input"))
}
fn format_ms(ms: i64) -> CoreResult<String> {
    let formatted = DateTime::<Utc>::from_timestamp_millis(ms)
        .map(|v| v.to_rfc3339_opts(SecondsFormat::Millis, true))
        .ok_or_else(|| error("invalid_input"))?;
    timestamp(&Value::String(formatted))
}
fn limit(input: &Value) -> CoreResult<i64> {
    input
        .get("limit")
        .map(|v| integer(v, 1, 500))
        .unwrap_or(Ok(50))
}

/// Stores local data. This type performs no network calls and is intentionally not Sync.
/// Put it behind a Mutex when sharing it with HTTP handlers.
pub struct UsageStore {
    db: Option<Connection>,
    clock_ms: Cell<Option<i64>>,
}
impl UsageStore {
    pub fn open(path: impl AsRef<Path>) -> CoreResult<Self> {
        Self::open_internal(path.as_ref(), None)
    }
    pub fn in_memory() -> CoreResult<Self> {
        Self::open(":memory:")
    }
    pub fn with_clock_ms(path: impl AsRef<Path>, now_ms: i64) -> CoreResult<Self> {
        format_ms(now_ms)?;
        Self::open_internal(path.as_ref(), Some(now_ms))
    }
    pub fn set_clock_ms(&self, now_ms: i64) -> CoreResult<()> {
        format_ms(now_ms)?;
        self.clock_ms.set(Some(now_ms));
        Ok(())
    }
    fn open_internal(path: &Path, clock_ms: Option<i64>) -> CoreResult<Self> {
        let path_text = path.to_str().ok_or_else(|| error("invalid_input"))?;
        require(!path_text.is_empty() && path_text.len() <= 4096 && !path_text.contains('\0'))?;
        let existed = path.exists();
        let db = safe(Connection::open(path))?;
        #[cfg(unix)]
        if path_text != ":memory:" && !existed {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|_| error("storage_error"))?;
        }
        #[cfg(not(unix))]
        let _ = existed;
        safe(db.execute_batch("PRAGMA secure_delete=ON; PRAGMA foreign_keys=ON; PRAGMA journal_mode=DELETE; PRAGMA busy_timeout=2000;"))?;
        let version: i64 = safe(db.query_row("PRAGMA user_version", [], |row| row.get(0)))?;
        if ![0, 1, 2].contains(&version) {
            return Err(error("unsupported_schema"));
        }
        let result = Self {
            db: Some(db),
            clock_ms: Cell::new(clock_ms),
        };
        result.write(|| {
            if version == 0 {
                result.migrate()?;
            }
            if version < 2 {
                result.migrate_responses()?;
            }
            Ok(())
        })?;
        Ok(result)
    }
    fn db(&self) -> CoreResult<&Connection> {
        self.db.as_ref().ok_or_else(|| error("store_closed"))
    }
    fn write<T>(&self, operation: impl FnOnce() -> CoreResult<T>) -> CoreResult<T> {
        self.transaction(true, operation)
    }
    fn read<T>(&self, operation: impl FnOnce() -> CoreResult<T>) -> CoreResult<T> {
        self.transaction(false, operation)
    }
    fn transaction<T>(
        &self,
        write: bool,
        operation: impl FnOnce() -> CoreResult<T>,
    ) -> CoreResult<T> {
        let db = self.db()?;
        let outer = db.is_autocommit();
        let begin = if outer {
            if write {
                "BEGIN IMMEDIATE"
            } else {
                "BEGIN DEFERRED"
            }
        } else {
            "SAVEPOINT usage_core"
        };
        let commit = if outer {
            "COMMIT"
        } else {
            "RELEASE usage_core"
        };
        let rollback = if outer {
            "ROLLBACK"
        } else {
            "ROLLBACK TO usage_core; RELEASE usage_core"
        };
        safe(db.execute_batch(begin))?;
        let result = operation().and_then(|value| {
            safe(db.execute_batch(commit))?;
            Ok(value)
        });
        if result.is_err() {
            let _ = db.execute_batch(rollback);
        }
        result
    }
    fn migrate(&self) -> CoreResult<()> {
        safe(self.db()?.execute_batch("CREATE TABLE sources(id TEXT PRIMARY KEY,payload TEXT NOT NULL);
        CREATE TABLE settings(singleton INTEGER PRIMARY KEY CHECK(singleton=1),payload TEXT NOT NULL);
        CREATE TABLE observations(id INTEGER PRIMARY KEY,source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,method TEXT NOT NULL,observed_at TEXT NOT NULL,digest TEXT NOT NULL,payload TEXT NOT NULL,UNIQUE(source_id,method,observed_at,digest));
        CREATE INDEX observations_latest ON observations(source_id,method,observed_at DESC,id DESC);
        CREATE TABLE daily_buckets(observation_id INTEGER NOT NULL REFERENCES observations(id) ON DELETE CASCADE,ordinal INTEGER NOT NULL,date_label TEXT,tokens TEXT,payload TEXT NOT NULL,PRIMARY KEY(observation_id,ordinal));
        CREATE INDEX daily_date ON daily_buckets(observation_id,date_label);
        CREATE TABLE attempts(id INTEGER PRIMARY KEY,source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,method TEXT NOT NULL,attempted_at TEXT NOT NULL,error_code TEXT NOT NULL,UNIQUE(source_id,method,attempted_at,error_code));
        CREATE INDEX attempts_latest ON attempts(source_id,method,attempted_at DESC,id DESC);
        CREATE TABLE events(source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,event_id TEXT NOT NULL,source_event_id TEXT,event_type TEXT NOT NULL,observed_at TEXT NOT NULL,occurred_at TEXT,model TEXT,tool_name TEXT,skill_name TEXT,payload TEXT NOT NULL,PRIMARY KEY(source_id,event_id));
        CREATE UNIQUE INDEX event_source_identity ON events(source_id,source_event_id,event_type) WHERE source_event_id IS NOT NULL;
        CREATE INDEX events_query ON events(source_id,observed_at DESC,event_id DESC);
        CREATE INDEX events_aggregate ON events(source_id,event_type,model);
        CREATE TABLE event_details(source_id TEXT NOT NULL,event_id TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(source_id,event_id),FOREIGN KEY(source_id,event_id) REFERENCES events(source_id,event_id) ON DELETE CASCADE);
        PRAGMA user_version=1;"))?;
        safe(
            self.db()?.execute(
                "INSERT INTO settings(singleton,payload) VALUES(1,?)",
                [
                    json!({"capturePaused":false,"contentCaptureEnabled":false,"retentionDays":30})
                        .to_string(),
                ],
            ),
        )?;
        Ok(())
    }
    fn migrate_responses(&self) -> CoreResult<()> {
        safe(self.db()?.execute_batch("CREATE TABLE response_tokens(id INTEGER PRIMARY KEY,source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,thread_id TEXT NOT NULL,session_id TEXT NOT NULL,response_id TEXT NOT NULL,imported_at TEXT NOT NULL,occurred_at TEXT,model TEXT,input_tokens TEXT NOT NULL,cached_input_tokens TEXT NOT NULL,cache_write_input_tokens TEXT NOT NULL,output_tokens TEXT NOT NULL,reasoning_output_tokens TEXT NOT NULL,total_tokens TEXT NOT NULL,payload TEXT NOT NULL,UNIQUE(source_id,thread_id,session_id,response_id));
        CREATE INDEX response_tokens_latest ON response_tokens(source_id,imported_at DESC,id DESC);
        CREATE TABLE imports(source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,fingerprint TEXT NOT NULL,imported_at TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(source_id,fingerprint)); PRAGMA user_version=2;"))
    }
    fn now(&self) -> CoreResult<String> {
        format_ms(
            self.clock_ms
                .get()
                .unwrap_or_else(|| Utc::now().timestamp_millis()),
        )
    }
    fn source(&self, id: &Value) -> CoreResult<Value> {
        let id = source_id(id)?;
        let row: Option<String> = safe(
            self.db()?
                .query_row("SELECT payload FROM sources WHERE id=?", [id], |r| r.get(0))
                .optional(),
        )?;
        parse(row.ok_or_else(|| error("source_not_found"))?)
    }
    fn capture_allowed(&self) -> CoreResult<()> {
        if self.get_settings()?["capturePaused"] == true {
            Err(error("capture_paused"))
        } else {
            Ok(())
        }
    }
    pub fn close(&mut self) -> CoreResult<()> {
        if let Some(db) = self.db.take() {
            db.close().map_err(|_| error("storage_error"))?;
        }
        Ok(())
    }
    pub fn create_source(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(
            input,
            &[
                "id",
                "displayName",
                "mode",
                "provider",
                "coverageDescription",
            ],
        )?;
        let id = source_id(&input["id"])?;
        let source = json!({"id":id,"displayName":bounded_text(&input["displayName"],128)?,"mode":enum_value(&input["mode"],&["demo","imported","live"])?,"provider":enum_value(&input["provider"],&["codex_app_server","user_import","synthetic"])?,"coverageDescription":bounded_text(&input["coverageDescription"],1000)?,"accountBinding":"unverified_local_namespace","createdAt":self.now()?});
        self.write(|| {
            let exists: bool = safe(self.db()?.query_row(
                "SELECT EXISTS(SELECT 1 FROM sources WHERE id=?)",
                [&id],
                |r| r.get(0),
            ))?;
            if exists {
                return Err(error("source_exists"));
            }
            let n: i64 = safe(
                self.db()?
                    .query_row("SELECT count(*) FROM sources", [], |r| r.get(0)),
            )?;
            if n >= 100 {
                return Err(error("input_too_large"));
            }
            safe(self.db()?.execute(
                "INSERT INTO sources(id,payload) VALUES(?,?)",
                params![id, source.to_string()],
            ))?;
            Ok(source)
        })
    }
    pub fn list_sources(&self) -> CoreResult<Value> {
        self.json_rows("SELECT payload FROM sources ORDER BY id LIMIT 100", [])
            .map(Value::Array)
    }
    pub fn get_settings(&self) -> CoreResult<Value> {
        parse(safe(self.db()?.query_row(
            "SELECT payload FROM settings WHERE singleton=1",
            [],
            |r| r.get(0),
        ))?)
    }
    pub fn update_settings(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(
            input,
            &["capturePaused", "contentCaptureEnabled", "retentionDays"],
        )?;
        for key in ["capturePaused", "contentCaptureEnabled"] {
            if let Some(v) = input.get(key) {
                require(v.is_boolean())?;
            }
        }
        if let Some(v) = input.get("retentionDays") {
            integer(v, 1, 3650)?;
        }
        self.write(|| {
            let mut value = self.get_settings()?;
            for (k, v) in input.as_object().ok_or_else(|| error("invalid_input"))? {
                value[k] = v.clone();
            }
            safe(self.db()?.execute(
                "UPDATE settings SET payload=? WHERE singleton=1",
                [value.to_string()],
            ))?;
            Ok(value)
        })
    }
    pub fn set_settings(&self, input: &Value) -> CoreResult<Value> {
        self.update_settings(input)
    }
    fn json_rows<P: rusqlite::Params>(&self, sql: &str, params: P) -> CoreResult<Vec<Value>> {
        let mut stmt = safe(self.db()?.prepare_cached(sql))?;
        let rows = safe(stmt.query_map(params, |r| r.get::<_, String>(0)))?;
        rows.map(|r| parse(safe(r)?)).collect()
    }
    pub fn ingest_observation(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(
            input,
            &[
                "sourceId",
                "method",
                "observedAt",
                "sourceAsOf",
                "adapterVersion",
                "schemaBaseline",
                "raw",
            ],
        )?;
        let source = self.source(&input["sourceId"])?;
        let kind = enum_value(&input["method"], METHODS)?;
        let at = timestamp(&input["observedAt"])?;
        let data = match kind.as_str() {
            "account/read" => normalize_account(&input["raw"]),
            "account/usage/read" => normalize_usage(&input["raw"]),
            _ => normalize_rate_limits(&input["raw"]),
        }?;
        let asof = if input["sourceAsOf"].is_null() {
            None
        } else {
            Some(timestamp(&input["sourceAsOf"])?)
        };
        let payload = json!({"data":data,"observedAt":at,"sourceAsOf":asof,"provenance":{"sourceId":source["id"],"provider":"codex_app_server","method":kind,"adapterVersion":identifier(&input["adapterVersion"])?,"schemaBaseline":identifier(&input["schemaBaseline"])?,"mode":source["mode"]}});
        let text = payload.to_string();
        let digest = Sha256::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        self.write(|| {self.capture_allowed()?;let inserted=safe(self.db()?.execute("INSERT OR IGNORE INTO observations(source_id,method,observed_at,digest,payload) VALUES(?,?,?,?,?)",params![s(&source["id"]),kind,at,digest,text]))?>0;
            let observation_id=self.db()?.last_insert_rowid();
            if inserted && kind=="account/usage/read" && data["dailyUsageBuckets"]["status"]=="reported" {for (ordinal,bucket) in data["dailyUsageBuckets"]["value"].as_array().ok_or_else(||error("storage_error"))?.iter().enumerate(){safe(self.db()?.execute("INSERT INTO daily_buckets(observation_id,ordinal,date_label,tokens,payload) VALUES(?,?,?,?,?)",params![observation_id,ordinal as i64,optional(&bucket["startDate"]["value"]),optional(&bucket["tokens"]["value"]),bucket.to_string()]))?;}}
            Ok(json!({"inserted":inserted}))})
    }
    pub fn record_failure(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(input, &["sourceId", "method", "attemptedAt", "errorCode"])?;
        let source = self.source(&input["sourceId"])?;
        let kind = enum_value(&input["method"], METHODS)?;
        let at = timestamp(&input["attemptedAt"])?;
        let code = enum_value(&input["errorCode"], FAILURE_CODES)?;
        self.write(||{self.capture_allowed()?;safe(self.db()?.execute("INSERT OR IGNORE INTO attempts(source_id,method,attempted_at,error_code) VALUES(?,?,?,?)",params![s(&source["id"]),kind,at,code]))?;Ok(Value::Null)})
    }
    fn prepare_event(&self, input: &Value) -> CoreResult<(Value, Option<Value>, Vec<String>)> {
        exact_keys(
            input,
            &[
                "eventId",
                "sourceId",
                "eventType",
                "evidenceType",
                "observedAt",
                "occurredAt",
                "sourceEventId",
                "collectorVersion",
                "model",
                "toolName",
                "skillName",
                "skillVersion",
                "sessionId",
                "turnId",
                "status",
                "durationMs",
                "reasoningEffort",
                "retryOfEventId",
                "skillEvidenceKind",
                "content",
            ],
        )?;
        let kind = enum_value(&input["eventType"], EVENT_TYPES)?;
        let evidence = match kind.as_str() {
            "user_prompt" => "explicit_user_message",
            "assistant_visible_message" => "explicit_assistant_visible_message",
            "tool_call" => "explicit_tool_call",
            "skill_requested" => "explicit_skill_input",
            "skill_loaded" => "successful_skill_read",
            _ => "explicit_execution_record",
        };
        require(
            input["evidenceType"] == evidence
                || (kind == "skill_loaded" && input["evidenceType"] == "typed_skill_injection"),
        )?;
        let tool = nullable_identifier(&input["toolName"])?;
        let skill = if input["skillName"].is_null() {
            None
        } else {
            Some(bounded_text(&input["skillName"], 256)?)
        };
        require(if kind == "tool_call" {
            tool.is_some() && skill.is_none()
        } else {
            tool.is_none()
        })?;
        require(if kind.starts_with("skill_") {
            skill.is_some()
        } else {
            skill.is_none()
        })?;
        require(kind.starts_with("skill_") || input["skillVersion"].is_null())?;
        let ek = if input["skillEvidenceKind"].is_null() {
            None
        } else {
            Some(enum_value(
                &input["skillEvidenceKind"],
                &[
                    "main_read",
                    "instruction_injection",
                    "resource_read",
                    "continuation_read",
                ],
            )?)
        };
        match ek.as_deref() {
            Some("main_read") => {
                require(kind == "skill_loaded" && input["evidenceType"] == "successful_skill_read")?
            }
            Some("instruction_injection") => {
                require(kind == "skill_loaded" && input["evidenceType"] == "typed_skill_injection")?
            }
            Some("resource_read" | "continuation_read") => require(kind == "tool_call")?,
            _ => {}
        }
        let duration = if input["durationMs"].is_null() {
            None
        } else {
            Some(count(&input["durationMs"]).ok_or_else(|| error("invalid_input"))?)
        };
        let metadata = json!({"eventId":identifier(&input["eventId"])?,"sourceId":self.source(&input["sourceId"])?["id"],"eventType":kind,"evidenceType":input["evidenceType"],"observedAt":timestamp(&input["observedAt"])?,"occurredAt":if input["occurredAt"].is_null(){None}else{Some(timestamp(&input["occurredAt"])?)},"sourceEventId":nullable_identifier(&input["sourceEventId"])?,"collectorVersion":identifier(&input["collectorVersion"])?,"model":nullable_identifier(&input["model"])?,"toolName":tool,"skillName":skill,"skillEvidenceKind":ek,"skillVersion":nullable_identifier(&input["skillVersion"])?,"sessionId":nullable_identifier(&input["sessionId"])?,"turnId":nullable_identifier(&input["turnId"])?,"status":if input.get("status").is_none(){"unknown".to_string()}else{enum_value(&input["status"],&["success","error","cancelled","unknown"])?},"durationMs":duration,"reasoningEffort":nullable_identifier(&input["reasoningEffort"])?,"retryOfEventId":nullable_identifier(&input["retryOfEventId"])?,"confidence":"direct","coverage":"partial"});
        let content = input.get("content").map(sanitize_content).transpose()?;
        let enabled = self.get_settings()?["contentCaptureEnabled"] == true;
        let warnings = if content.is_none() {
            vec![]
        } else if enabled {
            vec![CONTENT_WARNING.to_owned()]
        } else {
            vec!["Content capture is disabled; supplied content was not retained.".to_owned()]
        };
        Ok((metadata, if enabled { content } else { None }, warnings))
    }
    pub fn ingest_event(&self, input: &Value) -> CoreResult<Value> {
        let result = self.ingest_events(&json!([input]))?;
        Ok(result[0].clone())
    }
    pub fn ingest_events(&self, inputs: &Value) -> CoreResult<Value> {
        let inputs_array = inputs.as_array().ok_or_else(|| error("invalid_input"))?;
        require(inputs_array.len() <= 1000)?;
        check_size(inputs, 2 * 1024 * 1024)?;
        self.write(|| {self.capture_allowed()?;let prepared=inputs_array.iter().map(|v|self.prepare_event(v)).collect::<CoreResult<Vec<_>>>()?;let mut results=Vec::new();
            for (event,content,warnings) in prepared {
                let existing:Option<String>=safe(self.db()?.query_row("SELECT event_id FROM events WHERE source_id=? AND (event_id=? OR (? IS NOT NULL AND source_event_id=? AND event_type=?)) LIMIT 1",params![s(&event["sourceId"]),s(&event["eventId"]),optional(&event["sourceEventId"]),optional(&event["sourceEventId"]),s(&event["eventType"])],|r|r.get(0)).optional())?;
                if let Some(id)=existing {let retained:bool=safe(self.db()?.query_row("SELECT EXISTS(SELECT 1 FROM event_details WHERE source_id=? AND event_id=?)",params![s(&event["sourceId"]),id],|r|r.get(0)))?;results.push(json!({"inserted":false,"contentRetained":retained,"warnings":["Duplicate event identity ignored; the first observation was retained."]}));continue;}
                safe(self.db()?.execute("INSERT INTO events(source_id,event_id,source_event_id,event_type,observed_at,occurred_at,model,tool_name,skill_name,payload) VALUES(?,?,?,?,?,?,?,?,?,?)",params![s(&event["sourceId"]),s(&event["eventId"]),optional(&event["sourceEventId"]),s(&event["eventType"]),s(&event["observedAt"]),optional(&event["occurredAt"]),optional(&event["model"]),optional(&event["toolName"]),optional(&event["skillName"]),event.to_string()]))?;
                if let Some(ref value)=content {safe(self.db()?.execute("INSERT INTO event_details(source_id,event_id,payload) VALUES(?,?,?)",params![s(&event["sourceId"]),s(&event["eventId"]),value.to_string()]))?;}
                results.push(json!({"inserted":true,"contentRetained":content.is_some(),"warnings":warnings}));
            } Ok(Value::Array(results))})
    }
    pub fn ingest_response_token(&self, input: &Value) -> CoreResult<Value> {
        let source = self.source(&input["sourceId"])?;
        if source["mode"] != "imported" {
            return Err(error("imported_source_required"));
        }
        let value = normalize_response_token(input)?;
        self.write(||{self.capture_allowed()?;let prior:Option<String>=safe(self.db()?.query_row("SELECT payload FROM response_tokens WHERE source_id=? AND thread_id=? AND session_id=? AND response_id=?",params![s(&source["id"]),s(&value["threadId"]),s(&value["sessionId"]),s(&value["responseId"])],|r|r.get(0)).optional())?;
            if let Some(prior)=prior{if immutable_response(&parse(prior)?)!=immutable_response(&value){return Err(error("response_token_conflict"));}return Ok(json!({"inserted":false}));}
            let t=&value["usage"];
            safe(self.db()?.execute("INSERT INTO response_tokens(source_id,thread_id,session_id,response_id,imported_at,occurred_at,model,input_tokens,cached_input_tokens,cache_write_input_tokens,output_tokens,reasoning_output_tokens,total_tokens,payload) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",params![s(&source["id"]),s(&value["threadId"]),s(&value["sessionId"]),s(&value["responseId"]),s(&value["importedAt"]),optional(&value["occurredAt"]),optional(&value["model"]),s(&t["inputTokens"]),s(&t["cachedInputTokens"]),s(&t["cacheWriteInputTokens"]),s(&t["outputTokens"]),s(&t["reasoningOutputTokens"]),s(&t["totalTokens"]),value.to_string()]))?;Ok(json!({"inserted":true}))})
    }
    pub fn import_data(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(
            input,
            &[
                "sourceId",
                "observations",
                "events",
                "responseTokens",
                "importMetadata",
            ],
        )?;
        let source = self.source(&input["sourceId"])?;
        let obs = input["observations"]
            .as_array()
            .ok_or_else(|| error("invalid_input"))?;
        let events = input["events"]
            .as_array()
            .ok_or_else(|| error("invalid_input"))?;
        require(obs.len() <= 100 && events.len() <= 1000)?;
        check_size(input, 2 * 1024 * 1024)?;
        let empty = vec![];
        let tokens = match input.get("responseTokens") {
            None | Some(Value::Null) => &empty,
            Some(v) => v.as_array().ok_or_else(|| error("invalid_input"))?,
        };
        require(tokens.len() <= 1000)?;
        let metadata = input
            .get("importMetadata")
            .map(import_metadata)
            .transpose()?;
        if (metadata.is_some() || !tokens.is_empty()) && source["mode"] != "imported" {
            return Err(error("imported_source_required"));
        }
        self.write(||{self.capture_allowed()?;
            if let Some(m)=&metadata{let exists:bool=safe(self.db()?.query_row("SELECT EXISTS(SELECT 1 FROM imports WHERE source_id=? AND fingerprint=?)",params![s(&source["id"]),s(&m["fingerprint"])],|r|r.get(0)))?;if exists{return Ok(json!({"observationsInserted":"0","eventsInserted":"0","responseTokensInserted":"0","importAlreadyPresent":true}));}}
            let inject=|v:&Value|->CoreResult<Value>{require(v.is_object()&&v.get("sourceId").is_none())?;let mut v=v.clone();v["sourceId"]=source["id"].clone();Ok(v)};
            let mut observations_inserted=0usize;let mut responses_inserted=0usize;
            for o in obs{if self.ingest_observation(&inject(o)?)?["inserted"]==true{observations_inserted+=1;}}
            let event_inputs=events.iter().map(inject).collect::<CoreResult<Vec<_>>>()?;
            let event_results=self.ingest_events(&json!(event_inputs))?;
            let events_inserted=event_results.as_array().ok_or_else(||error("storage_error"))?.iter().filter(|v|v["inserted"]==true).count();
            for t in tokens{if self.ingest_response_token(&inject(t)?)?["inserted"]==true{responses_inserted+=1;}}
            if let Some(m)=&metadata{safe(self.db()?.execute("INSERT INTO imports(source_id,fingerprint,imported_at,payload) VALUES(?,?,?,?)",params![s(&source["id"]),s(&m["fingerprint"]),s(&m["importedAt"]),m.to_string()]))?;}
            let mut result=json!({"observationsInserted":observations_inserted.to_string(),"eventsInserted":events_inserted.to_string()});
            if input.get("responseTokens").is_some()||metadata.is_some(){result["responseTokensInserted"]=json!(responses_inserted.to_string());result["importAlreadyPresent"]=json!(false);} Ok(result)})
    }
    pub fn import_rollout(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(
            input,
            &[
                "sourceId",
                "fingerprint",
                "sourceVersion",
                "adapterVersion",
                "importedAt",
                "warningCodes",
                "events",
                "responseTokens",
            ],
        )?;
        check_size(input, 2 * 1024 * 1024)?;
        let strip = |name: &str| -> CoreResult<Vec<Value>> {
            let rows = input[name]
                .as_array()
                .ok_or_else(|| error("invalid_input"))?;
            require(rows.len() <= 1000)?;
            rows.iter()
                .map(|v| {
                    require(v.is_object() && v["sourceId"] == input["sourceId"])?;
                    let mut v = v.clone();
                    v.as_object_mut()
                        .ok_or_else(|| error("invalid_input"))?
                        .remove("sourceId");
                    Ok(v)
                })
                .collect()
        };
        self.import_data(&json!({"sourceId":input["sourceId"],"observations":[],"events":strip("events")?,"responseTokens":strip("responseTokens")?,"importMetadata":{"fingerprint":input["fingerprint"],"sourceVersion":input["sourceVersion"],"adapterVersion":input["adapterVersion"],"importedAt":input["importedAt"],"warningCodes":input["warningCodes"]}}))
    }
    pub fn get_local_event_detail(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.get_local_event_detail_impl(input))
    }
    fn get_local_event_detail_impl(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(input, &["sourceId", "eventId"])?;
        let source = self.source(&input["sourceId"])?;
        let id = identifier(&input["eventId"])?;
        let row:Option<(String,Option<String>)>=safe(self.db()?.query_row("SELECT e.payload,d.payload FROM events e LEFT JOIN event_details d ON d.source_id=e.source_id AND d.event_id=e.event_id WHERE e.source_id=? AND e.event_id=?",params![s(&source["id"]),id],|r|Ok((r.get(0)?,r.get(1)?))).optional())?;
        let (event, content) = row.ok_or_else(|| error("event_not_found"))?;
        Ok(
            json!({"event":parse(event)?,"content":content.clone().map(parse).transpose()?,"contentRetained":content.is_some(),"warnings":[if content.is_some(){CONTENT_WARNING}else{"No local content was retained for this event."}]}),
        )
    }
    pub fn search_local_details(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.search_local_details_impl(input))
    }
    fn search_local_details_impl(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(input, &["sourceId", "query", "limit"])?;
        let source = self.source(&input["sourceId"])?;
        let query = bounded_text(&input["query"], 200)?;
        let limit = limit(input)?;
        let events=self.json_rows("SELECT payload FROM (SELECT e.payload AS payload,d.payload AS content FROM events e JOIN event_details d ON e.source_id=d.source_id AND e.event_id=d.event_id WHERE e.source_id=? ORDER BY e.observed_at DESC,e.event_id DESC LIMIT 10000) WHERE instr(content,?)>0 LIMIT ?",params![s(&source["id"]),query,limit])?;
        Ok(
            json!({"events":events,"coverage":coverage(),"warnings":[CONTENT_WARNING,"Search examines at most the 10,000 most recently observed retained, redacted local content records. Results are capped."]}),
        )
    }
    pub fn clear_data(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(input, &["sourceId"])?;
        let id = input
            .get("sourceId")
            .map(|id| self.source(id).map(|v| s(&v["id"]).to_owned()))
            .transpose()?;
        self.write(|| {
            let mut result = json!({});
            for (table, key) in [
                ("observations", "observationsDeleted"),
                ("events", "eventsDeleted"),
                ("attempts", "attemptsDeleted"),
                ("response_tokens", "responseTokensDeleted"),
                ("imports", "importsDeleted"),
            ] {
                let n = safe(self.db()?.execute(
                    &format!("DELETE FROM {table} WHERE (? IS NULL OR source_id=?)"),
                    params![id, id],
                ))?;
                result[key] = json!(n.to_string());
            }
            Ok(result)
        })
    }
    pub fn clear_local_content(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(input, &["sourceId"])?;
        let id = input
            .get("sourceId")
            .map(|id| self.source(id).map(|v| s(&v["id"]).to_owned()))
            .transpose()?;
        self.write(|| {
            let n = safe(self.db()?.execute(
                "DELETE FROM event_details WHERE (? IS NULL OR source_id=?)",
                params![id, id],
            ))?;
            Ok(json!({"contentsDeleted":n.to_string()}))
        })
    }
    pub fn delete_source(&self, id: &str) -> CoreResult<Value> {
        let source = self.source(&json!(id))?;
        self.write(|| {
            safe(
                self.db()?
                    .execute("DELETE FROM sources WHERE id=?", [s(&source["id"])]),
            )?;
            Ok(Value::Null)
        })
    }
    pub fn apply_retention(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(input, &["now"])?;
        let now = match input.get("now") {
            Some(v) => timestamp(v)?,
            None => self.now()?,
        };
        let days = self.get_settings()?["retentionDays"]
            .as_i64()
            .ok_or_else(|| error("storage_error"))?;
        let cutoff = format_ms(millis(&now)? - days * 86400000)?;
        self.write(|| {
            let mut result = json!({});
            for (table, col, key) in [
                ("observations", "observed_at", "observationsDeleted"),
                ("events", "observed_at", "eventsDeleted"),
                ("attempts", "attempted_at", "attemptsDeleted"),
                ("response_tokens", "imported_at", "responseTokensDeleted"),
                ("imports", "imported_at", "importsDeleted"),
            ] {
                let n = safe(
                    self.db()?
                        .execute(&format!("DELETE FROM {table} WHERE {col}<?"), [&cutoff]),
                )?;
                result[key] = json!(n.to_string());
            }
            Ok(result)
        })
    }
    fn query_source(&self, input: &Value, extras: &[&str]) -> CoreResult<(Value, i64)> {
        let mut keys = vec!["sourceId", "maxAgeMs"];
        keys.extend(extras);
        exact_keys(input, &keys)?;
        let age = input
            .get("maxAgeMs")
            .map(|v| integer(v, 0, 30 * 86400000))
            .unwrap_or(Ok(900000))?;
        Ok((self.source(&input["sourceId"])?, age))
    }
    fn latest_failure(&self, id: &str, kind: &str, since: Option<&str>) -> CoreResult<Value> {
        let row:Option<(String,String)>=safe(self.db()?.query_row("SELECT attempted_at,error_code FROM attempts WHERE source_id=? AND method=? AND (? IS NULL OR attempted_at>=?) ORDER BY attempted_at DESC,id DESC LIMIT 1",params![id,kind,since,since],|r|Ok((r.get(0)?,r.get(1)?))).optional())?;
        Ok(match row {
            Some((at, code)) => {
                json!({"status":"unavailable","data":null,"errorCode":code,"attemptedAt":at})
            }
            None => Value::Null,
        })
    }
    fn row_observation(&self, stored: &Value, max_age: i64) -> CoreResult<Value> {
        let now = millis(&self.now()?)?;
        let at = s(&stored["observedAt"]);
        let observed = millis(at)?;
        let failure = self.latest_failure(
            s(&stored["provenance"]["sourceId"]),
            s(&stored["provenance"]["method"]),
            Some(at),
        )?;
        let mut warnings =
            vec!["Collection time is not backend data freshness; backend age may be unknown."];
        if !failure.is_null() {
            warnings.push("A collection attempt at or after this observation failed; last successful values are retained.");
        }
        if observed > now {
            warnings.push("Observation timestamp is in the future relative to the local clock.");
        }
        let age = (now - observed).max(0);
        Ok(
            json!({"status":"available","data":stored["data"],"provenance":stored["provenance"],"freshness":{"observedAt":stored["observedAt"],"sourceAsOf":stored["sourceAsOf"],"sourceAsOfStatus":if stored["sourceAsOf"].is_null(){"not_provided"}else{"reported"},"ageMs":age,"stale":age>max_age,"maxAgeMs":max_age},"lastFailure":failure,"warnings":warnings}),
        )
    }
    fn latest(&self, id: &str, kind: &str, max_age: i64) -> CoreResult<Value> {
        let row:Option<String>=safe(self.db()?.query_row("SELECT payload FROM observations WHERE source_id=? AND method=? ORDER BY observed_at DESC,id DESC LIMIT 1",params![id,kind],|r|r.get(0)).optional())?;
        match row {
            Some(row) => self.row_observation(&parse(row)?, max_age),
            None => Ok(
                json!({"status":"unavailable","data":null,"reason":"not_collected","lastFailure":self.latest_failure(id,kind,None)?,"warnings":["No successful observation has been collected."]}),
            ),
        }
    }
    fn import_warnings(&self, id: &str) -> CoreResult<Vec<String>> {
        let rows = self.json_rows(
            "SELECT payload FROM imports WHERE source_id=? ORDER BY imported_at DESC LIMIT 100",
            [id],
        )?;
        let mut seen = BTreeSet::new();
        let mut warnings = vec![];
        for row in rows {
            if let Some(codes) = row["warningCodes"].as_array() {
                for code in codes {
                    let code = s(code);
                    if seen.insert(code.to_owned()) && warnings.len() < 100 {
                        warnings.push(format!("Import coverage: {code}"));
                    }
                }
            }
        }
        Ok(warnings)
    }
    fn response_warnings(&self, id: &str) -> CoreResult<Vec<String>> {
        let mut warnings: Vec<String> = RESPONSE_WARNINGS.iter().map(|v| v.to_string()).collect();
        warnings.extend(self.import_warnings(id)?);
        Ok(warnings)
    }
    fn count_table(&self, table: &str) -> CoreResult<String> {
        let n: i64 = safe(self.db()?.query_row(
            &format!("SELECT count(*) FROM {table}"),
            [],
            |r| r.get(0),
        ))?;
        Ok(n.to_string())
    }
    pub fn get_status(&self) -> CoreResult<Value> {
        self.read(|| self.get_status_impl())
    }
    fn get_status_impl(&self) -> CoreResult<Value> {
        let sources = self.list_sources()?;
        let mut capabilities = vec![];
        let mut warnings: Vec<String> = WARNINGS.iter().map(|v| v.to_string()).collect();
        warnings.push(CONTENT_WARNING.to_owned());
        warnings.push("Retention cleanup requires an explicit confirmed action; startup never destroys stored data.".to_owned());
        for source in sources.as_array().ok_or_else(|| error("storage_error"))? {
            let id = s(&source["id"]);
            for method in METHODS {
                let latest = self.latest(id, method, 900000)?;
                let failed = !latest["lastFailure"].is_null();
                let available = latest["status"] == "available";
                let unsupported = latest["lastFailure"]["errorCode"] == "unsupported_method";
                capabilities.push(json!({"sourceId":id,"method":method,"state":if unsupported{"unsupported"}else if available{if failed{"partial"}else{"available"}}else{"unknown"},"reason":if failed{s(&latest["lastFailure"]["errorCode"])}else if available{"last_successful_observation_available"}else{"not_collected"}}));
            }
            let found: bool = safe(self.db()?.query_row(
                "SELECT EXISTS(SELECT 1 FROM response_tokens WHERE source_id=?)",
                [id],
                |r| r.get(0),
            ))?;
            capabilities.push(json!({"sourceId":id,"method":"response_tokens","state":if found{"partial"}else{"unknown"},"reason":if found{"imported_response_records_only_incomplete_history"}else{"no_response_records_imported"}}));
            for kind in ["events", "skills"] {
                let found:bool=safe(self.db()?.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE source_id=? AND (?='events' OR event_type IN ('skill_requested','skill_loaded','skill_invoked')))",params![id,kind],|r|r.get(0)))?;
                capabilities.push(json!({"sourceId":id,"method":kind,"state":if found{"partial"}else{"unknown"},"reason":if found{"direct_evidence_only_incomplete_history"}else{"no_direct_evidence_collected"}}));
            }
            warnings.extend(
                self.import_warnings(id)?
                    .iter()
                    .map(|w| format!("Source {id}: {w}")),
            );
        }
        Ok(
            json!({"responseTokenCount":self.count_table("response_tokens")?,"schemaVersion":2,"settings":self.get_settings()?,"sources":sources,"capabilities":capabilities,"observationCount":self.count_table("observations")?,"eventCount":self.count_table("events")?,"warnings":warnings}),
        )
    }
    fn groups<P: rusqlite::Params>(&self, sql: &str, params: P) -> CoreResult<Vec<Value>> {
        let mut stmt = safe(self.db()?.prepare_cached(sql))?;
        let rows = safe(stmt.query_map(params, |r| {
            Ok((r.get::<_, Option<String>>(0)?, r.get::<_, i64>(1)?))
        }))?;
        rows.map(|r| {
            let (name, n) = safe(r)?;
            Ok(json!({"name":name,"count":n.to_string()}))
        })
        .collect()
    }
    fn event_summary(&self, id: &str) -> CoreResult<Value> {
        let total: i64 = safe(self.db()?.query_row(
            "SELECT count(*) FROM events WHERE source_id=?",
            [id],
            |r| r.get(0),
        ))?;
        let unknown: i64 = safe(self.db()?.query_row(
            "SELECT count(*) FROM events WHERE source_id=? AND model IS NULL",
            [id],
            |r| r.get(0),
        ))?;
        let by_type=self.groups("SELECT event_type,count(*) FROM events WHERE source_id=? GROUP BY event_type ORDER BY count(*) DESC,event_type",[id])?;
        let by_model=self.groups("SELECT model,count(*) FROM events WHERE source_id=? GROUP BY model ORDER BY count(*) DESC,model LIMIT 500",[id])?;
        let tools=self.groups("SELECT tool_name,count(*) FROM events WHERE source_id=? AND event_type='tool_call' GROUP BY tool_name ORDER BY count(*) DESC,tool_name LIMIT 500",[id])?;
        let mut stmt=safe(self.db()?.prepare_cached("SELECT event_type,skill_name,json_extract(payload,'$.skillEvidenceKind'),count(*) FROM events WHERE source_id=? AND event_type IN ('skill_requested','skill_loaded','skill_invoked') GROUP BY event_type,skill_name,json_extract(payload,'$.skillEvidenceKind') ORDER BY event_type,count(*) DESC,skill_name LIMIT 500"))?;
        let skills=safe(stmt.query_map([id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,i64>(3)?))))?.map(|r|{let(kind,name,evidence,n)=safe(r)?;Ok(json!({"kind":kind.trim_start_matches("skill_"),"evidenceKind":evidence,"name":name,"count":n.to_string()}))}).collect::<CoreResult<Vec<_>>>()?;
        Ok(
            json!({"total":total.to_string(),"byType":by_type,"byModel":by_model,"tools":tools,"skills":skills,"unknownModelCount":unknown.to_string(),"coverage":coverage()}),
        )
    }
    pub fn get_overview(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.get_overview_impl(input))
    }
    fn get_overview_impl(&self, input: &Value) -> CoreResult<Value> {
        let (source, max_age) = self.query_source(input, &[])?;
        let id = s(&source["id"]);
        let mut warnings: Vec<String> = WARNINGS.iter().map(|v| v.to_string()).collect();
        warnings.extend(self.import_warnings(id)?);
        Ok(
            json!({"source":source,"account":self.latest(id,"account/read",max_age)?,"usage":self.latest(id,"account/usage/read",max_age)?,"quota":self.latest(id,"account/rateLimits/read",max_age)?,"events":self.event_summary(id)?,"warnings":warnings}),
        )
    }
    pub fn get_daily_usage(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.get_daily_usage_impl(input))
    }
    fn get_daily_usage_impl(&self, input: &Value) -> CoreResult<Value> {
        let (source, max_age) = self.query_source(input, &["fromDate", "toDate"])?;
        let (from, to) = date_range(&input["fromDate"], &input["toDate"])?;
        let observation = self.latest(s(&source["id"]), "account/usage/read", max_age)?;
        let buckets=self.json_rows("SELECT b.payload FROM daily_buckets b WHERE b.observation_id=(SELECT id FROM observations WHERE source_id=? AND method='account/usage/read' ORDER BY observed_at DESC,id DESC LIMIT 1) AND b.date_label>=? AND b.date_label<=? ORDER BY b.date_label,b.ordinal LIMIT 10000",params![s(&source["id"]),from,to])?;
        let mut sum = BigUint::from(0u8);
        let mut unknown = 0usize;
        for bucket in &buckets {
            if bucket["tokens"]["status"] == "reported" {
                sum += s(&bucket["tokens"]["value"])
                    .parse::<BigUint>()
                    .map_err(|_| error("storage_error"))?;
            } else {
                unknown += 1;
            }
        }
        let state = if observation["status"] == "available" {
            s(&observation["data"]["dailyUsageBuckets"]["status"])
        } else {
            "omitted"
        };
        let sum_value = if state == "reported" && unknown == 0 {
            json!({"status":"reported","value":sum.to_string()})
        } else {
            json!({"status":if state=="reported"{"not_reported"}else{state},"value":null})
        };
        Ok(
            json!({"source":source,"observation":observation,"fromDate":from,"toDate":to,"buckets":buckets,"sumOfReturnedBuckets":sum_value,"unknownTokenBucketCount":unknown.to_string(),"sourceTimezone":null,"timezoneStatus":"not_documented","warnings":["Sum covers only returned buckets in this source-date range; missing days remain unknown.","Invalid or duplicate source dates are excluded from date-filtered results.","Source date labels are not rebucketed into a local timezone."]}),
        )
    }
    pub fn get_quota(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.get_quota_impl(input))
    }
    fn get_quota_impl(&self, input: &Value) -> CoreResult<Value> {
        let (source, max_age) = self.query_source(input, &[])?;
        Ok(
            json!({"source":source,"observation":self.latest(s(&source["id"]),"account/rateLimits/read",max_age)?,"warnings":["Quota buckets are independent and must not be summed.","Percentages and reset times do not determine exact remaining tokens or prove access recovery."]}),
        )
    }
    fn cursor(
        &self,
        input: Option<&Value>,
        source: &str,
        kind: &str,
    ) -> CoreResult<Option<(String, String)>> {
        let Some(input) = input else { return Ok(None) };
        let text = input.as_str().ok_or_else(|| error("invalid_input"))?;
        require(
            !text.is_empty()
                && text.len() <= 800
                && text
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        )?;
        let decoded = URL_SAFE_NO_PAD
            .decode(text)
            .map_err(|_| error("invalid_input"))?;
        let v: Value = serde_json::from_slice(&decoded).map_err(|_| error("invalid_input"))?;
        exact_keys(&v, &["sourceId", "kind", "at", "key"])?;
        require(v["sourceId"] == source && v["kind"] == kind)?;
        let at = timestamp(&v["at"])?;
        let key = identifier(&v["key"])?;
        if kind != "events" {
            require(
                key.bytes().all(|b| b.is_ascii_digit())
                    && key
                        .parse::<u64>()
                        .map(|n| n <= 9007199254740991)
                        .unwrap_or(false),
            )?;
        }
        Ok(Some((at, key)))
    }
    fn encode_cursor(&self, source: &str, kind: &str, at: &str, key: &str) -> String {
        URL_SAFE_NO_PAD.encode(json!({"sourceId":source,"kind":kind,"at":at,"key":key}).to_string())
    }
    pub fn get_quota_history(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.get_quota_history_impl(input))
    }
    fn get_quota_history_impl(&self, input: &Value) -> CoreResult<Value> {
        let (source, max_age) = self.query_source(input, &["cursor", "limit"])?;
        let lim = limit(input)?;
        let cursor = self.cursor(input.get("cursor"), s(&source["id"]), "quota")?;
        let at = cursor.as_ref().map(|v| v.0.as_str());
        let key = cursor.as_ref().map(|v| v.1.parse::<i64>().unwrap_or(0));
        let mut stmt=safe(self.db()?.prepare_cached("SELECT id,observed_at,payload FROM observations WHERE source_id=? AND method='account/rateLimits/read' AND (? IS NULL OR observed_at<? OR (observed_at=? AND id<?)) ORDER BY observed_at DESC,id DESC LIMIT ?"))?;
        let rows = safe(stmt.query_map(
            params![s(&source["id"]), at, at, at, key, lim + 1],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        ))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| error("storage_error"))?;
        let more = rows.len() > lim as usize;
        let selected = &rows[..rows.len().min(lim as usize)];
        let next = if more {
            selected.last().map(|(key, at, _)| {
                self.encode_cursor(s(&source["id"]), "quota", at, &key.to_string())
            })
        } else {
            None
        };
        let observations = selected
            .iter()
            .map(|(_, _, text)| self.row_observation(&parse(text.clone())?, max_age))
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(
            json!({"source":source,"observations":observations,"nextCursor":next,"coverage":{"completeness":"unknown","betweenSamples":"unknown"},"warnings":["History shows sampled quota snapshots at collection times, not usage costs or causal deltas.","No state is inferred between samples. Independent buckets must not be summed."]}),
        )
    }
    fn filter(&self, input: &Value, extras: &[&str]) -> CoreResult<RecordFilter> {
        let mut allowed = vec!["sourceId", "fromDate", "toDate", "model"];
        allowed.extend(extras);
        exact_keys(input, &allowed)?;
        let source = self.source(&input["sourceId"])?;
        require(input.get("fromDate").is_some() == input.get("toDate").is_some())?;
        let (from, to) = if input.get("fromDate").is_some() {
            let (f, t) = date_range(&input["fromDate"], &input["toDate"])?;
            (Some(f), Some(t))
        } else {
            (None, None)
        };
        let model = input.get("model").map(identifier).transpose()?;
        Ok((source, from, to, model))
    }
    fn event_rows(&self, input: &Value, skills_only: bool) -> CoreResult<Value> {
        let (source, from, to, model) = self.filter(input, &["eventType", "cursor", "limit"])?;
        let kind = input
            .get("eventType")
            .map(|v| enum_value(v, EVENT_TYPES))
            .transpose()?;
        let lim = limit(input)?;
        let cursor = self.cursor(input.get("cursor"), s(&source["id"]), "events")?;
        let at = cursor.as_ref().map(|v| v.0.as_str());
        let key = cursor.as_ref().map(|v| v.1.as_str());
        let mut stmt=safe(self.db()?.prepare_cached("SELECT payload,observed_at,event_id FROM events WHERE source_id=? AND (? IS NULL OR substr(COALESCE(occurred_at,observed_at),1,10) BETWEEN ? AND ?) AND (? IS NULL OR event_type=?) AND (? IS NULL OR model=?) AND (?=0 OR event_type IN ('skill_requested','skill_loaded','skill_invoked')) AND (? IS NULL OR observed_at<? OR (observed_at=? AND event_id<?)) ORDER BY observed_at DESC,event_id DESC LIMIT ?"))?;
        let rows = safe(stmt.query_map(
            params![
                s(&source["id"]),
                from,
                from,
                to,
                kind,
                kind,
                model,
                model,
                skills_only as i64,
                at,
                at,
                at,
                key,
                lim + 1
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        ))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| error("storage_error"))?;
        let selected = &rows[..rows.len().min(lim as usize)];
        let next = if rows.len() > lim as usize {
            selected
                .last()
                .map(|(_, at, key)| self.encode_cursor(s(&source["id"]), "events", at, key))
        } else {
            None
        };
        let events = selected
            .iter()
            .map(|(v, _, _)| parse(v.clone()))
            .collect::<CoreResult<Vec<_>>>()?;
        let mut warnings=vec!["Event dates use reported occurrence time when available, otherwise observation time; unknown timestamps are not fabricated.".to_owned(),"Only directly evidenced event kinds are counted; mentions in content do not establish tool or skill use.".to_owned()];
        warnings.extend(self.import_warnings(s(&source["id"]))?);
        Ok(
            json!({"source":source,"events":events,"nextCursor":next,"coverage":coverage(),"warnings":warnings}),
        )
    }
    pub fn get_events(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.get_events_impl(input))
    }
    fn get_events_impl(&self, input: &Value) -> CoreResult<Value> {
        self.event_rows(input, false)
    }
    pub fn get_skill_evidence(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.get_skill_evidence_impl(input))
    }
    fn get_skill_evidence_impl(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(
            input,
            &[
                "sourceId", "fromDate", "toDate", "model", "cursor", "limit", "kind",
            ],
        )?;
        let mut input = input.clone();
        if let Some(kind) = input
            .as_object_mut()
            .ok_or_else(|| error("invalid_input"))?
            .remove("kind")
        {
            input["eventType"] = json!(format!(
                "skill_{}",
                enum_value(&kind, &["requested", "loaded", "invoked"])?
            ));
        }
        let mut result = self.event_rows(&input, true)?;
        let available = result["events"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        result["status"] = json!(if available {
            "available"
        } else {
            "unavailable"
        });
        result["reason"] = if available {
            Value::Null
        } else {
            json!("no_direct_evidence_collected")
        };
        Ok(result)
    }
    pub fn get_tool_usage(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.get_tool_usage_impl(input))
    }
    fn get_tool_usage_impl(&self, input: &Value) -> CoreResult<Value> {
        let (source, from, to, model) = self.filter(input, &[])?;
        let tools=self.groups("SELECT tool_name,count(*) FROM events WHERE source_id=? AND event_type='tool_call' AND (? IS NULL OR substr(COALESCE(occurred_at,observed_at),1,10) BETWEEN ? AND ?) AND (? IS NULL OR model=?) GROUP BY tool_name ORDER BY count(*) DESC,tool_name LIMIT 500",params![s(&source["id"]),from,from,to,model,model])?;
        let mut warnings=vec!["Counts cover explicit tool-call records only; unknown and unobserved calls are not zero.".to_owned(),"At most 500 distinct tool names are returned.".to_owned()];
        warnings.extend(self.import_warnings(s(&source["id"]))?);
        Ok(json!({"source":source,"tools":tools,"coverage":coverage(),"warnings":warnings}))
    }
    pub fn get_response_token_records(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.get_response_token_records_impl(input))
    }
    fn get_response_token_records_impl(&self, input: &Value) -> CoreResult<Value> {
        let (source, from, to, model) = self.filter(input, &["cursor", "limit"])?;
        let lim = limit(input)?;
        let cursor = self.cursor(input.get("cursor"), s(&source["id"]), "responses")?;
        let at = cursor.as_ref().map(|v| v.0.as_str());
        let key = cursor.as_ref().map(|v| v.1.parse::<i64>().unwrap_or(0));
        let mut stmt=safe(self.db()?.prepare_cached("SELECT id,imported_at,payload FROM response_tokens WHERE source_id=? AND (? IS NULL OR substr(COALESCE(occurred_at,imported_at),1,10) BETWEEN ? AND ?) AND (? IS NULL OR model=?) AND (? IS NULL OR imported_at<? OR (imported_at=? AND id<?)) ORDER BY imported_at DESC,id DESC LIMIT ?"))?;
        let rows = safe(stmt.query_map(
            params![
                s(&source["id"]),
                from,
                from,
                to,
                model,
                model,
                at,
                at,
                at,
                key,
                lim + 1
            ],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        ))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| error("storage_error"))?;
        let selected = &rows[..rows.len().min(lim as usize)];
        let next = if rows.len() > lim as usize {
            selected.last().map(|(key, at, _)| {
                self.encode_cursor(s(&source["id"]), "responses", at, &key.to_string())
            })
        } else {
            None
        };
        let records = selected
            .iter()
            .map(|(_, _, v)| parse(v.clone()))
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(
            json!({"source":source,"records":records,"nextCursor":next,"coverage":response_coverage(),"warnings":self.response_warnings(s(&source["id"]))?}),
        )
    }
    pub fn get_response_token_usage(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.get_response_token_usage_impl(input))
    }
    fn get_response_token_usage_impl(&self, input: &Value) -> CoreResult<Value> {
        let (source, from, to, model) = self.filter(input, &[])?;
        let mut stmt=safe(self.db()?.prepare_cached("SELECT model,input_tokens,cached_input_tokens,cache_write_input_tokens,output_tokens,reasoning_output_tokens,total_tokens FROM response_tokens WHERE source_id=? AND (? IS NULL OR substr(COALESCE(occurred_at,imported_at),1,10) BETWEEN ? AND ?) AND (? IS NULL OR model=?) ORDER BY model"))?;
        let mut rows = safe(stmt.query(params![s(&source["id"]), from, from, to, model, model]))?;
        let mut total = TokenSum::default();
        let mut current: Option<(Option<String>, TokenSum)> = None;
        let mut groups: Vec<(Option<String>, TokenSum)> = Vec::new();
        while let Some(row) = safe(rows.next())? {
            let model: Option<String> = safe(row.get(0))?;
            if current.as_ref().is_some_and(|(m, _)| *m != model)
                && let Some(group) = current.take()
            {
                keep_top_group(&mut groups, group);
            }
            let (_, group) = current.get_or_insert_with(|| (model, TokenSum::default()));
            let mut values = Vec::new();
            for index in 1..=6 {
                let value: String = safe(row.get(index))?;
                values.push(
                    value
                        .parse::<BigUint>()
                        .map_err(|_| error("storage_error"))?,
                );
            }
            total.add(&values);
            group.add(&values);
        }
        if let Some(group) = current {
            keep_top_group(&mut groups, group);
        }
        groups.sort_by(|a, b| b.1.n.cmp(&a.1.n).then(a.0.cmp(&b.0)));
        let by_model:Vec<Value>=groups.iter().map(|(model,total)|json!({"model":model,"responseCount":total.n.to_string(),"totals":total.value()})).collect();
        Ok(
            json!({"source":source,"responseCount":total.n.to_string(),"totals":total.value(),"byModel":by_model,"coverage":response_coverage(),"warnings":self.response_warnings(s(&source["id"]))?}),
        )
    }
}
#[derive(Default)]
struct TokenSum {
    n: u64,
    values: [BigUint; 6],
}
impl TokenSum {
    fn add(&mut self, values: &[BigUint]) {
        self.n += 1;
        for (target, value) in self.values.iter_mut().zip(values) {
            *target += value;
        }
    }
    fn value(&self) -> Value {
        let mut out = json!({});
        for (key, value) in TOKEN_KEYS.iter().zip(&self.values) {
            out[*key] = json!(value.to_string());
        }
        out
    }
}
fn keep_top_group(groups: &mut Vec<(Option<String>, TokenSum)>, group: (Option<String>, TokenSum)) {
    groups.push(group);
    if groups.len() > 500 {
        groups.sort_by(|a, b| b.1.n.cmp(&a.1.n).then(a.0.cmp(&b.0)));
        groups.pop();
    }
}
