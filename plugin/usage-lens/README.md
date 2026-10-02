# Usage Lens local plugin package

This package contains an aggregate-query skill and inert configuration examples.
It has not been installed, connected to a real account, or tested in a ChatGPT client.
No marketplace, client settings, hook registration, authentication, or tunnel is
created by building or validating this directory.

Build the parent project first. Review `examples/mcp.config.example.json`, replace
both absolute paths with your chosen local build and database, and configure it only
in a client that supports local stdio MCP. The seven tools only query already-collected
aggregates. They never launch Codex, refresh data, or expose stored content. Querying
through an assistant shares those aggregate results with that assistant provider.
A cloud ChatGPT client cannot directly connect to local stdio or 127.0.0.1; remote
transport needs separately authorized setup and its own integration test.

The optional `examples/hooks.config.example.json` follows the documented local hook
shape at https://learn.chatgpt.com/docs/hooks. Before configuring, create an explicit
source with `node /ABSOLUTE/usage-lens/dist/cli/main.js source --db /ABSOLUTE/usage.sqlite
--source local-hooks --mode imported`. Use absolute quoted paths and review the exact
config with the user. No hooks are activated by this package manifest.

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
