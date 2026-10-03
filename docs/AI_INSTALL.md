# AI-assisted installation: Usage Lens

This is the installation contract for an assistant acting **with the user's permission**. It is also a manual installation guide. A repository file cannot grant permission to inspect private records, use credentials, change security settings, or enable background collection.

中文：对话接入默认使用 Skill + 本地 CLI，MCP 仅供明确选择后配置。优先安装 GitHub Release 中与你的系统和 CPU 对应的原生程序；先校验 SHA-256，再解压和运行。无需安装 Node、Bun 或 Rust。先用合成演示数据验证；读取真实记录、访问已有账号、启用采集或修改客户端配置前，必须取得用户明确同意。不要复制密钥或扫描会话目录。

The primary conversational path is **Skill + local CLI**. MCP is optional and off
unless separately chosen and configured. Installing the Skill, collecting data,
and running the local dashboard are independent steps; no unified installer or
background service is implied. See [step 6](#6-primary-conversational-integration-skill--local-cli)
for local Skill installation after binary verification.

## 1. Establish scope before changing anything

Ask which computer to install on if unclear. Inspect only the selected computer's OS, CPU architecture, available disk space, and intended installation directory. Do not inspect the user's accounts, transcript directories, keychain, `.env` files, browser profiles, or existing databases to “discover” configuration.

Record a short install plan: exact version, repository, asset, installation directory, database location (if later authorized), and whether any client integration is requested. Installing the executable does not authorize running a collector, importing records, content capture, hooks, login, tunneling, startup registration, or sharing aggregate results with an assistant provider.

## 2. Choose one exact published asset

Canonical repository: [Z-Only/usage-lens](https://github.com/Z-Only/usage-lens)

Open [published releases](https://github.com/Z-Only/usage-lens/releases), read the chosen release notes, and pin the exact stable version. `0.6.0` below is an example, not a claim that the release already exists. If its assets have not been published, stop; never substitute GitHub's automatically generated source archives for an executable release.

| Detected system | CPU | Asset for version `0.6.0` |
| --- | --- | --- |
| Linux x64 (static musl) | x86_64 | `usage-lens-0.6.0-linux-x64.tar.gz` |
| macOS 11.0 deployment target; tested on 15 | arm64 / Apple Silicon | `usage-lens-0.6.0-macos-arm64.tar.gz` |
| macOS 11.0 deployment target; tested on 15 | x86_64 / Intel | `usage-lens-0.6.0-macos-x64.tar.gz` |
| Windows x64 | AMD64 | `usage-lens-0.6.0-windows-x64.tar.gz` |

The matching sidecar replaces `.tar.gz` with `.manifest.json`. Also download `SHA256SUMS` and `release.json` from **the same tag's release**. Assets include a native Rust executable, embedded Leptos UI, bundled SQLite, license, third-party license/notice bundles, documentation, and inert plugin examples. No Node, Bun, Rust, npm, compiler, or package installation is required on the user's computer. The Linux executable is statically linked with musl; it does not require a particular glibc version. Native OS/kernel compatibility still matters.

Linux uses a static musl target and is smoke-tested on Ubuntu 24.04. Older kernels, CentOS 7, and Alpine are not claimed compatible without their own tests. Linux ARM, Windows ARM, 32-bit platforms, and macOS below 11.0 are not published targets. macOS 11–14 runtime behavior remains unverified; the compiler deployment target is not evidence of testing those OS versions. macOS architecture means the actual machine, not an emulated shell: on Apple Silicon with a Rosetta shell, inspect `sysctl -n hw.optional.arm64` and choose ARM64. On Windows inspect `[System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture`. Use `uname -s` and `uname -m` on Linux. Do not silently install an emulated or different-architecture build.

Windows builds use the static MSVC runtime. GitHub tests them on Windows Server 2025; Windows desktop versions require their own verification. A normal browser with JavaScript and WebAssembly support is needed for the dashboard; CLI/MCP do not need a browser. These first-release assets are not OS code-signed/notarized. If Gatekeeper, SmartScreen, antivirus, or a browser security warning blocks them, report the exact warning and hand control to the user. Do not remove quarantine, disable security, or bypass a warning.

## 3. Download, verify, and inspect before execution

Use the browser or an ordinary HTTPS download tool. Never use `curl | sh`, run an installer fetched from another site, or install packages to make a checksum command work. Example URL pattern:

```text
https://github.com/Z-Only/usage-lens/releases/download/v0.6.0/usage-lens-0.6.0-macos-arm64.tar.gz
```

In a new temporary download directory, the following POSIX example downloads only public release files. Set `platform` from the table, and set `version` to the version the user selected:

```sh
version=0.6.0
platform=macos-arm64
asset="usage-lens-${version}-${platform}.tar.gz"
manifest="usage-lens-${version}-${platform}.manifest.json"
base="https://github.com/Z-Only/usage-lens/releases/download/v${version}"
for file in "$asset" "$manifest" SHA256SUMS release.json; do
  curl --fail --location --proto '=https' --tlsv1.2 --output "$file" "$base/$file" || exit 1
done
# Select exactly the three downloaded files covered by SHA256SUMS.
awk -v a="$asset" -v m="$manifest" '$2 == a || $2 == m || $2 == "release.json"' SHA256SUMS > selected-checksums
[ "$(wc -l < selected-checksums | tr -d ' ')" = 3 ] || exit 1
# macOS (on Linux use: sha256sum --check selected-checksums)
shasum -a 256 --check selected-checksums
```

Any mismatch is a hard stop. Do not ignore, edit, regenerate, or bypass a checksum to continue. On Windows, use `Invoke-WebRequest` to save the same four exact URLs. For each of the archive, manifest, and `release.json`, find its unique exact filename in `SHA256SUMS` and compare `Get-FileHash -Algorithm SHA256` to that entry (case-insensitive hex). Abort on a duplicate, missing entry, or mismatch. Do not execute the archive contents yet.

Checksums detect corruption and mismatched downloads, not a compromised publisher. The trust source remains the official GitHub repository, its release workflow, and HTTPS. Never accept a checksum solely from an unrelated chat, mirror, or third-party website.

Read the verified manifest as data. Require:

- `schemaVersion` is `1`; `name` is `usage-lens`; `version` matches the selected tag
- `commit` matches `release.json` and the release's full source commit
- `platform`/`rustTarget` match the chosen machine; review `minimumOS`
- `runtimePrerequisite` is `none`, `ui` is `embedded`, `sqlite` is `bundled`
- `files` names only the executable, named documentation and third-party notices, and inert plugin examples

List archive contents with `tar -tzf ASSET` and `tar -tvzf ASSET` (Windows: `tar.exe`). Every member must be a regular file inside the single expected `usage-lens-VERSION-PLATFORM/` directory. Reject absolute paths, `..`, links, devices, unexpected executables, databases, logs, credentials, or install scripts. Extract into a **new, empty user-owned directory**, never on top of an existing installation. Compare the internal `manifest.json` with the verified sidecar. Before execution, compare the extracted executable's SHA-256 to its `files` entry too (`shasum -a 256`, `sha256sum`, or `Get-FileHash`). Retain the checksums and manifest with the installation record.

A convenient final directory is `~/.local/share/usage-lens/0.4.0/` on macOS/Linux or `%LOCALAPPDATA%\UsageLens\0.4.0\` on Windows. Do not require administrator access. Leave previous versions in their own directories. Do not edit PATH or add a startup service unless separately requested.

## 4. Verify setup and the isolated synthetic demo

From the extracted directory, run only these local commands first:

```sh
./usage-lens --help
./usage-lens doctor
./usage-lens status --demo
./usage-lens serve --demo --port 4319
```

`doctor` returns JSON and checks the running binary and embedded dashboard without
opening a database. A failed check returns exit status 1. It does not scan local
configuration, read credentials, launch Codex, use the network, install anything,
or register background collection. With no selected store/source the overall
status is `incomplete` and those checks are `not_checked`; this alone exits 0.
This is a setup diagnostic, outside the
conversational Skill. It does not establish actual client integration.

On PowerShell replace `./usage-lens` with `& .\usage-lens.exe`. `--help` should show the selected version. `status --demo` should report synthetic sources, never the user's account. Visit the printed `http://127.0.0.1:4319` URL **on that same computer**, verify the dashboard and its synthetic labels, and optionally read `/api/status`. The demo uses an in-memory database; it does not launch Codex, log in, inspect account credentials, or run a model. Stop it with Ctrl+C. If the port is in use, choose another local port or `--port 0` and use the printed URL.

Do not bind to `0.0.0.0`, expose the port, open a firewall rule, or establish a tunnel. If a browser on another computer cannot reach loopback, explain that boundary instead of widening network access.

## 5. Configure persistent data only after approval

Explain the intended absolute database path, what will be retained, and which operation the user is authorizing. Keep data outside the executable directory, cloud-synced folders, and repositories. The default is no content capture. Create an explicit imported source only when requested:

**v0.6.0 trace upgrade warning:** before the first successful
`import-trace-bundle` into an existing store, stop all writers and make a verified
private backup. This explicit import upgrades schema 2/3 to schema 4 atomically;
v0.5.0 and older cannot read schema 4. Read-only queries accept schemas 2/3/4
without migration. A latest Mac app or Local-mode selection does not establish
that its runtime supports `CODEX_ROLLOUT_TRACE_ROOT` or inherits that environment
variable. Live desktop activation remains unverified and requires separate user
authorization. Do not change launch configuration or run a model to test this as
part of installation. See [trace import](trace-import.md).

**v0.4.0 upgrade warning:** before the first successful
`import-rollout-incremental` into an existing store, stop all writers and make a
verified private backup. That explicit write upgrades schema 2 to schema 3
atomically with its import. v0.3.0 and older cannot read schema 3; binary-only
rollback will not work. Ordinary writable opens/new stores stay on schema 2 until
this upgrade, and read-only queries never migrate. Follow the
[backup and rollback steps](#upgrade-rollback-and-uninstall), even if the import
file contains only synthetic records but the chosen store is real.

```sh
./usage-lens source --db /absolute/private/path/usage.sqlite \
  --source local-records --mode imported --name "Local records"
./usage-lens serve --db /absolute/private/path/usage.sqlite
```

Use the platform's quoting rules; do not literally use the example path. A persistent source starts empty. Data appears only after a separately authorized import, hook, or collector. Never fill an empty real view with demo data.

For an already configured store, optional diagnostics and health reads use only the
exact database/source selected by the user:

```sh
./usage-lens doctor --db '/absolute/private/path/usage.sqlite' --source 'local-records'
./usage-lens health --db '/absolute/private/path/usage.sqlite' --source 'local-records'
```

`doctor --db ABS` checks an existing database read-only, without creating or migrating
it. Add `--source ID` to check that source; `--max-age-ms N` is permitted only with
`--source`. `health` requires `--source` and accepts optional `--max-age-ms`. A
successful health query can still report stale, missing or unsupported observations
and retained failures; command success does not establish complete collection.
`doctor` status `ready` means selected setup checks passed, not that collection is
fresh or historically complete. Check [health semantics](data-contract.md#collection-health)
before interpreting counts or timestamps. Invalid explicit input or an incompatible
store remains an error, not permission to repair or collect.

- **Imports:** have the user select a specific compatible absolute file path. Snapshot `import-rollout` is unchanged. The separate `import-rollout-incremental --db ABS --source ID --file ABS --stream ID --source-version PINNED_COMMIT` requires every input each time; it boundedly rereads the complete prefix and defers every unterminated tail. It does not watch, schedule, discover/glob files or reopen a remembered path. Back up before its schema upgrade. Do not scan protected transcript directories or follow file paths found inside a record. See [record import](record-import.md). Do not copy private records into the install directory, support tickets, CI, or this repository
- **Trace bundles:** have the user choose one stable, closed bounded bundle directory. `import-trace-bundle --db ABS --source ID --directory ABS --source-version a956835d020762cb2b570053af06f643a11c0ecc` is a separate write with a schema-4 upgrade. Only its manifest, trace and validated referenced payloads may be read. No discovery, watcher, upstream trace activation, installation, configuration changes, subprocess or model request is included. Do not import a live-growing bundle; accepted bundle/attempt evidence is immutable. Local trace queries stay outside the Skill/MCP allowlists. See [trace import](trace-import.md)
- **Collector:** explain that it launches the user's installed Codex app-server and can initialize configured integrations, access existing authentication, and contact provider services. Get specific approval before running `collect --accept-startup-risk`. Do not log in or access authentication as a side effect of installation; do not copy, print, upload, or retain secrets
- **Content:** ask separately before enabling content capture, and explain that best-effort redaction is not a guarantee against sensitive data retention
- **Hooks/background startup:** show the exact configuration and scope first; get approval before changing client settings or enabling persistence. No registration is automatic
- **Conversation:** use the Skill + local CLI setup below to query an existing store. This does not authorize any of the collection or content operations above

## 6. Primary conversational integration: Skill + local CLI

Skip this step unless the user wants conversational queries and approves the exact
local Skill destination. No MCP setup is required. Queries need an authorized local
execution environment on the computer holding both the verified executable and the
existing database. Explain that returned aggregates will be shared with the chosen
assistant provider; the database and retained content must not be uploaded.

### Install the instruction-only Skill

The [official Skill contract](https://learn.chatgpt.com/docs/build-skills), checked
2026-10-02, documents standalone Skills in the ChatGPT desktop app, Codex CLI, and
IDE extension. For local Codex discovery, the user scope is `$HOME/.agents/skills`;
repo scope is `.agents/skills` in the selected project. The steps below use the
user scope. A managed client may restrict local Skills; verify its actual discovery
and execution support rather than inventing a registration command.

From a verified extracted release, copy only `plugin/usage-lens/skills/usage-summary`
to the chosen Skill directory. Replace `install_root` with the actual extracted
release directory. This POSIX example refuses to overwrite an existing Skill:

```sh
install_root='/ABSOLUTE/usage-lens-0.6.0'
skills_root="$HOME/.agents/skills"
destination="$skills_root/usage-summary"
if [ -e "$destination" ] || [ -L "$destination" ]; then
  printf '%s\n' 'Skill already exists; review an explicit upgrade before replacing it.' >&2
  exit 1
fi
mkdir -p "$skills_root" && cp -R "$install_root/plugin/usage-lens/skills/usage-summary" "$destination"
```

Equivalent Windows PowerShell (no administrator permissions):

```powershell
$ErrorActionPreference = 'Stop'
$installRoot = 'C:\ABSOLUTE\usage-lens-0.6.0'
$skillsRoot = Join-Path $HOME '.agents\skills'
$destination = Join-Path $skillsRoot 'usage-summary'
if (Test-Path -LiteralPath $destination) { throw 'Skill already exists; review an explicit upgrade before replacing it.' }
New-Item -ItemType Directory -Path $skillsRoot -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $installRoot 'plugin\usage-lens\skills\usage-summary') -Destination $destination -Recurse
```

Do not copy the entire plugin into a Skill directory. Its manifest and MCP/hook
examples are not needed for standalone Skill discovery. Do not edit shell profiles,
client permission settings, or MCP configuration. For a project-scoped install,
choose the project's `.agents/skills` instead, with approval; keep local database
paths and private configuration out of shared repositories.

Codex detects Skill changes automatically; if it is missing, restart the client.
In Codex CLI/IDE, inspect `/skills` or mention `$usage-summary`. In the ChatGPT
desktop app, inspect Skills and select it with `@`. Seeing the Skill listed proves
discovery only, not that this session can execute the local binary.

### Supply the three explicit query inputs

In the local client session, supply these inputs in ordinary text; they are not a
new application config file or environment-variable contract:

- Native executable: its exact verified absolute path (Windows: `usage-lens.exe`)
- Store: the exact absolute path to an existing, separately authorized Usage Lens database
- Source: the exact intended source ID, checked against `status`; ask if ambiguous

For example, after replacing all placeholders: “Use usage-summary through local
CLI at /ABSOLUTE/usage-lens, with database /ABSOLUTE/usage.sqlite and source SOURCE.
Summarize my collected usage with freshness and coverage gaps.” Do not guess these
values from unrelated files, saved credentials, transcript paths, or the install
directory. If the user wants the configuration retained in their local Skill copy,
review that exact local edit separately; never publish their paths or records.

The Skill begins with `status`, then permits only `overview`, `daily`, `quota`,
`tools`, `skill-summary`, `response-tokens`, and `health`. Prefer process argument vectors; otherwise
use correct OS/shell quoting as shown in the [package guide](../plugin/usage-lens/README.md).
Queries do not need a running dashboard, collector, or MCP server. Missing data is
reported as missing; it does not authorize collection, source creation, content
capture, or a demo fallback. The Skill's instruction allowlist is not an OS sandbox.
`skill-summary` returns the same aggregate projection as MCP `usage_skills` and
supports `--source`, optional `--max-age-ms`, paired `--from` / `--to` dates and an
optional exact `--skill` name besides the store/demo flags. Dates are `YYYY-MM-DD`,
inclusive UTC, at most 366 days. A skill-only summary is allowed, but daily trends
require both dates. MCP names these inputs `fromDate`, `toDate` and `skillName`.
Omitting date/skill filters preserves the previous response. Filtered totals and bounded
per-skill groups carry partial coverage, truncation and source-level import
warnings; unknown occurrence times are separately counted across all retained
source/skill-matching evidence and never assigned to a date range. Missing days
are unknown. The [data contract](data-contract.md#skill-evidence-summaries-and-utc-trends)
defines these scopes. There are no model, success, unique-execution or per-skill
token metrics. `health` returns selected-source collection and local-evidence aggregates, matching
MCP `usage_health`; it has source/freshness inputs, not date/model filters. `doctor`
is a separate setup diagnostic and is outside the Skill. The local `skills` evidence
command is explicitly outside the conversational allowlist; never use it as a
fallback or send its individual event/session/turn records to an assistant.

Persisted queries in v0.6.0 open the existing database read-only. They support
schema 2, 3 or 4 and rollback-journal mode; normal v0.1.0 stores remain compatible
without migration, as do v0.1.1, v0.2.0 and v0.3.0 schema-2 stores. Querying never
upgrades a store. Successful explicit incremental import can upgrade schema 2 to
3; successful explicit trace import upgrades schema 2/3 to 4. Both import workflows
are outside the Skill, as are local trace attempt/detail/summary queries. A missing, unreadable or invalid store, or an externally
WAL-converted store, returns `storage_error`; schema 0/1 or a future version returns `unsupported_schema`.
The errors intentionally omit private paths and contents. Do not infer a specific
cause from `storage_error`, and do not run a migration, SQLite/PRAGMA command, or
writable setup to make a query succeed. Any repair/upgrade is a separate reviewed
workflow. Concurrent normal Usage Lens dashboard writes use SQLite locking;
externally changing journal mode while queries run is unsupported.

For initial verification, use only the isolated synthetic CLI commands in step 4
and check Skill discovery. Test an actual database query only after its scope and
aggregate sharing are authorized. Record whether each was checked: CLI execution,
Skill discovery, source selection, and a query in the actual client. Synthetic
CLI/protocol tests alone are not a passed real-client integration test.

### Compatibility boundary

- Local-capable desktop/CLI/IDE: the documented Skill discovery route is available;
  this project has not installed or end-to-end tested those clients
- Cloud-only ChatGPT: a Skill does not grant access to the user's local executable,
  filesystem, stdio server, or loopback URL. No automatic local bridge is supplied
- Other Skill-capable clients: use that client's current official discovery and
  execution contract; this package does not claim universal installation support

Remote execution/computer access or transport requires separate authorization and
verification. Do not widen network access to make a failed local test pass.

## 7. Optional MCP, only on explicit request

Keep `examples/mcp.config.example.json` inert unless the user specifically chooses
MCP and approves the exact client configuration. Follow the
[optional MCP setup](../plugin/usage-lens/README.md#optional-mcp-explicit-opt-in)
in a client supporting local stdio MCP. The native process is
`/absolute/install/path/usage-lens mcp --db /absolute/private/path/usage.sqlite`
(Windows uses `.exe`); `command` and `args` are separate process arguments.

The eight aggregate tools, including `usage_health`, remain available, but the Skill does not start,
auto-activate, or silently fall back to them. No connector, tunnel, daemon, hook,
or collector is registered by the package manifest. MCP shares returned aggregates
with the assistant provider just as CLI queries do; it does not make collection or
local content review part of conversational setup.

## Installation report

Finish with the exact installed version/commit, path, demo result, data location if created, and every configuration changed. Clearly list anything that remains untested. Do not claim a live-account or real-client integration passed from a synthetic smoke test.

## Upgrade, rollback, and uninstall

**For v0.6.0, verify a closed-store backup before the first trace import.**
Successful explicit trace import upgrades schema 2/3 to 4; v0.5.0 and older cannot
read it. Failed validation does not migrate. Query-only use does not migrate older
stores. A rollback needs a separate compatible copy of the pre-upgrade backup;
replacing the binary does not reverse the migration. Raw trace files are separate
from the database and are not erased or sanitized by its deletion/retention.

**For v0.4.0, make and verify the backup before authorizing the first incremental
import.** A successful import upgrades schema 2 to 3 atomically. v0.3.0 and older
cannot open schema 3. A failed parse/validation does not migrate, and a read-only
v0.4.0 query works against schema 2 without upgrading. Do not downgrade the live
store or assume installing an older binary reverses the migration.

1. Read the target release's breaking-change/schema notes. Stop every server, MCP process, hook writer, and collector using this store
2. Make a verified private backup while the database is closed. Preserve the SQLite database and any remaining `-wal`/`-shm` files together, or use a documented SQLite backup operation. Do not copy only the main file from an active WAL database. Keep backups local and outside the installation directory
3. Install the new release side-by-side and repeat checksum, manifest, and synthetic-demo verification before opening existing data. Confirm the documented schema compatibility. If compatibility is unspecified, stop and ask; do not experimentally migrate the only copy
4. Keep the old executable and pre-upgrade backup. Rolling back the binary alone may fail after a schema migration. Restore a compatible **copy** of the pre-upgrade backup to a separate path; never overwrite current data or assume downgrades are supported
5. Uninstall by stopping Usage Lens and removing only its chosen versioned executable/docs directory and any exact Skill/client/hook/startup entries this installation created, with the user's permission. Keep the database and backups by default. Delete user data only after an explicit, scoped deletion request; do not use a broad home-directory cleanup command
