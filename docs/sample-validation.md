# Selecting and validating a redacted rollout sample

For a first compatibility check, prepare **one small, manually selected, redacted UTF-8 JSONL file** from a saved Codex rollout. Keep the original on your own machine. A private sample check can establish which records this importer understands; it cannot establish complete account history, billing accuracy, or live collector access.

This guide describes a procedure. It does not assert that a real uploaded sample has passed validation. Public tests and examples remain synthetic.

## Producer directories and filenames

The locations below come from the official Codex source at commit `a75987455a2879ca151cea5e118fa307be868583`, the same source baseline declared by the importer. They are locations a person may inspect on their own machine, **not directories Usage Lens scans**.

`CODEX_HOME` selects the producer's home directory. When unset or empty, that source uses the operating-system home directory plus `.codex`, commonly written as `~/.codex`. A custom `CODEX_HOME`, another OS user, WSL, a container, or a remote machine can have a separate set of records. No local directory is guaranteed to contain all activity. [Official home resolution](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/utils/home-dir/src/lib.rs#L5-L62)

| Producer location | Candidate files | What to select |
| --- | --- | --- |
| `<CODEX_HOME>/sessions/YYYY/MM/DD/` | `rollout-YYYY-MM-DDTHH-MM-SS-<thread-id>.jsonl` | One completed, ordinary plaintext rollout that you deliberately choose |
| `<CODEX_HOME>/sessions/YYYY/MM/DD/` | `rollout-YYYY-MM-DDTHH-MM-SS-<thread-id>_<rollout-id>.jsonl` | A reverted-thread variant; retained identities and inherited history still need review |
| `<CODEX_HOME>/archived_sessions/` | The same rollout basenames in a flat archive directory | One deliberately selected archived plaintext rollout |
| Either rollout area | Files ending in `.jsonl.zst` | A compressed producer representation; **not accepted directly** by this importer |

The producer creates the active date hierarchy using its local time. Filenames are discovery hints; Usage Lens uses the records' timestamps and identities, not the filename, to build observations. [Active path construction](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/rollout/src/recorder.rs#L1723-L1745), [filename forms](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/rollout/src/rollout_file_name.rs#L10-L73), [archive layout](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/rollout/src/recorder.rs#L1541-L1560), [directory constants](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/rollout/src/lib.rs#L86-L87)

