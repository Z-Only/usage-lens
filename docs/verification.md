# Verification scope

This document records what was exercised, not a claim of complete product or account compatibility.

## Current local evidence

The current implementation uses Rust 1.98.1, Axum, bundled SQLite, and Leptos 0.8.21. The genuine CSR WebAssembly assets are embedded in the native executable. Development uses synthetic records only; no real account, personal archive, authentication file, or installed user runtime has been inspected.

- Native workspace and WebAssembly frontend compile and pass their focused static checks
- Rust core, importer, CLI/HTTP/MCP, and frontend reducer/native-render tests have passed focused runs
- The official JavaScript MCP SDK has exercised the native stdio server
- Actual embedded asset serving, MIME types, loopback Host/Origin/CSRF restrictions, and local content boundaries have targeted tests
- Importer tests preserve per-response token semantics, stable identities, output ordering, and missing-versus-null distinctions

The serialized local Linux gate passed on 2026-10-02: 155 Rust tests, 52 gate/build tests, static checks, formatting, native and WASM Clippy, fresh Leptos CSR build, and native build. Total production-line coverage was 5803/6102 (95.1000%); changed-line coverage was 5468/5729 (95.4442%). This includes 216 unmeasured browser/event/bootstrap lines counted as uncovered, with no missing source records. Source/manifests/test-input hashes were unchanged during the run. Exact-commit GitHub browser, cross-platform, and release-package CI still require separate verification. Earlier TypeScript prototype coverage is not evidence for the Rust implementation.

Native-render tests do not execute WebAssembly browser event handlers. The coverage gate includes authored browser bridge/loader lines conservatively as uncovered, and requires at least 95% total and changed executable-line coverage without excluding application entrypoints.

## Corrections made during review

Regression tests cover these concrete corrections:

1. Removed retained content from active SQLite pages after explicit deletion, with secure deletion enabled; this does not promise erasure from backups or filesystem snapshots
2. Redacted recognized credentials embedded in serialized JSON strings as well as object fields; redaction remains best-effort
3. Scoped hook tool identities to session and turn, preventing equal tool IDs in different sessions from colliding
4. Recorded startup/initialization collection failures so retained observations do not appear to have refreshed successfully
5. Normalized an absent optional hook model to unknown rather than rejecting an otherwise valid event
6. Propagated imported coverage warnings to skill/event/aggregate query paths
7. Preserved main-read versus injected-instruction evidence instead of labeling both as distinct successful skill executions
8. Applied structured exclusions to serialized tool JSON and recognized secret-bearing header pairs, with quote-aware nesting limits that do not fall back to retaining over-deep structured content
9. Verified failed persisted writes return static errors and failed multi-event imports roll back earlier inserted rows before a later successful retry

## Browser and cross-platform boundaries

The managed cloud browser could not navigate to the development loopback URL. That browser's restriction was not changed. The project contains a separate synthetic Playwright workflow at desktop, 390px and 320px widths; its execution and screenshot evidence have not yet been recorded here.

CI definitions include Linux coverage/gates, macOS and Windows runtime checks, and browser tests. A configured job is not a passed job. Remote CI results must be checked for the exact submitted commit before release.

No real Android device, Safari, full screen reader audit or universal ChatGPT plugin integration has been verified.

## Source and account compatibility

The rollout importer targets the explicitly declared public source contract documented in [record-import.md](record-import.md). Synthetic files demonstrate supported shapes; they do not prove that every installed version or export contains those fields. Unsupported/missing data remains visible through warnings.

The account collector uses documented read methods, but the actual user's installed executable, account permissions, method availability and startup behavior have not been exercised. A live read must be initiated and reviewed locally by the user. The query plugin never starts that collector implicitly.

Tests and demos never execute a model turn. No statement here should be interpreted as complete account-wide usage, complete skill history, exact per-skill cost, or a billing reconciliation.

## Review-fix validation checkpoint

The follow-up local gate on 2026-10-02 passed 167 Rust tests and 58 Python build/gate/plugin tests, plus formatting, native/WASM Clippy and fresh builds. Coverage was 5853/6149 (95.1862%) total and 5518/5778 (95.5002%) changed against the initial PR base. The follow-up patch itself measured 90/90 (100%) changed executable lines against commit `9c63bb0d1c1ed5764578b754a4c4e84c9147378e`. The same 216 unmeasured browser bridge lines remained uncovered, with no missing source records.

