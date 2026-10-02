use serde_json::{Value, json};
use usage_lens::{
    adapters::{demo::seed_demo, hooks::normalize_hook, launch::parse_installed_version},
    cli::{parse_arguments, read_bounded_file, run_main},
    core::UsageStore,
};
const AT: &str = "2026-10-02T06:00:00.000Z";
fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}
async fn cli(args: &[&str], input: &[u8]) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run_main(&strings(args), input, &mut out, &mut err).await;
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}
#[test]
fn hooks_are_documented_scoped_direct_and_body_opt_in() {
    let raw = json!({"session_id":"session","turn_id":"turn","hook_event_name":"PostToolUse","tool_use_id":"call","tool_name":"mcp__fs__read","tool_input":{"path":"/secret"},"tool_response":"body","transcript_path":"/never/read/me","cwd":"/never/read"});
    let event = normalize_hook(&raw, "local", AT, false).unwrap();
    assert_eq!(event["status"], "unknown");
    assert!(event["model"].is_null());
    assert!(event.get("content").is_none());
    assert!(!event.to_string().contains("/never/read"));
    assert_eq!(normalize_hook(&raw, "local", AT, false).unwrap(), event);
    let mut other = raw.clone();
    other["session_id"] = json!("other");
    assert_ne!(
        normalize_hook(&other, "local", AT, false).unwrap()["sourceEventId"],
        event["sourceEventId"]
    );
    other = raw.clone();
    other["turn_id"] = json!("other");
    assert_ne!(
        normalize_hook(&other, "local", AT, false).unwrap()["eventId"],
        event["eventId"]
    );
    assert_eq!(
        normalize_hook(&raw, "local", AT, true).unwrap()["content"],
        json!({"toolArguments":{"path":"/secret"},"toolResult":"body"})
    );
    let mut minimal = raw.clone();
    minimal.as_object_mut().unwrap().remove("tool_input");
    minimal.as_object_mut().unwrap().remove("tool_response");
    assert_eq!(
        normalize_hook(&minimal, "local", AT, true).unwrap()["content"],
        json!({})
    );
    for name in ["UserPromptSubmit", "Stop"] {
        let mut raw = raw.clone();
        raw["hook_event_name"] = json!(name);
        raw["prompt"] = json!("Use spreadsheets");
        raw["last_assistant_message"] = json!("Skill was mentioned");
        raw["model"] = json!("model");
        let event = normalize_hook(&raw, "local", AT, true).unwrap();
        assert!(event["content"]["body"].is_string());
        assert!(event.get("skillName").is_none());
        assert!(
            normalize_hook(&raw, "local", AT, false)
                .unwrap()
                .get("content")
                .is_none()
        );
    }
    for raw in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"session_id":"a","turn_id":"b","hook_event_name":"PreToolUse"}),
        json!({"session_id":"a","turn_id":"b","hook_event_name":"Stop"}),
        json!({"session_id":"a","turn_id":"b","hook_event_name":"UserPromptSubmit","prompt":2}),
        json!({"session_id":"bad secret\n","turn_id":"b"}),
        json!({"session_id":"a","turn_id":"b","model":1}),
    ] {
        assert!(normalize_hook(&raw, "local", AT, false).is_err());
    }
}
#[test]
fn demo_is_explicit_synthetic_and_requires_empty_store() {
    let store = UsageStore::in_memory().unwrap();
    seed_demo(&store, 1790920800000).unwrap();
    assert_eq!(store.get_status().unwrap()["eventCount"], "56");
    assert_eq!(
        seed_demo(&store, 1790920800000).unwrap_err().0,
        "demo_requires_empty_store"
    );
}
#[test]
fn parser_limits_flags_and_version_sanitization() {
    for argv in [
        &[][..],
        &["help"][..],
        &["--help"][..],
        &["status", "--help"][..],
    ] {
        assert_eq!(parse_arguments(&strings(argv)).unwrap().command, "help");
    }
    for args in [
        &["unknown"][..],
        &["status", "value"][..],
        &["status", "--private"][..],
        &["status", "--db"][..],
        &["status", "--demo", "--demo"][..],
        &["status", "--db", "--demo"][..],
        &["status", "--db", ""][..],
    ] {
        assert!(parse_arguments(&strings(args)).is_err());
    }
    assert!(parse_arguments(&["status".into(), format!("--{}", "x".repeat(300))]).is_err());
    assert!(parse_arguments(&["status".into(), "--db".into(), "x".repeat(4097)]).is_err());
    for (input, expected) in [
        ("codex 0.99.0\n", "0.99.0"),
        ("codex-cli 1.0.2-beta.1\n", "1.0.2-beta.1"),
    ] {
        assert_eq!(parse_installed_version(input.as_bytes()).unwrap(), expected);
    }
    for input in [&b"SECRET"[..], &b"codex blah"[..], &[255][..]] {
        assert_eq!(
            parse_installed_version(input).unwrap_err().0,
            "unrecognized_installed_version"
        );
    }
}
#[tokio::test]
async fn cli_queries_help_settings_and_confirmation_guards() {
    let (code, help, err) = cli(&["--help"], b"").await;
    assert_eq!(code, 0);
    assert!(err.is_empty());
    assert!(help.starts_with(&format!("Usage Lens {} —", env!("CARGO_PKG_VERSION"))));
    for command in [
        "status",
        "overview",
        "quota",
        "history",
        "events",
        "skills",
        "skill-summary",
        "tools",
        "response-tokens",
        "settings",
    ] {
        let (code, out, err) = cli(&[command, "--demo"], b"").await;
        assert_eq!(code, 0, "{command}: {err}");
        assert!(serde_json::from_str::<Value>(&out).is_ok());
    }
    for args in [
        vec![
            "daily",
            "--demo",
            "--from",
            "2026-09-01",
            "--to",
            "2026-10-02",
        ],
        vec!["detail", "--demo", "--event", "demo-event-0"],
        vec![
            "settings",
            "--demo",
            "--pause",
            "true",
            "--content",
            "false",
            "--retention-days",
            "7",
        ],
        vec![
            "delete",
            "--demo",
            "--target",
            "content",
            "--source",
            "demo",
            "--confirm",
            "DELETE",
        ],
        vec!["delete", "--demo", "--target", "all", "--confirm", "DELETE"],
        vec!["retention", "--demo", "--confirm", "APPLY_RETENTION"],
        vec![
            "events",
            "--demo",
            "--from",
            "2026-09-01",
            "--to",
            "2026-10-02",
            "--model",
            "demo-model-a",
            "--limit",
            "1",
            "--event-type",
            "tool_call",
        ],
        vec!["skills", "--demo", "--kind", "loaded", "--limit", "1"],
        vec!["quota", "--demo", "--max-age-ms", "1"],
    ] {
        let (code, _, err) = cli(&args, b"").await;
        assert_eq!(code, 0, "{args:?}: {err}");
    }
    for args in [
        vec!["status"],
        vec!["status", "--db", "relative"],
        vec!["status", "--db", ":memory:"],
        vec!["status", "--demo", "--db", "/tmp/unused.sqlite"],
        vec!["collect", "--demo"],
        vec!["daily", "--demo"],
        vec!["settings", "--demo", "--pause", "yes"],
        vec!["settings", "--demo", "--retention-days", "-1"],
        vec!["settings", "--demo", "--retention-days", "99999999999"],
        vec!["delete", "--demo", "--target", "all"],
        vec!["delete", "--demo", "--target", "bad", "--confirm", "DELETE"],
        vec!["retention", "--demo"],
        vec!["serve", "--demo", "--host", "0.0.0.0"],
        vec!["serve", "--demo", "--port", "65536"],
    ] {
        let (code, out, err) = cli(&args, b"").await;
        assert_eq!(code, 1, "{args:?}");
        assert!(out.is_empty());
        assert!(err.starts_with("Usage Lens: "));
    }
}
#[tokio::test]
async fn cli_explicit_database_imports_hooks_and_paused_silence() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("usage.sqlite");
    let db = db.to_str().unwrap();
    assert_eq!(
        cli(
            &["source", "--db", db, "--source", "local", "--mode", "wrong"],
            b""
        )
        .await
        .0,
        1
    );
    assert_eq!(
        cli(
            &[
                "source", "--db", db, "--source", "local", "--mode", "imported", "--name", "Local"
            ],
            b""
        )
        .await
        .0,
        0
    );
    assert_eq!(
        cli(
            &["source", "--db", db, "--source", "live", "--mode", "live"],
            b""
        )
        .await
        .0,
        0
    );
    assert!(
        cli(&["collect", "--db", db, "--source", "live"], b"")
            .await
            .2
            .contains("live_startup_opt_in_required")
    );
    assert!(
        cli(
            &[
                "collect",
                "--db",
                db,
                "--source",
                "local",
                "--accept-startup-risk"
            ],
            b""
        )
        .await
        .2
        .contains("live_source_required")
    );
    assert_eq!(
        cli(&["settings", "--db", db, "--content", "true"], b"")
            .await
            .0,
        0
    );
    let raw = json!({"session_id":"session","turn_id":"turn","hook_event_name":"UserPromptSubmit","prompt":"local private body"});
    assert_eq!(
        cli(
            &["hook", "--db", db, "--source", "local"],
            raw.to_string().as_bytes()
        )
        .await,
        (0, String::new(), String::new())
    );
    assert!(
        cli(&["hook", "--db", db, "--source", "local"], b"{secret")
            .await
            .2
            .contains("invalid_hook_json")
    );
    assert!(
        cli(
            &["hook", "--db", db, "--source", "local"],
            &vec![b'x'; 2 * 1024 * 1024 + 1]
        )
        .await
        .2
        .contains("hook_too_large")
    );
    let import = dir.path().join("bundle.json");
    let name = import.to_str().unwrap();
    std::fs::write(&import, b"{bad").unwrap();
    assert!(
        cli(
            &["import", "--db", db, "--source", "local", "--file", name],
            b""
        )
        .await
        .2
        .contains("invalid_import_json")
    );
    for bundle in [
        json!([]),
        json!({"schemaVersion":1,"observations":[],"events":[],"private":"bad"}),
        json!({"schemaVersion":1,"observations":[],"events":[]}),
    ] {
        std::fs::write(&import, bundle.to_string()).unwrap();
        let (code, _, _) = cli(
            &["import", "--db", db, "--source", "local", "--file", name],
            b"",
        )
        .await;
        assert_eq!(
            code,
            if bundle == json!({"schemaVersion":1,"observations":[],"events":[]}) {
                0
            } else {
                1
            }
        );
    }
    assert!(
        cli(
            &["import", "--db", db, "--source", "live", "--file", name],
            b""
        )
        .await
        .2
        .contains("imported_source_required")
    );
    assert!(
        cli(
            &[
                "import", "--db", db, "--source", "local", "--file", "relative"
            ],
            b""
        )
        .await
        .2
        .contains("absolute_import_path_required")
    );
    assert_eq!(
        cli(&["settings", "--db", db, "--pause", "true"], b"")
            .await
            .0,
        0
    );
    assert_eq!(
        cli(
            &["hook", "--db", db, "--source", "local"],
            b"invalid ignored input"
        )
        .await,
        (0, String::new(), String::new())
    );
    assert!(
        cli(
            &[
                "collect",
                "--db",
                db,
                "--source",
                "live",
                "--accept-startup-risk"
            ],
            b""
        )
        .await
        .2
        .contains("capture_paused")
    );
}
#[tokio::test]
async fn bounded_files_reject_directories_symlinks_and_oversized_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixture");
    std::fs::write(&path, b"12345").unwrap();
    assert_eq!(read_bounded_file(&path, 5).await.unwrap(), b"12345");
    assert_eq!(
        read_bounded_file(&path, 4).await.unwrap_err().0,
        "import_too_large"
    );
    assert_eq!(
        read_bounded_file(dir.path(), 100).await.unwrap_err().0,
        "import_file_required"
    );
    assert!(
        read_bounded_file(&dir.path().join("missing"), 100)
            .await
            .is_err()
    );
    #[cfg(unix)]
    {
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_bounded_file(&link, 100).await.is_err());
    }
}

