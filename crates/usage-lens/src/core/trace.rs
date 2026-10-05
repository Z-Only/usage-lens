//! Explicitly imported, local-only prepared request evidence. Never joined to quota or rollout totals.
use super::*;
use crate::core::redact::redact_text;
use std::collections::BTreeMap;

pub const TRACE_ADAPTER_VERSION: &str = "usage-lens-trace/0.1.0";
pub const TRACE_SOURCE_VERSION: &str = "a956835d020762cb2b570053af06f643a11c0ecc";
pub const TRACE_WARNINGS: [&str; 7] = [
    "Prepared request evidence does not prove that a request was sent, accepted, or billed.",
    "Only explicitly imported local trace attempts are covered; missing attempts and history are unknown.",
    "Requested settings and observed response metadata are separate evidence; defaults and routing are not inferred.",
    "Token totals use directly associated completed responses only and are not combined with rollout or account totals.",
    "At most 500 groups per breakdown are displayed; totals include every matching attempt.",
    "Timeline order follows recorded UTC start timestamps, with attempt identifiers breaking ties; it is not causal or event order.",
    "Timestamp anomalies count completions earlier than starts, indicating clock inconsistency; they are not latency measurements.",
];

fn digest(value: &Value) -> String {
    Sha256::digest(value.to_string().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn fingerprint(value: &Value) -> CoreResult<String> {
    let value = value.as_str().ok_or_else(|| error("invalid_input"))?;
    require(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
    )?;
    Ok(value.to_owned())
}
fn warning_code(value: &Value) -> CoreResult<&str> {
    let text = value.as_str().ok_or_else(|| error("invalid_input"))?;
    require(
        !text.is_empty()
            && text.len() <= 128
            && text.as_bytes()[0].is_ascii_lowercase()
            && text
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
    )?;
    Ok(text)
}
fn state_cell(input: &Value, token: bool) -> CoreResult<Value> {
    exact_keys(input, &["state", "value"])?;
    let state = enum_value(
        &input["state"],
        &["reported", "omitted", "not_reported", "invalid"],
    )?;
    require(input.get("value").is_some())?;
    if state == "reported" {
        let value = if token {
            let value = input["value"]
                .as_str()
                .ok_or_else(|| error("invalid_input"))?;
            require(
                !value.is_empty()
                    && value.len() <= 19
                    && value.bytes().all(|b| b.is_ascii_digit())
                    && (value == "0" || !value.starts_with('0')),
            )?;
            value.parse::<i64>().map_err(|_| error("invalid_input"))?;
            value.to_owned()
        } else {
            let value = bounded_text(&input["value"], 128)?;
            if redact_text(&value) != value {
                return Ok(json!({"state":"invalid","value":null}));
            }
            value
        };
        Ok(json!({"state":state,"value":value}))
    } else {
        require(input["value"].is_null())?;
        Ok(json!({"state":state,"value":null}))
    }
}
fn projection(input: &Value) -> CoreResult<Value> {
    if input.is_null() {
        return Ok(Value::Null);
    }
    exact_keys(input, &["messages", "projection"])?;
    check_size(input, CONTENT_BYTES)?;
    require(input["projection"] == "visible_text_only")?;
    let messages = input["messages"]
        .as_array()
        .ok_or_else(|| error("invalid_input"))?;
    require(messages.len() <= BATCH_EVENTS)?;
    let messages: CoreResult<Vec<_>> = messages
        .iter()
        .map(|message| {
            exact_keys(message, &["role", "text"])?;
            let role = enum_value(&message["role"], &["user", "assistant"])?;
            let text = message["text"]
                .as_str()
                .ok_or_else(|| error("invalid_input"))?;
            Ok(json!({"role":role,"text":redact_text(text)}))
        })
        .collect();
    Ok(json!({"messages":messages?,"projection":"visible_text_only"}))
}
fn normalize_attempt(input: &Value) -> CoreResult<Value> {
    exact_keys(
        input,
        &[
            "attemptId",
            "threadId",
            "turnId",
            "inferenceId",
            "startedAt",
            "completedAt",
            "status",
            "request",
            "observed",
            "responseId",
            "upstreamRequestId",
            "tokens",
            "requestProjection",
            "responseProjection",
            "evidence",
        ],
    )?;
    let started_at = timestamp(&input["startedAt"])?;
    let completed_at = if input["completedAt"].is_null() {
        None
    } else {
        Some(timestamp(&input["completedAt"])?)
    };
    let status = enum_value(
        &input["status"],
        &["completed", "failed", "cancelled", "incomplete"],
    )?;
    require((status == "incomplete") == completed_at.is_none())?;
    require(input["evidence"] == "prepared_request")?;
    exact_keys(
        &input["request"],
        &["model", "reasoningEffort", "serviceTier"],
    )?;
    exact_keys(&input["observed"], &["model", "serviceTier"])?;
    let response_id = nullable_identifier(&input["responseId"])?;
    let tokens = if input["tokens"].is_null() {
        Value::Null
    } else {
        require(status == "completed" && response_id.is_some())?;
        exact_keys(&input["tokens"], TOKEN_KEYS)?;
        let mut normalized = serde_json::Map::new();
        for key in TOKEN_KEYS {
            normalized.insert((*key).to_owned(), state_cell(&input["tokens"][key], true)?);
        }
        Value::Object(normalized)
    };
    Ok(json!({
        "attemptId":identifier(&input["attemptId"])? ,
        "threadId":identifier(&input["threadId"])? ,
        "turnId":identifier(&input["turnId"])? ,
        "inferenceId":identifier(&input["inferenceId"])? ,
        "startedAt":started_at,"completedAt":completed_at,"status":status,
        "request":{"model":state_cell(&input["request"]["model"],false)?,"reasoningEffort":state_cell(&input["request"]["reasoningEffort"],false)?,"serviceTier":state_cell(&input["request"]["serviceTier"],false)?},
        "observed":{"model":state_cell(&input["observed"]["model"],false)?,"serviceTier":state_cell(&input["observed"]["serviceTier"],false)?},
        "responseId":response_id,"upstreamRequestId":nullable_identifier(&input["upstreamRequestId"])? ,
        "tokens":tokens,"requestProjection":projection(&input["requestProjection"])? ,"responseProjection":projection(&input["responseProjection"])? ,"evidence":"prepared_request"
    }))
}
#[path = "trace_filters.rs"]
mod trace_filters;
use trace_filters::{FILTER_KEYS, FILTER_SQL, TraceFilters, TraceOrder};
#[path = "trace_insights.rs"]
mod trace_insights;
use trace_insights::TraceInsights;

fn timestamp_anomaly(attempt: &Value) -> bool {
    // Imported timestamps are normalized UTC strings with fixed millisecond precision.
    attempt["completedAt"]
        .as_str()
        .zip(attempt["startedAt"].as_str())
        .is_some_and(|(completed, started)| completed < started)
}

fn optional_range(input: &Value) -> CoreResult<(Option<String>, Option<String>)> {
    match (input.get("fromDate"), input.get("toDate")) {
        (None, None) => Ok((None, None)),
        (Some(from), Some(to)) => date_range(from, to).map(|(f, t)| (Some(f), Some(t))),
        _ => Err(error("invalid_input")),
    }
}

#[derive(Default)]
struct Totals {
    count: u64,
    token_count: u64,
    timestamp_anomaly_count: u64,
    sums: [BigUint; 6],
    coverage: [[u64; 4]; 6],
}
impl Totals {
    fn add(&mut self, attempt: &Value) {
        self.count += 1;
        self.timestamp_anomaly_count += u64::from(timestamp_anomaly(attempt));
        let tokens = &attempt["tokens"];
        if !tokens.is_null() {
            self.token_count += 1;
        }
        for (index, key) in TOKEN_KEYS.iter().enumerate() {
            let cell = &tokens[key];
            let state = s(&cell["state"]);
            let position = match state {
                "reported" => 0,
                "omitted" => 1,
                "invalid" => 3,
                _ => 2,
            };
            self.coverage[index][position] += 1;
            if position == 0 {
                // Stored values were validated as nonnegative i64 decimal strings.
                self.sums[index] += s(&cell["value"]).parse::<BigUint>().unwrap_or_default();
            }
        }
    }
    fn value(&self) -> Value {
        let mut totals = serde_json::Map::new();
        let mut coverage = serde_json::Map::new();
        for (index, key) in TOKEN_KEYS.iter().enumerate() {
            let counts = self.coverage[index];
            totals.insert(
                (*key).into(),
                if counts[0] == 0 {
                    Value::Null
                } else {
                    json!(self.sums[index].to_string())
                },
            );
            coverage.insert((*key).into(), json!({"reportedCount":counts[0].to_string(),"omittedCount":counts[1].to_string(),"notReportedCount":counts[2].to_string(),"invalidCount":counts[3].to_string()}));
        }
        json!({"count":self.count.to_string(),"tokenAttemptCount":self.token_count.to_string(),"timestampAnomalyCount":self.timestamp_anomaly_count.to_string(),"totals":totals,"tokenCoverage":coverage})
    }
}
struct TraceBundle {
    source: String,
    bundle: String,
    fingerprint: String,
    imported_at: String,
    warnings: Vec<Value>,
    attempts: Vec<Value>,
    bundle_digest: String,
}
impl TraceBundle {
    fn parse(input: &Value) -> CoreResult<Self> {
        exact_keys(
            input,
            &[
                "sourceId",
                "fingerprint",
                "adapterVersion",
                "sourceVersion",
                "importedAt",
                "bundleId",
                "attempts",
                "warningCodes",
            ],
        )?;
        check_size(input, RAW_BYTES)?;
        let source = source_id(&input["sourceId"])?;
        let bundle = identifier(&input["bundleId"])?;
        let fingerprint = fingerprint(&input["fingerprint"])?;
        let imported_at = timestamp(&input["importedAt"])?;
        require(
            input["adapterVersion"] == TRACE_ADAPTER_VERSION
                && input["sourceVersion"] == TRACE_SOURCE_VERSION,
        )?;
        let warnings = input["warningCodes"]
            .as_array()
            .ok_or_else(|| error("invalid_input"))?;
        require(warnings.len() <= 100)?;
        for warning in warnings {
            warning_code(warning)?;
        }
        let rows = input["attempts"]
            .as_array()
            .ok_or_else(|| error("invalid_input"))?;
        require(rows.len() <= BATCH_EVENTS)?;
        let attempts: Vec<Value> = rows
            .iter()
            .map(normalize_attempt)
            .collect::<CoreResult<_>>()?;
        let bundle_digest = digest(&json!({"bundleId":bundle,"attempts":attempts}));
        Ok(Self {
            source,
            bundle,
            fingerprint,
            imported_at,
            warnings: warnings.clone(),
            attempts,
            bundle_digest,
        })
    }
}
struct TracePlan {
    indexes: Vec<usize>,
    already_present: bool,
    capture: bool,
    schema: i64,
}
impl TracePlan {
    fn retained(&self, bundle: &TraceBundle) -> usize {
        self.indexes
            .iter()
            .filter(|index| {
                let attempt = &bundle.attempts[**index];
                self.capture
                    && (!attempt["requestProjection"].is_null()
                        || !attempt["responseProjection"].is_null())
            })
            .count()
    }
}
impl UsageStore {
    pub(super) fn has_trace_schema(&self) -> CoreResult<bool> {
        Ok(safe(
            self.db()?
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0)),
        )? >= 4)
    }
    fn migrate_traces(&self) -> CoreResult<()> {
        if self.has_trace_schema()? {
            return Ok(());
        }
        self.migrate_incremental()?;
        safe(self.db()?.execute_batch("CREATE TABLE trace_imports(source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,bundle_id TEXT NOT NULL,fingerprint TEXT NOT NULL,digest TEXT NOT NULL,imported_at TEXT NOT NULL,warning_codes TEXT NOT NULL,PRIMARY KEY(source_id,bundle_id),UNIQUE(source_id,fingerprint));
        CREATE INDEX trace_imports_latest ON trace_imports(source_id,imported_at DESC,bundle_id DESC);
        CREATE TABLE trace_tombstones(source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,attempt_id TEXT NOT NULL,identity TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(source_id,attempt_id),UNIQUE(source_id,identity));
        CREATE TABLE trace_response_tombstones(source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,response_id TEXT NOT NULL,attempt_id TEXT NOT NULL,PRIMARY KEY(source_id,response_id));
        CREATE TABLE trace_attempts(source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,attempt_id TEXT NOT NULL,started_at TEXT NOT NULL,imported_at TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(source_id,attempt_id));
        CREATE INDEX trace_attempts_latest ON trace_attempts(source_id,started_at DESC,attempt_id DESC);
        CREATE TABLE trace_details(source_id TEXT NOT NULL,attempt_id TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(source_id,attempt_id),FOREIGN KEY(source_id,attempt_id) REFERENCES trace_attempts(source_id,attempt_id) ON DELETE CASCADE);
        PRAGMA user_version=4;"))
    }
    fn trace_coverage(&self, source: &str) -> CoreResult<Value> {
        let available = self.has_trace_schema()?
            && safe(self.db()?.query_row(
                "SELECT EXISTS(SELECT 1 FROM trace_imports WHERE source_id=?)",
                [source],
                |row| row.get::<_, bool>(0),
            ))?;
        Ok(
            json!({"capture":if available {"available"} else {"not_captured"},"completeness":"partial","missingAttempts":"unknown","requestEvidence":"prepared_not_confirmed_sent","tokenAttribution":"direct_response_only","accountTotalRelationship":"not_combined","dateBasis":"attempt_start_utc"}),
        )
    }
    fn trace_import_warnings(&self, source: &str) -> CoreResult<Value> {
        let mut codes = Vec::new();
        let mut seen = BTreeSet::new();
        let mut truncated = false;
        if self.has_trace_schema()? {
            let mut statement = safe(self.db()?.prepare("SELECT warning_codes FROM trace_imports WHERE source_id=? ORDER BY imported_at DESC,bundle_id DESC LIMIT 101"))?;
            let mut rows = safe(statement.query([source]))?;
            let mut imports = 0;
            while let Some(row) = safe(rows.next())? {
                imports += 1;
                if imports > 100 {
                    truncated = true;
                    break;
                }
                let value = parse(safe(row.get(0))?)?;
                let warnings = value.as_array().ok_or_else(|| error("storage_error"))?;
                if warnings.len() > 100 {
                    return Err(error("storage_error"));
                }
                for warning in warnings {
                    // Revalidate stored metadata so corruption cannot expose arbitrary content.
                    let code = warning_code(warning).map_err(|_| error("storage_error"))?;
                    if seen.insert(code.to_owned()) {
                        if codes.len() == 100 {
                            truncated = true;
                        } else {
                            codes.push(code.to_owned());
                        }
                    }
                }
            }
        }
        Ok(json!({"scope":"all_retained_source","codes":codes,"truncated":truncated}))
    }
    fn with_trace_warnings(&self, source: &str, mut result: Value) -> CoreResult<Value> {
        let import_warnings = self.trace_import_warnings(source)?;
        let warnings = result["warnings"]
            .as_array_mut()
            .expect("trace result warnings");
        if let Some(codes) = import_warnings["codes"]
            .as_array()
            .filter(|codes| !codes.is_empty())
        {
            warnings.push(json!("Import warning codes cover retained source-wide import metadata, independently of attempt date filters or record retention."));
            for code in codes {
                warnings.push(json!(format!("Import coverage: {}", s(code))));
            }
        }
        if import_warnings["truncated"] == true {
            warnings.push(json!("Import warning codes are truncated to the latest 100 imports and 100 distinct codes; older warnings may be missing."));
        }
        result["importWarnings"] = import_warnings;
        Ok(result)
    }
    /// Shared immutable-replay planner. It never migrates or writes. The writer
    /// calls it again inside its own transaction; a preview is not a reservation.
    fn plan_trace_import(&self, bundle: &TraceBundle) -> CoreResult<TracePlan> {
        self.capture_allowed()?;
        if self.source(&json!(bundle.source))?["mode"] != "imported" {
            return Err(error("imported_source_required"));
        }
        let schema = safe(
            self.db()?
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0)),
        )?;
        let mut plan = TracePlan {
            indexes: Vec::new(),
            already_present: false,
            capture: self.get_settings()?["contentCaptureEnabled"] == true,
            schema,
        };
        if schema >= 4 {
            let prior: Option<(String,String,String)> = safe(self.db()?.query_row("SELECT bundle_id,fingerprint,digest FROM trace_imports WHERE source_id=? AND (bundle_id=? OR fingerprint=?)", params![bundle.source,bundle.bundle,bundle.fingerprint], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional())?;
            if let Some((old_bundle, old_fingerprint, old_digest)) = prior {
                if old_bundle != bundle.bundle
                    || old_fingerprint != bundle.fingerprint
                    || old_digest != bundle.bundle_digest
                {
                    return Err(error("trace_identity_conflict"));
                }
                plan.already_present = true;
                return Ok(plan);
            }
        }
        let mut ids: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut identities = BTreeMap::new();
        let mut responses = BTreeMap::new();
        for (index, attempt) in bundle.attempts.iter().enumerate() {
            let id = s(&attempt["attemptId"]);
            let identity = digest(&json!([
                attempt["threadId"],
                attempt["turnId"],
                attempt["inferenceId"]
            ]));
            let hash = digest(attempt);
            if let Some((old_identity, old_digest)) = ids.get(id) {
                if old_identity != &identity || old_digest != &hash {
                    return Err(error("trace_identity_conflict"));
                }
                continue;
            }
            if identities.get(&identity).is_some_and(|owner| owner != id) {
                return Err(error("trace_identity_conflict"));
            }
            ids.insert(id.to_owned(), (identity.clone(), hash.clone()));
            identities.insert(identity.clone(), id.to_owned());
            if schema >= 4 {
                let old: Option<(String,String,String)> = safe(self.db()?.query_row("SELECT attempt_id,identity,digest FROM trace_tombstones WHERE source_id=? AND (attempt_id=? OR identity=?)", params![bundle.source,id,identity], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional())?;
                if let Some((old_id, old_identity, old_digest)) = old {
                    if old_id != id || old_identity != identity || old_digest != hash {
                        return Err(error("trace_identity_conflict"));
                    }
                    continue;
                }
            }
            if let Some(response) = attempt["responseId"].as_str() {
                if responses.get(response).is_some_and(|owner| owner != id) {
                    return Err(error("trace_identity_conflict"));
                }
                if schema >= 4 {
                    let owner: Option<String> = safe(self.db()?.query_row("SELECT attempt_id FROM trace_response_tombstones WHERE source_id=? AND response_id=?", params![bundle.source,response], |row| row.get(0)).optional())?;
                    if owner.as_deref().is_some_and(|owner| owner != id) {
                        return Err(error("trace_identity_conflict"));
                    }
                }
                responses.insert(response.to_owned(), id.to_owned());
            }
            plan.indexes.push(index);
        }
        Ok(plan)
    }
    /// Read-only preview against the current store snapshot. No content or record
    /// identifiers are returned; the exact same validation and replay plan runs on import.
    pub fn preflight_trace_bundle(&self, input: &Value) -> CoreResult<Value> {
        let bundle = TraceBundle::parse(input)?;
        self.read(|| {
            let plan = self.plan_trace_import(&bundle)?;
            Ok(json!({
                "schemaVersion":1,"operation":"trace_import_preflight","dryRun":true,
                "status":"ready","attemptsInBundle":bundle.attempts.len().to_string(),
                "attemptsWouldInsert":plan.indexes.len().to_string(),
                "attemptsAlreadyPresent":(bundle.attempts.len()-plan.indexes.len()).to_string(),
                "contentsWouldRetain":plan.retained(&bundle).to_string(),
                "importAlreadyPresent":plan.already_present,
                "database":{"accessMode":"read_only","schemaVersion":plan.schema,"targetSchemaVersion":4,"wouldUpgrade":plan.schema<4},
                "contentCaptureEnabled":plan.capture,"warningCodes":bundle.warnings,
                "warnings":TRACE_WARNINGS,
                "nextSteps":[
                    if plan.schema<4 {"Stop every writer and make a verified private backup before importing; success upgrades to schema 4, unreadable by v0.5.0 and older."} else {"Keep a verified private backup before changing the store; this preview creates no backup."},
                    if plan.capture {"Only eligible redacted visible-text projections from newly accepted attempts would be retained; redaction is best-effort."} else {"Content capture is off. An import would retain metadata only; enabling capture later cannot backfill these attempts."},
                    "This preview describes one read-only snapshot, not a reservation. Files, settings or records may change before the explicit import, which validates again atomically.",
                    "Run the same explicit import without --dry-run only after reviewing the warnings. No recording, client setup or live compatibility was checked."
                ]
            }))
        })
    }
    /// Atomically imports one explicitly selected immutable bundle. Metadata-only imports still
    /// retain content-free identity hashes, so a later import can never backfill deleted content.
    pub fn import_trace_bundle(&self, input: &Value) -> CoreResult<Value> {
        let bundle = TraceBundle::parse(input)?;
        self.write(|| {
            let plan = self.plan_trace_import(&bundle)?;
            if plan.already_present {
                return Ok(json!({"attemptsInserted":"0","contentsRetained":"0","importAlreadyPresent":true,"warningCodes":bundle.warnings,"warnings":TRACE_WARNINGS}));
            }
            self.migrate_traces()?;
            for index in &plan.indexes {
                let attempt = &bundle.attempts[*index];
                let id = s(&attempt["attemptId"]);
                let identity = digest(&json!([attempt["threadId"],attempt["turnId"],attempt["inferenceId"]]));
                let hash = digest(attempt);
                if let Some(response) = attempt["responseId"].as_str() {
                    safe(self.db()?.execute("INSERT INTO trace_response_tombstones(source_id,response_id,attempt_id) VALUES(?,?,?)",params![bundle.source,response,id]))?;
                }
                let mut metadata = attempt.clone();
                let map = metadata.as_object_mut().expect("normalized attempt");
                let request = map.remove("requestProjection").unwrap_or(Value::Null);
                let response = map.remove("responseProjection").unwrap_or(Value::Null);
                map.insert("sourceId".into(),json!(bundle.source));
                map.insert("bundleId".into(),json!(bundle.bundle));
                map.insert("importedAt".into(),json!(bundle.imported_at));
                safe(self.db()?.execute("INSERT INTO trace_tombstones(source_id,attempt_id,identity,digest) VALUES(?,?,?,?)",params![bundle.source,id,identity,hash]))?;
                safe(self.db()?.execute("INSERT INTO trace_attempts(source_id,attempt_id,started_at,imported_at,payload) VALUES(?,?,?,?,?)",params![bundle.source,id,s(&attempt["startedAt"]),bundle.imported_at,metadata.to_string()]))?;
                if plan.capture && (!request.is_null() || !response.is_null()) {
                    safe(self.db()?.execute("INSERT INTO trace_details(source_id,attempt_id,payload) VALUES(?,?,?)",params![bundle.source,id,json!({"requestProjection":request,"responseProjection":response}).to_string()]))?;
                }
            }
            safe(self.db()?.execute("INSERT INTO trace_imports(source_id,bundle_id,fingerprint,digest,imported_at,warning_codes) VALUES(?,?,?,?,?,?)",params![bundle.source,bundle.bundle,bundle.fingerprint,bundle.bundle_digest,bundle.imported_at,json!(bundle.warnings).to_string()]))?;
            Ok(json!({"attemptsInserted":plan.indexes.len().to_string(),"contentsRetained":plan.retained(&bundle).to_string(),"importAlreadyPresent":false,"warningCodes":bundle.warnings,"warnings":TRACE_WARNINGS}))
        })
    }
    pub fn get_trace_attempts(&self, input: &Value) -> CoreResult<Value> {
        let allowed = ["sourceId", "limit", "cursor", "fromDate", "toDate", "order"]
            .into_iter()
            .chain(FILTER_KEYS)
            .collect::<Vec<_>>();
        exact_keys(input, &allowed)?;
        let limit = limit(input)?;
        let filters = TraceFilters::parse(input)?;
        let order = TraceOrder::parse(input.get("order"))?;
        self.read(|| {
            let source = self.source(&input["sourceId"])?;
            let id = s(&source["id"]);
            let cursor = filters.cursor(input.get("cursor"),id,order)?;
            let mut records = Vec::new();
            let mut next = None;
            if self.has_trace_schema()? {
                let mut parameters = filters.params(id);
                parameters.push(cursor.as_ref().map(|(at,_)|at.clone()).into());
                parameters.push(cursor.as_ref().map(|(_,key)|key.clone()).into());
                parameters.push((limit+1).into());
                let (comparison, direction) = order.sql();
                let sql = format!("SELECT a.payload,a.started_at,a.attempt_id,d.attempt_id IS NOT NULL FROM trace_attempts a LEFT JOIN trace_details d ON d.source_id=a.source_id AND d.attempt_id=a.attempt_id WHERE {FILTER_SQL} AND (?9 IS NULL OR a.started_at{comparison}?9 OR (a.started_at=?9 AND a.attempt_id{comparison}?10)) ORDER BY a.started_at {direction},a.attempt_id {direction} LIMIT ?11");
                let mut statement = safe(self.db()?.prepare(&sql))?;
                let mut rows = safe(statement.query(rusqlite::params_from_iter(parameters)))?;
                let mut last: Option<(String, String)> = None;
                while let Some(row) = safe(rows.next())? {
                    if records.len() == limit as usize {
                        if let Some((at,key)) = last { next = Some(filters.encode_cursor(id,&at,&key,order)); }
                        break;
                    }
                    let mut value = parse(safe(row.get(0))?)?;
                    value["contentRetained"] = json!(safe(row.get::<_,bool>(3))?);
                    value["timestampAnomaly"] = json!(timestamp_anomaly(&value));
                    records.push(value);
                    last = Some((safe(row.get::<_,String>(1))?,safe(row.get::<_,String>(2))?));
                }
            }
            let mut result = json!({"source":source,"attempts":records,"order":order.name(),"nextCursor":next,"coverage":self.trace_coverage(id)?,"warnings":TRACE_WARNINGS});
            filters.echo(&mut result);
            self.with_trace_warnings(id,result)
        })
    }
    pub fn get_local_trace_detail(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(input, &["sourceId", "attemptId"])?;
        let attempt = identifier(&input["attemptId"])?;
        self.read(|| {
            let source = self.source(&input["sourceId"])?;
            if !self.has_trace_schema()? { return Err(error("trace_attempt_not_found")); }
            let row: Option<(String,Option<String>)> = safe(self.db()?.query_row("SELECT a.payload,d.payload FROM trace_attempts a LEFT JOIN trace_details d ON d.source_id=a.source_id AND d.attempt_id=a.attempt_id WHERE a.source_id=? AND a.attempt_id=?",params![s(&source["id"]),attempt],|row|Ok((row.get(0)?,row.get(1)?))).optional())?;
            let (value,content) = row.ok_or_else(|| error("trace_attempt_not_found"))?;
            let mut value = parse(value)?;
            value["timestampAnomaly"] = json!(timestamp_anomaly(&value));
            self.with_trace_warnings(s(&source["id"]),json!({"attempt":value,"content":content.as_ref().map(|v|parse(v.clone())).transpose()?,"contentRetained":content.is_some(),"warnings":[TRACE_WARNINGS[0],if content.is_some(){CONTENT_WARNING}else{"No local visible-text projection was retained for this attempt."}]}))
        })
    }
    pub fn get_trace_summary(&self, input: &Value) -> CoreResult<Value> {
        let allowed = ["sourceId", "fromDate", "toDate"]
            .into_iter()
            .chain(FILTER_KEYS)
            .collect::<Vec<_>>();
        exact_keys(input, &allowed)?;
        let filters = TraceFilters::parse(input)?;
        self.read(|| {
            let source = self.source(&input["sourceId"])?;
            let id = s(&source["id"]);
            let mut total = Totals::default();
            let mut status: BTreeMap<String, Totals> = BTreeMap::new();
            let mut groups: [BTreeMap<(String, String), Totals>; 5] =
                std::array::from_fn(|_| BTreeMap::new());
            let mut groups_truncated = false;
            let mut insights = TraceInsights::default();
            if self.has_trace_schema()? {
                let sql = format!("SELECT a.payload FROM trace_attempts a WHERE {FILTER_SQL}");
                let mut statement = safe(self.db()?.prepare(&sql))?;
                let mut rows =
                    safe(statement.query(rusqlite::params_from_iter(filters.params(id))))?;
                while let Some(row) = safe(rows.next())? {
                    let value = parse(safe(row.get(0))?)?;
                    total.add(&value);
                    insights.add(&value);
                    status
                        .entry(s(&value["status"]).to_owned())
                        .or_default()
                        .add(&value);
                    for (index, (section, key)) in [
                        ("request", "model"),
                        ("request", "reasoningEffort"),
                        ("request", "serviceTier"),
                        ("observed", "model"),
                        ("observed", "serviceTier"),
                    ]
                    .iter()
                    .enumerate()
                    {
                        let cell = &value[section][key];
                        let key = (s(&cell["state"]).to_owned(), s(&cell["value"]).to_owned());
                        let group = &mut groups[index];
                        if !group.contains_key(&key) && group.len() == 500 {
                            groups_truncated = true;
                            // Keep the lexicographically smallest keys while streaming. The
                            // threshold only decreases, so every retained group has complete counts.
                            if group.last_key_value().is_some_and(|(last, _)| &key < last) {
                                group.pop_last();
                            } else {
                                continue;
                            }
                        }
                        group.entry(key).or_default().add(&value);
                    }
                }
            }
            let mut result = total.value();
            result
                .as_object_mut()
                .expect("totals object")
                .remove("count");
            result["source"] = source.clone();
            result["attemptCount"] = json!(total.count.to_string());
            result["groupsTruncated"] = json!(groups_truncated);
            result["byStatus"] = Value::Array(
                status
                    .iter()
                    .map(|(status, total)| {
                        let mut value = total.value();
                        value["status"] = json!(status);
                        value
                    })
                    .collect(),
            );
            for (name, group) in [
                "byRequestedModel",
                "byRequestedReasoningEffort",
                "byRequestedServiceTier",
                "byObservedModel",
                "byObservedServiceTier",
            ]
            .iter()
            .zip(groups)
            {
                result[name] = Value::Array(
                    group
                        .into_iter()
                        .map(|((state, text), total)| {
                            let mut value = total.value();
                            value["value"] = if state == "reported" {
                                json!(text)
                            } else {
                                Value::Null
                            };
                            value["state"] = json!(state);
                            value
                        })
                        .collect(),
                );
            }
            insights.echo(&mut result);
            filters.echo(&mut result);
            result["coverage"] = self.trace_coverage(id)?;
            result["warnings"] = json!(TRACE_WARNINGS);
            self.with_trace_warnings(id, result)
        })
    }
}
