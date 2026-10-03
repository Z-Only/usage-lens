# Usage Lens

Local usage evidence for AI workflows: a private loopback dashboard, Skill + local CLI queries, optional MCP, and explicit collectors/importers.

中文：在本机查看可取得的 AI 使用记录、Token、官方额度窗口和 Skill／工具证据。对话查询默认使用 Skill + 本地 CLI，MCP 保留为可选项。正文与工具明细只在本地查看；通过助手查询时，仅返回所请求的统计。缺失记录不会被当作零，加载 Skill 不会被当作任务成功。

## What it can tell you

- Whether a selected source has observed, missing or unsupported collection methods, stale collection times, retained failures, or only partial local evidence
- Which service-reported account metrics and quota windows were actually returned, and when they were collected
- What supported local hooks observed after setup, with explicit gaps and optional local content retention
- What directly evidenced skill reads/injections and response-token records appear in explicitly supplied supported records
- How requested, loaded and invoked skill evidence varies across a bounded UTC date range, with exact skill-name filtering and explicit undated-evidence gaps
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

The dashboard supports English/中文, light/dark themes, source selection, activity filters, quota windows, skill evidence, collection health, local content details, and capture/retention controls. The Skills view includes a daily evidence panel using the shared date filters; its UTC dates come only from recorded occurrence times. Its refresh control reads the local store; it does not silently launch an account collector.

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

## Check setup and collection health

After verifying an installed binary, run its local setup diagnostic without a database:

```sh
usage-lens doctor
```

`doctor` prints JSON diagnostics for the running binary and embedded dashboard.
Optionally point it at one existing store and source:

```sh
usage-lens doctor --db "$HOME/.usage-lens/usage.sqlite" --source local-records
usage-lens health --db "$HOME/.usage-lens/usage.sqlite" \
  --source local-records --max-age-ms 3600000
```

The selected-store checks open the existing database read-only; they do not create
or migrate it. `doctor` exits with status 1 if a diagnostic check fails. Its optional
`--max-age-ms` requires `--source`; `health` always requires an explicit source and
supports the same freshness threshold. Use `health --demo --source SOURCE` only for
an explicitly labeled synthetic demonstration, choosing `SOURCE` from `status --demo`.

Health separates retained observations from unsupported or missing methods,
collection age from source-reported `sourceAsOf`, and current collection state from
retained past failures. Stored event, skill-evidence, response and import counts,
capture/occurrence bounds and unknown-time counts describe only the selected local
store. Empty evidence is not proof of zero historical use; coverage remains partial.