#[tokio::test]
async fn cli_rollout_explicit_file_round_trip_replay_and_safe_core_failures() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("usage.sqlite");
    let db = db.to_str().unwrap();
    assert_eq!(
        cli(
            &[
                "source", "--db", db, "--source", "local", "--mode", "imported"
            ],
            b""
        )
        .await
        .0,
        0
    );
    let filename = dir.path().join("explicit.jsonl");
    let rows = [
        json!({"timestamp":AT,"type":"session_meta","payload":{"id":"thread","session_id":"session","cwd":"/never/open"}}),
        json!({"timestamp":AT,"type":"turn_context","payload":{"turn_id":"turn","model":"model-a"}}),
        json!({"timestamp":AT,"type":"event_msg","payload":{"type":"user_message","message":"Synthetic local body"}}),
    ];
    let text = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    std::fs::write(&filename, text).unwrap();
    let args = [
        "import-rollout",
        "--db",
        db,
        "--source",
        "local",
        "--file",
        filename.to_str().unwrap(),
        "--source-version",
        "a75987455a2879ca151cea5e118fa307be868583",
    ];
    let (code, out, err) = cli(&args, b"").await;
    assert_eq!(code, 0, "{err}");
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["recordsSeen"], 3);
    assert!(value["fingerprint"].is_string());
    let (code, out, err) = cli(&args, b"").await;
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap()["importAlreadyPresent"],
        true
    );
    let (code, _, err) = cli(&["overview", "--db", db, "--source", "missing"], b"").await;
    assert_eq!(code, 1);
    assert!(err.contains("source_not_found"));
    let (code, _, err) = cli(
        &[
            "detail", "--db", db, "--source", "local", "--event", "missing",
        ],
        b"",
    )
    .await;
    assert_eq!(code, 1);
    assert!(err.contains("event_not_found"));
    assert!(cli(&["hook","--db",db,"--source","missing"],json!({"session_id":"s","turn_id":"t","hook_event_name":"UserPromptSubmit","prompt":"synthetic"}).to_string().as_bytes()).await.2.contains("source_not_found"));
}
#[test]
fn lossless_json_integer_lexemes_are_exact_and_unsafe_exponents_rejected() {
    use usage_lens::adapters::read_only_rpc::parse_lossless_json;
    let value = parse_lossless_json(
        br#"{"n":18446744073709551616123456789,"values":[1e3,1.25,null,true,"text"]}"#,
    )
    .unwrap();
    assert_eq!(value["n"].to_string(), "18446744073709551616123456789");
    for bytes in [
        &b"{\"n\":1e20}"[..],
        &b"[9007199254740993.0]"[..],
        &b"1e999"[..],
    ] {
        assert_eq!(
            parse_lossless_json(bytes).unwrap_err().0,
            "invalid_numeric_encoding"
        );
    }
    assert_eq!(
        parse_lossless_json(b"{bad").unwrap_err().0,
        "malformed_response"
    );
}

