# Changelog

Releases group useful features and fixes. Merging a PR does not automatically create a version or release. See [release policy](docs/releases.md).

## [Unreleased]

Further changes will be grouped here until a meaningful release is ready.

## [0.8.0] - 2026-10-05

Trace-session-insight release preparation. The local production gate passed;
release verification still requires fresh exact-commit CI and extracted-artifact
evidence. This entry does not publish a release or claim live desktop testing.

### Added

- Source-local `byThread` summaries with local thread IDs, earliest/latest matching recorded starts, all four status counts, exact token totals/coverage and timestamp-anomaly counts
- Joint `byRequestedSettings` groups preserving all request model/effort/tier state/value cells, and `byDay` rows derived only from normalized recorded start times in UTC; missing days remain absent/unknown
- Independent 500-group limits with lexicographically smallest keys, complete retained groups and explicit thread/settings/day truncation flags; global totals still cover the full matching retained population
- `trace-attempts --order oldest_first|newest_first` and matching HTTP list order, default newest-first, with deterministic recorded-start/attempt-ID ordering and echoed order
- Boolean `timestampAnomaly` on attempt list/detail and decimal-string `timestampAnomalyCount` on every existing/new trace total for known completion before start
- English/Chinese thread, joint requested-setting and daily views; thread navigation preserves submitted filters and selects oldest-first

### Compatibility and safety

- **Restart v0.7 and older trace cursors from the first page.** New cursors bind order as well as source, exact filters and paired dates
- No new database schema, migration or write path. Schema-4 trace storage, read-only schema-2/3/4 compatibility, immutable attempt/response ownership, deduplication and import replay/deletion rules remain unchanged
- Thread summaries cover only submitted filters/date bounds; no cross-source joins or account-wide uniqueness claims. Imported histories may overlap
- Timeline sorting is recorded wall-clock order with an attempt-ID tie-breaker, not retained source-event or causal order. Timestamp bounds/anomalies never establish active time, latency, speed, quota or cost
- `trace-summary` intentionally includes local thread IDs and remains outside the unchanged eight-command Skill/eight-tool MCP allowlists. Content stays local and opt-in; raw files remain untouched
- No capture activation, discovery, watcher, client configuration, live collection, credentials or model request. Exact-head verification and live desktop compatibility remain separate; see [verification requirements](docs/verification.md) and [trace contract](docs/trace-import.md)

## [0.7.0] - 2026-10-04

Trace-workflow refinement release preparation. This entry does not publish a
release; exact-commit gates and extracted-artifact verification remain required.

### Added

- Read-only `import-trace-bundle --dry-run` against an explicitly selected existing store, sharing bounded confinement, parsing, projection validation and immutable replay checks with the real import
- Safe preview counts for attempts, replay and content retention, current/schema-4 migration consequences, warnings and next steps without creating or changing the store or bundle; later changes can invalidate the prediction
- Exact thread, status, requested-model, requested-effort and requested-tier filters shared by local trace CLI, HTTP lists/summaries and the bilingual Traces view, with source-and-filter-bound cursors
- Actionable `doctor.compatibility` schema-2/3/4 support, conditional trace upgrade consequences, safe failure guidance and explicit closed-store backup/rollback steps; unknown state stays unknown and backups are never claimed to have been made or verified
- Complete installed-document link coverage through explicit allowlisting of the linked sample-validation, build/coverage and design guides, with all-platform archive link-closure tests

### Compatibility and safety

