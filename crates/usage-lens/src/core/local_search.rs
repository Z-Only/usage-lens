//! Local-only search over a bounded retained-content window. Returned events
//! contain metadata only; content remains behind the separate detail endpoint.
use super::*;

const CANDIDATE_LIMIT: usize = 10_000;
const CANDIDATES: &str = "SELECT e.rowid AS row_id,e.observed_at,e.event_id FROM events e JOIN event_details d ON e.source_id=d.source_id AND e.event_id=d.event_id WHERE e.source_id=?1 AND (?2 IS NULL OR substr(COALESCE(e.occurred_at,e.observed_at),1,10) BETWEEN ?2 AND ?3) AND (?4 IS NULL OR e.event_type=?4) AND (?5 IS NULL OR e.model=?5) AND e.rowid<=?6 AND (?7 IS NULL OR (e.observed_at,e.event_id)>=(?7,?8)) ORDER BY e.observed_at DESC,e.event_id DESC";

type Boundary = (String, String);
struct SearchCursor {
    ceiling: i64,
    floor: Boundary,
    after: Boundary,
    truncated: bool,
    window: String,
}

fn search_cursor(input: Option<&Value>, scope: &str) -> CoreResult<Option<SearchCursor>> {
    let Some(input) = input else { return Ok(None) };
    let text = bounded_text(input, 1600)?;
    require(
        text.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
    )?;
    let bytes = URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|_| error("invalid_input"))?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| error("invalid_input"))?;
    exact_keys(
        &value,
        &[
            "version",
            "scope",
            "ceiling",
            "floorAt",
            "floorKey",
            "at",
            "key",
            "truncated",
            "window",
        ],
    )?;
    require(value["version"] == 1 && value["scope"] == scope)?;
    let floor = (
        timestamp(&value["floorAt"])?,
        identifier(&value["floorKey"])?,
    );
    let after = (timestamp(&value["at"])?, identifier(&value["key"])?);
    require(after >= floor)?;
    Ok(Some(SearchCursor {
        ceiling: integer(&value["ceiling"], 1, MAX_SAFE_INTEGER)?,
        floor,
        after,
        window: bounded_text(&value["window"], 43)?,
        truncated: value["truncated"]
            .as_bool()
            .ok_or_else(|| error("invalid_input"))?,
    }))
}

impl UsageStore {
    pub fn search_local_details(&self, input: &Value) -> CoreResult<Value> {
        self.read(|| self.search_local_details_impl(input))
    }

