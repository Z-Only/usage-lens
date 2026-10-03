//! Read-only collection diagnostics. This is a child of `store` so it can use
//! its transaction and validation boundary without widening storage access.
use super::*;

const HEALTH_WARNINGS: &[&str] = &[
    "Source IDs are user-assigned local namespaces, not verified account bindings.",
    "Measured means locally retained record counts and collection timestamps only; it does not establish complete history or verified backend freshness.",
    "Collection time is not backend data freshness; source-as-of time is separately reported when supplied.",
    "Zero stored records means no retained evidence, not zero historical activity. Missing records and pre-collection history remain unknown.",
    "Skill evidence overlaps with events and does not establish task success or token attribution.",
    "Import counts describe retained import-ledger entries; imports without metadata and duplicate import attempts are not counted.",
    "Capture and content settings apply to all sources and describe current policy, not historical collection coverage.",
    "Retention cleanup and deletion can remove evidence; no completeness percentage, costs, or remaining quota are inferred.",
];

/// Future-dated timestamps have no meaningful nonnegative age. Keep that state
/// separate rather than clamping it to zero and accidentally declaring it fresh.
fn time_state(at: &str, now_ms: i64, max_age: i64) -> CoreResult<(&'static str, Option<i64>)> {
    let at_ms = millis(at)?;
    if at_ms > now_ms {
        Ok(("future", None))
    } else {
        let age = now_ms - at_ms;
        Ok((if age > max_age { "stale" } else { "fresh" }, Some(age)))
    }
}

