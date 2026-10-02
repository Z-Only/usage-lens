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

## Per-response token records

Supported `token_usage_record` inputs contain required source identity fields and per-response `usage`. Deduplication is by source/thread/session/response identity. Conflicting duplicates are rejected. The store keeps exact reported input, cached input, cache-write input, output, reasoning output and total quantities.

Only per-response usage is summed. Cumulative turn/thread usage is not added. Cached input is already part of input; reasoning output is already part of output. Preserve reported total rather than constructing a second bill. Legacy cumulative `token_count` records remain a coverage warning unless independently supported; they are never added to response totals.

Account totals and imported response totals remain separate. No response field assigns tokens to one particular skill. Co-occurrence in a turn is association, not cost attribution.

## Persistence and query safety

SQLite uses prepared statements, bounded inputs/queries, explicit migrations, atomic imports and secure deletion of active database content. This is not forensic erasure of backups, snapshots or exports. Retention is an explicit operation with an all-source scope clearly disclosed.

The dashboard binds only to loopback, validates Host/Origin and mutation requests, serves only embedded allowlisted assets, and never exposes arbitrary SQL or file paths. The plugin cannot collect, modify settings, delete records, or return stored bodies. Queries preserve provenance, freshness, missingness and imported coverage warnings.

## Verification limits

Synthetic protocol and format tests establish behavior for their tested inputs. They do not verify the user's installed version, actual account access, completeness of saved history, startup side effects, or a real ChatGPT client connection. Those capabilities must be checked independently on the user's own machine; unsupported fields remain visibly unavailable.
