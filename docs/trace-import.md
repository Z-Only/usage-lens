# Explicit local RolloutTrace bundle import

Usage Lens v0.7.0 reads one deliberately selected, bounded local RolloutTrace
bundle. It does not enable recording, discover trace directories, watch files,
change client configuration, install anything, launch another process or make a
model request. The import is a separate local write, outside the conversational
Skill and optional MCP allowlists.

This is a source-pinned diagnostic import. Imported files are editable and can
spoof every field. A declared source version selects a parser; it does not
authenticate the bundle, its account or its provenance. Coverage is always partial.

## Supported source and desktop compatibility

The supported public upstream baseline is OpenAI Codex `rust-v0.160.0`, commit:

`a956835d020762cb2b570053af06f643a11c0ecc`

A bundle consists of `manifest.json`, `trace.jsonl` and the payload JSON files
referenced by supported inference events. It is not the ordinary rollout JSONL
format accepted by [record import](record-import.md), and a generic conversation
export is not a compatible trace bundle.

The upstream [RolloutTrace overview](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/rollout-trace/README.md)
describes opt-in recording through `CODEX_ROLLOUT_TRACE_ROOT`. That environment
variable is an **upstream runtime contract only**, not an installation step that
Usage Lens performs. The [trace-root environment read](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/rollout-trace/src/thread.rs#L101-L117)
and [runtime integration](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/core/src/session/session.rs#L1195-L1228)
do not establish which runtime a desktop app bundles or which environment its
launcher inherits.

Having the latest macOS app, or selecting its Local mode, does not prove that its
bundled runtime has this contract, that it inherits the variable, or that tracing
has been activated. Local tool execution is also not sufficient evidence of local
orchestration: [Work orchestration and hook support](https://learn.chatgpt.com/docs/enterprise/cloud-local-access#check-hooks-and-network-compatibility)
differ between local and cloud execution. Plugin or Skill installation does not
enable trace capture, recover past traces, or establish ordinary Chat coverage.

**Live desktop compatibility is pending separately user-authorized validation.**
Tests use synthetic bundles only. They do not inspect the user's Mac, installed
runtime, private histories, credentials or protected transcript directories.
Any future capture setup or live model test needs its own reviewed scope and
permission; this guide is not authorization to perform it.

## Explicit import workflow

Use a private, absolute database path and one absolute bundle directory selected
by the user. Do not select an entire home directory, trace root or parent of many
bundles. Use a dedicated imported source when overlap with other imported evidence
is possible; trace and ordinary rollout totals are separate populations.

**Before the first successful trace import into an existing store, stop every
writer and make a verified private backup of the closed store.** A successful
explicit import upgrades schema 2 or 3 to schema 4 atomically with its evidence
and replay metadata. v0.5.0 and older cannot read schema 4. Failed parsing or
validation does not migrate. Ordinary startup and read-only queries never perform
this upgrade. See [backup and rollback](AI_INSTALL.md#upgrade-rollback-and-uninstall).

After creating a source explicitly, preview one chosen bundle against the existing
store. Every path, source, ID, date and filter in these commands is a placeholder;
replace it with the reviewed value. A preview is still a local file read requiring
authorized access to the chosen bundle and database.

```sh
'/ABSOLUTE/usage-lens' source --db '/ABSOLUTE/private/usage.sqlite' \
  --source 'SOURCE' --mode imported --name 'DISPLAY_NAME'

'/ABSOLUTE/usage-lens' import-trace-bundle --dry-run \
  --db '/ABSOLUTE/private/usage.sqlite' \
  --source 'SOURCE' \
  --directory '/ABSOLUTE/private/selected-trace-bundle' \
  --source-version a956835d020762cb2b570053af06f643a11c0ecc
```

For Windows PowerShell, use absolute Windows paths and the call operator for a
quoted executable:

```powershell
& 'C:\ABSOLUTE\usage-lens.exe' import-trace-bundle --dry-run --db 'C:\PRIVATE\usage.sqlite' --source 'SOURCE' --directory 'C:\PRIVATE\selected-trace-bundle' --source-version a956835d020762cb2b570053af06f643a11c0ecc
```

The installed native executable needs no Node.js, Bun or Rust. Supply directory,
source and version on every invocation. No path is remembered for a later read.
No embedded command is executed. Only validated payload references inside that
chosen bundle may be read, never arbitrary paths found in prompt or tool text.
Review the import's warnings even when the command succeeds.

### Read-only preflight, then an explicit write

`--dry-run` opens only the selected **existing** database read-only. It uses the
same bounded, confined file reader, parser, projection normalizer and immutable
replay checks as the write. It never creates a missing database, migrates a schema,
records an import/checkpoint, retains content or creates a backup. The selected
raw bundle remains untouched. Content-capture settings do not bypass projection
validation or its limits.

A successful preview is JSON with `operation: "trace_import_preflight"`,
`dryRun: true` and `status: "ready"`. Decimal-string counts are `attemptsInBundle`,
`attemptsWouldInsert`, `attemptsAlreadyPresent` and `contentsWouldRetain`.
`attemptsAlreadyPresent` counts accepted/replay-suppressed input attempts; it does
not mean their rows or content still exist after deletion. `contentsWouldRetain`
counts newly accepted attempts with an eligible visible-text projection under the
current capture setting, not messages, characters or a complete transcript.
`importAlreadyPresent` distinguishes a fully accepted bundle replay. The
`database` object reports `accessMode: "read_only"`, the current `schemaVersion`,
`targetSchemaVersion: 4` and `wouldUpgrade`. `contentCaptureEnabled` reflects the
current local setting, not permission to enable it. Review `warningCodes`,
`warnings` and `nextSteps`. No attempt IDs, content or private paths are echoed.

Malformed, unsafe, oversized, incompatible, paused, wrong-source or conflicting
inputs fail with the existing safe error codes and exit 1; they do not return a
successful preview. An immutable conflict is `trace_identity_conflict`, not an
invitation to rewrite accepted files. Missing/unreadable/unsupported-journal
stores can report `storage_error`; unsupported database schemas report
`unsupported_schema`. Correct the reviewed input or use a documented compatible
backup; do not probe arbitrary stores or change schema numbers.

**A preview is a point-in-time prediction, not a reservation or backup.** Later
changes to the source bundle, selected database, capture settings or competing
writes can invalidate it. After reviewing it, stop writers and verify a private
closed-store backup. Only with approval for the write, rerun the same explicit
command **without `--dry-run`**. The write reparses the bundle and rechecks replay
conflicts atomically; a successful preview does not guarantee later success.
Identical replays and already accepted attempts do not backfill content.

### Local queries

These commands read an existing database and remain outside the eight-command
conversational Skill and eight-tool MCP boundary:

```sh
'/ABSOLUTE/usage-lens' trace-attempts --db '/ABSOLUTE/private/usage.sqlite' --source 'SOURCE'
'/ABSOLUTE/usage-lens' trace-detail --db '/ABSOLUTE/private/usage.sqlite' --source 'SOURCE' \
  --attempt 'ATTEMPT_ID_FROM_TRACE_ATTEMPTS'
'/ABSOLUTE/usage-lens' trace-summary --db '/ABSOLUTE/private/usage.sqlite' --source 'SOURCE' \
  --from 'FROM_YYYY-MM-DD' --to 'TO_YYYY-MM-DD' \
  --thread 'THREAD_ID' --status 'STATUS' --requested-model 'EXACT_REQUESTED_MODEL' \
  --requested-effort 'EXACT_REQUESTED_EFFORT' --requested-tier 'EXACT_REQUESTED_TIER'
```

`trace-attempts` accepts `--limit` from 1 through 500 (default 50), `--cursor`
pagination and an optional paired `--from` / `--to` UTC date range. Dates are
inclusive and assigned by the attempt start timestamp, not completion, import
time or a quota reset. `trace-summary` accepts the same paired dates; omit both
for its retained-source scope. A range must ascend and span at most 3,660 days
between its endpoints (3,661 inclusive dates). Use an attempt ID returned for the
same source rather than inventing an ID or joining by similar text. Individual
attempt identities and optional visible content are local inspection data, not
new conversational tools.

Both `trace-attempts` and `trace-summary` additionally accept exact `--thread`,
`--status`, `--requested-model`, `--requested-effort` and `--requested-tier`
filters. Any subset may be used, with or without paired dates; all supplied
filters combine with AND. Status is exactly `completed`, `failed`, `cancelled` or
`incomplete`. Thread IDs follow the existing identifier contract (1–160 ASCII
characters, starting alphanumeric; remaining characters may also be `. _ : @ / + -`).
Requested values are nonempty, control-free strings of at most 128 UTF-16 units.
Matching is case-sensitive without trimming or alias/default inference.
Request metadata filters match only the `reported` state and its exact value;
explicit null, omitted and invalid states do not match a requested string.
They never filter observed response values or map Fast/Standard labels.

List rows, summary totals, token missingness and metadata groups use the same
submitted scope. Result fields `fromDate`, `toDate`, `threadId`, `status`,
`requestedModel`, `requestedReasoningEffort` and `requestedServiceTier` echo that
scope, using null for absent filters. Cursors are bound to the source and every
submitted filter, including dates. Changing or removing a filter requires a new
first-page query; reusing a mismatched cursor fails. Pre-v0.7 trace cursors must
also be restarted. Page size may change without changing the filter scope.

### Local dashboard and query scopes

The bilingual **Traces / 追踪** dashboard uses local `GET /api/traces`,
`GET /api/traces/detail` and `GET /api/traces/summary`. List and summary HTTP
filters are `threadId`, `status`, `requestedModel`, `requestedReasoningEffort` and
`requestedServiceTier`, plus paired `fromDate` / `toDate`. Its submitted filters
share the CLI's exact-match rules and attempt-start UTC date basis. The **This UTC
week** preset selects the current Monday–Sunday UTC calendar week; it does not identify a provider quota
cycle. Pagination applies to the submitted filters, not unsubmitted input edits.
Submitting filters resets pagination and closes the selected detail. Loading
and error states do not show a previous filter scope's rows or summary as current;
late responses from an older submission are ignored. The detail reader shows
optional supported visible text and safe metadata only. **View thread** applies
the displayed exact thread ID alongside the submitted filters and starts a new
first-page query; it does not infer relationships from similar text.

Summary groups distinguish `byRequestedModel`, `byRequestedReasoningEffort`,
`byRequestedServiceTier`, `byObservedModel` and `byObservedServiceTier`. Each
dimension returns at most 500 lexically first `(state, value)` groups.
The top-level boolean `groupsTruncated` flags omitted groups; overall attempt/token
totals still cover
all matching retained attempts, independently of the group limit. They are not
account-wide complete totals. Each token dimension reports its own missingness;
a total with no reported token values is null, not an inferred zero.

Every trace query carries `importWarnings: {scope: "all_retained_source", codes,
truncated}`. Safe codes come from the latest 100 retained imports for the source,
with at most 100 distinct codes. `truncated` reports either bound. These warnings
are source-wide import metadata, not date-filtered or specific to the displayed
attempt. They remain relevant even when record deletion or retention leaves no
matching attempts; absence of a warning in a bounded list is not proof of
complete collection. Preserve both coverage warnings and truncation notices when
interpreting a filtered result.

## What a traced attempt establishes

A recorded request payload is **prepared-request evidence**, not proof of network
transmission, delivery, provider acceptance or billing. It is recorded before transmission.
Upstream
[records the start before calling the stream request](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/core/src/client.rs#L1747-L1765).
A failure between those operations can leave a prepared request without a send.
One trace call can also include [lower-level transport retries](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/codex-api/src/endpoint/session.rs#L122-L154),
so traced attempt counts are not physical network-request counts.

For WebSocket reuse after an untraced warmup, upstream can record the
[logical full input instead of the actual incremental wire request](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/core/src/client.rs#L1977-L2052).
The absence of a `response.create` wrapper therefore does not prove HTTP, and the
stored payload is not universally byte-identical to network traffic. No unseen
server-added context is inferred.

The recorded response is an [output/completion summary](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/rollout-trace/src/inference.rs#L199-L301),
not all streaming frames or every delta. It can contain completed output items,
response identifiers and usage; failures or cancellation can leave partial output
and absent completion usage. A `token_usage` parent must be an object or null
(absence stays unavailable); another JSON type rejects the bundle. A partial
response for a failed/cancelled inference must not supply a non-null `response_id`
or `token_usage`; a conflicting shape is rejected rather than counted as a
completion. Missing completion evidence stays missing. Do not infer success,
delivery or a charged request from the existence of an attempt.

Sequence and lifecycle evidence, rather than wall-clock ordering, establish the
recorded start/terminal association. A backward-moving source clock is preserved
with a `trace_clock_regression` warning. It is not repaired with import time, and
no duration or clock-corrected ordering is invented from it.

### Model, effort, service tier and tokens

Keep request-side `model`, `reasoning.effort` and `service_tier` metadata separate
from response-side observations. This pinned completion summary does not report
an observed response model or service tier; those fields remain omitted, never
inherited from the request. A requested value is not evidence that the provider
executed that setting. The pinned
[request schema](https://github.com/openai/codex/blob/a956835d020762cb2b570053af06f643a11c0ecc/codex-rs/codex-api/src/common.rs#L278-L360)
can omit `service_tier` and permits nullable reasoning. Reported, explicit null,
omitted and invalid values remain distinct; missing effort is not a default effort,
and missing or unfamiliar service tier is not evidence for Fast or Standard.

Only recorded valid usage is counted; null or absent usage is not zero. Input,
cached input, output, reasoning output and reported total retain their own
meanings. Cached input is already part of input and reasoning output already part
of output, so do not add either subset twice. Preserve recorded total instead of
manufacturing an invoice. Trace totals are not added to ordinary rollout response
totals or account observations, because those sources can describe overlapping
activity.

A weekly date selection means recorded trace evidence in the selected dates. It
is not exact weekly quota consumption, a fixed allowance, purchased credits,
API-price conversion or billing reconciliation. Account quota percentages and
reset windows remain independent provider observations. No token multiplier for
model, effort or service tier is inferred; no per-skill cost is assigned.

## Privacy, replay and retention

Upstream raw traces may contain prompts, system/developer instructions, hidden
reasoning, tool data, private paths and other sensitive material. Content capture
in Usage Lens remains off by default. To retain the supported visible-text
projection, opt in before accepting new records:

```sh
'/ABSOLUTE/usage-lens' settings --db '/ABSOLUTE/private/usage.sqlite' --content true
```

The optional projection retains only supported visible user/assistant text.
Tool arguments/results, attachments, instructions, headers and unknown body fields
are excluded from this trace projection. Request user-role text is eligible only when
`content_item_kinds` is an exactly aligned array and the corresponding item is
`user.text`. Missing, mismatched or unfamiliar classification is not assumed to be
human-authored text. Eligible assistant text has `phase` absent/null or exactly
`commentary` / `final_answer`; an unknown phase is excluded. Assistant items with
nonempty or invalid typed `content_item_kinds` are excluded too. Message `channel`
must be absent/null, `final` or `commentary`; analysis/hidden-reasoning items are
excluded. The reader is partial: request input may remain unavailable even with
content enabled, and omitted projection content is never a complete transcript.
It excludes system/developer instructions,
hidden reasoning, excluded injected instruction/skill bodies and recognized
credentials. It never retains an entire raw request/response JSON blob as a
fallback. Redaction is best-effort; the retained database remains private.
The source bundle is read-only and untouched. Usage Lens deletion or redaction
does not sanitize or erase those raw files, their backups or exported copies.

Import acceptance, normalized evidence and replay identities are atomic. Identical
reimports are no-ops; immutable identity conflicts reject the entire import.
Enabling content later does not backfill old records. Content-only deletion and
retention preserve replay protection so reimporting cannot resurrect deleted
content or evidence. Explicit all-data deletion, including source-scoped deletion,
removes retained trace attempts/content but **preserves trace import/replay
metadata too**. Reimporting the same bundle cannot undo that deletion. This differs
from the ordinary incremental rollout reset behavior. A deliberate import into
a newly created source is a separate namespace with potentially overlapping
counts, not recovery of the deleted source. These protections cannot reconstruct
previously deleted pre-upgrade history or prove that named sources do not overlap.

v0.7.0 read-only queries support schema 2, 3 and 4 rollback-journal stores without
migration. Keep a compatible pre-upgrade backup for binary rollback. Do not try to
open schema 4 with an older binary or modify schema numbers to bypass that check.

## Bounded bundle contract

The importer fails closed at its limits; it never silently truncates a payload to
fit. Limits cover the selected bundle, not all historical traces:

- Manifest: 64 KiB; trace file: 8 MiB; JSONL physical line: 256 KiB
- Individual payload: 2 MiB; combined bounded input: 32 MiB
- At most 20,000 trace events, 1,000 attempts and 2,000 referenced payloads
- JSON nesting depth: 32; payload ordinals contain at most 20 digits
- Normalized import: 2 MiB core validation budget; each visible-text projection:
  512 KiB core validation budget and at most 1,000 messages, independently of the
  larger raw-input budget. Core budgets account for structural overhead and are
  not an exact serialized JSON-file byte allowance
- Referenced paths match the pinned writer's `payloads/[1-9][0-9]*.json` shape and
  its `raw_payload:<ordinal>` identity; absolute paths, traversal and linked paths
  are rejected
- Windows bundle roots must be local drive paths; UNC, network and device roots
  are rejected before any opening. Symlinks/reparse points and linked ancestors
  remain unsupported

These limits are cumulative requirements, not alternative budgets. A request can
fit the 2 MiB raw-payload limit yet exceed the 512 KiB visible-projection limit;
several individually valid projections can exceed the 2 MiB normalized-import
limit. Either case rejects the entire import, including when content capture is
off. The importer never silently clips messages or commits only token metadata.
Content capture controls persistence after validation; it does not bypass the
canonical projection used to detect conflicting replay and prevent content
backfill. `input_too_large` or `invalid_input` at these limits leaves existing
store rows, replay state and schema unchanged. Disabling content capture is not a
size-limit workaround. Use a smaller, independently complete source bundle;
do not rewrite an already accepted bundle or remove evidence to make it fit.

Only explicitly referenced supported payloads are read; this is not directory
recursion or an arbitrary-file endpoint. Missing files, malformed/incompatible
shapes, unsafe references and immutable conflicts fail without a partial import.
Choose a closed stable bundle, or an explicitly selected stable private copy,
before importing. Bundle and attempt evidence is immutable: importing an active
bundle and then changing it is not an append/update workflow; a later conflicting
version is rejected. An incomplete bundle can retain unknown completion evidence,
which does not authorize mutation of its accepted historical records. A successful
read of local diagnostic files is not proof of complete recording.
The upstream recorder itself is best-effort, so omitted evidence stays unknown.

All repository fixtures, packaging checks and documentation examples are synthetic.
A synthetic pass proves only the exercised parser, local query and safety cases.
Exact-commit gates and extracted-artifact checks remain required before publication;
see [verification scope](verification.md) and [release policy](releases.md).
