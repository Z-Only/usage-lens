# Usage Lens

Local usage evidence for AI workflows: a private loopback dashboard, a read-only query plugin, and explicit collectors/importers.

中文：在本机查看可取得的 AI 使用记录、Token、官方额度窗口和 Skill／工具证据。正文与工具明细只在本地查看；通过 ChatGPT 查询时，仅返回所请求的统计。缺失记录不会被当作零，加载 Skill 不会被当作任务成功。

## What it can tell you

- Which service-reported account metrics and quota windows were actually returned, and when they were collected
- What supported local hooks observed after setup, with explicit gaps and optional local content retention
- What directly evidenced skill reads/injections and response-token records appear in explicitly supplied supported records
- Which model/tool/evidence categories occur in the collected records, without inventing complete account-wide coverage

Account usage, imported response usage, and quota are different measurements. Usage Lens does not add them together, infer remaining tokens from a percentage, or allocate an entire turn's token usage to a skill.

## Start with synthetic data

The application is a native Rust executable with its Leptos/WebAssembly dashboard embedded. Installed release builds do not require Node.js, Bun, or Rust. See [AI-assisted installation](docs/AI_INSTALL.md) for platform selection, checksum verification, and local setup. Release availability is shown on the repository Releases page; a workflow definition alone is not a published release.

For a source build, install the pinned Rust toolchain, its wasm32-unknown-unknown target, Python 3, and wasm-bindgen-cli 0.2.129. Node 24 and Bun 1.4.2 are development-only tools for browser and MCP compatibility tests.

From the repository checkout:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
python3 scripts/build_ui.py
cargo build --locked --release -p usage-lens
./target/release/usage-lens serve --demo --port 4319
```

Open the printed `http://127.0.0.1:4319` address on the same computer. Demo records live in memory and are labeled. This command does not connect to your account or run a model. Stop the server with Ctrl+C.

The dashboard supports English/中文, light/dark themes, source selection, activity filters, quota windows, skill evidence, local content details, and capture/retention controls. Its refresh control reads the local store; it does not silently launch an account collector.

## Use a persistent local store

The following examples assume the installed binary is on PATH; otherwise use its full path. All persisted CLI commands require an absolute database path. On macOS/Linux, choose a directory outside any synced or public repository:

```sh
mkdir -p "$HOME/.usage-lens"
usage-lens source --db "$HOME/.usage-lens/usage.sqlite" \
  --source local-records --mode imported --name "Local records"
usage-lens serve --db "$HOME/.usage-lens/usage.sqlite"
```

On Windows, use an absolute path under your own profile and your shell's quoting rules. No account credentials are created by these commands.

Content capture is off by default. Enable it explicitly if your applicable data policies allow it:

```sh
usage-lens settings --db "$HOME/.usage-lens/usage.sqlite" --content true
```

This affects subsequent accepted records. Enable it before importing if you need local content details. Reimporting an identical file is a no-op and does not backfill content after enabling capture or deleting content; use a new explicit source for a deliberate import with a different capture scope. Known secret patterns are redacted, but redaction is best-effort. Do not treat the database as free of sensitive information.

## Account metrics and quota

The account adapter uses only documented read methods after initialization: `account/read`, `account/usage/read`, and `account/rateLimits/read`. It does not start a task or inference turn, log in, consume quota resets, send email, or call undocumented fallback endpoints.

Live collection is a separate explicit action:

```sh
usage-lens source --db "$HOME/.usage-lens/usage.sqlite" \
  --source account-local --mode live --name "My local account source"
usage-lens collect --db "$HOME/.usage-lens/usage.sqlite" \
  --source account-local --accept-startup-risk
```

This requires a compatible installed `codex` executable and an already configured supported account. It launches that executable's app-server; startup can read local configuration, initialize configured integrations, contact provider services, or handle existing credentials. The acknowledgement flag is not a sandbox. Review your installed runtime and configuration before using it. No live account is accessed by tests or demo mode.

Fields and methods may be unavailable in your installed version or account. Errors remain errors; they never fall back to demo data. Source namespaces are explicitly assigned by you, not verified account/workspace identities. Use a different namespace after changing accounts.

## Local records, hooks, and skill evidence

For file selection and safe sample testing, see [sample validation](docs/sample-validation.md). The versioned Usage Lens bundle importer accepts explicitly supplied observations and events. The source-backed rollout importer has a separate documented format boundary; see [record import](docs/record-import.md). A generic ChatGPT conversation export is not automatically a compatible rollout file.