Separately, 38 release-tool Python tests, official MCP SDK parity, E2E TypeScript checking and whitespace checks passed. Recorded test/build inputs were unchanged during the gate. Review fixes cover common cloud/registry credential names, native plugin installation examples, shared bounded rollout/store preflight, and explicit readiness synchronization for a fake subprocess fixture that raced on Intel Mac. The documented total import budget remains enforced; content is never silently truncated to make an oversized import fit.

Actual browser evidence on `9c63bb0d1c1ed5764578b754a4c4e84c9147378e` passed all six desktop/390px/320px tests; see [design verification](design.md). That commit passed all four native package jobs but failed the Intel-Mac timing fixture subsequently corrected above. The follow-up still requires exact-head remote CI before merge and release.

## v0.1.1 Skill-first validation checkpoint

The local pre-publication gate passed 176 Rust tests, 63 Python build/gate/plugin tests, 38 release-tool tests, official MCP SDK parity and E2E TypeScript checking, plus formatting, static checks, native/WASM Clippy and fresh builds. Total production coverage was 5928/6228 (95.1830%); changed executable lines against `50615490e3319bf68c5cb3e64efd79afaf7ded2d` were 81/83 (97.5904%). The same 216 browser-bridge lines remain counted uncovered and no runtime source records are missing. All recorded tested inputs stayed unchanged during the gate.

New synthetic regressions verify that ordinary queries and optional MCP startup do not create missing databases or migrate old schemas, retain database bytes/mtime and directory contents, reject pre-existing WAL stores, deny writes through the read-only handle, and preserve normal concurrent rollback-journal reads/writes. OS access timestamps and external concurrent path/journal-mode changes are outside that guarantee.

Independent source review caught and corrected a disclosure mismatch before publication: local `skills` returns individual evidence, so the conversational Skill now uses the new `skill-summary` aggregate command. CLI and MCP share the same projection; runtime tests compare the shape and exclude individual record/correlation IDs and content canaries. Local evidence remains available only through separate local workflows. Standalone Skill copy instructions were tested under a synthetic temporary home, not installed into a real client. Exact new-commit browser/cross-platform CI and release verification remain separate requirements.

## v0.2.0 collection-health validation checkpoint

The final local pre-publication gate on 2026-10-03 passed 201 Rust tests and 64
Python build/gate/plugin tests, plus formatting, static checks, native/WASM Clippy,
a fresh Leptos build and native build. Total production line coverage was
6350/6677 (95.1026%); changed executable lines against
`a4b7a7d583f176dfc0cd21e0f19ee55de7d1b5e6` were 416/419 (99.2840%). The same
216 unmeasured browser-bridge lines remain counted uncovered; no runtime source
records are missing. Separately, all 38 synthetic release-tool tests, the official
MCP SDK compatibility check and E2E TypeScript check passed. The updated release
smoke script passed against the freshly built debug executable, including
SQLite persistence, doctor/health read-only invariants, embedded UI/loopback HTTP
and all eight MCP tools; this is not an extracted release-archive or cross-platform
package result.

Local rendered QA remains blocked before page load: the official Playwright
installer could not extract Chromium v1243 (153.0.8010.12) downloaded from
`cdn.playwright.dev`, reporting “End of central directory record signature not
found.” No alternate host or existing browser profile was used. New synthetic
health tests cover 1440/390/320px, failed request/retry, delayed source switching,
large exact counts, English/light and Chinese/dark with screenshot attachments;
their actual execution and pixel review require the new commit's browser CI.

Exact-commit remote CI and packaged platform checks are still required before
publication. Earlier browser/package results above remain tied to their named
checkpoints. These synthetic checks did not install a Skill or plugin into a real
client, read a live account, exercise installed hooks, or test a daemon.

The new regression scope includes `doctor` with and without an explicit store,
failed diagnostic exit status, no-create/no-migrate read-only selected-store
checks, source-scoped health CLI/MCP/HTTP parity, missing/unsupported observations,
fresh/stale/future collection timestamps, retained older failures, unknown source
timestamps, local count/time bounds, empty/partial historical coverage, and
aggregate exclusion of content and individual record identities. Plugin examples
must allow exactly eight query commands and keep `doctor` in setup only. Extracted
artifact smoke must exercise diagnostics, health and the eighth MCP tool.

