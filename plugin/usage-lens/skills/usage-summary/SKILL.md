---
name: usage-summary
description: Answer questions about locally collected Usage Lens aggregate activity, quota windows, tools and direct skill evidence. Requires a separately configured local Usage Lens CLI or stdio MCP server.
---

# Usage Lens aggregate queries

Use the configured Usage Lens read-only MCP tools when available: usage_status,
usage_overview, usage_daily, usage_quota, usage_tools, usage_skills,
usage_response_tokens. The tools never
refresh collection. Start with usage_status and select the user's intended source.
Never merge sources or interpret a demo source as the user's real account.

If the user has already configured local execution and provides the exact built CLI
location and local database, the same aggregate queries are available via:

- node /ABSOLUTE/usage-lens/dist/cli/main.js status --db /ABSOLUTE/usage.sqlite
- node /ABSOLUTE/usage-lens/dist/cli/main.js overview --db /ABSOLUTE/usage.sqlite --source SOURCE
- node /ABSOLUTE/usage-lens/dist/cli/main.js daily --db /ABSOLUTE/usage.sqlite --source SOURCE --from YYYY-MM-DD --to YYYY-MM-DD
- node /ABSOLUTE/usage-lens/dist/cli/main.js quota --db /ABSOLUTE/usage.sqlite --source SOURCE
- node /ABSOLUTE/usage-lens/dist/cli/main.js tools --db /ABSOLUTE/usage.sqlite --source SOURCE

Use a subprocess argument vector without a shell where possible; otherwise correctly
quote each user-supplied path. Do not guess paths, inspect local transcripts, collect
credentials, create authentication, enable capture, import records, or launch the
collector as part of answering a query. Ask for missing configuration.

Never run detail, events, import, import-rollout, hook, settings, delete, retention, source, collect,
or serve through this skill. These are separate local workflows. Never transmit raw
bodies, prompts, tool arguments/results, titles, or file content to a model through
this plugin. Use the local dashboard for content review. The plugin must never call the local-only detail command.

In the answer preserve source mode, coverage, freshness, nulls and warnings. Counts
are decimal strings and may exceed JavaScript's safe integer range. Daily buckets
are the source's date labels with unknown timezone; missing days are unknown, not
zero. Repeated snapshots are not additive. Do not infer costs, remaining tokens,
per-model account tokens, or access recovery from quota percentages.

Skill requested, loaded, and invoked are separate direct-evidence categories.
There is no complete historical skill-use endpoint or automatic skill detector in
the shipped hooks. Skill statistics come from explicitly supplied evidence bundles or recognized,
version-pinned local rollout records. Successful structured skill reads establish
loaded evidence only; they do not establish invocation.
The local hooks cover only future supported local events, not all ChatGPT activity.

Cloud ChatGPT cannot directly reach this local stdio service. Any remote tunnel or
client setup requires separate user authorization and verification. Local package
and synthetic protocol tests are not proof of a tested ChatGPT client integration.
