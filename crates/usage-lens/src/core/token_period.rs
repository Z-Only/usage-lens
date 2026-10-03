//! Aggregate response usage for an explicitly selected UTC period. Unknown
//! occurrence times remain unassigned, regardless of when a record was imported.
use super::*;
use chrono::NaiveDate;

impl UsageStore {
    /// This additive projection does not change the legacy response query's
    /// import-time fallback. It does not load response bodies or join events.
    pub fn get_response_token_period(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.response_token_period_impl(input))
    }

    fn response_token_period_impl(&self, input: &Value) -> CoreResult<Value> {
        exact_keys(input, &["sourceId", "fromDate", "toDate"])?;
        let (from, to) = date_range(&input["fromDate"], &input["toDate"])?;
        let days = NaiveDate::parse_from_str(&to, "%Y-%m-%d")
            .unwrap()
            .signed_duration_since(NaiveDate::parse_from_str(&from, "%Y-%m-%d").unwrap())
            .num_days();
        require(days < 366)?;
        let source = self.source(&input["sourceId"])?;
        let id = s(&source["id"]);
        // Stored occurrence timestamps are canonical UTC. Date labels include
        // the entire final day, including its fractional-second boundary.
        let mut statement = safe(self.db()?.prepare_cached(
            "SELECT model,input_tokens,cached_input_tokens,cache_write_input_tokens,output_tokens,reasoning_output_tokens,total_tokens FROM response_tokens WHERE source_id=?1 AND occurred_at IS NOT NULL AND substr(occurred_at,1,10) BETWEEN ?2 AND ?3 ORDER BY model",
        ))?;
        let mut rows = safe(statement.query(params![id, from, to]))?;
        let mut total = TokenSum::default();
        let mut current: Option<(Option<String>, TokenSum)> = None;
        let mut groups = Vec::new();
        let mut truncated = false;
        while let Some(row) = safe(rows.next())? {
            let model: Option<String> = safe(row.get(0))?;
            if current.as_ref().is_some_and(|(m, _)| *m != model)
                && let Some(group) = current.take()
            {
                keep_period_group(&mut groups, &mut truncated, group);
            }
            let (_, group) = current.get_or_insert_with(|| (model, TokenSum::default()));
            let mut values = Vec::with_capacity(6);
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
            keep_period_group(&mut groups, &mut truncated, group);
        }
        groups.sort_by(|a, b| b.1.n.cmp(&a.1.n).then(a.0.cmp(&b.0)));
        let by_model: Vec<Value> = groups
            .iter()
            .map(|(model, total)| {
                json!({"model":model,"responseCount":total.n.to_string(),"totals":total.value()})
            })
            .collect();
        let undated: i64 = safe(self.db()?.query_row(
            "SELECT count(*) FROM response_tokens WHERE source_id=? AND occurred_at IS NULL",
            [id],
            |row| row.get(0),
        ))?;
        let imports = self.import_health(id)?;
        let mut warnings: Vec<String> = WARNINGS
            .iter()
            .chain(RESPONSE_WARNINGS.iter())
            .map(|v| v.to_string())
            .collect();
        warnings.extend([
            "Only known UTC occurrence dates contribute to this selected period. Import time never substitutes for occurrence time.",
            "Undated responses are excluded; their count covers all retained responses in this source, not this date range.",
            "Reasoning effort and service tier are not recorded for these response records and are not inferred from events, model names or token counts.",
            "This selected period is not a verified quota cycle. Token activity does not establish quota consumption or remaining tokens.",
            "Import warnings describe retained source imports, not proven coverage of this selected period. Retention and deletion may remove evidence.",
        ].map(str::to_owned));
        if truncated {
            warnings.push("Model groups are limited to 500, retaining the unknown-model group when present; overall totals include every matching response.".to_owned());
        }
        if imports["warningCodesTruncated"] == true {
            warnings.push("Import warnings are limited to the 100 latest retained imports and 100 distinct codes; additional warnings are unknown.".to_owned());
        }
        Ok(json!({
            "source":source,"fromDate":from,"toDate":to,
            "responseCount":total.n.to_string(),"totals":total.value(),
            "byModel":by_model,"byModelTruncated":truncated,
            "undatedResponseCount":undated.to_string(),"undatedResponseScope":"all_retained_source",
            "reasoningEffort":{"status":"not_recorded"},"serviceTier":{"status":"not_recorded"},
            "coverage":{
                "completeness":"partial","missingResponses":"unknown",
                "dateBasis":"occurred_at_utc","undatedResponses":"excluded",
                "cumulativeSnapshots":"excluded","accountTotalRelationship":"not_combined",
                "quotaAttribution":"not_provided"
            },
            "importWarnings":{"scope":"all_retained_source","codes":imports["warningCodes"],"truncated":imports["warningCodesTruncated"]},
            "warnings":warnings
        }))
    }
}

fn keep_period_group(
    groups: &mut Vec<(Option<String>, TokenSum)>,
    truncated: &mut bool,
    group: (Option<String>, TokenSum),
) {
    groups.push(group);
    if groups.len() > QUERY_ROWS as usize {
        *truncated = true;
        // Reserve a slot for unknown model evidence even when it has fewer
        // responses than the 500 most frequent directly recorded models.
        groups.sort_by(|a, b| {
            a.0.is_some()
                .cmp(&b.0.is_some())
                .then(b.1.n.cmp(&a.1.n))
                .then(a.0.cmp(&b.0))
        });
        groups.pop();
    }
}
