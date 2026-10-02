# AI-assisted installation: Usage Lens

This is the installation contract for an assistant acting **with the user's permission**. It is also a manual installation guide. A repository file cannot grant permission to inspect private records, use credentials, change security settings, or enable background collection.

中文：优先安装 GitHub Release 中与你的系统和 CPU 对应的原生程序；先校验 SHA-256，再解压和运行。无需安装 Node、Bun 或 Rust。先用合成演示数据验证；读取真实记录、访问已有账号、启用采集或修改客户端配置前，必须取得用户明确同意。不要复制密钥或扫描会话目录。

## 1. Establish scope before changing anything

Ask which computer to install on if unclear. Inspect only the selected computer's OS, CPU architecture, available disk space, and intended installation directory. Do not inspect the user's accounts, transcript directories, keychain, `.env` files, browser profiles, or existing databases to “discover” configuration.

Record a short install plan: exact version, repository, asset, installation directory, database location (if later authorized), and whether any client integration is requested. Installing the executable does not authorize running a collector, importing records, content capture, hooks, login, tunneling, startup registration, or sharing aggregate results with an assistant provider.

## 2. Choose one exact published asset

Canonical repository: [Z-Only/usage-lens](https://github.com/Z-Only/usage-lens)

Open [published releases](https://github.com/Z-Only/usage-lens/releases), read the chosen release notes, and pin the exact stable version. `0.1.0` below is an example, not a claim that the release already exists. If its assets have not been published, stop; never substitute GitHub's automatically generated source archives for an executable release.

| Detected system | CPU | Asset for version `0.1.0` |
| --- | --- | --- |
| Linux x64 (static musl) | x86_64 | `usage-lens-0.1.0-linux-x64.tar.gz` |
| macOS 11.0 deployment target; tested on 15 | arm64 / Apple Silicon | `usage-lens-0.1.0-macos-arm64.tar.gz` |
| macOS 11.0 deployment target; tested on 15 | x86_64 / Intel | `usage-lens-0.1.0-macos-x64.tar.gz` |
| Windows x64 | AMD64 | `usage-lens-0.1.0-windows-x64.tar.gz` |

The matching sidecar replaces `.tar.gz` with `.manifest.json`. Also download `SHA256SUMS` and `release.json` from **the same tag's release**. Assets include a native Rust executable, embedded Leptos UI, bundled SQLite, license, third-party license/notice bundles, documentation, and inert plugin examples. No Node, Bun, Rust, npm, compiler, or package installation is required on the user's computer. The Linux executable is statically linked with musl; it does not require a particular glibc version. Native OS/kernel compatibility still matters.

Linux uses a static musl target and is smoke-tested on Ubuntu 24.04. Older kernels, CentOS 7, and Alpine are not claimed compatible without their own tests. Linux ARM, Windows ARM, 32-bit platforms, and macOS below 11.0 are not published targets. macOS 11–14 runtime behavior remains unverified; the compiler deployment target is not evidence of testing those OS versions. macOS architecture means the actual machine, not an emulated shell: on Apple Silicon with a Rosetta shell, inspect `sysctl -n hw.optional.arm64` and choose ARM64. On Windows inspect `[System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture`. Use `uname -s` and `uname -m` on Linux. Do not silently install an emulated or different-architecture build.

Windows builds use the static MSVC runtime. GitHub tests them on Windows Server 2025; Windows desktop versions require their own verification. A normal browser with JavaScript and WebAssembly support is needed for the dashboard; CLI/MCP do not need a browser. These first-release assets are not OS code-signed/notarized. If Gatekeeper, SmartScreen, antivirus, or a browser security warning blocks them, report the exact warning and hand control to the user. Do not remove quarantine, disable security, or bypass a warning.

## 3. Download, verify, and inspect before execution

Use the browser or an ordinary HTTPS download tool. Never use `curl | sh`, run an installer fetched from another site, or install packages to make a checksum command work. Example URL pattern:

```text
https://github.com/Z-Only/usage-lens/releases/download/v0.1.0/usage-lens-0.1.0-macos-arm64.tar.gz
```

In a new temporary download directory, the following POSIX example downloads only public release files. Set `platform` from the table, and set `version` to the version the user selected:

```sh
version=0.1.0
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

A convenient final directory is `~/.local/share/usage-lens/0.1.0/` on macOS/Linux or `%LOCALAPPDATA%\UsageLens\0.1.0\` on Windows. Do not require administrator access. Leave previous versions in their own directories. Do not edit PATH or add a startup service unless separately requested.

## 4. Verify the isolated synthetic demo

From the extracted directory, run only these local commands first:

```sh
./usage-lens --help
./usage-lens status --demo
./usage-lens serve --demo --port 4319
```

On PowerShell replace `./usage-lens` with `& .\usage-lens.exe`. `--help` should show the selected version. `status --demo` should report synthetic sources, never the user's account. Visit the printed `http://127.0.0.1:4319` URL **on that same computer**, verify the dashboard and its synthetic labels, and optionally read `/api/status`. The demo uses an in-memory database; it does not launch Codex, log in, inspect account credentials, or run a model. Stop it with Ctrl+C. If the port is in use, choose another local port or `--port 0` and use the printed URL.

Do not bind to `0.0.0.0`, expose the port, open a firewall rule, or establish a tunnel. If a browser on another computer cannot reach loopback, explain that boundary instead of widening network access.

## 5. Configure persistent data only after approval

Explain the intended absolute database path, what will be retained, and which operation the user is authorizing. Keep data outside the executable directory, cloud-synced folders, and repositories. The default is no content capture. Create an explicit imported source only when requested:

```sh
./usage-lens source --db /absolute/private/path/usage.sqlite \
  --source local-records --mode imported --name "Local records"
./usage-lens serve --db /absolute/private/path/usage.sqlite
```

Use the platform's quoting rules; do not literally use the example path. A persistent source starts empty. Data appears only after a separately authorized import, hook, or collector. Never fill an empty real view with demo data.

- **Imports:** have the user select a specific compatible file. Do not scan protected transcript directories or follow file paths found inside a record. See [record import](record-import.md). Do not copy private records into the install directory, support tickets, CI, or this repository
- **Collector:** explain that it launches the user's installed Codex app-server and can initialize configured integrations, access existing authentication, and contact provider services. Get specific approval before running `collect --accept-startup-risk`. Do not log in or access authentication as a side effect of installation; do not copy, print, upload, or retain secrets
- **Content:** ask separately before enabling content capture, and explain that best-effort redaction is not a guarantee against sensitive data retention
- **Hooks/background startup:** show the exact configuration and scope first; get approval before changing client settings or enabling persistence. No registration is automatic
- **MCP/plugin:** the native command is `/absolute/install/path/usage-lens mcp --db /absolute/private/path/usage.sqlite` (Windows uses `.exe`). Review the inert [plugin examples](../plugin/usage-lens/README.md), use the native executable path, and request approval before editing client configuration. The seven tools expose aggregates only. Their results are shared with the assistant provider when requested. Cloud ChatGPT cannot directly reach a local stdio process or a loopback URL; remote integration needs separate authorized setup and testing

Finish with the exact installed version/commit, path, demo result, data location if created, and every configuration changed. Clearly list anything that remains untested. Do not claim a live-account or real-client integration passed from a synthetic smoke test.

## Upgrade, rollback, and uninstall

1. Read the target release's breaking-change/schema notes. Stop every server, MCP process, hook writer, and collector using this store
2. Make a verified private backup while the database is closed. Preserve the SQLite database and any remaining `-wal`/`-shm` files together, or use a documented SQLite backup operation. Do not copy only the main file from an active WAL database. Keep backups local and outside the installation directory
3. Install the new release side-by-side and repeat checksum, manifest, and synthetic-demo verification before opening existing data. Confirm the documented schema compatibility. If compatibility is unspecified, stop and ask; do not experimentally migrate the only copy
4. Keep the old executable and pre-upgrade backup. Rolling back the binary alone may fail after a schema migration. Restore a compatible **copy** of the pre-upgrade backup to a separate path; never overwrite current data or assume downgrades are supported
5. Uninstall by stopping Usage Lens and removing only its chosen versioned executable/docs directory and any exact client/hook/startup entries this installation created, with the user's permission. Keep the database and backups by default. Delete user data only after an explicit, scoped deletion request; do not use a broad home-directory cleanup command
