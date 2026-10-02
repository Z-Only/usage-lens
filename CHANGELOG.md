# Changelog

Releases group useful features and fixes. Merging a PR does not automatically create a version or release. See [release policy](docs/releases.md).

## [Unreleased]

Further changes will be grouped here until a meaningful release is ready.

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
