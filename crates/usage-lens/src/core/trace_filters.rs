//! Shared, exact SQL scope and filter-bound cursors for local trace queries.
use super::*;
use rusqlite::types::Value as SqlValue;

pub(super) const FILTER_KEYS: [&str; 5] = [
    "threadId",
    "status",
    "requestedModel",
    "requestedReasoningEffort",
    "requestedServiceTier",
];

// Only these constant SQL fragments are composed into statements. All user values
// are bound parameters, and requested fields match reported request evidence only.
pub(super) const FILTER_SQL: &str = "a.source_id=?1
    AND (?2 IS NULL OR substr(a.started_at,1,10) BETWEEN ?2 AND ?3)
    AND (?4 IS NULL OR json_extract(a.payload,'$.threadId')=?4 COLLATE BINARY)
    AND (?5 IS NULL OR json_extract(a.payload,'$.status')=?5 COLLATE BINARY)
    AND (?6 IS NULL OR (json_extract(a.payload,'$.request.model.state')='reported' AND json_extract(a.payload,'$.request.model.value')=?6 COLLATE BINARY))
    AND (?7 IS NULL OR (json_extract(a.payload,'$.request.reasoningEffort.state')='reported' AND json_extract(a.payload,'$.request.reasoningEffort.value')=?7 COLLATE BINARY))
    AND (?8 IS NULL OR (json_extract(a.payload,'$.request.serviceTier.state')='reported' AND json_extract(a.payload,'$.request.serviceTier.value')=?8 COLLATE BINARY))";

pub(super) struct TraceFilters {
    from: Option<String>,
    to: Option<String>,
    values: [Option<String>; 5],
}

impl TraceFilters {
    pub(super) fn parse(input: &Value) -> CoreResult<Self> {
        let (from, to) = optional_range(input)?;
        let mut values = [const { None }; 5];
        for (index, key) in FILTER_KEYS.iter().enumerate() {
            if let Some(value) = input.get(key) {
                values[index] = Some(match index {
                    0 => identifier(value)?,
                    1 => enum_value(value, &["completed", "failed", "cancelled", "incomplete"])?,
                    _ => bounded_text(value, 128)?,
                });
            }
        }
        Ok(Self { from, to, values })
    }

    pub(super) fn echo(&self, result: &mut Value) {
        result["fromDate"] = json!(self.from);
        result["toDate"] = json!(self.to);
        for (key, value) in FILTER_KEYS.iter().zip(&self.values) {
            result[key] = json!(value);
        }
    }

    pub(super) fn params(&self, source: &str) -> Vec<SqlValue> {
        std::iter::once(SqlValue::Text(source.to_owned()))
            .chain(
                [&self.from, &self.to]
                    .into_iter()
                    .chain(self.values.iter())
                    .map(|value| value.clone().map_or(SqlValue::Null, SqlValue::Text)),
            )
            .collect()
    }

    fn fingerprint(&self) -> String {
        let mut filters = json!({});
        self.echo(&mut filters);
        digest(&filters)
    }

    pub(super) fn cursor(
        &self,
        input: Option<&Value>,
        source: &str,
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
        let value: Value = serde_json::from_slice(&decoded).map_err(|_| error("invalid_input"))?;
        exact_keys(&value, &["sourceId", "kind", "filters", "at", "key"])?;
        require(
            value["sourceId"] == source
                && value["kind"] == "traces"
                && value["filters"] == self.fingerprint(),
        )?;
        Ok(Some((timestamp(&value["at"])?, identifier(&value["key"])?)))
    }

    pub(super) fn encode_cursor(&self, source: &str, at: &str, key: &str) -> String {
        URL_SAFE_NO_PAD.encode(
            json!({"sourceId":source,"kind":"traces","filters":self.fingerprint(),"at":at,"key":key})
                .to_string(),
        )
    }
}