    fn search_local_details_impl(&self, input: &Value) -> CoreResult<Value> {
        let (source, from, to, model) =
            self.filter(input, &["query", "eventType", "cursor", "limit"])?;
        let query = bounded_text(&input["query"], 200)?;
        let kind = input
            .get("eventType")
            .map(|v| enum_value(v, EVENT_TYPES))
            .transpose()?;
        let lim = limit(input)? as usize;
        // Bind every result-affecting request field without including the query
        // text in returned metadata or the opaque cursor. Page size may change.
        let scope = URL_SAFE_NO_PAD.encode(Sha256::digest(
            json!([source["id"], query, from, to, kind, model])
                .to_string()
                .as_bytes(),
        ));
        let cursor = search_cursor(input.get("cursor"), &scope)?;
        let ceiling = match &cursor {
            Some(cursor) => cursor.ceiling,
            None => safe(self.db()?.query_row(
                "SELECT COALESCE(MAX(rowid),0) FROM events",
                [],
                |r| r.get::<_, i64>(0),
            ))?,
        };
        let floor = cursor.as_ref().map(|c| &c.floor);
        // The extra metadata row detects truncation without searching its body.
        // A pinned lower boundary prevents deletion/retention from refilling the
        // window with older records. The rowid ceiling excludes later appends;
        // the content/identity digest below also detects rowid reuse.
        let mut stmt = safe(
            self.db()?
                .prepare_cached(&format!("{CANDIDATES} LIMIT 10001")),
        )?;
        let mut candidates = safe(stmt.query_map(
            params![
                s(&source["id"]),
                from,
                to,
                kind,
                model,
                ceiling,
                floor.map(|v| v.0.as_str()),
                floor.map(|v| v.1.as_str())
            ],
            |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        ))?
        .map(safe)
        .collect::<CoreResult<Vec<_>>>()?;
        let truncated = cursor
            .as_ref()
            .map(|c| c.truncated)
            .unwrap_or(candidates.len() > CANDIDATE_LIMIT);
        candidates.truncate(CANDIDATE_LIMIT);
        let floor = cursor
            .as_ref()
            .map(|c| c.floor.clone())
            .or_else(|| candidates.last().cloned());
        let mut rows = Vec::new();
        let mut window = Sha256::new();
        if let Some(floor) = &floor {
            // Materialize metadata keys only, never up to 10,000 content bodies.
            let sql = format!(
                "WITH candidates AS MATERIALIZED ({CANDIDATES} LIMIT 10000) SELECT e.payload,c.observed_at,c.event_id,d.payload,instr(d.payload,?9)>0,e.rowid FROM candidates c JOIN events e ON e.rowid=c.row_id JOIN event_details d ON e.source_id=d.source_id AND e.event_id=d.event_id"
            );
            let mut stmt = safe(self.db()?.prepare_cached(&sql))?;
            let mut scanned = safe(stmt.query_map(
                params![
                    s(&source["id"]),
                    from,
                    to,
                    kind,
                    model,
                    ceiling,
                    floor.0,
                    floor.1,
                    query
                ],
                |r| {
                    let metadata: String = r.get(0)?;
                    let content: String = r.get(3)?;
                    let mut record = Sha256::new();
                    record.update(r.get::<_, i64>(5)?.to_be_bytes());
                    for value in [&metadata, &content] {
                        record.update((value.len() as u64).to_be_bytes());
                        record.update(value.as_bytes());
                    }
                    Ok((
                        (r.get::<_, String>(1)?, r.get::<_, String>(2)?),
                        record.finalize(),
                        r.get::<_, bool>(4)?.then_some(metadata),
                    ))
                },
            ))?
            .map(safe)
            .collect::<CoreResult<Vec<_>>>()?;
            // Do not ask SQLite to sort content bodies. Only bounded metadata
            // and digests survive each row; sort those for deterministic pages.
            scanned.sort_unstable_by(|a, b| b.0.cmp(&a.0));
            for (boundary, digest, metadata) in scanned {
                window.update(digest);
                if let Some(metadata) = metadata {
                    rows.push((metadata, boundary));
                }
            }
        }

        let window = URL_SAFE_NO_PAD.encode(window.finalize());
        // Ordinary SQLite rowids can be reused after deletion. Fail closed if
        // any pinned candidate changes, including delete/reinsert with the same
        // event identity but different content. This is not a durable snapshot.
        require(cursor.as_ref().is_none_or(|c| c.window == window))?;
        let matched = rows.len();
        let remaining: Vec<_> = rows
            .iter()
            .filter(|(_, boundary)| cursor.as_ref().is_none_or(|c| boundary < &c.after))
            .collect();
        let selected = &remaining[..remaining.len().min(lim)];
        let next = if remaining.len() > lim {
            let (_, after) = selected[lim - 1];
            let floor = floor.as_ref().ok_or_else(|| error("storage_error"))?;
            Some(
                URL_SAFE_NO_PAD.encode(
                    json!({
                        "version":1,"scope":scope,"ceiling":ceiling,
                        "floorAt":floor.0,"floorKey":floor.1,
                        "at":after.0,"key":after.1,"truncated":truncated,"window":window,
                    })
                    .to_string(),
                ),
            )
        } else {
            None
        };
        let events = selected
            .iter()
            .map(|(text, _)| parse(text.clone()))
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(json!({
            "source":source,"events":events,"nextCursor":next,"coverage":coverage(),
            "search":{
                "scope":"retained_redacted_local_content","candidateLimit":CANDIDATE_LIMIT,
                "searchedRecordCount":candidates.len().to_string(),"matchedCount":matched.to_string(),"truncated":truncated,
            },
            "warnings":[
                CONTENT_WARNING,
                "Search uses a case-sensitive literal substring of retained, redacted local content; no content is returned in search results.",
                "Filters apply before the 10,000 most recently observed retained records are selected. Matching counts cover only this bounded window, not all history.",
                "Event dates use reported occurrence time when available, otherwise observation time.",
                "Pages keep the original candidate boundary and exclude later appends. Changes or deletions within that window invalidate the cursor; start a new search to refresh it.",
            ],
        }))
    }
}
