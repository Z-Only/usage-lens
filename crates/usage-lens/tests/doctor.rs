use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use usage_lens::{
    adapters::{
        demo::seed_demo, incremental::import_incremental_rollout, rollout::ROLLOUT_SOURCE_VERSION,
    },
    cli::run_main,
    core::{
        UsageStore,
        trace::{TRACE_ADAPTER_VERSION, TRACE_SOURCE_VERSION},
    },
    doctor::*,
};

fn assets() -> BTreeMap<String, Vec<u8>> {
    let mut assets: BTreeMap<String, Vec<u8>> = BTreeMap::from([
        ("/index.html".into(), b"<html>synthetic</html>".to_vec()),
        ("/bootstrap.js".into(), b"synthetic bootstrap".to_vec()),
        ("/usage_lens_ui.js".into(), b"synthetic loader".to_vec()),
        ("/usage_lens_ui_bg.wasm".into(), b"\0asm".to_vec()),
    ]);
    let hashes: BTreeMap<_, _> = assets
        .iter()
        .map(|(path, bytes)| {
            (
                path.trim_start_matches('/').to_owned(),
                Sha256::digest(bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
            )
        })
        .collect();
    assets.insert(
        "/frontend-build.json".into(),
        json!({"schemaVersion":1,"framework":"leptos","assets":hashes})
            .to_string()
            .into_bytes(),
    );
    assets
}
fn verifies(assets: &BTreeMap<String, Vec<u8>>) -> bool {
    dashboard_verified(|path| assets.get(path).map(Vec::as_slice))
}
#[test]
fn dashboard_manifest_and_all_hashes_fail_closed() {
    let good = assets();
    assert!(verifies(&good));
    for missing in good.keys() {
        let mut bad = good.clone();
        bad.remove(missing);
        assert!(!verifies(&bad), "{missing}");
    }
    for bad_manifest in [
        b"invalid".to_vec(),
        json!({"schemaVersion":2,"framework":"leptos","assets":{}})
            .to_string()
            .into_bytes(),
        json!({"schemaVersion":1,"framework":"other","assets":{}})
            .to_string()
            .into_bytes(),
        json!({"schemaVersion":1,"framework":"leptos"})
            .to_string()
            .into_bytes(),
        json!({"schemaVersion":1,"framework":"leptos","assets":{}})
            .to_string()
            .into_bytes(),
    ] {
        let mut bad = good.clone();
        bad.insert("/frontend-build.json".into(), bad_manifest);
        assert!(!verifies(&bad));
    }
    for corrupt in ["/usage_lens_ui_bg.wasm", "/index.html"] {
        let mut bad = good.clone();
        bad.insert(corrupt.into(), b"corrupted".to_vec());
        assert!(!verifies(&bad));
    }
    let mut bad = good;
    let mut manifest: Value = serde_json::from_slice(&bad["/frontend-build.json"]).unwrap();
    manifest["assets"]["index.html"] = Value::Null;
    bad.insert(
        "/frontend-build.json".into(),
        manifest.to_string().into_bytes(),
    );
    assert!(!verifies(&bad));
}

fn synthetic_store(path: &std::path::Path) {
    let mut store = UsageStore::open(path).unwrap();
    seed_demo(&store, 1790920800000).unwrap();
    store.close().unwrap();
}
#[test]
fn diagnostic_checks_never_repair_or_create_and_hide_paths_and_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic-private.sqlite");
    let no_store = report(Some(&path), None, None);
    assert_eq!(no_store["status"], "failed");
    assert_eq!(no_store["checks"][2]["code"], "storage_error");
    assert!(!path.exists());
    synthetic_store(&path);
    let before = std::fs::read(&path).unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let db_only = report(Some(&path), None, None);
    assert_eq!(db_only["status"], "incomplete");
    assert_eq!(db_only["database"]["schemaVersion"], 2);
    assert_eq!(db_only["database"]["accessMode"], "read_only");
    assert_eq!(db_only["database"]["sourceCount"], "1");
    assert_eq!(db_only["compatibility"]["selectedSchema"], 2);
    assert_eq!(db_only["compatibility"]["wouldUpgrade"], true);
    let valid = report(Some(&path), Some("demo"), Some(0));
    assert_eq!(valid["status"], "ready");
    assert_eq!(valid["sourceHealth"]["source"]["id"], "demo");
    assert_eq!(valid["sourceHealth"]["maxAgeMs"], 0);
    assert_eq!(report(Some(&path), Some("demo"), None)["status"], "ready");
    for source in ["missing", "invalid source"] {
        let failed = report(Some(&path), Some(source), None);
        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["checks"][2]["status"], "pass");
        assert_eq!(failed["checks"][3]["status"], "fail");
    }
    for secret in [
        "synthetic-private.sqlite",
        "Synthetic request:",
        "toolArguments",
        "toolResult",
        "eventId",
        "threadId",
    ] {
        assert!(!valid.to_string().contains(secret), "{secret}");
    }
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("PRAGMA user_version=999").unwrap();
    drop(connection);
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        report(Some(&path), None, None)["checks"][2]["code"],
        "unsupported_schema"
    );
    let unsupported = report(Some(&path), None, None);
    assert!(unsupported["compatibility"]["selectedSchema"].is_null());
    assert!(unsupported["compatibility"]["wouldUpgrade"].is_null());
    assert!(
        unsupported["checks"][2]["nextStep"]
            .as_str()
            .unwrap()
            .contains("Do not edit schema numbers")
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn compatibility_guidance_tracks_real_schema_upgrades_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let now = "2026-10-03T00:00:00.000Z";
    let store = UsageStore::open(&path).unwrap();
    store.create_source(&json!({"id":"synthetic","displayName":"Synthetic","mode":"imported","provider":"synthetic","coverageDescription":"Synthetic fixtures only"})).unwrap();
    drop(store);
    for schema in [2, 3, 4] {
        if schema > 2 {
            let store = UsageStore::open(&path).unwrap();
            if schema == 3 {
                import_incremental_rollout(&store, b"", &json!({"sourceId":"synthetic","streamId":"empty","observedAt":now,"sourceVersion":ROLLOUT_SOURCE_VERSION})).unwrap();
            } else {
                store.import_trace_bundle(&json!({"sourceId":"synthetic","fingerprint":"a".repeat(64),"adapterVersion":TRACE_ADAPTER_VERSION,"sourceVersion":TRACE_SOURCE_VERSION,"importedAt":now,"bundleId":"synthetic-bundle","attempts":[],"warningCodes":[]})).unwrap();
            }
        }
        let before = std::fs::read(&path).unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let report = report_with_dashboard(Some(&path), Some("synthetic"), None, true);
        let compatibility = &report["compatibility"];
        assert_eq!(report["status"], "ready");
        assert_eq!(compatibility["selectedSchema"], schema);
        assert_eq!(compatibility["readableSchemas"], json!([2, 3, 4]));
        assert_eq!(compatibility["requiredJournalMode"], "rollback");
        assert_eq!(compatibility["traceImportTargetSchema"], 4);
        assert_eq!(compatibility["wouldUpgrade"], schema < 4);
        assert_eq!(
            compatibility["upgradeTrigger"],
            "successful_explicit_trace_import"
        );
        assert_eq!(compatibility["backupStatus"], "not_verified");
        assert_eq!(compatibility["scope"], "selected_store_schema_only");
        let warning = compatibility["rollbackWarning"].as_str().unwrap();
        for text in [
            "v0.5.0 and older",
            "v0.3.0 and older",
            "Replacing the binary",
            "change schema numbers",
        ] {
            assert!(warning.contains(text), "{text}");
        }
        let steps = compatibility["backupSteps"].as_array().unwrap();
        assert_eq!(steps.len(), 4);
        let instructions = serde_json::to_string(steps).unwrap();
        for text in [
            "stop every",
            "closed store",
            "Verify the backup",
            "separate compatible copy",
            "Do not overwrite",
        ] {
            assert!(instructions.contains(text), "{text}");
        }
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}

#[test]
fn unavailable_store_guidance_does_not_guess_schema_or_backup_state() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.sqlite");
    let malformed = dir.path().join("malformed.sqlite");
    std::fs::write(&malformed, b"synthetic non-database canary").unwrap();
    for path in [missing.as_path(), malformed.as_path(), dir.path()] {
        let result = report_with_dashboard(Some(path), None, None, true);
        assert_eq!(result["checks"][2]["code"], "storage_error");
        assert!(result["compatibility"]["selectedSchema"].is_null());
        assert!(result["compatibility"]["wouldUpgrade"].is_null());
        assert_eq!(result["compatibility"]["backupStatus"], "not_verified");
        assert!(
            result["checks"][2]["nextStep"]
                .as_str()
                .unwrap()
                .contains("rollback-journal")
        );
        assert!(!result.to_string().contains("synthetic non-database canary"));
    }
    assert!(!missing.exists());
    assert_eq!(
        std::fs::read(&malformed).unwrap(),
        b"synthetic non-database canary"
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn executable_only_diagnostics_and_dashboard_failure_are_explicit() {
    let actual = report(None, None, None);
    assert_eq!(actual["status"], "incomplete");
    assert_eq!(actual["checks"][1]["status"], "pass");
    assert_eq!(actual["checks"][2]["status"], "not_checked");
    assert_eq!(actual["application"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(actual["application"]["binaryOs"], std::env::consts::OS);
    assert_eq!(actual["application"]["binaryArch"], std::env::consts::ARCH);
    assert_eq!(actual["scope"], "read_only_setup_diagnostics");
    assert_eq!(actual["compatibility"]["readableSchemas"], json!([2, 3, 4]));
    assert!(actual["compatibility"]["wouldUpgrade"].is_null());
    assert!(actual["compatibility"]["selectedSchema"].is_null());
    assert_eq!(actual["compatibility"]["backupStatus"], "not_verified");
    let bad = report_with_dashboard(None, None, None, false);
    assert_eq!(bad["status"], "failed");
    assert_eq!(bad["checks"][1]["code"], "embedded_assets_invalid");
    assert!(bad["sourceHealth"].is_null());
}
async fn cli(args: &[&str]) -> (i32, String, String) {
    let args = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run_main(&args, &b""[..], &mut out, &mut err).await;
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}
#[tokio::test]
async fn doctor_cli_clean_json_exit_codes_and_argument_boundaries() {
    let (code, out, err) = cli(&["doctor"]).await;
    assert_eq!(code, 0);
    assert!(err.is_empty());
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap()["status"],
        "incomplete"
    );
    for args in [
        vec!["doctor", "--demo"],
        vec!["doctor", "--db", "relative.sqlite"],
        vec!["doctor", "--source", "demo"],
        vec!["doctor", "--max-age-ms", "1"],
        vec!["doctor", "--max-age-ms", "no"],
        vec![
            "doctor",
            "--db",
            "/unused",
            "--source",
            "demo",
            "--max-age-ms",
            "2592000001",
        ],
        vec!["doctor", "--accept-startup-risk"],
    ] {
        let (code, out, _) = cli(&args).await;
        assert_eq!(code, 1, "{args:?}");
        assert!(out.is_empty());
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.sqlite");
    let path_text = path.to_str().unwrap();
    let (code, out, err) = cli(&["doctor", "--db", path_text]).await;
    assert_eq!(code, 1);
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap()["status"],
        "failed"
    );
    assert!(err.contains("doctor_checks_failed"));
    assert!(!path.exists());
    synthetic_store(&path);
    for command in ["doctor", "health"] {
        let (code, out, err) = cli(&[
            command,
            "--db",
            path_text,
            "--source",
            "demo",
            "--max-age-ms",
            "0",
        ])
        .await;
        assert_eq!(code, 0, "{err}");
        assert!(serde_json::from_str::<Value>(&out).is_ok());
    }
    let (code, out, err) = cli(&["health", "--demo"]).await;
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap()["source"]["mode"],
        "demo"
    );
    assert_eq!(cli(&["health", "--db", path_text]).await.0, 1);
}
