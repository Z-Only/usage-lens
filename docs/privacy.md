# Privacy and data boundaries

Usage Lens is designed for local inspection of explicitly connected usage sources. A local dashboard is not a guarantee that every integration remains local: account collection contacts the provider, and a query made through ChatGPT sends its returned result to ChatGPT.

## Three separate data paths

- **Collection:** an explicitly started local collector reads supported provider responses or receives supported local lifecycle events. It does not discover arbitrary files on the computer or automatically request new account permissions.
- **Local inspection:** the loopback dashboard reads the local database. Available message bodies, tool arguments/results, and supplied file contents can be retained only when content capture is enabled. File content means content present in an explicitly supplied record, not a request to crawl the filesystem.
- **Conversational queries:** Skill + local CLI is the default; explicitly opted-in MCP is optional. Both query interfaces return allowlisted usage statistics. It must not expose message bodies, tool arguments/results, file contents, arbitrary SQL, or an arbitrary file-reading endpoint. A user who requests these summaries through a remote assistant is sharing the returned summary with that assistant.

The dashboard, collector, and conversational query interfaces must report their source and coverage. A local hook does not cover cloud-orchestrated Work, an absent skill event is not proof of zero use, and account-wide token totals cannot establish an individual skill's cost.

## Skill + CLI does not grant access

Installing the Skill adds instructions, not execution permissions, collection,
automatic database discovery, or a background service. The user supplies the exact
native executable, existing database path, and intended source. A missing store or
failed query must remain an error, not trigger collection or demo substitution.
The query Skill allows only `status`, `overview`, `daily`, `quota`, `tools`, `skill-summary`,
and `response-tokens`; broader local commands stay outside that workflow.
`skill-summary` shares only aggregate counts/coverage, equivalent to MCP
`usage_skills`. The local `skills` command exposes individual evidence records and
must not be called through this Skill or used as an aggregate fallback. This
instruction boundary is not an OS sandbox for a client with general shell access.
Persisted query commands use a read-only SQLite connection and require a current
schema-2 rollback-journal store. They never create a missing database or migrate an
old schema. Errors intentionally omit database paths and contents. A failed query
must not trigger SQLite/PRAGMA commands or writable setup through the Skill.

Local execution does not make the assistant's answer local: returned aggregates
are shared with its provider. Do not upload the database or pass raw content to an
assistant. MCP is never activated automatically and needs explicit opt-in and
separate client setup. Installing a Skill on a cloud surface does not make files,
local stdio, or loopback ports on the user's computer accessible there.

## Content capture

Content capture is disabled by default for new installations. Enabling it applies to subsequently accepted records; it cannot recover earlier content that was never collected. Pausing capture stops acceptance of new observations/events, rather than presenting missing activity as zero.

Credentials, authentication cookies, and keys are not collection targets. Known secret patterns are redacted from retained content, but pattern matching cannot guarantee that arbitrary sensitive text has been removed. Company code, personal information, and confidential business content may remain. Review applicable company rules before collecting them, even on a local machine.

System/developer instruction records and hidden reasoning are outside the supported content event types. Never interpret an unsupported record as an ordinary assistant message merely to keep it.

## Local storage and retention

Treat the database and any exports or backups as sensitive files. Restrict access to your operating-system account. Local-only processing does not protect against another program running as that account, device compromise, a shared machine, or automatic backup/sync software.

Retention settings and deletion actions must show their scope before removing records. Deletion from the active database does not guarantee erasure from backups, filesystem snapshots, or previously exported copies. Changing the retention preference is not permission to delete unrelated files.

Do not include your database, account responses, logs, exported conversations, or credentials in a bug report or a pull request. Public examples and tests use synthetic records only.

## Data quality

Every number needs a meaning and a source:

- Service-reported quota percentages and reset times remain service-reported observations; they are not an exact remaining-token allowance.
- Collector timestamps identify when a response was observed, not necessarily when the provider last updated it.
- Missing, null, invalid, and reported zero values are different states.
- Demo data is isolated and labeled. It never substitutes for a failed live read.
- Requested, loaded, and invoked skills are separate evidence categories. Mere text mentions do not establish any of them.

There is no promise of complete account-wide history or complete skill attribution. Unsupported interfaces, incompatible installed versions, unavailable fields, and unverified account bindings remain visible limitations.