New dashboard health-card layout and interaction checks require the actual v0.2.0
browser run; old screenshots cannot establish the changed UI's rendered behavior.


### Health screenshot capture correction

The initial PR browser run at commit
`72432921517b7497e725834cf2e8821620c23ab2` passed its nine synthetic checks. Review of its mobile health-panel
screenshots found a capture artifact: cropping a panel taller than the viewport
could reposition the unfocused fixed skip link into the image, even though the
link was above the real viewport. The follow-up changes only test capture to
full-page screenshots, attaches successful offscreen skip-link geometry, and adds
an isolated keyboard Tab/Enter check for visible focus and navigation to main
content. Application CSS is unchanged. The follow-up's actual browser execution
and corrected screenshot review remain requirements of its exact-commit CI.


## Historical v0.3.0 skill-evidence-trends validation scope

The historical results above remain tied to their original checkpoints. The new
regression scope and local validation checkpoint are recorded below. Version
metadata and documentation alone do not establish rendered UI, packaged artifact
or publication status.

The regression scope is CLI/MCP/HTTP parity for paired valid dates, UTC boundary
normalization, exact skill filtering and skill-only summaries; one missing date,
invalid/reversed/over-366-day ranges and malformed freshness inputs must fail.
No date/skill filters must preserve the previous aggregate JSON shape and the
conversational allowlists must remain exactly eight commands/tools. A date-only
query includes all skill names; a skill-only query includes undated matching
evidence in totals and omits `daily`.

Fixtures must cover known and unknown `occurredAt`, capture-time/occurrence-time
mismatch, empty sources, missing days, overlapping loaded evidence, exact decimal
counts, more than 500 skill groups with totals unaffected by truncation, and
source-level import warnings with bounded/truncated inspection. Assertions must
verify the distinct dated/all-retained/unknown/import-warning scopes and exclude
content canaries and individual record/correlation IDs. Existing read-only,
no-create/no-migrate, rollback-journal, source isolation and safe-error guarantees
remain applicable.

New Skills-panel English/Chinese, light/dark, 1440/390/320px, shared-date filtering,
empty/partial/unknown evidence, stale source/date response handling and
failure/retry behavior require actual browser execution and screenshot review for
the final commit. Native renders alone do not establish that browser behavior.
Extracted release smoke must exercise the new filtered aggregates while retaining
all eight MCP tools and the existing setup/health checks.

No live account, personal transcript, real-client installation, background daemon
or incremental collection is verified by these synthetic requirements. Production
coverage gates, cross-platform runtime and final extracted release artifacts must
be checked separately before publication; no old release binary is updated in place.


### v0.3.0 local pre-publication checkpoint

The frozen-input local gate on 2026-10-03 passed 218 Rust tests, 64 Python
build/gate/plugin tests, formatting, static checks, native/WASM Clippy and fresh
Leptos/native builds. Production line coverage was 6635/6966 (95.2483%); changed
executable coverage against `18ba84120690e7ee47845e7de85960d2779c28d4` was
280/283 (98.9399%). The existing 216 browser-bridge lines remain conservatively
uncovered; no runtime source records are missing. Hashes of all tracked and new
non-ignored inputs stayed unchanged during the gate.

Separately, all 38 synthetic release-tool tests, official JavaScript MCP SDK
compatibility, E2E TypeScript checking and whitespace checks passed. The updated
release smoke passed against the freshly built debug executable, including
filtered CLI/HTTP/MCP skill aggregates, invalid ranges and read-only database
invariants. This was not an extracted release package or cross-platform check.
Review found and fixed one exact-name parity edge case: `--skill` now consumes
leading-`--` names literally, with a CLI/MCP/HTTP regression.

The nine new Playwright cases were authored and discovered across the configured
desktop/390/320px projects (21 cases total), but were not executed locally. The
known official Chromium-download extraction failure was not bypassed or retried
through another host. English/light and Chinese/dark screenshot paths assert zero
scroll, record offscreen unfocused skip-link geometry, and attach full-page images
when CI executes them. Actual browser results/pixel review, all four platform
runtime jobs, extracted-package verification and exact-commit remote gates remain
required before publication. No v0.3.0 release has been published by this check.

## v0.4.0 incremental-import validation scope

