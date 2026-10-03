# Explicit local rollout import

Usage Lens can project an explicitly selected Codex JSONL rollout file into its local database. The existing `import-rollout` is a snapshot import; v0.4.0 adds the separate `import-rollout-incremental` one-shot write workflow. Both are version-pinned, bounded file imports, not filesystem collectors or service-authenticated usage APIs. Imported files are editable and can spoof every field. The source remains `imported`, its account binding is unverified, and coverage is always partial.

The implementation and all tests were developed with synthetic strings. No real user histories are needed by the test suite. Importing does not launch Codex, start an app-server, authenticate, call a model, execute imported instructions, or open any path mentioned inside a record. There is no automatic history scan, directory import, watch process, or import on startup.

For the separate `rust-v0.160.0` RolloutTrace directory contract, use
[trace import](trace-import.md). It reads one chosen bounded bundle with validated
payload references, not an ordinary rollout file. Its schema-4 upgrade is separate
from the schema-3 incremental workflow below; v0.5.0 and older cannot read schema 4.
The "no directory import" and embedded-path statements here describe only these
ordinary rollout commands, not an authorization to discover trace bundles.

## Supported source declaration

The importer requires the caller to explicitly declare this source baseline:

`a75987455a2879ca151cea5e118fa307be868583`

This declaration selects a parser. It does not verify that a file came from that revision or from Codex at all. Other versions are rejected instead of silently guessed.

Relevant pinned upstream definitions:

