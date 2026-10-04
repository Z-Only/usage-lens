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
`response-tokens`, and `health`; broader local commands stay outside that workflow.
`skill-summary` shares only aggregate counts/coverage, equivalent to MCP
`usage_skills` and local `GET /api/skill-summary`. Optional paired UTC date filters
and exact skill-name filtering do not widen that boundary. Filtered results return
bounded per-skill groups, requested/loaded/invoked totals, loaded-evidence subtypes,
daily aggregates when dates are supplied, and explicit partial/undated/truncation
warnings. They contain no individual evidence/session/turn identities, retained
bodies, import fingerprints, credentials or local paths. Skill names themselves
are returned metadata; review aggregate-sharing scope before querying a private
source through a remote assistant. Import warnings are source-level and must not
be represented as date-range or skill-specific completeness. The local `skills`
command exposes individual evidence records and
must not be called through this Skill or used as an aggregate fallback. This
instruction boundary is not an OS sandbox for a client with general shell access.
Persisted query commands use a read-only SQLite connection and accept a supported
schema-2, schema-3 or schema-4 rollback-journal store. They never create a missing database or migrate an
old schema. Errors intentionally omit database paths and contents. A failed query
must not trigger SQLite/PRAGMA commands or writable setup through the Skill.
`health` and optional MCP `usage_health` return selected-source aggregate counts,
time bounds, collection states and safe failure metadata; no retained bodies,
record identities, file fingerprints, credentials or local paths are returned.
`doctor` is a separate setup diagnostic outside the Skill. It checks the running
binary/embedded dashboard and an optional explicitly selected read-only database,
without configuration discovery, subprocesses, network access or repair.

Local execution does not make the assistant's answer local: returned aggregates
are shared with its provider. Do not upload the database or pass raw content to an
assistant. MCP is never activated automatically and needs explicit opt-in and
separate client setup. Installing a Skill on a cloud surface does not make files,
local stdio, or loopback ports on the user's computer accessible there.

## Content capture

Content capture is disabled by default for new installations. Enabling it applies to subsequently accepted records; it cannot recover earlier content that was never collected. Pausing capture stops acceptance of new observations/events, rather than presenting missing activity as zero.

Credentials, authentication cookies, and keys are not collection targets. Known secret patterns are redacted from retained content, but pattern matching cannot guarantee that arbitrary sensitive text has been removed. Company code, personal information, and confidential business content may remain. Review applicable company rules before collecting them, even on a local machine.

System/developer instruction records and hidden reasoning are outside the supported content event types. Never interpret an unsupported record as an ordinary assistant message merely to keep it.

## Explicit trace bundles

The separate [trace import](trace-import.md) reads only one user-selected bounded
bundle and validated payload references inside it. It does not discover traces,
watch a directory, enable upstream recording, modify configuration, install a
client, launch another process or make a model request. Raw files may contain
sensitive prompts, instructions, hidden reasoning, tool data and paths; they remain
untouched by Usage Lens, including after local retention or deletion.

Only an explicitly enabled redacted visible-text projection may be retained.
System/developer instructions, hidden reasoning, excluded injected instructions
and recognized credentials remain excluded; uncertain request user content is
not assumed human-authored. Unknown assistant phases and nonempty/invalid typed
assistant content classifications are excluded. Only supported visible text is
projected; tool arguments/results and attachments are excluded from trace content.
The entire raw request/response JSON is never retained
as a fallback. Redaction is best-effort and cannot make a private store safe to
publish. Enabling capture later or reimporting does not restore missing/deleted
content. Retention, content-only deletion and explicit all-data deletion preserve
trace import/replay protection. All-data deletion removes retained trace evidence
but reimport cannot resurrect it; a new source is a separate explicit namespace.

`import-trace-bundle --dry-run` reads the same explicitly selected bounded bundle
and existing database with the same confinement and validation, without writing,
migrating, retaining content or making a backup. Its safe counts and predictions
exclude raw content, IDs and paths; later input/store/setting changes can invalidate
it. Exact local thread/status/requested-model/effort/tier filters do not widen
content sharing. `doctor` compatibility/backup guidance reads no extra files or
processes and never claims a verified backup or live desktop compatibility.

Local `trace-attempts`, `trace-detail` and `trace-summary` are outside the Skill
and MCP's unchanged eight aggregate commands/tools. Request settings and response
completion evidence remain distinct. A prepared request is not transmission,
delivery or billing proof; raw diagnostic files do not establish complete history.

**Back up the closed store before its first successful explicit trace import.**
It upgrades schema 2/3 to schema 4 atomically. v0.5.0 and older cannot read schema 4;
read-only queries in v0.7.0 also support schema 2/3 without automatic migration.
Latest Mac app/Local mode does not establish runtime or environment compatibility;
live desktop capture validation remains pending separate user authorization.

## Local storage and retention

Incremental import is a separate explicit write, outside the conversational Skill
and MCP tools. The caller supplies the file path, source, logical stream ID and
version each time. It never stores a path to reopen, discovers files, schedules
work or installs a watcher. Its checkpoint contains complete byte/line counts, a
SHA-256 prefix digest and parser/source versions, plus a source-wide hashed
identity ledger. Pending tails, pending call bodies and raw parse state are not
persisted. Ordinary opt-in retained content remains subject to the rules above.

Retention and content-only deletion preserve this replay metadata so old prefixes
cannot restore deleted evidence/content or backfill content after capture is
enabled. Explicit delete-all, including source-scoped delete-all, resets the
corresponding replay/checkpoint metadata. Schema 3 cannot reconstruct history
already deleted before upgrade, and no cross-mode anonymous-record deduplication
is claimed. Hashes and metadata are not anonymization guarantees; treat the whole
store as private. See the [incremental contract](record-import.md#incremental-one-shot-workflow).

**Make a verified private backup of the closed store before its first incremental
import.** The first successful incremental transaction upgrades to schema 3,
which v0.3.0 and older cannot read. Query-only use does not migrate schema 2.

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