The new one-shot import requires an explicit bounded file, source, logical stream
and pinned format version on each call. Its acceptance suite must cover complete
prefix append, no-op replay, deferred valid-JSON and partial-UTF-8 tails,
truncation/prefix mutation, copied-prefix continuation, immutable cross-run
conflicts, stale concurrent checkpoint rejection and atomic failures. Existing
snapshot imports and aggregate queries remain regression requirements.

Store tests must establish schema-2 read-only compatibility without migration,
atomic schema-3 migration only on successful incremental import, schema-3
read-only compatibility, retention/content-delete replay protection, content
opt-in without backfill, explicit all/source deletion reset and the documented
pre-upgrade-history/cross-mode anonymous-identity limitations. Existing hard
source/line/record/projection limits remain enforced; no bound can be bypassed by
changing the logical stream name.

The packaged-artifact smoke now exercises a synthetic schema-2 store and readonly
queries, a valid unterminated JSON tail, newline append, unchanged-prefix no-op,
schema-3 read-only queries and truncation without database mutation. It retains
all existing local doctor/health, embedded HTTP/UI and eight-tool MCP checks in an
isolated temporary home. It does not read personal histories, install a client,
start Codex, authenticate, use real hooks or configure a daemon.

Local documentation/build-script checkpoint on 2026-10-03: owned product-version
static checks and all 67 Python scripts tests passed, including explicit Skill
exclusion and backup/schema guidance. `node --check scripts/release/smoke.mjs`
and all 38 synthetic release-tool tests passed. The updated release smoke passed
against the local v0.4.0 debug executable, including all new incremental/schema
checks and the existing local HTTP/UI/MCP checks. This was **not an extracted
release package**, browser execution or cross-platform verification. Final
exact-head production gates and release verification must be recorded
independently; historical v0.3.0 results do not validate this change.

Implementation checkpoint on 2026-10-03: the full Linux source gate passed with
253 native Rust/SSR tests, all 67 script tests, rustfmt, native and WebAssembly
clippy, embedded UI build and strict coverage inventory. Coverage was 7,207/7,560
(95.3307%) total and 574/585 (98.1197%) changed executable lines against
`e1a12e3fc5aa6750edb2dfda9a0bd1e393ef5abc`. The existing browser-only runtime
remains conservatively uncovered; no suppression or missing-file exemption was
added. The 35 new incremental tests include canonical source-event alias adoption,
retention/capture-toggle non-resurrection, checkpoint-write failure rollback,
concurrent compare-and-swap, read-only schema compatibility and replay node bounds.
Independent synthetic CLI checks reproduced and then verified the alias-retention
fix, including absence of the old-body canary in stored bytes.

E2E TypeScript checking, the official JavaScript MCP SDK test, all 38 release-tool
tests and the updated smoke against the current debug executable passed. Local
Playwright could not launch: its pinned Chromium binary was absent, and a temporary
configuration using installed Chromium was blocked by the environment's socket
restrictions before any page loaded. That temporary configuration was removed.
No browser pass is claimed. Exact-commit CI browser execution, all four native
platform jobs and extracted release-package smoke remain required before release.

## v0.5.0 message reading and token-period checkpoint (2026-10-03)

This checkpoint adds a local retained-content reader, event/model count drilldowns,
bounded metadata-filtered content search, and an explicit known-occurrence token
period beside independently reported quota windows. It does not establish actual
remote-send payloads or per-message token/quota attribution. See
[message reading](message-reading.md) for the contract and privacy limits.

The local source gate ran against base
`d24b0d961dd721225326f18febe5106e7a9911bb` (tree
`5ffc71e236c5134e6b7b6f472b59c6061fbc27d3`) with 287 Rust tests passing,
69 Python gate/static/build tests, native and WASM Clippy with `-D warnings`,
formatting, the Leptos production build, embedded native build and complete LCOV
inventory. Total production line coverage is **7765 / 8126 = 95.5575%**;
changed executable coverage is **555 / 560 = 99.1071%**. No production file was
missing. The same conservative 216 unmeasured bridge/bootstrap physical lines
remain uncovered; native/SSR evidence is not browser source coverage.

Additional local checks passed: E2E TypeScript checking, official-SDK MCP
compatibility, 38 synthetic release-tool tests, and the updated release smoke
script against the freshly built **debug executable**. That last check exercises
new HTTP search continuation and known-date period contracts as well as existing
schema/replay, CLI, embedded assets and aggregate-only MCP checks; it is not
extracted release-package or cross-platform release evidence.

