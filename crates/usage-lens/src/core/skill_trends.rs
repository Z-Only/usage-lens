//! Bounded, aggregate-only skill evidence. Daily dates never fall back to capture
//! or import time, and independent evidence types are never deduplicated into runs.
use super::*;

const SKILL_FILTER: &str = "source_id=?1 AND event_type IN ('skill_requested','skill_loaded','skill_invoked') AND (?2 IS NULL OR skill_name=?2)";
const RANGE_FILTER: &str = "(?3 IS NULL OR substr(occurred_at,1,10) BETWEEN ?3 AND ?4)";
const COUNTS: &str = "coalesce(sum(event_type='skill_requested'),0),coalesce(sum(event_type='skill_loaded'),0),coalesce(sum(event_type='skill_invoked'),0),coalesce(sum(event_type='skill_loaded' AND json_extract(payload,'$.skillEvidenceKind')='main_read'),0),coalesce(sum(event_type='skill_loaded' AND json_extract(payload,'$.skillEvidenceKind')='instruction_injection'),0),coalesce(sum(event_type='skill_loaded' AND coalesce(json_extract(payload,'$.skillEvidenceKind'),'unknown') NOT IN ('main_read','instruction_injection')),0)";

fn counts(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<Value> {
    Ok(json!({
        "requested":row.get::<_,i64>(offset)?.to_string(),
        "loaded":row.get::<_,i64>(offset+1)?.to_string(),
        "invoked":row.get::<_,i64>(offset+2)?.to_string(),
        "loadedEvidence":{
            "mainRead":row.get::<_,i64>(offset+3)?.to_string(),
            "instructionInjection":row.get::<_,i64>(offset+4)?.to_string(),
            "unknown":row.get::<_,i64>(offset+5)?.to_string()
        }
    }))
}

impl UsageStore {
    /// No-filter calls preserve the original summary contract. Filters extend
    /// that projection; no account observation or individual event is loaded.
    pub fn get_skill_summary(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.skill_summary_impl(input))
    }

    fn skill_summary_impl(&self, input: &Value) -> CoreResult<Value> {
        let (source, _) = self.query_source(input, &["fromDate", "toDate", "skillName"])?;
        let (range, skill) = skill_summary_filters(input)?;
        let id = s(&source["id"]);
        let from = range.as_ref().map(|r| r.0.as_str());
        let to = range.as_ref().map(|r| r.1.as_str());
        let filtered = range.is_some() || skill.is_some();
        let group_limit = if filtered { QUERY_ROWS + 1 } else { QUERY_ROWS };
        let sql = format!(
            "SELECT event_type,skill_name,json_extract(payload,'$.skillEvidenceKind'),count(*) FROM events WHERE {SKILL_FILTER} AND {RANGE_FILTER} GROUP BY event_type,skill_name,json_extract(payload,'$.skillEvidenceKind') ORDER BY event_type,count(*) DESC,skill_name LIMIT ?5"
        );
        let mut statement = safe(self.db()?.prepare_cached(&sql))?;
        let mut skills = safe(statement.query_map(
            params![id, skill, from, to, group_limit],
            |r| {
                let kind: String = r.get(0)?;
                Ok(json!({
                    "kind":kind.trim_start_matches("skill_"),
                    "name":r.get::<_,Option<String>>(1)?,
                    "evidenceKind":r.get::<_,Option<String>>(2)?,
                    "count":r.get::<_,i64>(3)?.to_string()
                }))
            },
        ))?
        .map(safe)
        .collect::<CoreResult<Vec<_>>>()?;
        let truncated = skills.len() > QUERY_ROWS as usize;
        skills.truncate(QUERY_ROWS as usize);
        let mut warnings: Vec<String> = WARNINGS.iter().map(|v| v.to_string()).collect();
        if !filtered {
            warnings.extend(self.import_warnings(id)?);
            return Ok(
                json!({"source":source,"skills":skills,"coverage":coverage(),"warnings":warnings}),
            );
        }
        let imports = self.import_health(id)?;
        for code in imports["warningCodes"].as_array().into_iter().flatten() {
            warnings.push(format!("Import coverage: {}", s(code)));
        }
        warnings.extend([
            "Counts describe retained evidence, not unique executions or task success; requested, loaded and invoked may describe the same activity.",
            "Daily buckets use UTC occurredAt only. Missing days are absent and unknown, never inferred as zero activity.",
            "Unknown occurrence times cannot be assigned to the requested date range; their count covers all retained evidence matching this source and exact skill filter.",
            "Import warnings describe retained source imports, not proven coverage of the selected date range. Retention and deletion may remove evidence.",
        ].map(str::to_owned));
        if truncated {
            warnings.push("Per-skill groups are limited to 500; totals and daily buckets still cover all matching retained evidence.".to_owned());
        }
        if imports["warningCodesTruncated"] == true {
            warnings.push("Import warnings are limited to the 100 latest retained imports and 100 distinct codes; additional warnings are unknown.".to_owned());
        }
        let total_sql =
            format!("SELECT {COUNTS} FROM events WHERE {SKILL_FILTER} AND {RANGE_FILTER}");
        let totals = safe(
            self.db()?
                .query_row(&total_sql, params![id, skill, from, to], |r| counts(r, 0)),
        )?;
        let unknown_sql =
            format!("SELECT count(*) FROM events WHERE {SKILL_FILTER} AND occurred_at IS NULL");
        let unknown: i64 = safe(
            self.db()?
                .query_row(&unknown_sql, params![id, skill], |r| r.get(0)),
        )?;
        let mut result = json!({
            "source":source,"skills":skills,"skillsTruncated":truncated,
            "fromDate":from,"toDate":to,"skillName":skill,
            "totals":totals,"totalsScope":if range.is_some(){"dated_range"}else{"all_retained_matching_evidence"},
            "basis":"occurred_at_utc","unknownOccurredAtCount":unknown.to_string(),
            "unknownOccurredAtScope":"all_retained_source_matching_skill",
            "coverage":coverage(),"warnings":warnings,
            "importWarnings":{"scope":"all_retained_source","codes":imports["warningCodes"],"truncated":imports["warningCodesTruncated"]}
        });
        if range.is_some() {
            let sql = format!(
                "SELECT substr(occurred_at,1,10),{COUNTS} FROM events WHERE {SKILL_FILTER} AND {RANGE_FILTER} GROUP BY substr(occurred_at,1,10) ORDER BY substr(occurred_at,1,10) LIMIT 366"
            );
            let mut statement = safe(self.db()?.prepare_cached(&sql))?;
            let daily = safe(statement.query_map(params![id, skill, from, to], |r| {
                let mut value = counts(r, 1)?;
                value["date"] = json!(r.get::<_, String>(0)?);
                Ok(value)
            }))?
            .map(safe)
            .collect::<CoreResult<Vec<_>>>()?;
            result["daily"] = json!(daily);
        }
        Ok(result)
    }
}
