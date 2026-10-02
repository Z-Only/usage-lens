# Third-party software notices

Usage Lens's MIT license covers its own code. The native program and embedded WebAssembly UI also use third-party software under its respective terms. Static linking does not remove those terms.

Every release archive includes:

- `THIRD_PARTY_COMPONENTS.json`: exact locked package versions, archive and manifest SHA-256 identities, declared licenses, upstream authors/repositories, notice sources, and native/WASM dependency roles
- `THIRD_PARTY_LICENSES.txt`: preserved upstream license/notice texts, reviewed exact-commit supplements, the bundled SQLite public-domain notice, and standard license texts referenced by the Rust toolchain
- `RUST_STANDARD_LIBRARY_NOTICES.html`: the complete official copyright bundle from the pinned Rust toolchain, including its runtime/allocator/compiler-builtins and musl notices where applicable

The component list conservatively includes build dependencies and procedural macros as well as normal dependencies. `build/proc-macro` means build-time tooling/code generation; it does **not** assert that the tool itself is shipped. `normal-dependency` is dependency-graph evidence, not a byte-level link map. A component can have both roles. Dev-only dependency edges are excluded. No external font, icon library, or CDN asset is bundled by the UI build.

## License evidence and reviewed exceptions

The generator reads registry archives selected by `Cargo.lock`, preserves their license/notice files, and verifies archive identity. Crates that omit a license file can use a checked-in supplement from their exact upstream commit. Each supplement has its source URL, upstream commit, exact package version, and SHA-256 in `scripts/release/licenses/overrides.json`.

Seven initial locked packages declare a standard SPDX license but contain no full license notice in their crate archive:

- `erased 0.1.2`, `convert_case_extras 0.2.0`, `quote-use 0.8.4`, `quote-use-macros 0.8.4`: MIT
- `drain_filter_polyfill 0.1.3`, `syn_derive 0.2.0`: MIT OR Apache-2.0
- `server_fn_macro_default 0.8.5`: MIT

For the first six, complete exact-commit upstream trees also contain no license/notice file. For `server_fn_macro_default`, its registry-recorded upstream commit is unavailable; that is an availability gap, not evidence that the upstream repository never had a license. The published crate still explicitly declares MIT and its author.

These **exact versions only** have reviewed declaration-based notice supplements: their original Cargo metadata is reproduced verbatim, with the locked crate archive identity and canonical complete SPDX texts. Existing notices are retained, author fields stay as provided, and no copyright holder or year is invented. Canonical license templates retain any placeholders; they are not presented as an upstream-authored copyright notice. This is a transparent distribution-notice fallback, not a legal assurance. New or changed versions require fresh review rather than inheriting this exception.

Cargo defines its [`license` field](https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields) as the license under which a package is released, expressed using SPDX. Canonical fallback texts are pinned to [SPDX License List 3.29.0, commit `31ba1a50e5397e00a304dbadc76531740e89ee48`](https://github.com/spdx/license-list-data/tree/31ba1a50e5397e00a304dbadc76531740e89ee48/text). Exact evidence/source links are retained beside each generated entry.

## Platform components

SQLite is compiled from the bundled `libsqlite3-sys` amalgamation, and its actual source public-domain notice is preserved; see [SQLite copyright](https://www.sqlite.org/copyright.html). The Rust toolchain's own notice bundle is included rather than trying to reconstruct standard-library dependencies from Cargo metadata.

Windows builds use the statically linked Microsoft C runtime from the GitHub runner's Microsoft toolchain. Its redistribution is governed by that toolchain's terms; see Microsoft's [Visual C++ redistribution documentation](https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files). This notice does not relicense Microsoft or Apple platform components. Maintainers must review the applicable toolchain redistribution terms when changing platform/toolchain setup.

## Maintainer checks

Run `python scripts/release/notices.py --target PLATFORM --out target/release-notices/PLATFORM` after the locked dependencies and pinned toolchain have been installed. Release builds do this automatically before packaging. Collection is offline after the locked Cargo fetch. Missing texts, unknown license families, changed exact-version supplements, changed archive identities, or missing Rust/SQLite notices stop packaging. Do not bypass a failure by replacing upstream notices with Usage Lens's own license.
