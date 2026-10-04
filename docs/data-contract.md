# Data contract and scope

Usage Lens uses documented provider interfaces and explicitly supplied supported local records. It does not depend on a proprietary monitoring agent, discover arbitrary files, or infer complete account history from partial evidence.

The native implementation is Rust/Axum with SQLite. The Leptos UI uses the same local query core through loopback HTTP. The stdio plugin exposes a narrower aggregate-only interface. See [privacy](privacy.md), [record import](record-import.md), and [build/coverage](build-and-coverage.md).

## Account observations

After initialization, the account reader allows only:

- `account/read` with `refreshToken: false`
- `account/usage/read`, account-wide without a thread parameter
- `account/rateLimits/read`, without reserve/fallback flags

See the [official app-server contract](https://learn.chatgpt.com/docs/app-server). Method availability depends on the installed runtime and account. Startup may initialize existing configuration/integrations and handle existing credentials; `refreshToken: false` is not a guarantee against internal refresh. Live collection is an explicit local action, never a side effect of opening the dashboard or asking the plugin a question.

The account projection omits email, account IDs and credentials. A local source namespace is assigned by the user; it is not a verified account/workspace binding. Use a separate source after changing accounts. A null or unfamiliar account type is not proof of being signed out; known API-key-only and Bedrock modes cannot supply the documented account usage endpoint.

Each observation records its source, method, adapter/schema version, collection time, optional source-as-of time, and last safe collection failure. The collection timestamp is not the backend's freshness timestamp. Startup or initialization failures remain visible alongside retained observations. A successful null response replaces an older value rather than silently carrying it forward.

## Collection health

`health --db ABS --source ID [--max-age-ms N]` and optional MCP `usage_health`
return the same aggregate projection for one explicitly selected source. A local
HTTP/UI health view uses that projection too. `--demo` is explicit synthetic input,
never a fallback when a real store or query fails. Health is read-only and does not
run the account adapter, import files, scan configuration, or install a collector.

Interpret the independent signals together:

- `availability` is `available` or `missing`; independently, `capability` is
  `observed`, `unsupported` or `unknown`. An unsupported method can still have a
  retained observation; retain both facts
- Collection freshness describes local receipt time using the requested maximum
  age. Fresh, stale and future-dated collection times remain distinct. This does
  not establish when the provider last updated its data
- `sourceAsOf` is a source-reported timestamp when supplied. Missing source time
  stays unknown even when local collection is recent
- Last recorded failure is separate from the latest successful observation. A
  later successful observation does not erase that failure;
  `atOrAfterLatestObservation` identifies whether it is at least as recent as the
  observation. A retained value is not evidence that a more recent failed refresh
  succeeded
- Stored event, skill-evidence, response and import counts are local-store counts,
  not account totals or unique successful tasks. Skill evidence may overlap event
  records; do not add these counters into a single total
- Capture/occurrence time bounds and unknown-time counts refer only to stored
  evidence. A capture bound is not an occurrence bound. Missing timestamps remain
  unknown; earliest/latest records do not prove continuous coverage between them
- Historical coverage remains partial, including an empty store. A zero stored
  count says there are no stored records of that kind, not that no historical use
  occurred. No backfill, incremental daemon or complete-history guarantee is implied

The projection also includes current capture pause, content-capture and retention
settings labeled `scope: all_sources`. These are current global policy, not a
history of when capture was enabled. `provenance.measurement: measured` means local
stored counts/times; `estimated: false` is not a completeness or backend-freshness
claim. Import warnings are bounded to 100 latest imports and 100 distinct safe
codes; `warningCodesTruncated` marks potentially omitted warnings.

The projection contains no individual event/session/turn/response identities,
retained content, import fingerprints, credentials or database paths. Warnings and
safe failure codes must remain visible in conversational answers. Source identity
is still the user's namespace, not an authenticated account binding.

`doctor [--db ABS [--source ID]] [--max-age-ms N]` is a local setup diagnostic,
not part of the conversational Skill. Without a database it checks the running
binary and embedded dashboard. With `--db`, it opens the selected existing store
read-only; `--source` adds selected-source diagnostics. `--max-age-ms` requires a
source. It returns JSON diagnostics and exits 1 if any check fails. It never creates
or migrates a database, runs a subprocess, scans client configuration, reads
credentials, makes network requests, installs integrations or repairs a problem.
Checks not requested are `not_checked`. The overall status is `incomplete` when
store/source checks were not selected, `failed` if a check fails, or `ready` when
the selected setup checks pass. `incomplete` alone exits 0. `ready` means the local
setup can be queried; it does not imply recent or complete collection. A passed
diagnostic does not prove live account or actual client compatibility.

In v0.7, `compatibility` documents `readableSchemas: [2,3,4]`,
`requiredJournalMode: "rollback"`, `traceImportTargetSchema: 4`, `selectedSchema`
and `wouldUpgrade`. The latter two fields are null unless the selected store was
successfully read; otherwise `wouldUpgrade` is true for schema 2/3 and false for 4,
conditional on a successful explicit trace import. `upgradeTrigger` names that
condition. This is schema guidance, not validation of a trace bundle. Static
`rollbackWarning` and `backupSteps` describe stopping all writers, verifying a
private closed-store backup and restoring a separate compatible copy without
changing schema numbers or opening upgraded data with incompatible older binaries.
`backupStatus` remains `not_verified`; doctor neither makes nor verifies backups.
Safe `storage_error` / `unsupported_schema` guidance does not claim a specific
failure cause or probe other files, client processes or configuration.

## Values and precision

A metric cell has one of these states:

- `reported`: a supplied validated value, including actual zero
- `not_reported`: an explicit provider null
- `omitted`: the field was absent
- `invalid`: a present value did not match the supported contract

Integer quantities cross JSON boundaries as decimal strings and use exact arithmetic. They are never converted through floating-point numbers for aggregation. Percentages remain finite numeric values; source strings such as a credit balance do not acquire an invented currency or billing meaning.

## Daily and account usage

Account summary fields are independent observations: lifetime tokens, peak daily tokens, longest turn duration, current streak and longest streak. Returned daily buckets contain source date labels and token counts. Their source timezone is unknown unless supplied by the source.

Do not fill missing days with zero, infer a lifetime total from the returned date range, or reassign date-only buckets to an assumed timezone. Repeated snapshots replace previous observations; they are not additive events. A date-range sum means only the sum of returned valid buckets, with gaps and unknown values disclosed.

These endpoints do not provide complete per-model or per-skill token attribution. A quota bucket's model alias is not a model-consumption breakdown.

## Quota windows

When `rateLimitsByLimitId` is present and valid, it is authoritative, including an empty map. Otherwise the supported legacy bucket is an explicitly labeled fallback. Never add the legacy mirror to the multi-bucket view.

Preserve independent primary/secondary windows, reported percentages, window durations, reset timestamps, backend limit states, nullable access flags, and identity mismatches. Values above 100% are observations, not a reason to clamp stored evidence. Reset times are Unix instants; display their timezone explicitly.

Do not convert quota percentages into exact remaining tokens or requests. A percentage drop or an elapsed reset timestamp does not establish that access has recovered. `ordinaryUsageAllowed` and backend limit flags remain direct, possibly unknown facts. Earned-reset counts may be supplied, but this reader cannot redeem them or send billing nudges.

## Local events and content

Supported event categories distinguish visible user/assistant messages, tool calls, requested skills, loaded/read skills and explicitly evidenced invocation. Names found in ordinary text do not establish any of these events.

Tool identities are scoped to session and turn. Missing models, reasoning effort, status, source timestamps, durations and retry relationships remain unknown. Similar commands are not automatically labeled retries.

Optional retained content is accessible only through local detail/search routes. It is excluded from aggregate/plugin results. Content capture is separately enabled; known secret redaction is best-effort. System/developer/hidden-reasoning records and typed injected-instruction/catalog bodies are outside ordinary visible-message capture. Imported commands and embedded paths are data and are never executed or followed.

## Supplied rollout records

The importer recognizes only its declared pinned source contract and explicit file input. It preserves a file fingerprint, adapter/source version, safe warning codes, normalized event records and separate response-token records in one atomic import. An identical file is a no-op; conflicts roll back the import. Unsupported or incomplete record shapes are not silently presented as complete history.

A successful initial `skills.read` requires the exact namespace/function and a schema-valid subsequent matching response. Its occurrence time is the first matching completion time. Pagination and resource-only reads are not new skill invocations. Typed instruction injection and main-prompt reads remain different evidence categories because they may overlap. A prompt mention or skill catalog is not use evidence.

See [the precise pinned importer contract and examples](record-import.md).

## Skill evidence summaries and UTC trends

`skill-summary` (CLI), `usage_skills` (optional MCP), and local
`GET /api/skill-summary` share one aggregate-only projection for a selected source.
They do not expose individual event/session/turn/response IDs, raw content,
credentials, database paths or import fingerprints. The CLI/Skill and MCP
allowlists stay at eight commands/tools. The local `skills` evidence view is a
separate surface and is never an aggregate-query fallback.

### Inputs and compatibility

- Source is required: CLI `--source`, MCP/HTTP `sourceId`
- Optional date filters must be paired: CLI `--from` / `--to`, MCP/HTTP `fromDate` /
  `toDate`. Values are valid `YYYY-MM-DD` calendar dates, ascending and inclusive,
  with at most 366 days. A partial pair, explicit null, reversed range or wider
  range is invalid
- Optional exact skill-name filtering: CLI `--skill`, MCP/HTTP `skillName`. It is
  not a substring, case-folded or model filter. The name must be nonempty, at most
  256 UTF-16 code units, with no control characters. Omit an unused filter rather
  than supplying null. A skill-only summary is supported. `--skill` consumes its
  next argument literally, including names that begin with `--`; use normal shell
  quoting so names containing spaces or shell metacharacters remain one argument
- Optional `--max-age-ms` / `maxAgeMs` retains the existing integer validation,
  0 through 2592000000 inclusive, default 900000. It is not an evidence-date filter
  and does not make stored history complete or launch a refresh
- With no date or skill filter, the original response shape remains unchanged:
  `source`, `skills`, `coverage`, `warnings`. Passing only a freshness threshold
  also preserves that shape. New fields are present only for filtered queries

Example bounded CLI query against an explicitly selected existing store:

```sh
usage-lens skill-summary --db '/ABSOLUTE/usage.sqlite' --source 'SOURCE' \
  --from '2026-09-27' --to '2026-10-03' --skill 'EXACT_NAME'
```

Equivalent MCP arguments are `sourceId: "SOURCE"`, `fromDate: "2026-09-27"`,
`toDate: "2026-10-03"`, `skillName: "EXACT_NAME"`. HTTP uses the same names as
query parameters. Omitting the skill name includes all skill evidence in that
source. Omit both dates for an all-retained exact-skill summary, never to imply an
unbounded daily trend.

### Filtered response

The original source/coverage/warning fields remain. The added fields are:

- `fromDate`, `toDate`, `skillName`: supplied filters; each unsupplied field is null
- `basis: "occurred_at_utc"`: only `occurredAt` assigns an evidence date
- `totals`: exact decimal-string `requested`, `loaded`, `invoked` counts and
  `loadedEvidence: {mainRead, instructionInjection, unknown}`, also decimal strings
- `totalsScope: "dated_range"` with dates; only dated evidence inside the inclusive
  UTC range contributes. With a skill-only filter the value is
  `"all_retained_matching_evidence"`; totals include undated matching evidence
- `skills`: the existing aggregate group shape (`kind`, `evidenceKind`, `name`,
  decimal-string `count`), filtered to the same evidence scope as totals. At most
  500 groups are returned, with `skillsTruncated` indicating omitted groups.
  Totals cover all matching evidence, independently of the group limit
- `daily`: present only with a date range, sorted by UTC `date`. Each row has
  `date`, `requested`, `loaded`, `invoked`, and
  `loadedEvidence: {mainRead, instructionInjection, unknown}`. Skill-only results
  omit `daily`; `fromDate` and `toDate` are null
- `unknownOccurredAtCount`: a decimal-string count of undated matching skill
  evidence across **all retained** records for the source and optional exact
  skill, regardless of the supplied date range
- `unknownOccurredAtScope: "all_retained_source_matching_skill"`: this deliberately
  differs from the dated totals scope. It is not a count of undated events known
  to fall inside the range
- `importWarnings: {scope: "all_retained_source", codes: [...], truncated: bool}`:
  safe import-warning codes for the source, bounded to the latest 100 retained
  imports and at most 100 distinct codes. The scope label identifies the source
  population, while `truncated` discloses the bounded inspection; warnings are not
  filtered to a date range or skill

For example, the count object can be
`{"requested":"2","loaded":"4","invoked":"1","loadedEvidence":{"mainRead":"2","instructionInjection":"1","unknown":"1"}}`.
A daily row adds a `date` such as `"2026-10-03"` to that same shape. These are
synthetic illustrative counts, never a fallback for missing data.

### Evidence and time interpretation

Valid occurrence instants are normalized into UTC days. Local capture time,
import time, observation time and source account-usage calendar labels are never
substituted for `occurredAt`. Undated records are excluded from date-filtered
totals, groups and daily rows. Missing days are absent and mean unknown coverage;
they must not be filled with zero. Zero category counts within an observed day
mean no retained matching evidence in that category, not no historical use.

Requested, loaded and invoked remain separate evidence categories. Loaded counts
are broken down into source evidence kinds `main_read`, `instruction_injection`
and unknown/unclassified, serialized as `mainRead`, `instructionInjection`, and
`unknown`. These subtypes partition loaded evidence; do not add them again to
loaded totals. A main read and an instruction injection may overlap in the same
workflow, and imported/forked histories can overlap. No count proves task success,
unique execution, complete use history or tokens attributable to a skill.

Coverage stays partial even for an empty result. Coverage and import warnings are
source-level context; filtering does not establish complete collection for the
chosen window. Retention or deletion can reduce retained evidence without changing
what historically happened. This query adds no collector, import, hook or retention
behavior and makes no schema change. The separate explicit incremental importer
does not widen the conversational query allowlist or make collection automatic.

## Per-response token records

Supported `token_usage_record` inputs contain required source identity fields and per-response `usage`. Deduplication is by source/thread/session/response identity. Conflicting duplicates are rejected. The store keeps exact reported input, cached input, cache-write input, output, reasoning output and total quantities.

Only per-response usage is summed. Cumulative turn/thread usage is not added. Cached input is already part of input; reasoning output is already part of output. Preserve reported total rather than constructing a second bill. Legacy cumulative `token_count` records remain a coverage warning unless independently supported; they are never added to response totals.

Account totals and imported response totals remain separate. No response field assigns tokens to one particular skill. Co-occurrence in a turn is association, not cost attribution.

## Persistence and query safety

SQLite uses prepared statements, bounded inputs/queries, explicit migrations, atomic imports and secure deletion of active database content. This is not forensic erasure of backups, snapshots or exports. Retention is an explicit operation with an all-source scope clearly disclosed.

The dashboard binds only to loopback, validates Host/Origin and mutation requests, serves only embedded allowlisted assets, and never exposes arbitrary SQL or file paths.

v0.4.0 conversational queries and selected-store doctor checks accept schema-2
and schema-3 rollback-journal stores read-only, without implicit creation or
migration. Ordinary writable opens/new stores remain schema 2. The first
successful explicit incremental import upgrades to schema 3 in the same atomic
transaction as evidence, replay identities and checkpoint; failed parse/validation
does not migrate. **Back up the closed store before that write. v0.3.0 and older
cannot open schema 3.** Rollback needs a compatible copy of the pre-upgrade backup.

### Incremental record ingestion

`import-rollout-incremental --db ABS --source ID --file ABS --stream ID
--source-version PINNED_COMMIT` is a one-shot write, separate from snapshot
`import-rollout`, conversational Skill queries and optional MCP. Every invocation
requires all explicit inputs. It boundedly rereads/reparses the complete
newline-terminated prefix, not an O(delta) tail. Every unterminated tail is
deferred, even valid JSON and partial UTF-8. There is no watcher, scheduler,
directory/glob discovery, remembered-path reopen or embedded-path traversal.

The checkpoint contains complete byte/physical-line counts, a SHA-256 prefix
digest and parser/source-version metadata. A source-wide hashed identity ledger
detects replay and immutable conflicts across calls/streams without retaining
pending record bodies. Complete-prefix changes and truncation reject without
mutation; byte-identical copied prefixes are accepted as the same caller-declared
logical stream, without claiming physical-file identity. Rotation requires an
explicit new stream or source. Compare-and-swap progress checks and a single
transaction prevent stale concurrent commits and partial acceptance.

Replay metadata survives retention and content-only deletion. Old prefixes cannot
resurrect removed records or backfill old content after enabling capture. Explicit
all-data deletion, with an optional source scope, resets the matching checkpoints
and replay metadata. Schema 3 also records minimal replay tombstones for subsequent
snapshot/event writes, but cannot reconstruct previously deleted pre-upgrade data.
Snapshot anonymous file identities and incremental anonymous stream identities
differ; use separate sources when switching modes if overlap is possible.

The 8 MiB source, 256 KiB line, 20,000 physical-line and 1,000-event/response limits
remain, alongside existing depth and projection caps. A new stream ID cannot
exempt oversized input. At a bound, select another bounded logical file explicitly;
no automatic rotation/splitting occurs. Evidence remains partial: no inferred skill
invocation, success, complete history or per-skill token attribution. See
[record import](record-import.md#incremental-one-shot-workflow) for the full contract.

The plugin cannot collect, modify settings, delete records, or return stored bodies. Queries preserve provenance, freshness, missingness and imported coverage warnings.

## Verification limits

Synthetic protocol and format tests establish behavior for their tested inputs. They do not verify the user's installed version, actual account access, completeness of saved history, startup side effects, or a real ChatGPT client connection. Those capabilities must be checked independently on the user's own machine; unsupported fields remain visibly unavailable.

Incremental retention tombstones cover local event IDs and canonical
`(sourceEventId, eventType)` aliases. Alternative local IDs may refer to the same
immutable canonical evidence; accepting a zero-insert alias records replay
protection for that ID too. An alias with different immutable metadata is a
transaction-wide conflict. Content is never recovered through an alias after
retention or a capture-setting change.

Adoption of pre-incremental rows validates retained immutable metadata only.
Unretained raw bodies (for example, capture-off imports) have no historical hash
to compare and cannot be reconstructed; adoption never backfills their content.
Raw-evidence conflict checks apply from the first accepted incremental digest
onward, independent of subsequent capture settings.

## Selected RolloutTrace bundles (v0.6–v0.7)

The separate `import-trace-bundle --db ABS --source ID --directory ABS
--source-version a956835d020762cb2b570053af06f643a11c0ecc` reads one bounded,
explicitly selected upstream `rust-v0.160.0` diagnostic bundle. The contract,
limits, exact source links and local query workflow are in [trace import](trace-import.md).
It does not discover directories, watch files, enable recording, install or
configure a client, launch another process or make a model request.

The v0.7 `--dry-run` variant opens the selected existing store read-only and shares
the write's confinement, bounded parsing, projection and replay checks. Success
returns `operation: "trace_import_preflight"`, `dryRun: true`, `status: "ready"`,
`attemptsInBundle`, `attemptsWouldInsert`, `attemptsAlreadyPresent`,
`contentsWouldRetain` (decimal strings), `importAlreadyPresent`,
`contentCaptureEnabled`, safe warnings and `nextSteps`. `database` reports the
current `schemaVersion`, `targetSchemaVersion: 4`, `wouldUpgrade` and read-only
access. It returns no attempt identities, content or private paths. Validation,
source, paused-capture and immutable-conflict failures use the existing safe error
codes and nonzero exit status. Nothing is created, migrated, captured or backed up.
Later file/store/setting changes can invalidate the preview; an approved write
without `--dry-run` reparses and atomically rechecks conflicts.

Both local trace lists and summaries accept exact `threadId`, `status`,
`requestedModel`, `requestedReasoningEffort` and `requestedServiceTier` HTTP/core
filters; CLI uses `--thread`, `--status`, `--requested-model`, `--requested-effort`
and `--requested-tier`. Any subset combines with AND and optional paired UTC dates.
Status is `completed`, `failed`, `cancelled` or `incomplete`; thread IDs use the
existing 160-character identifier bound and requested strings are nonempty,
control-free and at most 128 UTF-16 units. Matches are case-sensitive with no
trimming, aliases or default inference. Requested filters match reported request
values only. Null/omitted/invalid states and response-side observations do not
match requested strings. List/summary filter echoes use null for absent fields.
Cursors bind source and all submitted filters, so changed scope or pre-v0.7
cursors require a fresh first page. Summary groups and totals share that scope;
source-wide import warnings remain independent of all attempt filters.

Attempt records are prepared-request evidence; upstream records them before
transmission, and WebSocket warmup can record logical full input rather than exact
wire bytes. A trace attempt may encompass lower-level transport retries. Completion
payloads summarize completed output, identifiers and optional usage rather than
all stream frames. An absent terminal event or missing usage stays unknown.
Neither attempt count nor completion evidence is a delivery or billing receipt.

Request model, `reasoning.effort` and `service_tier` remain request-side metadata.
The pinned completion summary omits observed model/tier, so they are not copied
from the request. Reported, explicit null, omitted and invalid states are distinct.
Missing effort/tier never means a default, Fast or Standard. Recorded trace tokens
are a separate possibly overlapping population from ordinary rollout tokens and
account usage. Date-selected trace totals are not exact weekly quota, purchased
credits, API charges, a quota-to-token conversion or per-skill attribution.

Trace query date ranges use attempt-start UTC instants. The local dashboard's
current-week preset is Monday–Sunday UTC, not a provider quota cycle. Sequence and
lifecycle determine start/terminal association; backward wall-clock timestamps
remain unchanged with `trace_clock_regression`, without inferred durations.
A completion `token_usage` parent must be an object or null/absent. Failed/cancelled
partial responses cannot carry non-null response IDs or usage; incompatible parent
or partial-response shapes reject the bundle rather than implying completion.

Trace imports persist safe warning codes. Read-only trace queries return them
in `importWarnings` with `scope: "all_retained_source"`, bounded to the latest
100 imports and 100 distinct codes plus an explicit `truncated` flag. This source-wide import scope is independent
of attempt-date filters, retained attempt counts and record deletion. Summary
metadata dimensions return at most 500 lexically first `(state, value)` groups;
The top-level boolean `groupsTruncated` marks omitted groups while overall totals cover all matching
retained attempts. Neither bounded groups nor retained totals imply full history.

The local trace attempt/detail/summary queries do not expand the conversational
Skill/MCP allowlists. Optional content is a redacted supported visible-text
projection with conservative classification, excluding system/developer and hidden
reasoning content; raw files stay untouched. User-role text needs exact aligned
`user.text` classification. Assistant phase is absent/null, `commentary` or
`final_answer`, with nonempty/invalid typed content kinds excluded. Windows UNC,
network and device bundle roots are rejected before opening; only local drive
paths with no linked/reparse components are supported. Atomic import/replay
metadata prevents
unchanged imports, retention, content deletion or all-data deletion from
resurrecting accepted trace data. All-data deletion preserves trace replay metadata,
unlike the ordinary incremental rollout reset behavior.
Bundle/attempt records are immutable; later modifications to an already imported
bundle are conflicts, not incremental updates. Select a closed stable bundle.

Only a successful explicit trace import upgrades schema 2/3 to schema 4. **Make a
verified closed-store backup first. v0.5.0 and older cannot read schema 4.**
v0.7.0 read-only queries support rollback-journal schemas 2/3/4 without migration;
normal opens do not automatically enable trace storage. Exact local synthetic
checks do not establish desktop runtime compatibility or environment inheritance.
Live desktop validation remains pending separate user authorization.

## v0.5 local reading queries

The local-only retained-content search now supports bounded cursor pagination and
metadata filters, returning event metadata only. The additive
`GET /api/response-tokens/period` requires explicit UTC dates and excludes all
responses without `occurredAt`; it does not reuse the older response summary's
import-time fallback. These local HTTP surfaces do not expand the conversational
Skill/MCP allowlists. Full fields, bounds, ordering and scope are documented in
[message reading](message-reading.md). No schema migration is required.
