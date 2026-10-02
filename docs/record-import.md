# Explicit local rollout import

Usage Lens can project an explicitly selected Codex JSONL rollout file into its local database. This is a version-pinned, bounded file import, not a filesystem collector or a service-authenticated usage API. Imported files are editable and can spoof every field. The source remains `imported`, its account binding is unverified, and coverage is always partial.

The implementation and all tests were developed with synthetic strings. No real user histories are needed by the test suite. Importing does not launch Codex, start an app-server, authenticate, call a model, execute imported instructions, or open any path mentioned inside a record. There is no automatic history scan, directory import, watch process, or import on startup.

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

## CLI workflow

Use absolute paths to a local database and to one file you have deliberately selected:

```sh
node dist/cli/main.js source --db /absolute/path/usage.sqlite --source rollout-local --mode imported --name 'Local rollout records'

node dist/cli/main.js import-rollout \
  --db /absolute/path/usage.sqlite \
  --source rollout-local \
  --file /absolute/path/selected-rollout.jsonl \
  --source-version a75987455a2879ca151cea5e118fa307be868583
```

Content capture is off by default. If you deliberately want local review of available plain user messages, visible assistant messages, and generic tool arguments/results, enable it **before** importing:

```sh
node dist/cli/main.js settings --db /absolute/path/usage.sqlite --content true
```

This increases what is retained locally. Secret redaction is best-effort. Aggregate, HTTP summary, and plugin methods do not expose this content; only the separate local detail surface can return it.

The identical file is imported once per source. Re-importing after enabling content capture does **not** backfill earlier events. Re-importing after deleting local content does **not** restore it. To deliberately import the same file under a different capture scope, create a new explicit source, understanding that this is a separate namespace and its counts overlap. Turning capture off does not delete previously retained content; use the existing explicit confirmed local-content deletion operation if that is what you want.

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

## Identity, limits, and atomicity

- Exact file bytes are SHA-256 fingerprinted. The core import ledger makes a repeated file fingerprint a no-op within the same source
- Function calls deduplicate by source namespace, thread, and call ID. Conflicting call definitions or outputs reject the entire parse
- Response tokens deduplicate by source, thread, session, and response ID. Conflicting per-response counts or immutable context reject atomically; changes only in cumulative snapshots do not double count or conflict
- Typed injection identities use the thread, stable source item ID, and content index. When an item ID is absent, the fingerprint plus source line/ordinal and content index scope the observation to this file
- Without preceding session metadata, tool identities are deliberately file-scoped and produce a warning. Missing `session_meta.session_id` can fall back to that metadata's thread ID for upstream compatibility; missing token-record session IDs never receive this fallback
- Copied files with changed bytes, inherited/forked histories, and missing stable IDs can still overlap. Counts represent observed evidence within their declared source, not universal invocation counts or guaranteed complete history

Parser hard limits are 8 MiB total bytes, 256 KiB per line, 20,000 physical lines, nesting depth 32, 1,000 projected events, and 1,000 response-token records. Callers can tighten these limits but cannot raise them. The core also caps the combined normalized import payload at 2 MiB, so a file under the parser's byte limit can still be rejected atomically by storage. Splitting a file without preserving identity context can defeat deduplication and is not an automatic workaround.

Invalid UTF-8, malformed JSON, invalid envelopes, unsupported source declarations, invalid required fields, identity conflicts, and exceeded limits stop the whole import before any batch is committed. A complete valid final JSON line without a newline is accepted with a warning. A truncated final record is rejected. Unknown record types are skipped with an explicit coverage warning. Errors and persisted warning codes contain no imported bodies or embedded paths.

## Pure adapter API and verification

`parseRollout(stringOrBytes, {sourceId, observedAt, captureContent, sourceVersion, limits?})` returns normalized `events`, `responseTokens`, `fingerprint`, source/adapter versions, human-readable `warnings`, safe fixed `warningCodes`, and `recordsSeen`. It does not read or write files. The caller supplies the bounded bytes and passes the entire result to the core's atomic import operation.

`tests/adapters/rollout.test.ts` uses synthetic in-memory JSONL to cover exact skill proof, false mentions, catalog exclusion, pagination, typed injection alignment, failures, malformed payloads, output ordering, duplicate/conflicting identities, unsafe integers, cumulative exclusions, content opt-in, unknown records, truncation, and all bounds. Its core integration case verifies fingerprint idempotence and separate skill evidence categories.
