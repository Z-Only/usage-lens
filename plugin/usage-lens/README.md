# Usage Lens: Skill + local CLI

The default conversational integration is **Skill + local CLI**. The `usage-summary`
skill queries an already configured native executable and local database directly.
MCP is optional; neither a server nor the dashboard has to run for CLI queries.
This package includes the skill, an inert plugin manifest, and optional examples.
No marketplace, client settings, hook registration, authentication, or tunnel is
created by building or validating it. Real-client installation and live-account
integration have not been tested.

## Primary setup: install the Skill, configure the CLI

1. Install a verified native release or build the parent project. Installed releases
   do not need Node.js, Python, or Rust for queries
2. With approval, copy `skills/usage-summary` into the client's documented local
   skill directory. The [installation guide](../../docs/AI_INSTALL.md#6-primary-conversational-integration-skill--local-cli)
   gives concrete POSIX/PowerShell copy commands and the official discovery contract
3. In the authorized local session, supply the exact absolute executable path,
   absolute existing database path, and intended source ID. These are explicit
   session inputs, not automatically discovered settings. Review `status` to confirm
   the source; ask the user if its identity is ambiguous
4. Use `usage-summary` to request a summary. Its CLI allowlist is `status`,
   `overview`, `daily`, `quota`, `tools`, `skill-summary`, and `response-tokens`. It does not
   collect, import, inspect content, or fix a missing store by creating one

`skill-summary` returns aggregate skill counts and coverage, matching MCP
`usage_skills`. It supports `--source` and optional `--max-age-ms`, not date/model
filters. The existing `skills` command returns local evidence records and is
excluded from conversational queries.

Example POSIX query after replacing the reviewed placeholders:

```sh
'/ABSOLUTE/usage-lens' status --db '/ABSOLUTE/usage.sqlite'
'/ABSOLUTE/usage-lens' skill-summary --db '/ABSOLUTE/usage.sqlite' --source 'SOURCE'
```

Windows PowerShell:

```powershell
& 'C:\ABSOLUTE\usage-lens.exe' status --db 'C:\ABSOLUTE\usage.sqlite'
& 'C:\ABSOLUTE\usage-lens.exe' skill-summary --db 'C:\ABSOLUTE\usage.sqlite' --source 'SOURCE'
```

Prefer a subprocess executable/argument vector without a shell; otherwise quote
paths and values for the actual host shell. Installing the instruction-only Skill
does not grant local execution permissions. The local database must already have
been created and populated through separately authorized workflows. The dashboard,
hooks, imports, and collectors remain independent of conversational setup.
Persisted queries require a schema-2 rollback-journal store; normal v0.1.0 stores
remain compatible. They fail rather than create or migrate a database. See the
[query compatibility notes](../../docs/AI_INSTALL.md#supply-the-three-explicit-query-inputs)
for safe handling of `storage_error` and `unsupported_schema`.

CLI queries share their returned aggregates with the assistant provider, just as
MCP queries do. The skill must not send retained content or the database. Its
allowlist guides the assistant; it is not a sandbox restricting a host's broader
shell permissions. Use the local dashboard for content review.

## Optional MCP: explicit opt-in

Only if the user chooses MCP, review `examples/mcp.config.example.json`, replace its
`command` with the absolute native executable path and its database argument with
the chosen absolute existing database path, and separately approve client setup.
Use a client that supports local stdio MCP. Do not configure or start MCP merely
because the Skill was installed, and do not switch to it automatically if CLI
execution is unavailable. The seven tools query the same existing aggregates;
they never launch Codex, refresh data, or expose stored content.

Cloud-only ChatGPT cannot directly run a local CLI or connect to local stdio or
127.0.0.1. A Skill/plugin can be available in a client without having access to the
machine holding the database. Remote transport or computer access needs separately
authorized setup and its own real-client test; no such bridge is installed here.

## Optional hooks: separate collection setup

The optional `examples/hooks.config.example.json` follows the documented local hook
shape at https://learn.chatgpt.com/docs/hooks. Before configuring, create an explicit
source with the native binary. For a POSIX shell on macOS/Linux:

```sh
'/ABSOLUTE/usage-lens' source --db '/ABSOLUTE/usage.sqlite' --source local-hooks --mode imported
```

For Windows PowerShell, include the call operator before a quoted executable path:

```powershell
& 'C:\ABSOLUTE\usage-lens.exe' source --db 'C:\ABSOLUTE\usage.sqlite' --source local-hooks --mode imported
```

Use absolute quoted paths and review the exact config with the user. No hooks are
activated by this package manifest.

## Optional MCP and hook path handling

MCP `command` and `args` are separate process arguments, not a shell command. Paths
with spaces remain single arguments; do not add shell quotes inside their JSON
strings. For example, a Windows client can use:

```json
{
  "command": "C:\\ABSOLUTE\\usage-lens.exe",
  "args": ["mcp", "--db", "C:\\ABSOLUTE\\usage.sqlite"]
}
```

The supplied hooks example contains POSIX-shell command strings. Its single quotes
keep spaces and shell metacharacters in literal path arguments. Do not paste those
strings unchanged into a Windows hook host. Confirm which shell the host uses:

- PowerShell command string: `& 'C:\ABSOLUTE\usage-lens.exe' hook --db 'C:\ABSOLUTE\usage.sqlite' --source local-hooks`
- `cmd.exe` command string: `"C:\ABSOLUTE\usage-lens.exe" hook --db "C:\ABSOLUTE\usage.sqlite" --source local-hooks`

The documented `commandWindows` override may carry the reviewed Windows command
where the client supports it. JSON requires each backslash to be escaped as `\\`;
JSON-escape any double quotes in a command string too. POSIX/PowerShell paths that
contain an apostrophe need their shell's literal-quote escaping, and `cmd.exe` paths
containing `%` or `!` need particular care with expansion settings. Prefer simple
installation/database paths for string-based hooks, and never construct a hook
command from event payload fields. Client-specific shell behavior still requires a
separate integration test before enabling hooks.

## Hook coverage and local content

Hooks capture only future PostToolUse, UserPromptSubmit and Stop events. Hosted tools
and special paths may not emit hooks. Stop is only the reported last visible assistant
message, not every assistant message. Missing source event timestamps remain null;
observedAt is local receipt time. Tool-call dedup keys are scoped to session and turn.
Capture failures or missing hook invocations mean gaps; no complete-history claim is
made. Paused capture exits successfully without output. Hook success never prints
content to stdout. An invalid input reports a fixed safe error, never its content.

Content capture defaults off. Enabling it locally retains supported visible prompt,
last-assistant, and tool input/output fields only in the local store. The redactor is
best-effort; arbitrary free text may still contain sensitive data. No credentials,
transcript paths, cwd, hidden reasoning, or system/developer content are collection
targets. These hooks do not infer skill use from names or text. Direct skill evidence
requires an explicitly prepared Usage Lens v1 import bundle or recognized version-pinned
local rollout records supplied through import-rollout. Neither route scans directories
or imports ordinary ChatGPT exports.