On a conventional macOS or Linux installation, the active directory is therefore `~/.codex/sessions/YYYY/MM/DD/`; on a conventional Windows user profile, the equivalent is `%USERPROFILE%\.codex\sessions\YYYY\MM\DD\`. Prefer the actual `CODEX_HOME` chosen for that producer when it differs. These defaults do not authorize access to a computer or directory.

The pinned producer supports `.jsonl.zst` storage, while Usage Lens reads bounded plaintext bytes only. If the chosen original is compressed, prepare a bounded plaintext copy locally with a trusted decompression tool, then review and redact that copy. Renaming compressed bytes to `.jsonl` does not convert them. This guide does not require starting Codex to obtain or inspect a copy. [Official compression support](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/rollout/src/compression.rs#L30-L75)

## Supported formats and version assumptions

- **Rollout input:** one UTF-8 JSON object per line, with `timestamp`, `type`, and `payload`; an `ordinal` can be supplied. The importer requires the exact declared source baseline above. It does not infer compatibility from a filename, a producer version string, or the existence of a `.jsonl` extension. [Pinned envelope](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/history/src/lib.rs#L355-L367), [record variants](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/history/src/rollout_payload.rs#L30-L71)
- **Usage Lens bundle:** the separate `import` command accepts the application's normalized v1 JSON bundle. This is not an automatic conversion path for arbitrary conversation exports. See the native CLI help and [data contract](data-contract.md).
- **Hook input:** the separate `hook` command accepts one supported lifecycle JSON event on stdin. A hook configuration or an unrelated provider's transcript is not a rollout file. There is no hook-log directory to upload for this workflow.
- **Account observations:** account usage and quota come from separate, explicitly initiated read methods. Rollout files cannot substitute for those responses, and this sample check does not start account collection.
- **Not accepted as rollout input:** ZIP archives, compressed bytes, JSON arrays of conversations, HTML chat transcripts, screenshots, SQLite databases, arbitrary application logs, or another provider's JSONL format.

An ordinary ChatGPT data export is a ZIP containing chat history and other account data. It has a different purpose and is not an input contract supported by this rollout adapter. Do not upload the whole ZIP for this check or rename a chat-history JSON/HTML file to make it appear compatible. No per-response token, tool-call, or typed-skill coverage is assumed from a normal ChatGPT export. [Official ChatGPT export description](https://help.openai.com/en/articles/7260999-exporting-your-chatgpt-history-and-data)

If the installed producer differs from the pinned baseline, state its version if already known and let the sample establish compatibility or a specific mismatch. Do not add missing fields, convert a legacy snapshot into response usage, or rewrite error records into successful results to make validation pass. A source declaration selects the parser; it does not authenticate the file.

## Source to field matrix

The following is the supported projection, not a promise that every saved rollout contains these records. Exact constraints and warning behavior are in [record import](record-import.md) and the native [parser](../crates/usage-lens/src/adapters/rollout.rs).

| Input evidence | Fields required for this projection | Usage Lens result and limits |
| --- | --- | --- |
| Every accepted envelope | `timestamp`, `type`, `payload`; optional nonnegative `ordinal` | UTC occurrence time, record count, deterministic file/line identity when no stable item ID exists |
| `session_meta` | `payload.id`; optional `session_id` and fork metadata | Thread context; event session can fall back to thread ID; fork declarations add an overlap warning |
| `turn_context` | Direct `turn_id` and `model` when supplied | Current turn/model context; invalid or absent model remains unknown, and model attribution requires matching thread and turn |
| `response_item` / visible `message` | User `input_text`, or assistant `output_text` on final/unchanneled messages; correctly aligned metadata if present | Prompt or visible-assistant evidence; text retained only with content capture enabled |
| `response_item` / `function_call` | `call_id`, `name`, optional namespace, JSON-string `arguments` | Generic tool evidence with unknown success; arguments retained only with content capture enabled |
| Matching `function_call_output` | Same thread and `call_id`, after the call | Generic result evidence when captured; an orphan or earlier output establishes no success |
| Exact `skills.read` initial call and matching result | `namespace: "skills"`, `name: "read"`, JSON-string arguments containing `package` with no non-null `resource`/`cursor`; JSON-string result with `resource`, string `contents`, `next_cursor`, and optional string `skill_root` | `main_read` skill-load observation at the first subsequent matching output time; non-null `next_cursor` warns that the first page may be incomplete; resource/continuation reads add no main-read count |
| Typed selected-skill instructions | User content item aligned to `internal_chat_message_metadata_passthrough.content_item_kinds[i] == "skills.selected_skill_instructions"`; generated leading `<skill>`, `<name>`, `<path>` wrapper and closing `</skill>` | `instruction_injection` evidence, with unknown execution status; instruction body and path are not retained as ordinary content |
| `token_usage_record` | `thread_id`, `turn_id`, `session_id`, `root_turn_id`, `response_id`; `usage`, `turn_token_usage`, `thread_token_usage` | Exact per-response accounting from `usage` only; required cumulative snapshots are validated and excluded from totals |
| The six `TokenUsage` fields | `input_tokens`, `cached_input_tokens`, `cache_write_input_tokens`, `output_tokens`, `reasoning_output_tokens`, `total_tokens` | Nonnegative signed-64-bit integers preserved as decimal strings; only omitted `cache_write_input_tokens` defaults to zero |
| Legacy `event_msg` / `token_count` | Any old cumulative snapshot | Unsupported-response-accounting warning; no response usage invented or summed |
| Catalogs, ordinary skill mentions, system/developer content, hidden reasoning, unknown record kinds | No accepted skill-use proof | Excluded from ordinary retained content or skipped with applicable coverage warnings; no inferred invocation |

The token schema is [defined upstream](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/protocol/src/protocol.rs#L2240-L2273). Skill evidence follows the [typed content kinds and tool encoding](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/protocol/src/models.rs#L954-L1133), [read input/result schema](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/ext/skills/src/tools/read.rs#L34-L56), and [generated injection wrapper](https://github.com/openai/codex/blob/a75987455a2879ca151cea5e118fa307be868583/codex-rs/ext/skills/src/fragments.rs#L62-L109).

Cached input is a component of input; reasoning output is a component of output. Neither is added again. The reported total is preserved. Main-read and instruction-injection observations may overlap and are not added as unique invocations. No field establishes a skill's token cost, verified account binding, service-side completeness, or task success.

## Prepare a safe copy manually

1. Choose one known conversation or task on your own machine and copy only its selected rollout to a separate review location. Do not send a home directory, recursive archive, all sessions, or an application backup. Do not alter the live original.
2. Keep a small representative excerpt with complete JSONL records in original order. Preserve the relevant preceding `session_meta` and `turn_context`, the chosen call/output pairs, and complete token records. Avoid taking an arbitrary tail that drops their identity context. Label omitted sections in a separate note, not as invalid comment lines inside JSONL.
3. Remove secrets and private content **before uploading**. Review plain text and nested JSON strings, tool arguments/results, URLs and query strings, repository names, local paths, email addresses, attachments, and free-form instruction bodies. The importer's later redaction and content exclusions do not protect an unredacted file during upload.
4. Replace identities consistently with distinct valid aliases such as `thread-1`, `session-1`, `turn-1`, `response-1`, `call-1`, and `item-1`. Keep repeated references equal and unrelated identities different. Preserve model/skill/tool labels only if comfortable sharing them; otherwise use consistent aliases and disclose that those labels were changed.
5. Keep field names, JSON types, record ordering, call/output relationships, content-array alignment, and null-versus-absent distinctions. For a successful skill-read result, replace private `contents` with a harmless string and redact path strings, but preserve the result shape and whether `next_cursor` is null. Never turn an error into a successful result. For typed instructions, preserve the wrapper shape and array annotation while replacing private name/path/body text.
6. Preserve token quantities as exact integer digits. Do not use a spreadsheet or a JSON editor that rounds large integers. If the counts themselves cannot be shared, omit the complete token records and mark response-token validation unavailable. Do not replace counts with invented zeros. Keep timestamps when possible; if privacy requires shifting dates, shift them consistently and disclose that the dates were changed.
7. Reopen the final copy in a local text editor and verify that it contains only what you intend to share. Use a neutral filename such as `selected-rollout.redacted.jsonl`. Provide a short note describing the producer/version if known, whether the sample was excerpted, which fields were pseudonymized, whether timestamps changed, and what you want checked. Do not include the original private path.

Never upload `auth.json`, full `config.toml`, cookies, browser profiles, keychain exports, passwords, API keys, access or refresh tokens, `.env` files, private keys, or any complete provider/Usage Lens SQLite database and its WAL/SHM files. They are not required for a rollout compatibility check. Do not include raw provider/account responses or proprietary source code unless that specific sharing is deliberately authorized. If safe redaction is uncertain, stop with the file local and share only a structural description of the record types.

Keep the reviewed input below 8 MiB, each physical line below 256 KiB, and the file at or below 20,000 lines, nesting depth 32, 1,000 projected events, and 1,000 response-token records. The normalized combined storage payload also has a 2 MiB limit. Exceeding a limit is an atomic rejection, not permission to silently truncate or split away required context.

## Validation plan for a supplied sample

1. **Record intake scope.** Work only on the explicitly supplied redacted copy. Record its byte count, fingerprint, claimed producer/source version, and disclosed redactions. Do not discover sibling files, open embedded paths, follow inherited-history references, start Codex, or contact a provider.
2. **Check syntax and schema.** Parse the bounded bytes with content capture off. Report safe error/warning codes and missing or unsupported record categories. A mismatch is a useful result; do not manufacture a compatible shape.
3. **Reconcile the projection.** Compare accepted records and projected event/token counts against the selected lines. Check direct model context, exact token digits, excluded cumulative snapshots, matched call/output order, first completion timestamp, separate skill evidence categories, and omitted content.
4. **Exercise storage in isolation.** Import into a disposable database under a new explicitly imported source. Repeat the identical file and verify that the fingerprint ledger prevents duplication or content backfill. Test conflicting duplicates with separate synthetic derivatives, clearly labeled as such, and verify atomic rollback.
5. **Check the exposed surfaces.** Verify that local aggregates preserve imported/partial warnings and that aggregate/plugin responses expose no message bodies, instruction bodies, catalog text, or generic tool contents. If optional local content capture is requested, use a separate disposable source and only the already reviewed copy.
6. **Write a scoped result.** Report accepted/rejected status, parser and declared source versions, record-category coverage, exact response-token totals where supported, warnings, redaction limitations, and the checks actually run. Keep the supplied file and any sample-derived database out of the repository, public issues, screenshots, published build artifacts, and synthetic fixtures unless separately authorized.

For a local trial, the native commands are `usage-lens source --db /absolute/path/disposable.sqlite --source sample-check --mode imported --name 'Redacted sample check'` followed by `usage-lens import-rollout --db /absolute/path/disposable.sqlite --source sample-check --file /absolute/path/selected-rollout.redacted.jsonl --source-version a75987455a2879ca151cea5e118fa307be868583`. These are optional instructions for a deliberately chosen local copy; they do not enable live collection. Content capture is off by default in a new database.

## How to describe the evidence

Keep these claims separate in every result:

| Evidence class | What it establishes | What remains unverified |
| --- | --- | --- |
| Synthetic regression tests | Specified parser, identity, precision, exclusion, bounds, and storage behavior on generated inputs | Compatibility with any particular installed producer or real account |
| Accepted real redacted sample | Compatibility and observed coverage for the exact supplied, modified bytes | Authenticity, unredacted originals, omitted records, other versions or files, complete history |
| Synthetic derivatives of a real sample | A specific duplicate/conflict/error regression, with the modification stated | That the introduced condition occurred in the real producer |
| Explicit local live collection | Only the authorized method/runtime/account response actually exercised | Earlier history, all accounts/workspaces, per-skill attribution, untested startup behavior |
| Account-wide coverage | Only directly supported provider observations with their freshness and missingness | A complete service ledger or billing reconciliation inferred from local files |

A sample import can pass while account collection, real plugin-client connection, and complete historical coverage all remain untested. Missing evidence stays unknown rather than becoming zero.
