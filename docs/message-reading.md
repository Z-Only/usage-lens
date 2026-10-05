# Local message reading and quota context

Usage Lens reads **retained local evidence**. It does not capture the transport,
proxy requests, collect credentials, or reconstruct the actual remote request
body. A stored message or tool call is not a delivery receipt. Session and turn
identifiers are displayed only when the event supplies them; they do not establish
a one-to-one link between an event and a completed-response token record.

## Read from aggregates to individual records

The Overview's **Explore recorded activity** shows the current source's retained
event counts by event type and directly reported model. Selecting a known group
opens Activity with that exact filter and clears unrelated date/content filters.
Unknown-model counts remain visible but are not converted to an exact-model
filter. These are event counts, not request counts or token/quota charges.
The model-group display includes at most 500 returned groups plus the separately
reported unknown-model count if that count is absent from the returned groups.

Activity supports source-scoped date, event type, exact model, and retained-content
search filters. Date filters use recorded occurrence time when available, otherwise
observation time, as the event API has always done. Results are ordered by
observation time and event ID descending, not a reconstructed conversation order.
Draft input does not change the current result set or its next-page request.
Submitting filters (or revisiting Activity) applies the selection. New selections,
reset, source changes, and closing a detail invalidate superseded responses.

Event detail presents message text, tool arguments, tool results, and explicitly
retained file text in separate sections. Stored JSON and metadata remain available
in expandable sections. All content is escaped text, never rendered HTML, executed
Markdown, active file links, or commands. The panel distinguishes optional reported
model, reasoning effort, session, turn, and status from unrecorded Fast/Standard.
No success or transmitted bytes are inferred from an event's existence.

## Search scope and pagination

The local-only `GET /api/events/search` accepts:

- Required `sourceId` and nonempty case-sensitive `query` (at most 200 UTF-16 code units)
- Optional paired `fromDate` and `toDate`, `eventType`, and exact `model`, as for events
- Optional `limit` (1–500, default 50) and opaque `cursor`

Search applies metadata filters before selecting at most the newest 10,000 retained
content rows. It uses substring matching on **redacted stored JSON**, including
field names and JSON escapes. It is not a case-folded, full-text, semantic, or
complete-history search. An empty result can mean content was never retained,
was deleted, is outside the bounded candidate window, or does not contain the
literal stored substring. It does not prove there was no activity.

The response contains metadata-only `events`, `source`, `nextCursor`, coverage,
warnings, and `search` with `scope`, `candidateLimit`, `searchedRecordCount`,
`matchedCount`, and `truncated`. Counts refer to the bounded candidate window.
The opaque cursor binds source, query, filters, and the original window, and excludes
later ingests, including backdated rows. It is not permission to change query
scope. A bounded digest of the pinned candidate identities, metadata and retained content
rejects continuation if that evidence changes, including deletion/retention and
rowid-reuse replacements. Observationally identical records are equivalent; no
physical-row identity is claimed. This is not a durable snapshot or a promise to
recover deleted content. Retry starts a fresh search with the submitted filters;
restart to include new records. Open an event separately to retrieve its retained content.

## Token dates beside weekly quota

The Quotas page labels a reported window as weekly only when its recorded
`windowDurationMins` is exactly `10080`, whether primary or secondary. The reported
percentage and reset remain independent observations. A reset timestamp minus
seven days is **not** used to invent a cycle start.

**Tokens alongside quota** requires an explicitly selected UTC date pair, at most
366 days inclusive. It shows completed-response token totals and model groups
for those dates, separately from the provider's quota window. Only `occurredAt`
assigns a response to the period. Missing occurrence times never fall back to
`importedAt`; excluded undated response counts are source-wide, with their true
position inside/outside the selected dates unknown. No missing evidence becomes
zero historical activity.

The additive local `GET /api/response-tokens/period` accepts exactly `sourceId`,
`fromDate`, and `toDate`. It returns source and date scope, decimal-string
`responseCount`, six exact decimal-string `totals`, at most 500 `byModel` groups,
`byModelTruncated`, and source-wide `undatedResponseCount`. The unknown-model group
is preserved when present; overall totals include all matching records even if
model groups are truncated. `reasoningEffort.status` and `serviceTier.status` are
`not_recorded`: the response schema does not provide either dimension. An event's
optional reasoning effort cannot be joined to response tokens without direct
identity evidence. Cached input is a subset of input and reasoning output a
subset of output, so neither is added again.

A local source ID is user-assigned, not an authenticated account binding. Token
records may cover only a subset of the activity behind an account-level quota
snapshot. The selected token dates are not claimed to match the provider's billing
or allowance cycle. These views do not estimate weekly token capacity, quota cost
per message, fixed quota-to-token conversion, remaining tokens, API cost, or
purchased credits. Model, effort, and service tier can affect usage rules; there
is no fixed conversion hardcoded into this product.

## Privacy and compatibility

Content capture stays off by default. It must have been explicitly enabled before
records were accepted. Enabling it later does not recover earlier or deleted
content. Turning it off does not remove content already retained. The app does not
know whether unavailable content was never retained or later deleted. Existing
redaction remains best-effort; protect the local database accordingly.

System/developer instructions, hidden reasoning, and excluded skill payloads remain
outside ordinary message capture. This iteration changes neither import collection
nor transport interception. Skill and MCP allowlists remain aggregate-only and do
not expose message bodies, search queries, search results, or local detail methods.
The new HTTP period endpoint is local UI-only; it does not add a conversational tool.

There is no schema migration in v0.5.0. Existing schema 2 and 3 stores retain their
v0.4.0 compatibility, replay, and rollback rules. A v0.4.0 binary can still read
those stores, but lacks these new UI surfaces and local HTTP query options.


## Separate trace evidence in v0.6

The [selected trace-bundle importer](trace-import.md) is a different explicit
source contract and write workflow. Its local attempt/detail/summary views keep
prepared request model/effort/tier separate from completion evidence. The older
response-token period fields above remain `not_recorded`; importing trace metadata
does not fabricate joins into that response population. Trace requests are not
universal exact-wire evidence or delivery/billing proof. The new schema-4 upgrade
happens only on successful explicit trace import and requires a backup; v0.5.0 and
older cannot read that upgraded store. v0.8.0 queries still read schema 2/3 without
migration. The v0.8 source-local thread, joint requested-setting and recorded-start
UTC-day summaries add no migration or new write path. Their clock-based timelines
and anomaly counts are not causal order, active time, latency or speed; overlapping
histories are not account-wide unique activity. Local summaries intentionally
include thread IDs. The eight conversational CLI/MCP aggregate allowlists remain
unchanged; see the [trace query contract](trace-import.md#local-queries).