impl UsageStore {
    /// Aggregate metadata only, scoped to one explicit source. This query does
    /// not collect, migrate, modify capture settings, or return record contents.
    pub fn get_health(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.get_health_impl(input))
    }

    fn get_health_impl(&self, input: &Value) -> CoreResult<Value> {
        let (source, max_age) = self.query_source(input, &[])?;
        let id = s(&source["id"]);
        let checked_at = self.now()?;
        let now_ms = millis(&checked_at)?;
        let observations = METHODS
            .iter()
            .map(|method| self.observation_health(id, method, now_ms, max_age))
            .collect::<CoreResult<Vec<_>>>()?;
        let imports = self.import_health(id)?;
        let settings = self.get_settings()?;
        let mut warnings: Vec<String> = HEALTH_WARNINGS.iter().map(|v| v.to_string()).collect();
        if imports["warningCodesTruncated"] == true {
            warnings.push("Import warning codes are limited to the 100 latest stored imports and 100 distinct codes; additional warnings may be unknown.".to_owned());
        }
        if let Some(codes) = imports["warningCodes"].as_array() {
            warnings.extend(
                codes
                    .iter()
                    .map(|code| format!("Import coverage: {}", s(code))),
            );
        }
        Ok(json!({
            "schemaVersion":1,
            "source":{
                "id":source["id"],"mode":source["mode"],"provider":source["provider"],
                "accountBinding":"unverified_local_namespace"
            },
            "checkedAt":checked_at,"maxAgeMs":max_age,
            "settings":{
                "capturePaused":settings["capturePaused"],
                "contentCaptureEnabled":settings["contentCaptureEnabled"],
                "retentionDays":settings["retentionDays"],"scope":"all_sources"
            },
            "observations":observations,
            "stored":{
                "events":self.record_health(id,"events")?,
                "skills":self.record_health(id,"skills")?,
                "responseTokens":self.record_health(id,"response_tokens")?,
                "imports":imports
            },
            "coverage":{
                "completeness":"partial","missingRecords":"unknown",
                "preCollectionHistory":"unknown"
            },
            "provenance":{"basis":"stored_records","measurement":"measured","estimated":false},
            "warnings":warnings
        }))
    }

    fn observation_health(
        &self,
        id: &str,
        method: &str,
        now_ms: i64,
        max_age: i64,
    ) -> CoreResult<Value> {
        // Select only metadata; neither normalized account values nor raw content
        // are necessary for determining collection health.
        let row: Option<(String, Option<String>, String)> = safe(self.db()?.query_row(
            "SELECT observed_at,json_extract(payload,'$.sourceAsOf'),json_extract(payload,'$.provenance') FROM observations WHERE source_id=? AND method=? ORDER BY observed_at DESC,id DESC LIMIT 1",
            params![id,method],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
        ).optional())?;
        let mut failure = self.latest_failure(id, method, None)?;
        let at_or_after = match &row {
            Some((at, _, _)) => s(&failure["attemptedAt"]) >= at.as_str(),
            None => true,
        };
        let unsupported = failure["errorCode"] == "unsupported_method" && at_or_after;
        if !failure.is_null() {
            let (state, age) = time_state(s(&failure["attemptedAt"]), now_ms, max_age)?;
            failure = json!({
                "errorCode":failure["errorCode"],"attemptedAt":failure["attemptedAt"],
                "state":if state=="fresh" {"recent"} else {state},
                "ageMs":age,"atOrAfterLatestObservation":at_or_after
            });
        }
        let (freshness, provenance) = match &row {
            Some((at, as_of, provenance)) => {
                let (state, age) = time_state(at, now_ms, max_age)?;
                let provenance = parse(provenance.clone())?;
                (
                    json!({
                        "state":state,"observedAt":at,"sourceAsOf":as_of,
                        "sourceAsOfStatus":if as_of.is_some() {"reported"} else {"not_provided"},
                        "ageMs":age,"maxAgeMs":max_age,"basis":"collection_time"
                    }),
                    json!({
                        "sourceId":id,"provider":provenance["provider"],"method":method,
                        "adapterVersion":provenance["adapterVersion"],
                        "schemaBaseline":provenance["schemaBaseline"],"mode":provenance["mode"]
                    }),
                )
            }
            None => (
                json!({
                    "state":"unknown","observedAt":null,"sourceAsOf":null,
                    "sourceAsOfStatus":"not_provided","ageMs":null,"maxAgeMs":max_age,
                    "basis":"collection_time"
                }),
                Value::Null,
            ),
        };
        Ok(json!({
            "method":method,"availability":if row.is_some() {"available"} else {"missing"},
            "capability":if unsupported {"unsupported"} else if row.is_some() {"observed"} else {"unknown"},
            "freshness":freshness,"lastFailure":failure,"provenance":provenance
        }))
    }

    fn record_health(&self, id: &str, kind: &str) -> CoreResult<Value> {
        // The table and column fragments are fixed internal choices, never input.
        let (table, capture, condition) = match kind {
            "response_tokens" => ("response_tokens", "imported_at", ""),
            "skills" => (
                "events",
                "observed_at",
                " AND event_type IN ('skill_requested','skill_loaded','skill_invoked')",
            ),
            _ => ("events", "observed_at", ""),
        };
        let sql = format!(
            "SELECT count(*),min({capture}),max({capture}),min(occurred_at),max(occurred_at),count(*)-count(occurred_at) FROM {table} WHERE source_id=?{condition}"
        );
        safe(self.db()?.query_row(&sql, [id], |r| {
            Ok(json!({
                "count":r.get::<_,i64>(0)?.to_string(),
                "firstCapturedAt":r.get::<_,Option<String>>(1)?,
                "lastCapturedAt":r.get::<_,Option<String>>(2)?,
                "firstOccurredAt":r.get::<_,Option<String>>(3)?,
                "lastOccurredAt":r.get::<_,Option<String>>(4)?,
                "unknownOccurredAtCount":r.get::<_,i64>(5)?.to_string()
            }))
        }))
    }

    fn import_health(&self, id: &str) -> CoreResult<Value> {
        let (count, first, last): (i64, Option<String>, Option<String>) =
            safe(self.db()?.query_row(
                "SELECT count(*),min(imported_at),max(imported_at) FROM imports WHERE source_id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            ))?;
        let rows = self.json_rows(
            "SELECT payload FROM imports WHERE source_id=? ORDER BY imported_at DESC,fingerprint LIMIT 100",
            [id],
        )?;
        let mut codes = BTreeSet::new();
        let mut truncated = count > rows.len() as i64;
        for row in &rows {
            // Enforce the existing safe warning-code grammar before projection.
            // In particular, hand-edited metadata cannot echo a path or a body.
            let metadata = import_metadata(row).map_err(|_| error("storage_error"))?;
            for code in metadata["warningCodes"]
                .as_array()
                .ok_or_else(|| error("storage_error"))?
            {
                if codes.len() < 100 || codes.contains(s(code)) {
                    codes.insert(s(code).to_owned());
                } else {
                    truncated = true;
                }
            }
        }
        Ok(json!({
            "count":count.to_string(),"firstImportedAt":first,"lastImportedAt":last,
            "warningCodes":codes,"warningCodesTruncated":truncated,
            "importsExamined":rows.len().to_string()
        }))
    }
}