struct FaultIo {
    flush_only: bool,
}
impl tokio::io::AsyncRead for FaultIo {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        _: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(std::io::Error::other("PRIVATE_IO_FAILURE")))
    }
}
impl tokio::io::AsyncWrite for FaultIo {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        if self.flush_only {
            std::task::Poll::Ready(Ok(bytes.len()))
        } else {
            std::task::Poll::Ready(Err(std::io::Error::other("PRIVATE_IO_FAILURE")))
        }
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(std::io::Error::other("PRIVATE_IO_FAILURE")))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
#[tokio::test]
async fn cli_broken_streams_are_sanitized_and_release_the_listener() {
    for args in [
        strings(&["--help"]),
        strings(&["status", "--demo"]),
        strings(&["serve", "--demo", "--port", "0"]),
        strings(&["serve", "--demo"]),
    ] {
        for flush_only in [false, true] {
            let mut err = Vec::new();
            let code = run_main(&args, &b""[..], FaultIo { flush_only }, &mut err).await;
            assert_eq!(code, 1);
            let err = String::from_utf8(err).unwrap();
            assert!(
                err.contains("output_failed") || err.contains("server_start_failed"),
                "{err}"
            );
            assert!(!err.contains("PRIVATE_IO_FAILURE"));
        }
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    assert!(
        cli(&["serve", "--demo", "--port", &port], b"")
            .await
            .2
            .contains("server_start_failed")
    );
    assert_eq!(
        run_main(
            &strings(&["unknown"]),
            &b""[..],
            Vec::new(),
            FaultIo { flush_only: false }
        )
        .await,
        1
    );
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("usage.sqlite");
    let db = db.to_str().unwrap();
    assert_eq!(
        cli(
            &[
                "source", "--db", db, "--source", "local", "--mode", "imported"
            ],
            b""
        )
        .await
        .0,
        0
    );
    let mut error = Vec::new();
    assert_eq!(
        run_main(
            &strings(&["hook", "--db", db, "--source", "local"]),
            FaultIo { flush_only: false },
            Vec::new(),
            &mut error
        )
        .await,
        1
    );
    assert_eq!(error, b"Usage Lens: invalid_hook_json\n");
    let path = dir.path().join("invalid.jsonl");
    std::fs::write(&path, "{bad").unwrap();
    assert!(
        cli(
            &[
                "import-rollout",
                "--db",
                db,
                "--source",
                "local",
                "--file",
                path.to_str().unwrap(),
                "--source-version",
                "a75987455a2879ca151cea5e118fa307be868583"
            ],
            b""
        )
        .await
        .2
        .contains("rollout_invalid_json")
    );
    // A store-backed server has no synthetic label, even when writing the URL fails.
    let mut error = Vec::new();
    assert_eq!(
        run_main(
            &strings(&["serve", "--db", db, "--port", "0"]),
            &b""[..],
            FaultIo { flush_only: false },
            &mut error
        )
        .await,
        1
    );
    assert_eq!(error, b"Usage Lens: output_failed\n");
}
#[cfg(target_os = "linux")]
#[tokio::test]
async fn bounded_descriptor_reads_do_not_trust_metadata_size() {
    // This kernel-generated file reports length zero but returns actual bytes.
    // It contains only this synthetic test process's metadata, never a user log.
    assert_eq!(std::fs::metadata("/proc/self/stat").unwrap().len(), 0);
    assert_eq!(
        read_bounded_file(std::path::Path::new("/proc/self/stat"), 1)
            .await
            .unwrap_err()
            .0,
        "import_too_large"
    );
}

#[tokio::test]
async fn mcp_broken_input_output_and_flush_are_safe() {
    let store = UsageStore::in_memory().unwrap();
    assert_eq!(
        usage_lens::server::mcp::serve_stdio(&store, FaultIo { flush_only: false }, Vec::new())
            .await
            .unwrap_err()
            .0,
        "stdio_error"
    );
    for flush_only in [false, true] {
        assert_eq!(
            usage_lens::server::mcp::serve_stdio(&store, &b"{bad\n"[..], FaultIo { flush_only })
                .await
                .unwrap_err()
                .0,
            "stdio_error"
        );
    }
}
