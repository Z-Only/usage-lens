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