The configured browser suite now has **27 tests** across desktop, 390px and 320px
viewports. Its new flows cover event drilldown, message/tool/file reading, absent
content, modal Escape/reopen/focus return, bounded-search notices, submitted-query
pagination, delayed response/reset races, invalid-date retry, quota-period
missingness and English/light plus Chinese/dark layout screenshots. Local
Chromium could not launch: its process-singleton socket failed with
`Operation not permitted`, including the approved escalation attempt. No page was
reached and no local browser visual/interaction pass is claimed. Browser plugin
runtime was unavailable; the attempted fallback used the installed system
Chromium through a temporary Playwright config outside the repository. It did
not change the standard committed CI configuration.

Before merge/release, the exact commit still requires successful browser CI with
inspection of the new screenshots, native Linux/macOS/Windows jobs, and extracted
release-artifact smoke. A version/changelog entry is preparation, not publication.
No real account, transcript, client hook, user computer or transport interception
was used in this checkpoint; test data was synthetic.


## v0.6.0 selected trace-bundle verification scope (2026-10-03)

This release targets only the source-pinned public RolloutTrace contract described
in [trace import](trace-import.md). The test plan uses synthetic manifests, trace
events and referenced payloads, never real Mac files, protected transcripts,
authentication files, accounts or installed client processes. Upstream source
review is not live desktop validation. `CODEX_ROLLOUT_TRACE_ROOT` support in the
public source does not prove the latest desktop app's bundled runtime or launcher
environment; separately user-authorized live compatibility remains pending.

Required checks cover bounded file/reference/depth handling; safe missing,
malformed, duplicate and conflicting evidence; prepared-before-transmission and
completion-summary labels; metadata missing/null/invalid distinctions; conservative
visible-text projection and redaction; schema-2/3 read-only compatibility; atomic
schema-4 migration on successful explicit import only; read-only schema-4 queries;
no-op replay, immutable conflict rollback, deletion and retention protection.
Trace counts are not physical network-request counts, and recorded token totals
are not account-wide quota, purchased credits or billing verification.

Release archive tests must include the new guide and resolve its bundled references
on every platform, reject missing allowlisted docs, and exclude synthetic payloads
or private trace files from archives. The extracted executable smoke must exercise
synthetic trace import, local queries, replay/conflict behavior, capture/privacy,
and unchanged eight-tool aggregate-only MCP discovery in an isolated temporary
home. The smoke does not execute a model or start Codex.

The checks listed here are requirements, not a claim that the current uncommitted
head has passed them. Record focused results separately and require the complete
exact-commit production gate, browser/cross-platform CI and all extracted release
artifacts before publication. Historical v0.5.0 or earlier results above apply only
to their recorded checkpoints and must not be carried forward as v0.6.0 evidence.


Focused local documentation/package checks on the v0.6.0 working tree passed at
2026-10-03 14:19 UTC: 70 Python gate/static/build/plugin tests, 42 synthetic
release-tool tests, the owned-version static guard and JavaScript smoke syntax.
A fresh `cargo build --locked -p usage-lens` succeeded and the full updated smoke
passed against its **debug executable**, including schema-2/3 trace reads,
schema-4 import/read-only checks, prepared/observed metadata states, trace token
separation, pagination, replay/conflict rollback, capture-off/no-backfill,
redaction/excluded-content database canaries, content/all-data deletion with no
resurrection, raw-file preservation and unchanged eight-tool MCP discovery. This is focused
local evidence only: it is not an extracted release-artifact result, a full
production coverage gate, cross-platform execution or real desktop compatibility.
Later source changes require the applicable checks to run again.


The subsequent independent-review changes require fresh production verification:
strict response usage-parent and failed/cancelled partial-response validation,
backward-clock warning preservation, assistant phase/typed-content exclusions,
Windows local-root confinement, persisted source-wide bounded import warnings,
and explicit metadata-group truncation. Documentation describes those final
contracts; the earlier debug-smoke checkpoint alone does not verify later edits.
The Traces UI browser scenarios were authored and typechecked, but local browser
execution remains blocked by the recorded socket restriction. No new browser pass
or cross-platform runtime pass is claimed from documentation checks.


## v0.7.0 trace workflow verification scope (2026-10-04)

This batch targets read-only preflight, exact local trace filters and doctor
schema/backup guidance. All inputs remain synthetic; no user computer, private
history, credentials, live account, trace activation or model request is involved.
The [trace guide](trace-import.md) documents the exact contracts and caveats.