- No new database schema: successful explicit trace writes still upgrade schema 2/3 to 4; queries, previews and diagnostics never migrate. **Stop all writers and verify a private closed-store backup before the write.** v0.5.0 and older cannot read schema 4; never edit schema numbers or open upgraded data with an incompatible older binary
- Existing trace cursors must be restarted after this upgrade; new cursors bind the source, paired UTC dates and every submitted exact filter. Requested metadata filters match reported request values only, without inferring response settings, defaults or Fast/Standard
- No discovery, watcher, client configuration, model request or live desktop verification. Tests and packaged smoke use synthetic data only; raw source files remain untouched and supported visible content stays local and opt-in
- Skill + local CLI remains the default with exactly eight allowed aggregate commands, and optional MCP remains exactly eight aggregate tools. Doctor, import previews and local trace queries stay outside these allowlists
- See [trace import and preflight](docs/trace-import.md), [backup guidance](docs/AI_INSTALL.md#upgrade-rollback-and-uninstall) and [verification scope](docs/verification.md)

## [0.6.0] - 2026-10-03

Selected-local-trace-bundle release preparation. This entry does not publish a
release; exact-commit gates and extracted-artifact verification remain required.

### Added

- Explicit bounded `import-trace-bundle` for one chosen directory and the pinned OpenAI Codex `rust-v0.160.0` RolloutTrace contract, reading only its manifest, trace and validated referenced payloads
- Local trace attempt/detail/summary queries and bilingual Traces reader with submitted UTC dates, a Monday–Sunday UTC-week preset and pagination, separating prepared request metadata from response completion evidence and preserving absent/null/invalid values
- Source-wide bounded persisted import warnings and bounded metadata groups with explicit truncation; overall totals remain independent of group truncation
- Opt-in redacted visible-text projection, atomic import/replay handling and schema-4 storage, without retaining complete raw request/response payloads
- A bundled [trace import guide](docs/trace-import.md) with source links, bounded selection, evidence caveats, privacy and backup/rollback guidance

### Compatibility and safety

- **Back up the closed store before its first successful explicit trace import.** That transaction upgrades schema 2/3 to 4; v0.5.0 and older cannot read schema 4. Read-only queries accept schema 2/3/4 without automatic migration
- No trace discovery, watcher, startup collection, configuration changes, installation, subprocess or model request. The Skill and MCP allowlists remain eight aggregate commands/tools; local trace queries are outside them
- Prepared requests are recorded before transmission, with a WebSocket-warmup logical-input exception; completion summaries are not every streaming frame. No send/delivery, actual physical-request count, billing, exact quota or purchased-credit claim is made
- Request model/effort/service tier remain separate from observed response metadata. Missing values do not infer defaults, Fast/Standard, token multipliers or per-skill costs
- Content stays opt-in and excludes system/developer instructions, hidden reasoning and recognized secrets. Raw source files are untouched. Retention, content deletion and all-data deletion preserve trace replay protection and cannot be undone by reimport
- `CODEX_ROLLOUT_TRACE_ROOT` is an upstream runtime contract, not verified desktop activation. A latest Mac app or Local mode does not prove its runtime or environment compatibility. Live desktop validation remains pending separate user authorization; tests use only synthetic data

## [0.5.0] - 2026-10-03

Message-reading and quota-context release preparation. This entry does not publish
a release; exact-commit gates and extracted-artifact verification remain required.

### Added

- Bilingual local reading sections for retained message text, tool arguments/results and file text, with expandable JSON/metadata and explicit missing remote-send evidence
- Event/model aggregate drilldowns and retained-content substring search with honest 10,000-candidate bounds, metadata filters, query-bound pagination and stale-response protection
- An explicit UTC-date token period beside reported quota snapshots, using only known occurrence times, exact totals/model groups and separately disclosed source-wide undated exclusions
- Weekly labels only for directly reported 10,080-minute windows; effort and Fast/Standard stay unrecorded where the response schema does not supply them
- Synthetic native, HTTP, reducer, SSR and desktop/mobile browser coverage for reading, search, filter/pagination races, missing evidence and token-period boundaries

### Compatibility and safety

- No schema migration. Existing schema 2/3 compatibility and incremental replay rules remain unchanged; v0.4.0 can still read these stores
- Content capture remains off by default and never backfills missing/deleted content; system/developer instructions, hidden reasoning and excluded skill payloads remain excluded
- Skill/MCP aggregate allowlists are unchanged. Content search and detail remain local-only; no transport interception, credential collection, actual-send claim or fabricated response/event linkage is added
- Selected token dates are not an inferred quota cycle. No reset-minus-seven-days start, fixed token allowance, effort/speed multiplier, per-message quota cost or credit/API-price conversion is inferred
- See [message reading and quota context](docs/message-reading.md) for exact scopes and limitations

## [0.4.0] - 2026-10-03

Incremental-record-import release preparation. This entry does not publish a
release; exact-commit gates and extracted-artifact verification remain required.

### Added

- Explicit one-shot `import-rollout-incremental` with a caller-selected absolute file, source, logical stream ID and pinned source version on every invocation
- Bounded reread/reparse of the complete prefix with newline-boundary checkpoints; every unterminated tail is deferred, including complete JSON and partial UTF-8
- Atomic evidence, source-wide hashed replay identities and progress commits with compare-and-swap checks; changed committed prefixes, truncation and immutable identity conflicts reject the entire incremental import
- Replay protection across retention and content deletion, without retaining pending record bodies or restoring old content when capture is later enabled
- Synthetic incremental append, no-op, truncation and read-only schema compatibility checks in packaged-artifact smoke

### Compatibility and safety

- **Back up a closed existing store before its first incremental import.** A successful explicit incremental import transaction upgrades schema 2 to schema 3. Failed parsing/validation does not migrate; ordinary writable opens and new stores remain schema 2 until that upgrade
- v0.4.0 read-only queries accept schema 2 and 3 without migration. **v0.3.0 and older cannot read schema 3.** Binary-only rollback is unsafe; use a separate compatible copy of the verified pre-upgrade backup
- Existing snapshot `import-rollout` behavior and conversational eight-command/tool allowlists remain unchanged. Use a separate source when switching modes if anonymous records may overlap; no cross-mode anonymous-identity deduplication is claimed
- Existing 8 MiB source, 256 KiB line, 20,000-line and 1,000-event/response bounds remain. This is not O(delta) tailing, a watcher, scheduler, directory/glob collector or remembered-path reopen mechanism
- Logical streams accept byte-identical copied prefixes without claiming physical-file identity. Rotation/truncation requires an explicit new stream or source; no automatic reset occurs
- Retention/content deletion preserve replay metadata; explicit delete-all or source-scoped delete-all resets it. Already-deleted pre-upgrade history cannot be reconstructed
- Imported skill reads/injections are evidence, not invented invocations, successes or per-skill token attribution. Real client, account, hook, macOS and background-daemon operation are not established by synthetic tests
- See [record import](docs/record-import.md), [upgrade guidance](docs/AI_INSTALL.md#upgrade-rollback-and-uninstall) and [verification notes](docs/verification.md)

## [0.3.0] - 2026-10-03

Skill-evidence-trends release preparation. A version entry does not publish a
release; exact-commit checks and extracted-artifact verification are still required.

### Added

- Paired inclusive UTC date filters (at most 366 days) and optional exact skill-name filtering on aggregate-only `skill-summary` and optional MCP `usage_skills`; a skill-only summary is also supported
- Local aggregate `GET /api/skill-summary`, sharing the same validated projection
- Filtered requested/loaded/invoked totals, daily occurrence-time evidence, loaded main-read/instruction-injection/unknown subtypes, and at most 500 aggregate skill groups with explicit truncation
- Explicit all-retained source/skill-matching unknown-occurrence counts and source-level partial-coverage/import-warning scopes; missing days remain unknown
- English/Chinese Skills daily-evidence panel sharing the dashboard's date filters

### Compatibility and verification

- Unfiltered `skill-summary` / `usage_skills` response shape and the eight-command/tool conversational allowlists are unchanged; optional freshness validation remains supported
- No database migration: schema-2 rollback-journal stores from v0.1.0, v0.1.1 and v0.2.0 remain compatible. Binary rollback remains schema-compatible; older versions do not support the new filters or HTTP route
- Stable adapter identities are unchanged. This feature adds no collection, import, hook, retention or background-daemon behavior. Incremental collection is deferred until stable identities, checkpoints, retention interaction and a schema-3 design are defined
- Only `occurredAt` assigns UTC dates. Unknown-time evidence is not assigned to a date range; counts do not imply task success, unique executions or per-skill token attribution
- See [verification notes](docs/verification.md) for current checks and pending gates. Historical test results apply only to their named checkpoints; synthetic checks do not establish live-account or real-client integration

## [0.2.0] - 2026-10-03

Collection-health release preparation. Downloadable artifacts exist only after the exact-commit checks and release workflow have passed and published them; a version entry is not evidence of publication.

### Added

- Read-only `doctor` setup diagnostics for the running binary, embedded dashboard and an optional explicitly selected existing database/source, with JSON output and a failing exit status for failed checks
- Source-scoped `health` aggregate query, optional eighth MCP tool `usage_health`, and a dashboard collection-health card
- Separate observed/missing/unsupported method states, fresh/stale/future local collection times, source-reported `sourceAsOf`, and retained historical collection failures
- Local stored event, skill-evidence, response and import counts with capture/occurrence bounds and unknown-time counts; preserve partial historical coverage rather than inferring zero use or complete history

### Changed

- Extend the self-contained Skill's aggregate allowlist from seven to eight commands with `health`; keep `doctor` outside the conversational workflow
- Add setup/health interpretation, privacy boundaries and synthetic release smoke coverage to installation and release documentation

### Compatibility and verification

- No database migration or schema change: existing schema-2 rollback-journal stores from v0.1.0/v0.1.1 remain compatible. These additions do not change stored data, so binary rollback to those versions remains schema-compatible; their interfaces do not include the new commands
- `doctor` and `health` do not discover configuration, launch subprocesses, use the network, read credentials, install clients or collect records. No incremental/background daemon is installed or claimed
- Local checks for this release, exact-head browser/cross-platform CI and extracted release artifacts must be verified separately; see [verification notes](docs/verification.md). Synthetic tests do not establish real-client installation or live-account compatibility

## [0.1.1] - 2026-10-02

### Changed

- Make the self-contained usage-summary Skill and native local CLI the default conversational query path; keep MCP as an explicitly optional alternative
- Document approved standalone Skill setup, exact local executable/database/source configuration, and the distinction between local data processing and aggregate results shared into an AI conversation

### Fixed

- Add an aggregate-only `skill-summary` CLI query shared with MCP; keep individual `skills` evidence local and outside the conversational Skill allowlist
- Prevent persisted read-only query commands from implicitly initializing or migrating the local database
- Keep product-version reporting aligned with the native package version while preserving unchanged adapter contract identities

## [0.1.0] - 2026-10-02

Initial local-first release. This entry describes the intended first release; downloadable artifacts exist only after the tag workflow has passed and published them.

### Added

- Native Rust CLI, loopback HTTP service, bundled SQLite store, and embedded Leptos/WebAssembly dashboard
- Explicit source namespaces and coverage/freshness metadata for service-reported account metrics, quota windows, local events, and imported response-token evidence
- Bounded, version-pinned explicit record imports; supported opt-in future hooks; direct skill evidence with requested/read/invoked distinctions
- Seven read-only stdio MCP tools and an inert plugin package for aggregate queries
- Local-only content views, opt-in content capture, best-effort redaction, scoped deletion, and explicitly applied retention
- Synthetic demo, automated tests, production coverage gates, and real-platform extracted-artifact smoke tests
- Four native release archives, SHA-256 checksums, source/platform/file manifests, bundled third-party license/notice inventories, and AI-assisted installation/upgrade/rollback instructions

### Safety and correctness

- Preserve missing/partial/unknown observations instead of inventing zero usage or complete history
- Keep account totals, imported response usage, and quota windows separate; do not infer remaining token counts from quota percentages or charge a turn's full tokens to a skill
- Bound parsing and transport input; never execute imported commands or follow recorded file paths
- Require explicit startup-risk acknowledgement for the account collector; demo, tests, and queries never initiate account collection
- Restrict HTTP to loopback with Host/Origin checks; keep raw content, SQL, arbitrary paths, refresh, and deletion out of the MCP allowlist

### Compatibility and limitations

- First native database release: create a new store or a verified compatible copy. Migration from experimental development stores and downgrade after a future schema change are not guaranteed
- Account methods and source formats depend on the installed upstream version and available permissions. Missing records and unsupported fields remain visible gaps
- Generic ChatGPT exports are not automatically supported record imports. Source labels are user-assigned namespaces, not authenticated account identities
- Live account collection, real ChatGPT client integration, background hooks, and remote transports are not established by synthetic tests; they need separately authorized verification
- Linux x64/static musl, macOS ARM64/x64 with deployment target 11.0 (CI tested on 15 only), and Windows x64 are the release targets. The workflow must verify all four before publishing. Windows desktop compatibility, other OS/architectures, code signing, and notarization are not claimed
- Checksums provide integrity, not code-signing identity. No automatic update, telemetry, credential setup, remote tunnel, or background collector is installed
