# Changelog

Releases group useful features and fixes. Merging a PR does not automatically create a version or release. See [release policy](docs/releases.md).

## [Unreleased]

Further changes will be grouped here until a meaningful release is ready.

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
