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

## v0.3.0 skill-evidence-trend compatibility

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

Each `.tar.gz` contains the native executable, embedded WebAssembly UI, bundled SQLite, license, selected docs, inert plugin examples, full third-party license/notice bundles, and `manifest.json`. See [third-party evidence and reviewed declaration supplements](third-party.md). The package allowlist is in `scripts/release/package.py`; the checkout, source fixtures, node_modules, logs, databases, environment files, and user records are never packaged. The manifest identifies version, full source commit, platform, Rust target, tested runner, OS baseline, runtime requirements, and every packaged file's SHA-256. No Node/Bun/Rust runtime is needed by users. Build/test tools are not shipped.

The Linux executable uses static musl and is smoke-tested on Ubuntu 24.04; it does not require an installed glibc version. The build checks that it has no dynamic ELF interpreter. Older kernels, CentOS 7, and Alpine are not claimed supported without tests. macOS builds set `MACOSX_DEPLOYMENT_TARGET=11.0` for both architectures and verify the produced Mach-O minimum using `otool`. Rust officially supports ARM64 from 11.0 and x64 from an earlier baseline ([Rust target requirements](https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html)). This broadens the compiler/linker target without claiming runtime verification on macOS 11–14: CI runs on macOS 15 only. Windows builds use the static MSVC runtime and are tested on Server 2025; other Windows variants remain unverified. No ARM Linux, ARM Windows, 32-bit, older-OS compatibility, OS notarization, or code-signing claim is made. SHA-256 verifies integrity, not a publisher signature.

Every platform extracts its own archive to a new temporary directory outside the checkout and runs the native CLI including setup/health checks, SQLite create/reopen, synthetic demo HTTP and embedded UI, and MCP initialize/list/query smoke. The smoke creates only temporary synthetic data and an isolated home directory. No real account, installed Codex process, credentials, or user records are used.

## Prepare a release PR

1. Decide the coherent release scope. Confirm functionality, regression fixes, documentation, and safety boundaries are ready together
2. In one reviewed PR, align the native Cargo package version, frontend/tooling version metadata where present, and plugin manifest version. Add a dated `## [VERSION]` changelog section with actual features/fixes, limitations, and schema compatibility. Build output/MCP version must come from the native package version
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
