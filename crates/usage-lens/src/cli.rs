use crate::{
    adapters::{
        AdapterError, collect::collect_account, demo::seed_demo, hooks::normalize_hook, now_iso,
        rollout,
    },
    core::{CoreError, UsageStore},
    server::{http, mcp},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const HELP: &str = concat!(
    "Usage Lens ",
    env!("CARGO_PKG_VERSION"),
    " — local evidence, never complete account history\nusage-lens COMMAND --db /absolute/path/usage.sqlite [options]\nusage-lens serve --demo [--port 4319] [--host 127.0.0.1]\nCommands: doctor, health, status, overview, daily, quota, history, events, skill-summary, skills, tools, response-tokens, detail,\n          source, settings, delete, retention, import, import-rollout, import-rollout-incremental, import-trace-bundle, trace-attempts, trace-detail, trace-summary, collect, hook, serve, mcp\nDoctor: [--db /absolute/existing.sqlite [--source ID [--max-age-ms N]]]\n  Read-only setup checks; no scanning, installation, startup, or collection.\nHealth: --source ID [--max-age-ms N]; source evidence and collection freshness only\nQueries: --source ID; daily also --from YYYY-MM-DD --to YYYY-MM-DD\nSkill summary: aggregate counts only; --source ID [--max-age-ms N] [--from YYYY-MM-DD --to YYYY-MM-DD] [--skill EXACT_NAME]\n  Skill trends use UTC occurrence dates, at most 366 inclusive days; missing days are unknown.\nEvents/skills/tools: optional --from --to --model; events/skills/history --limit --cursor\nDetail: --source ID --event ID (local content only, never use from a plugin)\nSource: --source ID --mode imported|live --name NAME\nSettings: --pause true|false --content true|false --retention-days 1..3650\nDelete: --target all|content --confirm DELETE [--source ID]\nRetention: --confirm APPLY_RETENTION\nImport: --source ID --file /absolute/path/bundle.json (Usage Lens v1 bundle only)\nImport rollout: --source ID --file /absolute/file.jsonl --source-version PINNED_COMMIT\n  Explicit supplied local Codex records only; no scans, ordinary Chat export, or cumulative token summation.\nIncremental rollout: --source ID --file /absolute/file.jsonl --stream ID --source-version PINNED_COMMIT\n  One explicit bounded read; complete lines only, no watching. Back up schema-2 databases before upgrade.\nTrace bundle: --source ID --directory /absolute/bundle --source-version PINNED_COMMIT [--dry-run]\n  --dry-run requires an existing database and predicts acceptance read-only, without creating or migrating.\n  Explicit immutable Rollout Trace bundle only; prepared requests are not delivery proof.\nTrace reader: trace-attempts --source ID [--from DATE --to DATE] [--limit N --cursor VALUE]\n  trace-detail --source ID --attempt ID (local redacted projection); trace-summary --source ID [--from DATE --to DATE]\n  Trace filters: --thread ID --status completed|failed|cancelled|incomplete\n  --requested-model VALUE --requested-effort VALUE --requested-tier VALUE (exact recorded request values)\n  List and summary share filters; cursors are bound to the source and submitted filters.\nCollect: --source ID --accept-startup-risk\n  Starts installed codex app-server; local configuration/plugins/credentials may\n  initialize or refresh, and Codex services may be contacted. No login is created.\nHook: --source ID; one documented JSON hook event from stdin; no stdout on success\nMCP: read-only stdio aggregates, no content, no collection, no automatic tunnel\n--demo uses synthetic data in an isolated in-memory store; never combine with --db\n"
);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arguments {
    pub command: String,
    pub flags: BTreeMap<String, String>,
}
pub fn parse_arguments(argv: &[String]) -> Result<Arguments, AdapterError> {
    let Some(command) = argv.first() else {
        return Ok(Arguments {
            command: "help".into(),
            flags: BTreeMap::new(),
        });
    };
    if command == "help" || command == "--help" {
        return Ok(Arguments {
            command: "help".into(),
            flags: BTreeMap::new(),
        });
    }
    let allowed: &[&str] = match command.as_str() {
        "status" | "mcp" => &[],
        "doctor" | "health" => &["source", "max-age-ms"],
        "overview" | "quota" => &["source", "max-age-ms"],
        "skill-summary" => &["source", "max-age-ms", "from", "to", "skill"],
        "daily" => &["source", "from", "to", "max-age-ms"],
        "response-tokens" | "tools" => &["source", "from", "to", "model"],
        "history" => &["source", "max-age-ms", "cursor", "limit"],
        "events" => &[
            "source",
            "from",
            "to",
            "event-type",
            "model",
            "cursor",
            "limit",
        ],
        "skills" => &["source", "from", "to", "kind", "model", "cursor", "limit"],
        "detail" => &["source", "event"],
        "trace-detail" => &["source", "attempt"],
        "trace-attempts" => &[
            "source",
            "from",
            "to",
            "limit",
            "cursor",
            "thread",
            "status",
            "requested-model",
            "requested-effort",
            "requested-tier",
        ],
        "trace-summary" => &[
            "source",
            "from",
            "to",
            "thread",
            "status",
            "requested-model",
            "requested-effort",
            "requested-tier",
        ],
        "import-trace-bundle" => &["source", "directory", "source-version", "dry-run"],
        "source" => &["source", "mode", "name"],
        "settings" => &["pause", "content", "retention-days"],
        "delete" => &["source", "target", "confirm"],
        "retention" => &["confirm"],
        "import" => &["source", "file"],
        "import-rollout" => &["source", "file", "source-version"],
        "import-rollout-incremental" => &["source", "file", "stream", "source-version"],
        "collect" => &["source", "accept-startup-risk"],
        "hook" => &["source"],
        "serve" => &["host", "port"],
        _ => return Err(AdapterError("unknown_command")),
    };
    let mut flags = BTreeMap::new();
    let mut index = 1;
    while index < argv.len() {
        let arg = &argv[index];
        index += 1;
        let key = arg
            .strip_prefix("--")
            .ok_or(AdapterError("invalid_argument"))?;
        if arg.len() > 256
            || (!allowed.contains(&key) && !["db", "demo", "help"].contains(&key))
            || flags.contains_key(key)
        {
            return Err(AdapterError("invalid_argument"));
        }
        let value = if ["demo", "accept-startup-risk", "help", "dry-run"].contains(&key) {
            "true".to_owned()
        } else {
            let value = argv.get(index).ok_or(AdapterError("missing_argument"))?;
            index += 1;
            if value.is_empty()
                || (value.starts_with("--")
                    && ![
                        "skill",
                        "requested-model",
                        "requested-effort",
                        "requested-tier",
                    ]
                    .contains(&key))
                || value.len() > 4096
            {
                return Err(AdapterError("missing_argument"));
            }
            value.clone()
        };
        flags.insert(key.to_owned(), value);
    }
    Ok(Arguments {
        command: if flags.contains_key("help") {
            "help".into()
        } else {
            command.clone()
        },
        flags,
    })
}
fn required<'a>(flags: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, AdapterError> {
    flags
        .get(key)
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .ok_or(AdapterError("missing_argument"))
}
fn integer(value: &str) -> Result<u64, AdapterError> {
    if value.is_empty() || value.len() > 10 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AdapterError("invalid_argument"));
    }
    value.parse().map_err(|_| AdapterError("invalid_argument"))
}
fn boolean(value: &str) -> Result<bool, AdapterError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(AdapterError("invalid_argument")),
    }
}
impl From<CoreError> for AdapterError {
    fn from(error: CoreError) -> Self {
        Self(error.code())
    }
}
impl From<http::HttpError> for AdapterError {
    fn from(error: http::HttpError) -> Self {
        Self(error.code)
    }
}
async fn emit(output: &mut (dyn AsyncWrite + Unpin), value: &Value) -> Result<(), AdapterError> {
    let text = serde_json::to_string_pretty(value).map_err(|_| AdapterError("operation_failed"))?;
    output
        .write_all(format!("{text}\n").as_bytes())
        .await
        .map_err(|_| AdapterError("output_failed"))?;
    output
        .flush()
        .await
        .map_err(|_| AdapterError("output_failed"))
}
/// Reads an explicit ordinary file through its opened descriptor, bounded even if it grows.
pub async fn read_bounded_file(filename: &Path, max_bytes: usize) -> Result<Vec<u8>, AdapterError> {
    let mut options = tokio::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        // Open a reparse point itself, never its target. File metadata below rejects it.
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x00200000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(filename)
        .await
        .map_err(|_| AdapterError("import_file_required"))?;
    let info = file
        .metadata()
        .await
        .map_err(|_| AdapterError("import_file_required"))?;
    if !info.is_file() {
        return Err(AdapterError("import_file_required"));
    }
    if info.len() > max_bytes as u64 {
        return Err(AdapterError("import_too_large"));
    }
    let mut data = Vec::new();
    file.take(max_bytes as u64 + 1)
        .read_to_end(&mut data)
        .await
        .map_err(|_| AdapterError("import_read_failed"))?;
    if data.len() > max_bytes {
        return Err(AdapterError("import_too_large"));
    }
    Ok(data)
}
async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut term) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}};
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
pub async fn run_main<R: AsyncRead + Unpin, W: AsyncWrite + Unpin, E: AsyncWrite + Unpin>(
    argv: &[String],
    mut input: R,
    mut output: W,
    mut error_output: E,
) -> i32 {
    let result = execute(argv, &mut input, &mut output).await;
    match result {
        Ok(()) => 0,
        Err(error) => {
            let _ = error_output
                .write_all(format!("Usage Lens: {}\n", error.0).as_bytes())
                .await;
            let _ = error_output.flush().await;
            1
        }
    }
}
async fn execute(
    argv: &[String],
    input: &mut (dyn AsyncRead + Unpin),
    output: &mut (dyn AsyncWrite + Unpin),
) -> Result<(), AdapterError> {
    let Arguments { command, flags } = parse_arguments(argv)?;
    if command == "help" {
        output
            .write_all(HELP.as_bytes())
            .await
            .map_err(|_| AdapterError("output_failed"))?;
        output
            .flush()
            .await
            .map_err(|_| AdapterError("output_failed"))?;
        return Ok(());
    }
    let demo = flags.contains_key("demo");
    if demo && flags.contains_key("db") {
        return Err(AdapterError("demo_database_must_be_isolated"));
    }
    if demo
        && [
            "collect",
            "hook",
            "source",
            "import",
            "import-rollout",
            "import-rollout-incremental",
            "import-trace-bundle",
        ]
        .contains(&command.as_str())
    {
        return Err(AdapterError("demo_command_not_allowed"));
    }
    if command == "doctor" {
        if demo {
            return Err(AdapterError("demo_command_not_allowed"));
        }
        let database = flags.get("db").map(Path::new);
        let source = flags.get("source").map(String::as_str);
        let age = flags
            .get("max-age-ms")
            .map(|value| integer(value))
            .transpose()?;
        if database.is_some_and(|path| !path.is_absolute()) {
            return Err(AdapterError("absolute_database_path_required"));
        }
        if (source.is_some() && database.is_none()) || (age.is_some() && source.is_none()) {
            return Err(AdapterError("missing_argument"));
        }
        if age.is_some_and(|value| value > 2592000000) {
            return Err(AdapterError("invalid_argument"));
        }
        let report = crate::doctor::report(database, source, age);
        emit(output, &report).await?;
        return if report["status"] == "failed" {
            Err(AdapterError("doctor_checks_failed"))
        } else {
            Ok(())
        };
    }
    let store = if demo {
        UsageStore::in_memory()?
    } else {
        let filename = required(&flags, "db")?;
        if filename == ":memory:" {
            return Err(AdapterError("explicit_database_path_required"));
        }
        if !Path::new(filename).is_absolute() {
            return Err(AdapterError("absolute_database_path_required"));
        }
        let read_only = matches!(
            command.as_str(),
            "status"
                | "health"
                | "overview"
                | "daily"
                | "quota"
                | "history"
                | "events"
                | "skills"
                | "skill-summary"
                | "tools"
                | "response-tokens"
                | "detail"
                | "trace-detail"
                | "trace-attempts"
                | "trace-summary"
                | "mcp"
        ) || (command == "import-trace-bundle" && flags.contains_key("dry-run"))
            || (command == "settings"
                && !["pause", "content", "retention-days"]
                    .iter()
                    .any(|key| flags.contains_key(*key)));
        if read_only {
            UsageStore::open_read_only(filename)?
        } else {
            UsageStore::open(filename)?
        }
    };
    if demo {
        seed_demo(&store, chrono::Utc::now().timestamp_millis())?;
    }
    if command == "serve" {
        http::validate_host(flags.get("host").map(String::as_str))?;
        let port = match flags.get("port") {
            Some(value) => {
                u16::try_from(integer(value)?).map_err(|_| AdapterError("invalid_port"))?
            }
            None => 4319,
        };
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|_| AdapterError("server_start_failed"))?;
        let port = listener
            .local_addr()
            .map_err(|_| AdapterError("server_start_failed"))?
            .port();
        output
            .write_all(
                format!(
                    "Usage Lens{}: {}\n",
                    if demo {
                        " DEMO — synthetic data only"
                    } else {
                        ""
                    },
                    http::canonical_origin(port)
                )
                .as_bytes(),
            )
            .await
            .map_err(|_| AdapterError("output_failed"))?;
        output
            .flush()
            .await
            .map_err(|_| AdapterError("output_failed"))?;
        axum::serve(listener, http::router(store, port))
            .with_graceful_shutdown(shutdown())
            .await
            .map_err(|_| AdapterError("server_failed"))?;
        return Ok(());
    }
    if command == "mcp" {
        return mcp::serve_stdio(&store, input, output).await;
    }
    let result = match command.as_str() {
        "status" => Some(store.get_status()?),
        "source" => {
            let mode = required(&flags, "mode")?;
            if mode != "live" && mode != "imported" {
                return Err(AdapterError("invalid_source_mode"));
            }
            let source = required(&flags, "source")?;
            Some(store.create_source(&json!({"id":source,"mode":mode,"displayName":flags.get("name").map(String::as_str).unwrap_or(source),"provider":if mode=="live" {"codex_app_server"} else {"user_import"},"coverageDescription":if mode=="live" {"Account-wide documented reads only; installed schema and account binding unverified."} else {"Explicitly supplied local events only; history is partial."}}))?)
        }
        "settings" => {
            let mut settings = json!({});
            if let Some(value) = flags.get("pause") {
                settings["capturePaused"] = json!(boolean(value)?);
            }
            if let Some(value) = flags.get("content") {
                settings["contentCaptureEnabled"] = json!(boolean(value)?);
            }
            if let Some(value) = flags.get("retention-days") {
                settings["retentionDays"] = json!(integer(value)?);
            }
            Some(if settings.as_object().is_some_and(|v| v.is_empty()) {
                store.get_settings()?
            } else {
                store.update_settings(&settings)?
            })
        }
        "delete" => {
            if flags.get("confirm").map(String::as_str) != Some("DELETE")
                || !flags
                    .get("target")
                    .is_some_and(|s| s == "all" || s == "content")
            {
                return Err(AdapterError("confirmation_required"));
            }
            let mut scope = json!({});
            if let Some(source) = flags.get("source") {
                scope["sourceId"] = json!(source);
            }
            Some(if flags["target"] == "all" {
                store.clear_data(&scope)?
            } else {
                store.clear_local_content(&scope)?
            })
        }
        "retention" => {
            if flags.get("confirm").map(String::as_str) != Some("APPLY_RETENTION") {
                return Err(AdapterError("confirmation_required"));
            }
            Some(store.apply_retention(&json!({}))?)
        }
        _ => {
            let source = flags
                .get("source")
                .map(String::as_str)
                .or(if demo { Some("demo") } else { None })
                .ok_or(AdapterError("missing_argument"))?;
            match command.as_str() {
                "import" | "import-rollout" | "import-rollout-incremental" => {
                    let filename = required(&flags, "file")?;
                    if !Path::new(filename).is_absolute() {
                        return Err(AdapterError("absolute_import_path_required"));
                    }
                    let imported = store
                        .list_sources()?
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|s| s["id"] == source && s["mode"] == "imported");
                    if !imported {
                        return Err(AdapterError("imported_source_required"));
                    }
                    if command == "import-rollout-incremental" {
                        let version = required(&flags, "source-version")?;
                        let stream = required(&flags, "stream")?;
                        let bytes =
                            read_bounded_file(Path::new(filename), rollout::MAX_BYTES).await?;
                        Some(crate::adapters::incremental::import_incremental_rollout(
                            &store,
                            &bytes,
                            &json!({"sourceId":source,"streamId":stream,"observedAt":now_iso(),"sourceVersion":version}),
                        )?)
                    } else if command == "import-rollout" {
                        let version = required(&flags, "source-version")?;
                        let imported_at = now_iso();
                        let bytes =
                            read_bounded_file(Path::new(filename), rollout::MAX_BYTES).await?;
                        let parsed = rollout::parse_rollout(
                            &bytes,
                            &json!({"sourceId":source,"observedAt":imported_at,"captureContent":store.get_settings()?["contentCaptureEnabled"],"sourceVersion":version}),
                        )?;
                        let mut value=store.import_rollout(&json!({"sourceId":source,"fingerprint":parsed["fingerprint"],"adapterVersion":parsed["adapterVersion"],"sourceVersion":parsed["sourceVersion"],"importedAt":imported_at,"warningCodes":parsed["warningCodes"],"events":parsed["events"],"responseTokens":parsed["responseTokens"]}))?;
                        for key in ["fingerprint", "sourceVersion", "recordsSeen", "warnings"] {
                            value[key] = parsed[key].clone();
                        }
                        Some(value)
                    } else {
                        let bytes = read_bounded_file(Path::new(filename), 2 * 1024 * 1024).await?;
                        let bundle: Value =
                            crate::adapters::read_only_rpc::parse_lossless_json(&bytes)
                                .map_err(|_| AdapterError("invalid_import_json"))?;
                        if bundle.as_object().is_none_or(|m| {
                            m.keys().any(|k| {
                                !["schemaVersion", "observations", "events"].contains(&k.as_str())
                            })
                        }) || bundle["schemaVersion"] != 1
                            || !bundle["observations"].is_array()
                            || !bundle["events"].is_array()
                        {
                            return Err(AdapterError("invalid_import_bundle"));
                        }
                        Some(store.import_data(&json!({"sourceId":source,"observations":bundle["observations"],"events":bundle["events"]}))?)
                    }
                }
                "import-trace-bundle" => {
                    let directory = required(&flags, "directory")?;
                    let version = required(&flags, "source-version")?;
                    let parsed = crate::adapters::trace::read_trace_bundle(
                        Path::new(directory),
                        &json!({"sourceId":source,"importedAt":now_iso(),"sourceVersion":version}),
                    )?;
                    Some(if flags.contains_key("dry-run") {
                        store.preflight_trace_bundle(&parsed)?
                    } else {
                        store.import_trace_bundle(&parsed)?
                    })
                }
                "collect" => Some(
                    collect_account(&store, source, flags.contains_key("accept-startup-risk"))
                        .await?,
                ),
                "hook" => {
                    let settings = store.get_settings()?;
                    if settings["capturePaused"] == true {
                        return Ok(());
                    }
                    let mut bytes = Vec::new();
                    input
                        .take(2 * 1024 * 1024 + 1)
                        .read_to_end(&mut bytes)
                        .await
                        .map_err(|_| AdapterError("invalid_hook_json"))?;
                    if bytes.len() > 2 * 1024 * 1024 {
                        return Err(AdapterError("hook_too_large"));
                    }
                    let raw = crate::adapters::read_only_rpc::parse_lossless_json(&bytes)
                        .map_err(|_| AdapterError("invalid_hook_json"))?;
                    store.ingest_event(&normalize_hook(
                        &raw,
                        source,
                        &now_iso(),
                        settings["contentCaptureEnabled"] == true,
                    )?)?;
                    None
                }
                _ => {
                    let mut params = json!({"sourceId":source});
                    for (flag, key) in [("max-age-ms", "maxAgeMs"), ("limit", "limit")] {
                        if let Some(value) = flags.get(flag) {
                            params[key] = json!(integer(value)?);
                        }
                    }
                    for (flag, key) in [
                        ("from", "fromDate"),
                        ("to", "toDate"),
                        ("model", "model"),
                        ("cursor", "cursor"),
                        ("event-type", "eventType"),
                        ("kind", "kind"),
                        ("skill", "skillName"),
                        ("thread", "threadId"),
                        ("status", "status"),
                        ("requested-model", "requestedModel"),
                        ("requested-effort", "requestedReasoningEffort"),
                        ("requested-tier", "requestedServiceTier"),
                    ] {
                        if let Some(value) = flags.get(flag) {
                            params[key] = json!(value);
                        }
                    }
                    Some(match command.as_str() {
                        "trace-attempts" => store.get_trace_attempts(&params)?,
                        "trace-summary" => store.get_trace_summary(&params)?,
                        "trace-detail" => store.get_local_trace_detail(
                            &json!({"sourceId":source,"attemptId":required(&flags,"attempt")?}),
                        )?,
                        "health" => store.get_health(&params)?,
                        "overview" => store.get_overview(&params)?,
                        "daily" => {
                            required(&flags, "from")?;
                            required(&flags, "to")?;
                            store.get_daily_usage(&params)?
                        }
                        "quota" => store.get_quota(&params)?,
                        "history" => store.get_quota_history(&params)?,
                        "events" => store.get_events(&params)?,
                        "skills" => store.get_skill_evidence(&params)?,
                        "skill-summary" => store.get_skill_summary(&params)?,
                        "response-tokens" => store.get_response_token_usage(&params)?,
                        "tools" => store.get_tool_usage(&params)?,
                        "detail" => store.get_local_event_detail(
                            &json!({"sourceId":source,"eventId":required(&flags,"event")?}),
                        )?,
                        _ => return Err(AdapterError("unknown_command")),
                    })
                }
            }
        }
    };
    if let Some(value) = result {
        emit(output, &value).await?;
    }
    Ok(())
}