For the pinned supported rollout format, select one local file explicitly:

```sh
usage-lens import-rollout --db "$HOME/.usage-lens/usage.sqlite" \
  --source local-records --file /absolute/path/to/supplied-records.jsonl \
  --source-version a75987455a2879ca151cea5e118fa307be868583
usage-lens response-tokens --db "$HOME/.usage-lens/usage.sqlite" \
  --source local-records
```

The version flag declares the format you intend to parse; it does not authenticate the file or prove complete collection. Review returned warnings. The importer never follows embedded file paths or executes recorded commands.

The optional hook configuration in [the plugin package](plugin/usage-lens/README.md) observes supported future local events. It does not scan transcript directories or enable itself. Cloud-orchestrated Work does not support ordinary plugin hooks, and missing hooks are not evidence that no activity occurred.

Skill metrics distinguish requested, read/loaded, and explicitly recorded invocation evidence. Pagination and resource reads are not new invocations. Instruction injection is not task success. Imported or forked histories can overlap; coverage and evidence categories must remain visible.

## ChatGPT / MCP integration

[plugin/usage-lens](plugin/usage-lens/README.md) contains a validated plugin manifest, aggregate-query skill, and inert configuration examples. Building this project does not install the plugin, change your client settings, register hooks, authenticate, or open a tunnel.

For a client supporting local stdio MCP, the process entry is:

```sh
/absolute/path/to/usage-lens mcp \
  --db /absolute/path/to/usage.sqlite
```

The query server exposes a fixed aggregate allowlist. It has no raw-content, SQL, arbitrary file, account-refresh, or deletion tools. Returned summaries leave the machine when you request them through a remote assistant. Use the local web dashboard for retained content.

Cloud ChatGPT cannot directly reach this local stdio process or your loopback address. Any remote transport requires separate setup, authorization, and real-client verification. Protocol tests are not proof of a working integration on every ChatGPT surface.

## Privacy and deletion

Read [the privacy boundary](docs/privacy.md) before enabling content capture. The dashboard binds to loopback, checks Host/Origin, and uses explicit confirmation for destructive actions. It does not need an external model API or upload your data for analysis.

Pause capture:

```sh
usage-lens settings --db "$HOME/.usage-lens/usage.sqlite" --pause true
```

Local deletion and retention are explicit CLI/UI operations. Review the command's scope first with `usage-lens help`. Database cleanup does not erase backups, exported files, or filesystem snapshots. Never attach a real database or conversation log to an issue or PR.

## Development and verification

```sh
bun install --frozen-lockfile --ignore-scripts
cargo install cargo-llvm-cov --version 0.9.1 --locked
rustup component add llvm-tools-preview
bun run typecheck
bun run lint
bun run test:coverage
bun run test:gate
bun run build
bun run ci-gate
bun run test:mcp
# Install the official Playwright browser before this check
bun run test:e2e
```

The aggregate CI gate requires build, static checks, tests, total production-line coverage ≥95%, and changed executable-line coverage ≥95%. Cross-platform runtime jobs and a synthetic loopback browser workflow are configured. All application runtime modules, including entrypoints, remain in the coverage inventory.

Tests use synthetic data, fake app-server peers, an in-memory MCP client, and local disposable databases. Current verification status and any untested live integrations belong in [verification notes](docs/verification.md).

## Architecture

- `crates/usage-lens/src/core`: validation, normalization, bundled SQLite persistence, aggregate queries, local-only content
- `crates/usage-lens/src/adapters`: bounded account protocol, supported hook records, explicit import formats
- `crates/usage-lens/src/server`: Axum loopback HTTP and aggregate-only stdio MCP
- `crates/usage-lens/src/cli.rs`: explicit collection/import/query/server lifecycle commands
- `crates/usage-lens-ui`: Leptos dashboard, shared Rust view models and native-render tests
- `scripts/build_ui.py`: genuine CSR WebAssembly build, hashed asset manifest, native embedding validation
- `plugin`: distributable instructions and opt-in configuration examples
- `.github/workflows`: PR checks, native package checks, and version-tag releases

The database/query core is shared. The remote-assistant query surface is deliberately narrower than the local detail surface.

## Upstream references

- [Official app-server methods](https://learn.chatgpt.com/docs/app-server)
- [Supported plugin surfaces](https://learn.chatgpt.com/docs/plugins)
- [Local hook contract](https://learn.chatgpt.com/docs/hooks)
- [Data contract and source semantics](docs/data-contract.md)

MIT licensed. No affiliation with OpenAI is implied.