Required tests cover unchanged database bytes, mtime and directory inventory for
preflight on schema 2/3/4; missing/unsupported/unreadable stores; shared bounded
parser/path/projection rejection; pause/source errors; fresh/replay/conflict
counts; capture settings, deletion tombstones and no-backfill behavior; and
revalidation by the later write. A successful preview does not reserve evidence
or validate a backup. Doctor tests must preserve safe errors, unknown schemas,
read-only bytes/mtime/no-sidecar behavior and explicit unverified backup status.

Trace filters need exact case-sensitive thread/status/requested metadata matching,
valid boundaries, absent/null/invalid state distinctions, AND combinations, paired
dates, empty sets, summary/list agreement and source-plus-all-filter cursor binding.
UI coverage must include submitted/unsubmitted inputs, pagination, source changes,
filter resets, request races, detail dismissal, error/loading states and bilingual
mobile/desktop rendering. Old evidence must not look current after a scope change.

Release-package tests must show the updated existing allowlisted guides are shipped
with resolving relative links on every platform, without any selected bundle or
private data. Extracted executable smoke must check the preview, filtered local
queries and schema guidance while preserving exactly eight aggregate MCP tools.
No new conversational command or tool is allowed.

These are verification requirements, not claims that the current tree or future
release artifacts passed. Results from earlier releases are historical only. The
complete production coverage gate, exact-commit browser/native cross-platform CI
and all extracted release artifacts still need their own recorded evidence before
publication. Live desktop/runtime compatibility remains separately authorized and
unverified by synthetic tests.


Focused v0.7.0 working-tree checks passed by 2026-10-04 13:30 UTC: all 6 doctor
integration tests; 72 Python script/gate/plugin tests; 43 synthetic release-tool
fixture tests; the static owned-version/safety guard; JavaScript smoke syntax; and
the updated native smoke against the local **debug executable**. Smoke exercised
schema-2/3/4 preflight/read-only behavior, no-create/no-sidecar invariants,
replay/conflict and content prediction, exact filters and cursor scope, doctor
backup guidance, existing embedded HTTP/UI checks and eight-tool MCP discovery.
The release-tool tests package synthetic fixtures across platform labels and
verify every relative Markdown file link in each archive; they do not execute
those platforms. The explicit allowlist now also includes the linked sample,
build/coverage and design guides, without including source or QA assets. This checkpoint is not an extracted release artifact,
new rendered-UI/browser pass, production coverage gate, cross-platform runtime
pass, publication or live desktop validation. Rerun applicable checks after later
edits and record the final exact-commit gate independently.

### v0.7.0 final local production checkpoint (2026-10-04)

The local aggregate source gate passed against published base
`df70aa96a5cda35244365bd978582095887b65b1`: 363 native Rust/SSR tests,
72 Python gate/build/plugin tests, formatting, static/version checks, native and
WebAssembly Clippy with warnings denied, a fresh Leptos bundle and native build.
Total production coverage was **9331/9742 = 95.7812%**; changed executable coverage
was **543/559 = 97.1377%**. Every runtime source is accounted for; the 238 authored
browser bridge/bootstrap/event-handler lines remain conservatively unmeasured
and uncovered. No coverage suppression or missing-file exception was added.

Independent review found and corrected a valid-filter HTTP boundary: maximum
percent-encoded trace scopes with pagination exceeded the old 4096-byte URI
budget. Only the two trace list/summary routes now have an 8192-byte budget;
synthetic tests cover maximal exact values, real continuation cursors, oversized
rejection and the unchanged budget on other routes. Review and focused reruns
also covered read-only preview bytes/mtime/sidecars, WAL refusal, conflicts after
preview, replay/deletion protection and UI stale-response/source isolation.

Separately, all 43 synthetic release-tool tests, the official-SDK MCP check,
E2E TypeScript checking and the updated smoke against the fresh **debug executable**
passed. The package fixtures check relative Markdown link closure on all four
archive labels. Those fixtures are not native cross-platform executables or
extracted release artifacts. The browser suite discovers 48 tests across
1440/390/320px, including 21 trace cases and English/light plus Chinese/dark
screenshots. The local browser socket restriction was not bypassed; no new
rendered browser pass is claimed. Exact-commit CI, screenshot inspection and all
extracted native release packages remain required before publication.