Both commands are local checks. They do not scan configuration/transcript directories,
start another process, contact the network, read credentials, install a client,
collect data, or register a background service. `doctor` is a setup workflow outside
the conversational Skill; `health` is an allowed aggregate query. See the
[data contract](docs/data-contract.md#collection-health) for interpretation.

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

For a deliberately selected append-only logical stream, v0.4.0 also offers a
separate one-shot write command. **Before its first successful import into an
existing store, stop other writers and make a verified private backup.** That
import upgrades the store to schema 3; v0.3.0 and older cannot open schema 3.

```sh
usage-lens import-rollout-incremental --db "$HOME/.usage-lens/usage.sqlite" \
  --source local-records --file /absolute/path/to/supplied-records.jsonl \
  --stream selected-rollout-1 \
  --source-version a75987455a2879ca151cea5e118fa307be868583
```

Supply the path, source, logical stream ID and version every time. Each call
boundedly rereads and reparses the complete newline-terminated prefix; this is
not O(delta) tailing. Every unterminated tail is deferred, even valid JSON or
partial UTF-8. A shorter file or changed committed prefix fails without changing
records or progress. A byte-identical copied prefix can continue the same logical
stream, but no physical-file identity is verified. Rotation requires an explicit
new stream or source. The existing 8 MiB/256 KiB/20,000-line/1,000-record bounds
are unchanged; this command cannot follow an unbounded file. See the full
[incremental workflow and replay rules](docs/record-import.md#incremental-one-shot-workflow).
It never watches, schedules, discovers files or reopens a remembered path.
`import-rollout` keeps its snapshot behavior. When switching import modes, prefer
a separate source because anonymous identities are not deduplicated across modes.

The optional hook configuration in [the plugin package](plugin/usage-lens/README.md) observes supported future local events. It does not scan transcript directories or enable itself. Cloud-orchestrated Work does not support ordinary plugin hooks, and missing hooks are not evidence that no activity occurred.

Skill metrics distinguish requested, read/loaded, and explicitly recorded invocation evidence. Pagination and resource reads are not new invocations. Instruction injection is not task success. Imported or forked histories can overlap; coverage and evidence categories must remain visible.

## Conversational integration: Skill + local CLI

The default is **Skill + local CLI**. Install the self-contained `usage-summary`
skill in a local-capable client, then supply the exact native executable, existing
absolute database path, and source ID. Follow the concrete, permission-aware
[Skill installation steps](docs/AI_INSTALL.md#6-primary-conversational-integration-skill--local-cli).
No MCP server or running dashboard is needed for queries.

The [plugin package](plugin/usage-lens/README.md) contains the Skill, an inert plugin
manifest, and optional examples. Its eight allowed commands are `status`, `overview`,
`daily`, `quota`, `tools`, `skill-summary`, `response-tokens`, and `health`. For example, on a POSIX
shell after replacing the placeholders:

```sh
'/ABSOLUTE/usage-lens' status --db '/ABSOLUTE/usage.sqlite'
'/ABSOLUTE/usage-lens' skill-summary --db '/ABSOLUTE/usage.sqlite' --source 'SOURCE'
'/ABSOLUTE/usage-lens' health --db '/ABSOLUTE/usage.sqlite' --source 'SOURCE'
```

Use the intended source from `status`; never guess a database or account binding.
The Skill only queries existing aggregates. Collection, imports, hooks, dashboard
startup, and local content review remain separate workflows. Building this project
does not install the Skill, change client settings, authenticate, or open a tunnel.
`skill-summary` matches MCP `usage_skills`. Add paired `--from YYYY-MM-DD` and
`--to YYYY-MM-DD` for an inclusive UTC range of at most 366 days, and optionally
`--skill 'EXACT_NAME'`; a skill-only filter is also supported. MCP uses `fromDate`,
`toDate`, and `skillName`. With no date/skill filter the existing response is
unchanged. Date trends use only `occurredAt`, exclude undated evidence, and report
its count separately across all retained source/skill-matching records. Missing
days remain unknown, not zero. Per-skill groups are bounded to 500 with an explicit
truncation flag; totals are independent of that limit. See the
[skill-summary contract](docs/data-contract.md#skill-evidence-summaries-and-utc-trends).
No model, success, unique-execution, or per-skill token attribution is inferred.

The local `skills` evidence command is outside this conversational allowlist.
The instruction-level allowlist is not an OS sandbox. Persisted queries open an
existing schema-2 or schema-3 rollback-journal store read-only; normal older
schema-2 databases remain query-compatible without migration in v0.4.0. Queries
fail rather than create or migrate a store. Incremental import is a separate
explicit write workflow, outside the Skill's eight-command allowlist and optional
MCP tools. It does not install a background process or make collection automatic.

### Optional MCP

MCP remains available only when explicitly chosen and separately configured in a
client supporting local stdio MCP. The process entry is:

```sh
'/ABSOLUTE/usage-lens' mcp --db '/ABSOLUTE/usage.sqlite'
```

The server's eight aggregate tools (including `usage_health`) have no raw-content, SQL, arbitrary-file,
account-refresh, or deletion endpoints. The Skill does not auto-activate MCP or
silently switch transports. Both CLI and MCP results leave the machine when shared
with a remote assistant; use the local dashboard for retained content.

A Skill does not grant computer access. Cloud-only ChatGPT cannot directly execute
the user's local CLI or reach local stdio/loopback. Any remote setup requires
separate authorization and real-client verification. Synthetic tests do not prove
integration with every ChatGPT surface; no real client has been installed by this
project's tests.

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
- `plugin`: CLI-first aggregate Skill and optional MCP/hook configuration examples
- `.github/workflows`: PR checks, native package checks, and version-tag releases

The database/query core is shared. The remote-assistant query surface is deliberately narrower than the local detail surface.

## Upstream references

- [Official app-server methods](https://learn.chatgpt.com/docs/app-server)
- [Official Skill discovery and invocation](https://learn.chatgpt.com/docs/build-skills)
- [Supported plugin surfaces](https://learn.chatgpt.com/docs/plugins)
- [Local hook contract](https://learn.chatgpt.com/docs/hooks)
- [Data contract and source semantics](docs/data-contract.md)

MIT licensed. No affiliation with OpenAI is implied.
