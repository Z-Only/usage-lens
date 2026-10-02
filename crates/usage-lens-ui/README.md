# Usage Lens Leptos frontend

The production dashboard is a Rust client-side Leptos application compiled to
WebAssembly. The native Axum binary embeds its generated files. No Node or Bun
runtime, external CDN, account login, model API, or telemetry is used by the app.

## Boundaries

- `model.rs`: platform-neutral state machine, request generations, source/event
  identity checks, exact decimal-string formatting, calendar-gap handling,
  settings validation and typed destructive confirmation
- `views.rs`: shared Leptos view functions; native SSR tests render the same
  structure and text used by the browser
- `event_bridge.rs`: browser event value extraction and default prevention
- `browser.rs`: WASM entry point, same-origin fetch, request aborts, saved
  preferences, native modal activation and focus restoration
- `public/styles.css`: accepted dashboard stylesheet, with native-dialog rules
- `public/bootstrap.js`: minimal local WASM-module bootstrap

HTTP payloads preserve the existing camelCase JSON contract. Account snapshots,
local event counts, and imported completed-response counters remain independent.
Source namespaces are never merged. Missing cells remain unknown, and retained
content is rendered as escaped text rather than HTML.

A request can update state only while its generation, source and (for detail)
event identity still match. A source change clears previous source evidence.
Draft-input transitions do not recreate focused fields. Dialog transitions leave
their opener mounted; native modal dialogs contain keyboard focus and dismiss
through a reducer action. In-flight destructive mutations cannot be dismissed or
retargeted.

## Build and verification

From the workspace root:

```sh
python3 scripts/build_ui.py
cargo test -p usage-lens-ui --features ssr
cargo clippy -p usage-lens-ui --all-targets --features ssr -- -D warnings
cargo clippy -p usage-lens-ui --lib --target wasm32-unknown-unknown \
  --no-default-features --features csr -- -D warnings
```

The frontend build emits a genuine WASM module, generated JavaScript loader,
local bootstrap, CSS and HTML, plus a SHA256 asset manifest. It requires the
pinned Rust toolchain, wasm32 target and matching wasm-bindgen CLI.

Native tests use only committed synthetic fixtures. SSR verifies generated
structure, safe text, labels and semantics, but does not verify browser layout,
real event dispatch, focus, fetch or storage. Those need the project Playwright
workflow against the compiled application. Viewport targets are 320, 390 and
1440 pixels, with English/Chinese and light/dark modes.

The production coverage gate explicitly counts all nonblank authored lines of
`browser.rs`, `event_bridge.rs` and `public/bootstrap.js` as uncovered when no
verified WASM source-coverage run exists. Native SSR results must not be described
as browser coverage. Generated compiler/dependency output is not authored source.
