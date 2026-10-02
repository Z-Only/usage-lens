# Native build and source coverage

## Toolchain and deliverables

- Rust 1.98.1, pinned in `rust-toolchain.toml`; Leptos 0.8.21 and Axum 0.8.9
- `wasm32-unknown-unknown` compilation target; `wasm-bindgen-cli` 0.2.129, exactly matching the crate
- SQLite is compiled into the native backend with rusqlite's `bundled` feature
- `python3 scripts/build_ui.py` builds the Leptos CSR crate with Cargo and runs wasm-bindgen. No Node or Bun is used in this production build
- `cargo build --locked -p usage-lens --release` embeds that frontend into the native executable. Users do not need a Rust, Node, or Bun runtime
- Node/Bun and Playwright are test tooling for the real-browser acceptance suite

The UI build replaces only `dist/ui` after successful compilation. It emits `frontend-build.json` with the framework/tool versions and SHA-256 hashes of all assets. Backend compilation rejects a missing/stale manifest, an old Vue-only build, a non-WASM payload, symlinks, missing files, extra unlisted files, or mismatched hashes. Generated wasm-bindgen JavaScript is a local WebAssembly loader, not a Node server.

## The required gate

Run `bash scripts/ci_gate.sh` with the pinned tools installed. CI passes the exact PR base SHA through `COVERAGE_BASE`. Local runs default to the empty Git tree and include staged, unstaged, and untracked production source.

The gate runs build-script/gate regression tests, source guardrails, formatting, warning-free native and WASM Clippy, native backend and Leptos SSR/shared-state tests with LLVM instrumentation, and a combined LCOV total/changed-line check. Both total production line coverage and changed production line coverage must reach 95%, without rounding. Tests and generated dependency output do not pad that denominator. The earlier TypeScript prototype is not accepted as evidence for Rust coverage.

Every crate `src/**/*.rs` file is inventoried, including all CLI/main/worker/server entrypoints. All authored browser JavaScript under the UI public directory is also inventoried. Ordinary missing LCOV records fail the gate, even for unchanged files. A missing Rust file can be treated as zero-executable only when its contents match a deliberately narrow grammar of module declarations/reexports, comments, explicit WASM module cfgs, and compile-only recursion-limit metadata; filename alone never grants an exception. Build scripts and dedicated integration-test directories are build/test code, not end-user runtime source.

## WebAssembly measurement limitation

Native SSR/shared-state tests do not prove browser execution. In this setup, `crates/usage-lens-ui/src/browser.rs`, DOM event handlers in `src/event_bridge.rs`, and the authored `public/bootstrap.js` are **unmeasured**. The gate explicitly counts every nonblank physical line of each as **uncovered**, including braces/comments. Any larger known LLVM executable-instance denominator is also retained as uncovered. It does not omit these files, manufacture coverage hits, or mix old TypeScript coverage into the Rust result. Changed nonblank bridge lines are likewise included as uncovered. This yields a conservative whole-product bound, not a claim that browser coverage has been measured. If this bound misses 95%, the gate fails.

Browser-only application code must stay in these explicit bridges. DOM event handlers are accounted separately because SSR macros can omit handlers even when their surrounding view source is instrumented. New unmeasured files are ordinary missing coverage failures until accurately measured or explicitly accounted for conservatively; no general exclusion exists. Playwright acceptance tests check rendered behavior but do not create LLVM source-line hits.

The [official wasm-bindgen coverage guide](https://wasm-bindgen.github.io/wasm-bindgen/wasm-bindgen-test/coverage.html) documents an experimental path using nightly Rust, `wasm_bindgen_unstable_test_coverage`, `-Cinstrument-coverage -Zno-profiler-runtime`, matching LLVM/Clang, and the wasm-bindgen test runner. It warns that this path may be unreliable and change. It has not been verified for this project; stable native coverage must not be described as a substitute. See also [Rust instrumentation coverage](https://doc.rust-lang.org/rustc/instrument-coverage.html) and [Leptos CSR build documentation](https://book.leptos.dev/getting_started/index.html).

## Platform validation

CI tests native Linux x64, macOS arm64, macOS x64, and Windows x64. The separate release workflow builds and smoke-tests packaged native artifacts. A configured workflow is not evidence that those jobs have passed: release readiness requires successful checks on the exact commit and actual packaged-binary smoke results.
