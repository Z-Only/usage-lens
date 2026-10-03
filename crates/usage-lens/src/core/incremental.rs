//! Durable, content-free replay metadata for explicitly selected logical streams.
use super::*;
use crate::adapters::rollout::{
    INCREMENTAL_ADAPTER_VERSION, MAX_BYTES, MAX_LINES, ROLLOUT_SOURCE_VERSION,
};
use std::collections::BTreeMap;

fn digest(value: &Value) -> String {
    Sha256::digest(value.to_string().as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn checked_digest(value: &Value) -> CoreResult<&str> {
    let value = value.as_str().ok_or_else(|| error("invalid_input"))?;
    require(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
    )?;
    Ok(value)
}
fn identity(kind: &str, value: &Value) -> String {
    if kind == "event" {
        s(&value["eventId"]).to_owned()
    } else {
        format!(
            "response-{}",
            digest(&json!([
                value["sourceId"],
                value["threadId"],
                value["sessionId"],
                value["responseId"]
            ]))
        )
    }
}
fn immutable(kind: &str, value: &Value) -> Value {
    if kind == "response" {
        return immutable_response(value);
    }
    let mut value = value.clone();
    if let Some(map) = value.as_object_mut() {
        for field in ["observedAt", "collectorVersion"] {
            map.remove(field);
        }
    }
    value
}
fn tombstone_keys(kind: &str, value: &Value) -> Vec<(&'static str, String, String)> {
    let metadata = immutable(kind, value);
    let mut keys = vec![(
        if kind == "event" { "event" } else { "response" },
        identity(kind, value),
        digest(&metadata),
    )];
    if kind == "event" && !value["sourceEventId"].is_null() {
        let mut canonical = metadata;
        canonical
            .as_object_mut()
            .expect("normalized event")
            .remove("eventId");
        keys.push((
            "event_source",
            digest(&json!([value["sourceEventId"], value["eventType"]])),
            digest(&canonical),
        ));
    }
    keys
}
fn checkpoint(value: &Value) -> CoreResult<()> {
    exact_keys(
        value,
        &[
            "completeBytes",
            "completeLines",
            "fingerprint",
            "adapterVersion",
            "sourceVersion",
        ],
    )?;
    integer(&value["completeBytes"], 0, MAX_BYTES as i64)?;
    integer(&value["completeLines"], 0, MAX_LINES as i64)?;
    checked_digest(&value["fingerprint"])?;
    require(
        value["adapterVersion"] == INCREMENTAL_ADAPTER_VERSION
            && value["sourceVersion"] == ROLLOUT_SOURCE_VERSION,
    )
}
impl UsageStore {
    pub(super) fn has_incremental_schema(&self) -> CoreResult<bool> {
        Ok(safe(
            self.db()?
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)),
        )? == 3)
    }
    /// Called only inside a successful explicit incremental-import transaction.
    fn migrate_incremental(&self) -> CoreResult<()> {
        if self.has_incremental_schema()? {
            return Ok(());
        }
        safe(self.db()?.execute_batch("CREATE TABLE rollout_checkpoints(source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,stream_id TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(source_id,stream_id));
        CREATE TABLE rollout_replays(source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,kind TEXT NOT NULL,identity TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(source_id,kind,identity));
        CREATE TABLE record_tombstones(source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,kind TEXT NOT NULL,identity TEXT NOT NULL,digest TEXT NOT NULL,PRIMARY KEY(source_id,kind,identity));
        PRAGMA user_version=3;"))?;
        // Stream rows instead of materializing all existing history during migration.
        for (table, kind) in [("events", "event"), ("response_tokens", "response")] {
            let mut statement = safe(self.db()?.prepare(&format!("SELECT payload FROM {table}")))?;
            let mut rows = safe(statement.query([]))?;
            while let Some(row) = safe(rows.next())? {
                self.remember_retained_record(kind, &parse(safe(row.get(0))?)?)?;
            }
        }
        Ok(())
    }
    pub(super) fn remember_retained_record(&self, kind: &str, value: &Value) -> CoreResult<()> {
        if self.has_incremental_schema()? {
            for (kind, identity, digest) in tombstone_keys(kind, value) {
                safe(self.db()?.execute(
                    "INSERT OR IGNORE INTO record_tombstones(source_id,kind,identity,digest) VALUES(?,?,?,?)",
                    params![s(&value["sourceId"]), kind, identity, digest],
                ))?;
            }
        }
        Ok(())
    }
    /// Both local IDs and canonical source-event aliases survive deletion. Content is never backfilled.
    pub(super) fn retained_record_seen(&self, kind: &str, value: &Value) -> CoreResult<bool> {
        if !self.has_incremental_schema()? {
            return Ok(false);
        }
        let mut seen = false;
        for (key_kind, identity, digest) in tombstone_keys(kind, value) {
            let previous: Option<String> = safe(self.db()?.query_row(
                "SELECT digest FROM record_tombstones WHERE source_id=? AND kind=? AND identity=?",
                params![s(&value["sourceId"]), key_kind, identity], |r| r.get(0),
            ).optional())?;
            if let Some(previous) = previous {
                if previous != digest {
                    return Err(error("rollout_identity_conflict"));
                }
                seen = true;
            }
        }
        if !seen {
            return Ok(false);
        }
        // Keep existing duplicate responses for rows that still exist under either ID.
        let exists: bool = if kind == "event" {
            safe(self.db()?.query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE source_id=? AND (event_id=? OR (source_event_id=? AND event_type=?)))",
                params![s(&value["sourceId"]), s(&value["eventId"]), optional(&value["sourceEventId"]), s(&value["eventType"])], |r| r.get(0),
            ))?
        } else {
            safe(self.db()?.query_row(
                "SELECT EXISTS(SELECT 1 FROM response_tokens WHERE source_id=? AND thread_id=? AND session_id=? AND response_id=?)",
                params![s(&value["sourceId"]), s(&value["threadId"]), s(&value["sessionId"]), s(&value["responseId"])], |r| r.get(0),
            ))?
        };
        Ok(!exists)
    }
    pub fn get_rollout_checkpoint(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(input, &["sourceId", "streamId"])?;
        let source = self.source(&input["sourceId"])?;
        let stream = source_id(&input["streamId"])?;
        self.read(|| {
            if !self.has_incremental_schema()? {
                return Ok(Value::Null);
            }
            let value: Option<String> = safe(
                self.db()?
                    .query_row(
                        "SELECT payload FROM rollout_checkpoints WHERE source_id=? AND stream_id=?",
                        params![s(&source["id"]), stream],
                        |r| r.get(0),
                    )
                    .optional(),
            )?;
            value
                .map(parse)
                .transpose()
                .map(|v| v.unwrap_or(Value::Null))
        })
    }
    /// Commits a bounded parser projection and its expected checkpoint atomically.
    /// Callers verify the old complete-prefix hash against the explicitly supplied file first.
    pub fn import_rollout_incremental(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(
            input,
            &[
                "sourceId",
                "streamId",
                "expectedCheckpoint",
                "fingerprint",
                "adapterVersion",
                "sourceVersion",
                "importedAt",
                "warningCodes",
                "warnings",
                "recordsSeen",
                "completeBytes",
                "completeLines",
                "deferredBytes",
                "events",
                "responseTokens",
                "eventPositions",
                "responsePositions",
                "replayRecords",
            ],
        )?;
        let source = source_id(&input["sourceId"])?;
        let stream = source_id(&input["streamId"])?;
        let imported_at = timestamp(&input["importedAt"])?;
        integer(&input["completeBytes"], 0, MAX_BYTES as i64)?;
        integer(&input["completeLines"], 0, MAX_LINES as i64)?;
        checked_digest(&input["fingerprint"])?;
        require(
            input["adapterVersion"] == INCREMENTAL_ADAPTER_VERSION
                && input["sourceVersion"] == ROLLOUT_SOURCE_VERSION,
        )?;
        integer(&input["recordsSeen"], 0, MAX_LINES as i64)?;
        let warnings = input["warnings"]
            .as_array()
            .ok_or_else(|| error("invalid_input"))?;
        require(warnings.len() <= 100)?;
        for warning in warnings {
            bounded_text(warning, 1024)?;
        }
        // Bound borrowed fields before creating the shared snapshot preflight projection.
        check_event_batch_size(&input["events"], RAW_BYTES)?;
        check_size(&input["responseTokens"], RAW_BYTES)?;
        check_size(&input["warningCodes"], 16 * 1024)?;
        let candidate = json!({"completeBytes":input["completeBytes"],"completeLines":input["completeLines"],"fingerprint":input["fingerprint"],"adapterVersion":input["adapterVersion"],"sourceVersion":input["sourceVersion"]});
        checkpoint(&candidate)?;
        let expected = &input["expectedCheckpoint"];
        if !expected.is_null() {
            checkpoint(expected)?;
        }
        let old_lines = expected["completeLines"].as_u64().unwrap_or(0);
        let lines = input["completeLines"]
            .as_u64()
            .ok_or_else(|| error("invalid_input"))?;
        integer(&input["recordsSeen"], 0, lines as i64)?;
        require(
            lines >= old_lines
                && candidate["completeBytes"].as_u64() >= expected["completeBytes"].as_u64(),
        )?;
        let deferred = integer(&input["deferredBytes"], 0, MAX_BYTES as i64)?;
        require(candidate["completeBytes"].as_i64().unwrap_or(0) + deferred <= MAX_BYTES as i64)?;
        let base = json!({"sourceId":source,"fingerprint":input["fingerprint"],"adapterVersion":input["adapterVersion"],"sourceVersion":input["sourceVersion"],"importedAt":imported_at,"warningCodes":input["warningCodes"],"events":input["events"],"responseTokens":input["responseTokens"]});
        preflight_rollout_import(&base)?;
        let events = input["events"]
            .as_array()
            .ok_or_else(|| error("invalid_input"))?;
        let responses = input["responseTokens"]
            .as_array()
            .ok_or_else(|| error("invalid_input"))?;
        let positions = |field: &str, length: usize| -> CoreResult<Vec<u64>> {
            let values = input[field]
                .as_array()
                .ok_or_else(|| error("invalid_input"))?;
            require(values.len() == length)?;
            values
                .iter()
                .map(|v| integer(v, 1, lines as i64).map(|n| n as u64))
                .collect()
        };
        let event_positions = positions("eventPositions", events.len())?;
        let response_positions = positions("responsePositions", responses.len())?;
        let replays = input["replayRecords"]
            .as_array()
            .ok_or_else(|| error("invalid_input"))?;
        require(replays.len() <= MAX_LINES)?;
        check_size(&input["replayRecords"], 8 * 1024 * 1024)?;
        let mut replay_map = BTreeMap::new();
        for replay in replays {
            exact_keys(replay, &["kind", "identity", "digest", "line"])?;
            let kind = enum_value(&replay["kind"], &["event", "tool_output", "response"])?;
            let id = identifier(&replay["identity"])?;
            let hash = checked_digest(&replay["digest"])?;
            let line = integer(&replay["line"], 1, lines as i64)? as u64;
            if let Some(old) = replay_map.insert((kind, id), (hash, line)) {
                require(old == (hash, line))?;
            }
        }
        self.write(|| {
            self.capture_allowed()?;
            if self.source(&json!(source))?["mode"] != "imported" { return Err(error("imported_source_required")); }
            let current = self.get_rollout_checkpoint(&json!({"sourceId":source,"streamId":stream}))?;
            if current != *expected { return Err(error("rollout_checkpoint_conflict")); }
            self.migrate_incremental()?;
            // Check every immutable identity before writing any projected records.
            for ((kind,id),(hash,_)) in &replay_map {
                let prior: Option<String> = safe(self.db()?.query_row("SELECT digest FROM rollout_replays WHERE source_id=? AND kind=? AND identity=?",params![source,kind,id],|r|r.get(0)).optional())?;
                if prior.as_deref().is_some_and(|old|old != *hash) { return Err(error("rollout_identity_conflict")); }
                require(current != candidate || prior.is_some())?;
            }
            let mut events_inserted = 0;
            let mut responses_inserted = 0;
            for (event,line) in events.iter().zip(event_positions) {
                require(replay_map.get(&("event".to_owned(),s(&event["eventId"]).to_owned())).is_some_and(|(_,position)|*position == line))?;
                // Validate immutable projected fields even on old/duplicate evidence.
                let prepared = self.prepare_event(event)?.0;
                self.retained_record_seen("event", &prepared)?;
                if line > old_lines && self.ingest_event(event)?["inserted"] == true { events_inserted += 1; }
            }
            for (response,line) in responses.iter().zip(response_positions) {
                let prepared = normalize_response_token(response)?;
                require(replay_map.get(&("response".to_owned(),identity("response", &prepared))).is_some_and(|(_,position)|*position == line))?;
                self.retained_record_seen("response", &prepared)?;
                if line > old_lines && self.ingest_response_token(response)?["inserted"] == true { responses_inserted += 1; }
            }
            if current != candidate {
            for ((kind,id),(hash,_)) in replay_map {
                safe(self.db()?.execute("INSERT OR IGNORE INTO rollout_replays(source_id,kind,identity,digest) VALUES(?,?,?,?)",params![source,kind,id,hash]))?;
            }
            safe(self.db()?.execute("INSERT INTO rollout_checkpoints(source_id,stream_id,payload) VALUES(?,?,?) ON CONFLICT(source_id,stream_id) DO UPDATE SET payload=excluded.payload",params![source,stream,candidate.to_string()]))?;
            // Preserve the existing source-health import contract without storing a selected path.
            let metadata = import_metadata(&json!({"fingerprint":input["fingerprint"],"adapterVersion":input["adapterVersion"],"sourceVersion":input["sourceVersion"],"importedAt":imported_at,"warningCodes":input["warningCodes"]}))?;
            safe(self.db()?.execute("INSERT OR IGNORE INTO imports(source_id,fingerprint,imported_at,payload) VALUES(?,?,?,?)",params![source,s(&input["fingerprint"]),imported_at,metadata.to_string()]))?;
            }
            Ok(json!({"eventsInserted":events_inserted.to_string(),"responseTokensInserted":responses_inserted.to_string(),"streamId":stream,"completeBytes":candidate["completeBytes"],"completeLines":candidate["completeLines"],"deferredBytes":deferred,"checkpointAdvanced":current != candidate,"recordsSeen":input["recordsSeen"],"warnings":input["warnings"],"sourceVersion":input["sourceVersion"]}))
        })
    }
}
