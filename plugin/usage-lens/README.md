# Usage Lens local plugin package

This package contains an aggregate-query skill and inert configuration examples.
It has not been installed, connected to a real account, or tested in a ChatGPT client.
No marketplace, client settings, hook registration, authentication, or tunnel is
created by building or validating this directory.

Use a verified native release executable or build the parent project first; installed
releases do not need Node.js. Review `examples/mcp.config.example.json`, replace its
`command` with the absolute native executable path and the database argument with
your chosen absolute database path, and configure it only
in a client that supports local stdio MCP. The seven tools only query already-collected
aggregates. They never launch Codex, refresh data, or expose stored content. Querying
through an assistant shares those aggregate results with that assistant provider.
A cloud ChatGPT client cannot directly connect to local stdio or 127.0.0.1; remote
transport needs separately authorized setup and its own integration test.

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

## Path and shell handling

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