- [History JSONL envelope](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/history/src/lib.rs#L355-L367) and [rollout payload variants](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/history/src/rollout_payload.rs#L30-L71)
- [Response-token record and count fields](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/protocol/src/protocol.rs#L2240-L2273)
- [Session metadata](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/protocol/src/protocol.rs#L3123-L3141), [session-ID compatibility](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/protocol/src/protocol.rs#L3235-L3268), and [turn metadata](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/protocol/src/protocol.rs#L3296-L3335)
- [Typed content kinds](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/protocol/src/models.rs#L954-L974) and [tool call/output encoding](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/protocol/src/models.rs#L1074-L1133)
- [Skill read inputs/results](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/ext/skills/src/tools/read.rs#L34-L56), [successful read output](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/ext/skills/src/tools/read.rs#L275-L290), [tool namespace](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/ext/skills/src/tools/mod.rs#L294-L313), and [generated injection wrapper](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/ext/skills/src/fragments.rs#L62-L109)

## Snapshot CLI workflow

Use the installed native `usage-lens` executable on PATH, or replace its name with
the quoted absolute executable path. Release builds do not need Node.js. Database
and import-file paths must be absolute and deliberately selected. These examples
use POSIX-shell quoting:

```sh
usage-lens source --db '/absolute/path/usage.sqlite' --source rollout-local --mode imported --name 'Local rollout records'

usage-lens import-rollout \
  --db '/absolute/path/usage.sqlite' \
  --source rollout-local \
  --file '/absolute/path/selected-rollout.jsonl' \
  --source-version a75987455a2879ca151cea5e118fa307be868583
```

Content capture is off by default. If you deliberately want local review of available plain user messages, visible assistant messages, and generic tool arguments/results, enable it **before** importing:

```sh
usage-lens settings --db '/absolute/path/usage.sqlite' --content true
```

This increases what is retained locally. Secret redaction is best-effort. Aggregate, HTTP summary, and plugin methods do not expose this content; only the separate local detail surface can return it.

For Windows PowerShell, use the call operator when invoking a quoted executable path.
The following are the equivalent setup and import commands; replace the literal
placeholder paths. Run the settings command only if you deliberately choose content
capture before importing:

```powershell
& 'C:\ABSOLUTE\usage-lens.exe' source --db 'C:\ABSOLUTE\usage.sqlite' --source rollout-local --mode imported --name 'Local rollout records'
# Optional local content opt-in:
& 'C:\ABSOLUTE\usage-lens.exe' settings --db 'C:\ABSOLUTE\usage.sqlite' --content true
& 'C:\ABSOLUTE\usage-lens.exe' import-rollout --db 'C:\ABSOLUTE\usage.sqlite' --source rollout-local --file 'C:\ABSOLUTE\selected-rollout.jsonl' --source-version a75987455a2879ca151cea5e118fa307be868583
```

POSIX and PowerShell single-quoted paths prevent variable/command expansion and keep
spaces within one argument. If a real path contains a single quote, use that shell's
escaping rules instead of substituting it verbatim. When invoking from code, prefer
a subprocess argument vector without a shell; do not include quote characters in
individual arguments. Never insert imported record fields into a shell command.

The identical file is imported once per source. Re-importing after enabling content capture does **not** backfill earlier events. Re-importing after deleting local content does **not** restore it. To deliberately import the same file under a different capture scope, create a new explicit source, understanding that this is a separate namespace and its counts overlap. Turning capture off does not delete previously retained content; use the existing explicit confirmed local-content deletion operation if that is what you want.

## Incremental one-shot workflow

**Back up the closed store before the first successful incremental import.** Stop
other Usage Lens processes/writers and make a verified private backup. The first
successful explicit incremental import migrates schema 2 to schema 3 in the same
transaction as its records and checkpoint. Failed parsing or validation does not
migrate. Ordinary writable opens and new databases remain schema 2 until this
upgrade. v0.4.0 read-only queries and `doctor` accept schema 2 and 3 without
migrating either. **v0.3.0 and older cannot open schema 3.** To roll back, use an
older executable with a separate compatible copy of the pre-upgrade backup, never
the upgraded store. See [upgrade guidance](AI_INSTALL.md#upgrade-rollback-and-uninstall).

After explicitly creating an imported source, invoke:

```sh
usage-lens import-rollout-incremental \
  --db '/absolute/path/usage.sqlite' \
  --source rollout-local \
  --file '/absolute/path/selected-rollout.jsonl' \
  --stream selected-rollout-1 \
  --source-version a75987455a2879ca151cea5e118fa307be868583
```

Windows PowerShell equivalent:

```powershell
& 'C:\ABSOLUTE\usage-lens.exe' import-rollout-incremental --db 'C:\ABSOLUTE\usage.sqlite' --source rollout-local --file 'C:\ABSOLUTE\selected-rollout.jsonl' --stream selected-rollout-1 --source-version a75987455a2879ca151cea5e118fa307be868583
```

Run that explicit command again after appending records. Every invocation requires
the absolute path, source, logical stream ID and supported source version. There
is no watch mode, scheduler, glob, directory discovery, remembered path to reopen,
embedded-path traversal or automatic startup. This write is outside the
conversational Skill's eight aggregate commands and the optional MCP tools;
asking for a summary never authorizes an import or migration.

### Complete-prefix checkpoints

Each call boundedly rereads and reparses the complete newline-terminated prefix.
This is **not O(delta) tailing**. Reparsing reconstructs context and matches later
tool outputs to preceding calls without persisting pending call content. Every
unterminated tail is deferred, even if it is valid JSON or ends in partial UTF-8.
It becomes eligible only after a newline is supplied. Invalid UTF-8 or malformed
JSON in a complete line rejects the whole call. The snapshot command retains its
different final-line behavior described below.

The store retains complete-prefix byte and physical-line counts, a SHA-256 prefix
digest, parser/source-version metadata, and source-wide hashed identity/replay
metadata. It does not save pending tail bytes, raw tool payloads, transcript paths,
or an incremental parse-state blob. Separately enabled ordinary content capture
still follows the existing content allowlist and best-effort redaction rules.

The next call verifies the already committed bytes before advancing. Truncation
below that prefix or any committed-prefix change produces a fixed safe error and
does not mutate evidence, replay state or checkpoint. A byte-identical copied
prefix is accepted under the same logical stream; this is a user-declared logical
identity, **not a verified physical-file identity**. Do not silently retry rotation
with reset progress. Choose a new logical stream or source explicitly for a new
file. A new stream in the same source still shares that source's replay ledger.

The JSON result includes numeric `completeBytes`, `completeLines` and
`deferredBytes`, boolean `checkpointAdvanced`, decimal-string `eventsInserted`
and `responseTokensInserted`, and stream/source-version/record-count/warning
context. Deferred bytes are not accepted evidence. Review warnings even when no
new rows are inserted. Fixed safe failures include `rollout_prefix_changed` for
truncation or committed-prefix changes, `rollout_stream_version_mismatch` for an
existing stream's parser/source-version mismatch, `rollout_identity_conflict`
for conflicting immutable identities and `rollout_checkpoint_conflict` for stale
concurrent progress. Do not treat any of these as permission to reset metadata.

### Replay, conflicts, retention and capture

Evidence, hashed replay entries and checkpoint advancement commit atomically.
Compare-and-swap checks reject stale concurrent progress rather than overwriting
another import. Replaying an unchanged committed prefix is a no-op. Cross-run
immutable identity conflicts anywhere in the same source reject the complete
incremental import, including changes encountered through a different stream.

Retention and content-only deletion preserve replay protection. Re-reading old
prefixes cannot resurrect deleted events/response records, restore deleted
content, or backfill old content after capture is enabled. An output arriving
later may complete an eligible retained call; it does not authorize recovery of
an already deleted call or its earlier content. Explicit `delete --target all`
(all sources, or scoped by `--source`) resets the corresponding evidence and
incremental metadata. A deliberate reimport after such a reset is a new write.

Schema 3 retains minimal replay tombstones for subsequent snapshot/event writes
too. Both local event IDs and canonical `(sourceEventId, eventType)` aliases are
protected, including zero-insert adoption under a different local ID; conflicting
immutable metadata rejects the transaction. Tombstones cannot reconstruct
already-deleted pre-upgrade history. Snapshot
file-scoped anonymous identities and incremental stream-scoped identities differ.
When switching modes, use a separate source if overlap is possible; no cross-mode
deduplication of anonymous records is promised. Separate source counts must not
be added as unique history.

When adopting existing pre-incremental records, the comparison can verify only
retained immutable metadata. A record imported with content capture disabled has
no original raw-body hash to reconstruct, so adoption cannot prove that its
unretained body matches. Adoption never backfills that missing content. Once
incremental evidence has a replay digest, subsequent incremental-to-incremental
imports compare the immutable raw evidence hash regardless of content settings.

### Bounded files, not unbounded tails

The same 8 MiB source-file, 256 KiB line, 20,000 physical-line, 1,000 projected-event
and 1,000 response-record limits apply to each bounded reread, as do the depth and
projected-batch limits below. Changing a stream ID does not make an oversized file
acceptable. Once a stream reaches a bound, continue with another deliberately
selected bounded logical file/stream and preserve the necessary context. There is
no automatic split, rotation or unlimited growing-file support.

## What the projection means

### Per-response token observations

Only `token_usage_record.payload.usage` contributes to response-token totals. All required thread, turn, session, root-turn, and response identities must be present and nonempty. The importer additionally requires bounded identifiers; this is stricter than an upstream Rust `String` alone.

All six count fields are validated as nonnegative signed-64-bit integers. Only omitted `cache_write_input_tokens` defaults to zero. Integers above JavaScript's safe numeric range are parsed losslessly and normalized to decimal strings. Fractional, negative, missing, out-of-range, or unsafe noncanonical numeric encodings are not silently rounded.

`turn_token_usage` and `thread_token_usage` are required and validated because they belong to this pinned record schema, but are cumulative snapshots. They are never summed, exposed as additional response usage, or used to change a duplicate response's totals. Legacy `event_msg` / `token_count` snapshots, including `info.total_token_usage` and `info.last_token_usage`, are unsupported for response accounting and produce a coverage warning.

Cached input is already included in input tokens; reasoning output is already included in output tokens. These components must not be added again. Response-token observations are not combined with account-wide token activity to make a purported complete total.

Model attribution comes only from directly supplied `turn_context` metadata with a matching thread and turn. Missing or invalid model metadata stays unknown. The importer never infers a model from text, filenames, or token counts.

### Skill evidence

Two distinct categories can produce a `skill_loaded` evidence row:

1. `main_read`: an exact `function_call` with `namespace: "skills"`, `name: "read"`, JSON-string arguments containing a package, no resource, and no cursor, matched to a subsequent same-thread `function_call_output` with the same call ID. The output must be a JSON string matching the successful `{resource, contents, next_cursor, skill_root?}` schema. There is no invented serialized success flag. If `next_cursor` is non-null, the importer warns that only a successful first page is proven, not complete skill loading.
2. `instruction_injection`: a user message content item annotated at the same array index with exactly `skills.selected_skill_instructions`, containing the generated leading `<skill>`, `<name>`, and `<path>` wrapper. Only the name/path header is checked, and no path is followed. The instruction body is never retained as ordinary message content.

These categories may describe the same underlying skill interaction. They are kept separate and must not be added together as unique loads. Neither category proves that a task succeeded or that a skill was invoked to execute work. No `skill_invoked` rows are inferred by this importer.

Plain mentions, assistant claims, catalogs, malformed annotations, unmatched/error outputs, and lookalike tool namespaces do not establish loaded-skill evidence. Explicit resource reads and pagination continuations remain generic tool evidence and do not add main-read counts. This version has no catalog-backed main-resource resolver, so it conservatively declines to treat any explicit resource read as a main read.

### Local visible events and exclusions

The projection supports ordinary plain-text user messages, visible final or unchanneled assistant messages, and function calls. Unknown response item kinds are skipped. Typed catalogs, selected skill instruction bodies, system/developer messages, reasoning items, and non-visible assistant channels are excluded. Any malformed typed-content alignment excludes the whole message instead of reclassifying its text as ordinary user content.

Every `skills`-namespace output body is excluded from retained generic tool results, including catalog and read outputs. Generic tool arguments/results are only passed to the local store when capture is enabled, and the store applies its content allowlist/redaction. A tool call alone has unknown success status. Outputs preceding their matching calls are not accepted as result/success evidence.

## Snapshot identity, shared limits, and atomicity

- Exact file bytes are SHA-256 fingerprinted. The core import ledger makes a repeated file fingerprint a no-op within the same source
- Function calls deduplicate by source namespace, thread, and call ID. Conflicting call definitions or outputs reject the entire parse
- Response tokens deduplicate by source, thread, session, and response ID. Conflicting per-response counts or immutable context reject atomically; changes only in cumulative snapshots do not double count or conflict
- Typed injection identities use the thread, stable source item ID, and content index. When an item ID is absent, the fingerprint plus source line/ordinal and content index scope the observation to this file
- Without preceding session metadata, tool identities are deliberately file-scoped and produce a warning. Missing `session_meta.session_id` can fall back to that metadata's thread ID for upstream compatibility; missing token-record session IDs never receive this fallback
- Copied files with changed bytes, inherited/forked histories, and missing stable IDs can still overlap. Counts represent observed evidence within their declared source, not universal invocation counts or guaranteed complete history

Parser input limits are 8 MiB of source-file bytes, 256 KiB per line, 20,000 physical lines, JSON nesting depth 32, 1,000 projected events, and 1,000 response-token records. Callers can tighten these limits but cannot raise them.

The parser and store also share a projected-batch preflight: the combined normalized import payload is limited to 2 MiB and 100,000 JSON nodes across the batch, with at most 1,000 events and 512 KiB per event's local content. The total cap still applies when every individual content object fits its own limit. The 8 MiB source-file allowance is not an 8 MiB retained-content allowance.

Projected content depth is measured from each event's content-object root, without counting the surrounding import wrapper. For example, tool arguments with 29–31 nested arrays fit the depth limit when the other budgets permit; 32 nested argument arrays plus the content wrapper do not. Exceeding a projected budget produces the safe `rollout_projection_limit` error before storage. The same file may fit with content capture disabled because its projection is smaller. Splitting a file without preserving identity context can defeat deduplication and is not an automatic workaround.

Invalid UTF-8, malformed JSON, invalid envelopes, unsupported source declarations, invalid required fields, identity conflicts, and exceeded limits stop the whole snapshot import before any batch is committed. In snapshot mode, a complete valid final JSON line without a newline is accepted with a warning, and a truncated final record is rejected. Incremental mode instead defers every unterminated tail as described above. Unknown record types are skipped with an explicit coverage warning. Errors and persisted warning codes contain no imported bodies or embedded paths.

## Pure adapter API and verification

`usage_lens::adapters::rollout::parse_rollout(bytes: &[u8], options: &serde_json::Value) -> Result<serde_json::Value, AdapterError>` accepts the camelCase options `sourceId`, `observedAt`, `captureContent`, `sourceVersion`, and optional `limits`. On success it returns normalized `events`, `responseTokens`, `fingerprint`, source/adapter versions, human-readable `warnings`, safe fixed `warningCodes`, and `recordsSeen`. It does not read or write files. The caller supplies bounded bytes, then passes the parser's `sourceId`, `fingerprint`, `adapterVersion`, `sourceVersion`, `warningCodes`, `events`, and `responseTokens` to `UsageStore::import_rollout`, adding `importedAt` from the same caller-supplied `observedAt`. Presentation-only fields such as `warnings` and `recordsSeen` are not storage inputs; the native CLI performs this mapping.

The separate `usage_lens::adapters::incremental::import_incremental_rollout`
orchestrates a bounded byte slice and explicitly supplied `sourceId`, `streamId`,
`observedAt` and `sourceVersion` against the selected `UsageStore`. It verifies
the prior checkpoint, applies the store's content settings, invokes the separately
versioned incremental parser and commits through `UsageStore::import_rollout_incremental`.
It does not select or reopen a file; the CLI supplies the explicitly selected bytes.

`crates/usage-lens/tests/rollout.rs` uses synthetic in-memory JSONL to cover exact skill proof, false mentions, catalog exclusion, pagination, typed injection alignment, failures, malformed payloads, output ordering, duplicate/conflicting identities, unsafe integers, cumulative exclusions, content opt-in, unknown records, truncation, and all bounds. Its core integration case verifies fingerprint idempotence and separate skill evidence categories.
