# Release policy and maintainer runbook

## A release is a useful batch, not a pull request counter

Usage Lens follows [Semantic Versioning](https://semver.org/). The initial public release is `0.1.0`, grouping the local evidence core, native CLI/server, embedded dashboard, aggregate-query plugin, privacy safeguards, and validated packaging. A merged PR does **not** bump a version, create a tag, or publish anything.

- Accumulate meaningful features and fixes under `Unreleased`
- Cut a **patch** release for a coherent batch of backward-compatible bug/security fixes; an urgent fix may justify its own patch
- Cut a **minor** release for useful new functionality. Before 1.0, breaking CLI, API, import, plugin, or database changes also require a minor version and explicit migration/rollback notes
- After 1.0, incompatible public contracts require a **major** version. Define the stability boundary before declaring 1.0
- Do not claim data-schema compatibility from a version number alone. Every release must say whether an existing store can be opened and whether downgrade is supported
- Do not republish a version, move a published tag, or replace published binaries. Corrections require a new version

The publishing pipeline currently accepts stable `vMAJOR.MINOR.PATCH` tags only, with no leading zeros. Prerelease/build-metadata tags require a reviewed workflow change first. All source, workflow, version, and changelog changes go through a PR and the required tests.

## v0.8.0 trace session-insight compatibility

This batch prepares source-local thread summaries, joint requested-setting
breakdowns, recorded-start UTC daily rows and oldest/newest timeline ordering.
The local production gate has passed as recorded in [verification](verification.md);
release verification still requires fresh exact-head remote CI and extracted
artifacts. This documentation and the version bump are not publication.
The complete field/scope/limit contract is in the already packaged
[trace guide](trace-import.md#summary-rows-limits-and-timestamp-evidence).

`byThread`, `byRequestedSettings` and `byDay` each retain at most 500
lexicographically smallest groups, with independent `threadsTruncated`,
`requestedSettingsTruncated` and `daysTruncated` flags. Retained groups are complete
for the submitted source, exact filters and dates; global totals cover every
matching retained attempt. Thread bounds are recorded starts, not active time or
latency. Joint grouping preserves full request state/value cells. Missing dates
are unknown and absent; no import-time replacement or zero filling is introduced.
All trace totals add decimal-string `timestampAnomalyCount`, and attempt
list/detail rows add boolean `timestampAnomaly` for known completion before start.
No causal, speed, quota, cost or account-wide uniqueness inference is added.

CLI `trace-attempts --order oldest_first|newest_first` and HTTP list `order` default
to `newest_first`. They deterministically sort recorded `startedAt` plus `attemptId`
in the chosen direction; original event sequence is not retained. The
English/Chinese **View thread** flow preserves submitted filters and starts
oldest-first. List responses echo order and cursors bind it along with source and
all filters. **v0.7 and older trace cursors must restart at the first page.** No
new order parameter applies to summaries.

There is no new schema, migration or write path. Schema-4 trace storage,
read-only schema-2/3/4 support, immutable attempt/response ownership and deduplication,
import replay/deletion rules, capture-off default and opt-in redacted content
boundaries are unchanged. Existing explicit trace imports still require the
closed-store backup precautions below. Thread summaries now intentionally include
local thread IDs, so all trace queries remain outside the unchanged eight-command
Skill and eight-tool MCP allowlists. Sources are never joined; imported histories
can overlap.

Before publication, require exact-commit production coverage/static/native tests,
browser screenshots and interactions in English/Chinese at desktop/mobile widths,
native cross-platform CI and all extracted-artifact smoke. Verify order/cursor
scope and restart, tied timestamps, anomalies, normalized UTC days, complete
retained groups under independent truncation, exact large totals, missingness,
read-only compatibility and unchanged MCP discovery. No Mac activation, live
collection, credentials or real-client validation is included.

## v0.7.0 trace workflow compatibility

This batch adds a read-only trace import preflight, exact trace list/summary
filters and actionable doctor backup/schema guidance. It introduces no new schema
and does not alter the eight-command Skill or optional eight-tool MCP allowlists.
The [trace guide](trace-import.md) ships in the existing release file allowlist;
its instructions describe both preview and separately authorized write workflows.

`import-trace-bundle --dry-run` requires an existing selected database and the
same explicit directory/source/version as import. It shares confined bounded
parsing, normalized content validation and immutable replay checks, without
creating, migrating or retaining anything. A successful preview is only a
point-in-time prediction; source/store/capture-setting changes can invalidate it.
Actual import reparses and atomically rechecks conflicts. Doctor's compatibility
object is schema guidance, not a backup check or live desktop validation.

**Stop all writers and verify a private closed-store backup before a write.**
A successful trace import still upgrades schema 2/3 to 4. v0.5.0 and older cannot
read schema 4; v0.3.0 and older cannot read schema 3. Never edit schema numbers or
open upgraded data with an incompatible older binary. For rollback, preserve the
current store and use a separate compatible copy of the pre-upgrade backup.
Read-only v0.7.0 queries/doctor/preflight support rollback-journal schemas 2/3/4.

Exact trace filters combine with paired dates and apply consistently to rows,
summary totals and metadata groups. Request filters do not infer response values
or default settings. Cursors bind source and all submitted filters; old v0.6
trace cursors or cursors from another scope require a fresh first-page query.
Source-wide import warnings retain their independent scope. Raw content remains
local; no ordinary conversational query can read it.

Before publication, run the exact-commit production gate, browser and native
cross-platform CI, and extracted-artifact smoke. The smoke must verify preflight
no-write invariants, replay/conflicts, schema guidance, filtered lists/summaries,
cursor scope and the unchanged MCP discovery boundary with synthetic data. A
working-tree test result is not proof that a tagged artifact or real desktop works.

## v0.6.0 selected trace-bundle compatibility

This minor release adds a bounded explicit `import-trace-bundle` write for one
chosen stable RolloutTrace directory at the pinned public Codex `rust-v0.160.0`
contract. It reads only the manifest, trace and validated referenced payloads.
There is no discovery, watcher, automatic trace activation, client configuration,
installation, subprocess or model request. Local attempt/detail/summary queries
remain outside the eight-command/tool conversational allowlists.

**Back up a closed store before its first successful explicit trace import.**
That atomic transaction upgrades schema 2/3 to schema 4; v0.5.0 and older cannot
read schema 4. Failed parsing/validation does not migrate. v0.6.0 read-only queries
support rollback-journal schemas 2/3/4 without migration. Ordinary startup does
not upgrade a store. For rollback, retain an old executable and a verified
compatible pre-upgrade backup, restored only to a separate path.

Bundle and attempt evidence is immutable; changed reimports conflict rather than
append to accepted history. Identical imports are no-ops. Content deletion,
retention and all-data deletion preserve trace replay protection; none can be
undone by reimporting the same bundle. Optional retained content is a conservative
redacted visible-text projection, excluding system/developer instructions, hidden
reasoning and recognized credentials. Raw source files are untouched.

Prepared requests precede transmission; a WebSocket warmup can record logical full
input rather than wire bytes. Completion payloads summarize output and optional
usage, not all frames. Request model/effort/tier do not establish observed response
settings, Fast/Standard, delivery, billing or exact weekly quota/credit costs.
See [trace import](trace-import.md) for source evidence and complete limitations.

The upstream `CODEX_ROLLOUT_TRACE_ROOT` contract does not establish macOS desktop
runtime compatibility or environment inheritance, even with a latest app or Local
mode. Live validation remains pending separate user authorization. Only synthetic
fixtures may enter tests, CI and release archives. A version/changelog entry is
preparation, not proof of publication or validation: require exact-head production
gates, browser/cross-platform CI and extracted-artifact smoke on every target.

## v0.4.0 incremental-record-import compatibility

This minor release adds a separately authorized one-shot incremental file import.
It requires an absolute database/file path, source, logical stream ID and pinned
source version on every call. It boundedly rereads/reparses the complete prefix,
defers every unterminated tail, and atomically commits records, source-wide hashed
replay identities and a compare-and-swap checkpoint. Truncation, changed committed
prefixes and immutable identity conflicts reject the whole import. It installs no
watcher, scheduler, discovery/glob collector or remembered-path reopen mechanism.
Snapshot `import-rollout` and the eight-command/tool conversational boundary stay
unchanged. Evidence does not establish skill invocation, success or token cost.

**Stop writers and make a verified private backup before the first successful
incremental import.** That explicit transaction upgrades schema 2 to schema 3;
failed parsing/validation does not migrate. Ordinary writable opens/new stores
remain schema 2. v0.4.0 queries/doctor support both schema 2 and 3 read-only without
migration. **v0.3.0 and older cannot open schema 3.** Keep the old executable and
verified pre-upgrade backup; rollback requires a separate compatible backup copy,
not merely changing binaries. Do not downgrade or overwrite the current store.

Retention and content-only deletion preserve replay metadata; explicit delete-all
or source-scoped delete-all resets it. Already-deleted pre-upgrade history cannot
be reconstructed. When switching snapshot/incremental modes, use separate sources
if anonymous records may overlap. A copied byte-identical prefix can continue a
logical stream but does not prove physical-file identity. Rotation requires an
explicit new stream/source, and the existing bounded-file limits remain. See the
[full import contract](record-import.md#incremental-one-shot-workflow).

The incremental adapter has a separate versioned identity; unchanged adapter
contracts retain their own version strings. Align owned package/plugin/lockfile
versions only; never blanket-replace dependency versions or historical release
notes. `scripts/check_static.py` checks the explicit owned-version inventory.

Version preparation is not publication or a test result. Require the complete
exact-head production gate, browser/cross-platform CI and extracted-artifact smoke
on every supported target. The synthetic smoke covers schema-2 reads before
upgrade, incremental append/no-op/truncation, and schema-3 read-only queries. It
does not prove real-client/account integration or background collection.

## Historical v0.3.0 skill-evidence-trend compatibility

This minor release adds bounded occurrence-time skill trends and exact skill-name
filtering to existing aggregate queries, plus the matching loopback route and
Skills panel. It keeps the unfiltered response shape and eight query commands/MCP
tools unchanged. The date range is paired, inclusive UTC and at most 366 days;
only known `occurredAt` values contribute to dated totals and daily rows. See the
[complete contract](data-contract.md#skill-evidence-summaries-and-utc-trends) for
undated evidence, partial coverage, source-level warnings and group truncation.

Database schema remains 2. Normal v0.1.0, v0.1.1 and v0.2.0 rollback-journal stores
remain compatible without migration. Binary rollback is schema-compatible because
this feature changes no persisted data; older interfaces do not support its new
filters or HTTP route. Keep the usual closed-store backup and side-by-side install.
Stable adapter versions are independent of the product version and remain unchanged.

There are no collector, importer, hook, retention or background-process changes.
Incremental collection remains a separate future design requiring stable identities,
checkpoints, retention interaction and a reviewed schema-3 plan. Evidence counts
must not be advertised as success rates, unique executions or skill-token costs.

Release metadata prepares v0.3.0; it does not establish publication or verification.
Require a new complete gate, exact-head browser/cross-platform CI and extracted
artifact smoke for all supported platforms. Preserve old release artifacts and tags.
Historical verification notes remain tied to their recorded commits.

## v0.2.0 collection-health compatibility

This minor release adds collection-health aggregates and local setup diagnostics;
it does not migrate the database. Existing schema-2 rollback-journal stores from
v0.1.0 and v0.1.1 remain compatible. Binary rollback remains schema-compatible for
these additions, although older binaries do not expose `doctor`, `health` or
`usage_health`. Keep the normal closed-store backup and side-by-side installation
practice; do not treat schema compatibility as permission to overwrite a store.

`doctor` is a local setup command. `health` is the eighth allowed conversational
CLI aggregate and `usage_health` the eighth optional MCP tool. Neither command
installs an incremental/background daemon, initiates collection, scans client
configuration, accesses credentials, or reaches the network. The health view
reports partial local evidence; no full-history or actual-client coverage is added.

Version metadata and a changelog entry prepare a release; publication still requires
the exact reviewed commit and all checks below. Consult [verification notes](verification.md)
for tested and pending checks, without carrying older results forward to a new head.

## Native artifacts

| Platform ID | Actual GitHub runner | Rust target |
| --- | --- | --- |
| `linux-x64` | `ubuntu-24.04` | `x86_64-unknown-linux-musl` |
| `macos-arm64` | `macos-15` | `aarch64-apple-darwin` |
| `macos-x64` | `macos-15-intel` | `x86_64-apple-darwin` |
| `windows-x64` | `windows-2025` | `x86_64-pc-windows-msvc` |

These labels/architectures are listed in [GitHub's official runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), checked October 2, 2026. Architecture is verified again in each package job; nothing is labeled as a cross-architecture build. A failed platform blocks the entire release; do not silently drop it.

Each `.tar.gz` contains the native executable, embedded WebAssembly UI, bundled SQLite, license, selected docs, inert plugin examples, full third-party license/notice bundles, and `manifest.json`. See [third-party evidence and reviewed declaration supplements](third-party.md). The package allowlist is in `scripts/release/common.py`, enforced by `scripts/release/package.py`; the checkout, source fixtures, node_modules, logs, databases, environment files, and user records are never packaged. The manifest identifies version, full source commit, platform, Rust target, tested runner, OS baseline, runtime requirements, and every packaged file's SHA-256. No Node/Bun/Rust runtime is needed by users. Build/test tools are not shipped.

The Linux executable uses static musl and is smoke-tested on Ubuntu 24.04; it does not require an installed glibc version. The build checks that it has no dynamic ELF interpreter. Older kernels, CentOS 7, and Alpine are not claimed supported without tests. macOS builds set `MACOSX_DEPLOYMENT_TARGET=11.0` for both architectures and verify the produced Mach-O minimum using `otool`. Rust officially supports ARM64 from 11.0 and x64 from an earlier baseline ([Rust target requirements](https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html)). This broadens the compiler/linker target without claiming runtime verification on macOS 11–14: CI runs on macOS 15 only. Windows builds use the static MSVC runtime and are tested on Server 2025; other Windows variants remain unverified. No ARM Linux, ARM Windows, 32-bit, older-OS compatibility, OS notarization, or code-signing claim is made. SHA-256 verifies integrity, not a publisher signature.

Every platform extracts its own archive to a new temporary directory outside the checkout and runs the native CLI including setup/health checks, SQLite create/reopen, read-only schema-2/3/4 checks, synthetic incremental append/no-op/truncation, selected trace import/replay/privacy checks, synthetic demo HTTP and embedded UI, and MCP initialize/list/query smoke. The smoke creates only temporary synthetic data and an isolated home directory. No real account, installed Codex process, credentials, or user records are used.

## Prepare a release PR

1. Decide the coherent release scope. Confirm functionality, regression fixes, documentation, and safety boundaries are ready together
2. In one reviewed PR, align both owned Cargo package versions and their Cargo.lock entries, frontend/tooling package version, and plugin manifest version. Keep dependency versions unchanged unless separately reviewed. Add a dated `## [VERSION]` changelog section with actual features/fixes, limitations, and schema compatibility. Build output/MCP version must come from the native package version
3. Require the production quality gates (at least 95% total and changed-line coverage) and `Release package checks / package-gate`. Packaging tests are separate from runtime coverage; they do not shrink or weaken its inventory
4. Merge the PR through the normal review/ruleset process. Wait for **both workflows' push-to-main runs on the exact merged commit** to succeed. PR success alone is insufficient
5. Check the source commit and release notes. Create the annotated stable tag only on that reviewed main-history commit, then push that tag. No tag is created automatically

Example, after the commit is merged and verified:

```sh
git fetch origin main --tags
git switch --detach FULL_VERIFIED_MAIN_COMMIT
git tag -a v0.1.0 -m 'Usage Lens 0.1.0'
git push origin refs/tags/v0.1.0
```

These commands require maintainer authorization; instructions in this document do not grant it. Restrict tag creation/update/deletion to trusted maintainers with a tag ruleset, protect `main`, and require PR checks. Workflow code cannot protect a repository from an administrator deliberately replacing the workflow or tags.

## Tag-triggered release pipeline

`.github/workflows/release.yml` canonically runs on pushes matching `v*`. It also provides a guarded `workflow_dispatch` for maintainers who cannot create a tag directly: select the **main** branch and enter the exact stable tag (for example `v0.1.0`). It rejects any other dispatch branch, pins the original main event commit, and requires the same version/changelog/CI checks. It never runs with `pull_request_target` and does not publish from PRs or arbitrary refs.

1. A read-only guard checks out trusted `main`, validates strict SemVer, resolves the original tag event (or original main dispatch) to a full commit, requires main ancestry, version/changelog agreement, advancing version, and successful exact-commit main push runs of both `ci.yml` and `release-check.yml`, including their aggregate gate jobs
2. Four read-only native matrix builds check out only that validated SHA, install pinned official build tools, build with the committed Cargo lockfile, and package/smoke the extracted artifacts. They upload uniquely named current-run artifacts
3. The publishing job runs only after every build succeeds. It downloads only this run/attempt's four named artifacts. The complete platform/file set, all archived file hashes, and embedded/sidecar identities are verified before creating release metadata and `SHA256SUMS`
4. Only the final job has `contents: write`. For a manual dispatch, it creates the missing lightweight version tag only after all builds and complete artifact verification have succeeded; a different existing tag commit is rejected. This full pipeline publishes directly and does not rely on a token-created tag triggering a second workflow. It creates a **draft**, uploads all archives/manifests plus `release.json`, `CHANGELOG.md`, and `SHA256SUMS`, and verifies GitHub's recorded SHA-256, size, and upload state for the complete asset set
5. Main ancestry, live tag identity, exact-commit CI, and version progression are checked again. Only then is the draft published. Incomplete uploads never become a public release

No protected credentials are supplied to build steps. The publisher uses only the narrowly scoped job `GITHUB_TOKEN`, not a personal access token. Checkout does not persist credentials. Actions are pinned by full commit. Downloads/build dependencies come from official toolchains and the committed package lock, not ad hoc binary mirrors.

If repository tag rules block the workflow token from creating the dispatch tag, do not weaken those rules or add a personal token automatically. A trusted maintainer should create the intended tag and use the canonical tag path. Both entry points share one per-tag concurrency group.

## Failure and recovery

- **Tag pushed before main CI finished:** no publication occurs. Wait for both exact-commit main checks, inspect the reason, then rerun the original release workflow; do not move the tag
- **Any build/package/smoke fails:** no release is created. Fix through a PR. If the tagged code itself changes, choose a new version/tag; never reassign an existing published identity
- **Upload or final validation fails:** an incomplete **draft** may remain. Inspect it in GitHub. The workflow refuses to append to/overwrite any existing draft or public release for the tag, preventing stale assets from different runs. With explicit maintainer authorization, remove only the failed draft (not the tag), then rerun the full workflow
- **Publication result is uncertain:** inspect the release state before retrying. Never assume a failed API response means nothing was published
- **Already public release is wrong:** explain the issue, mark/document it as appropriate through authorized actions, and ship a new corrected version. Do not silently mutate artifacts

For a rerun, rerun the **whole** workflow, not only failed jobs: asset names include the run attempt, so mixing attempts intentionally fails. Artifact retention is 7 days; an expired build requires a complete rerun. Published assets are never sourced from another workflow or run.

## User installation and data safety

Use [AI_INSTALL.md](AI_INSTALL.md) for exact asset selection, checksum verification before extraction/execution, manifest review, isolated demo checks, opt-in persistent setup, backup, schema-aware rollback, and scoped uninstall. Installation does not authorize authentication, collection, hook registration, a remote tunnel, or content retention.
