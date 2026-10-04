//! Explicit, read-only setup diagnostics. Never searches for stores or starts a collector.
use crate::{core::UsageStore, server::http::embedded_asset};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Verify the embedded build manifest and every listed payload without opening files.
pub fn dashboard_verified<'a>(asset: impl Fn(&str) -> Option<&'a [u8]>) -> bool {
    let Some(manifest) = asset("/frontend-build.json") else {
        return false;
    };
    let Ok(manifest) = serde_json::from_slice::<Value>(manifest) else {
        return false;
    };
    if manifest["schemaVersion"] != 1 || manifest["framework"] != "leptos" {
        return false;
    }
    let Some(hashes) = manifest["assets"].as_object() else {
        return false;
    };
    for required in [
        "index.html",
        "bootstrap.js",
        "usage_lens_ui.js",
        "usage_lens_ui_bg.wasm",
    ] {
        if !hashes.contains_key(required) {
            return false;
        }
    }
    if !asset("/usage_lens_ui_bg.wasm").is_some_and(|bytes| bytes.starts_with(b"\0asm")) {
        return false;
    }
    hashes.iter().all(|(name, expected)| {
        asset(&format!("/{name}")).is_some_and(|bytes| {
            let digest = Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            expected.as_str() == Some(digest.as_str())
        })
    })
}

fn check(id: &str, status: &str, code: &str, action: &str) -> Value {
    json!({"id":id,"status":status,"code":code,"nextStep":action})
}

fn compatibility(selected_schema: Option<u64>) -> Value {
    json!({
        "readableSchemas":[2,3,4],
        "requiredJournalMode":"rollback",
        "selectedSchema":selected_schema,
        "traceImportTargetSchema":4,
        "wouldUpgrade":selected_schema.map(|schema| schema < 4),
        "upgradeTrigger":"successful_explicit_trace_import",
        "backupStatus":"not_verified",
        "rollbackWarning":"A successful trace import upgrades schema 2/3 to 4. v0.5.0 and older cannot read schema 4; v0.3.0 and older cannot read schema 3. Replacing the binary does not reverse an upgrade. Never open an upgraded store with an incompatible older binary or change schema numbers to bypass compatibility checks.",
        "backupSteps":[
            "Before a write or upgrade, stop every dashboard server, MCP process, hook writer and collector using the selected store.",
            "Make a private backup of the closed store outside the installation directory and synced folders. Preserve any remaining SQLite journal sidecars with it, or use a documented SQLite backup operation; never copy only the main file of an active WAL database.",
            "Verify the backup is readable with a compatible binary and keep the pre-upgrade backup unchanged. This diagnostic neither creates nor verifies a backup.",
            "For rollback, use a separate compatible copy of the pre-upgrade backup. Do not overwrite the current store; newer evidence is absent from that older copy."
        ],
        "scope":"selected_store_schema_only"
    })
}

fn database_next_step(code: &str) -> &'static str {
    if code == "unsupported_schema" {
        "This executable reads schemas 2, 3 and 4 only. Use a binary documented for the selected store, or a separate compatible pre-upgrade backup. Do not edit schema numbers, open it with an incompatible older binary, or migrate the only copy."
    } else {
        "Check the selected existing database path and read permission. Only rollback-journal stores are supported; missing, unreadable, malformed or WAL stores can report storage_error. Stop writers and use a verified compatible backup; do not create a replacement, convert journaling or repair the only copy."
    }
}

/// The caller validates absolute paths and option combinations. Failed store reads are
/// diagnostics, not a request to create/migrate/repair the selected database.
pub fn report(database: Option<&Path>, source: Option<&str>, max_age_ms: Option<u64>) -> Value {
    report_with_dashboard(
        database,
        source,
        max_age_ms,
        dashboard_verified(|name| embedded_asset(name).map(|(_, bytes)| bytes)),
    )
}

pub fn report_with_dashboard(
    database: Option<&Path>,
    source: Option<&str>,
    max_age_ms: Option<u64>,
    dashboard_ok: bool,
) -> Value {
    let mut checks = vec![
        check(
            "runtime",
            "pass",
            "native_runtime_ready",
            "No Node, Bun, or Rust runtime is required by this executable.",
        ),
        check(
            "dashboard",
            if dashboard_ok { "pass" } else { "fail" },
            if dashboard_ok {
                "embedded_assets_verified"
            } else {
                "embedded_assets_invalid"
            },
            if dashboard_ok {
                "Start an explicitly chosen demo or local dashboard to test browser behavior."
            } else {
                "Obtain and checksum-verify a fresh official release; do not bypass failed verification."
            },
        ),
    ];
    let mut store_info = Value::Null;
    let mut source_health = Value::Null;
    let mut selected_schema = None;
    if let Some(path) = database {
        match UsageStore::open_read_only(path).and_then(|store| {
            let status = store.get_status()?;
            Ok((store, status))
        }) {
            Ok((store, status)) => {
                selected_schema = status["schemaVersion"].as_u64();
                store_info = json!({"schemaVersion":status["schemaVersion"],"sourceCount":status["sources"].as_array().map_or(0, Vec::len).to_string(),"accessMode":"read_only","journalMode":"rollback"});
                checks.push(check("database", "pass", "existing_store_readable", "Review compatibility and backupSteps before any explicit import; no backup has been checked. Select a source explicitly to inspect its recorded evidence."));
                if let Some(source) = source {
                    let mut input = json!({"sourceId":source});
                    if let Some(age) = max_age_ms {
                        input["maxAgeMs"] = json!(age);
                    }
                    match store.get_health(&input) {
                        Ok(health) => {
                            source_health = health;
                            checks.push(check("source", "pass", "source_health_readable", "Review sourceHealth for missing, stale, future-dated, or failed collection evidence; readable does not mean complete."));
                        }
                        Err(error) => checks.push(check("source", "fail", error.code(), "Use the exact intended source ID from status. Do not substitute another source or collect automatically.")),
                    }
                } else {
                    checks.push(check(
                        "source",
                        "not_checked",
                        "source_not_selected",
                        "Run status, then pass the explicitly selected source ID with --source.",
                    ));
                }
            }
            Err(error) => checks.push(check(
                "database",
                "fail",
                error.code(),
                database_next_step(error.code()),
            )),
        }
    } else {
        checks.push(check("database", "not_checked", "database_not_selected", "Pass --db with the authorized existing absolute database path; do not search for private records."));
    }
    let failed = checks.iter().any(|item| item["status"] == "fail");
    let incomplete = checks.iter().any(|item| item["status"] == "not_checked");
    json!({
        "schemaVersion":1,
        "application":{"name":"usage-lens","version":env!("CARGO_PKG_VERSION"),"binaryOs":std::env::consts::OS,"binaryArch":std::env::consts::ARCH},
        "status":if failed {"failed"} else if incomplete {"incomplete"} else {"ready"},
        "checks":checks,"database":store_info,"sourceHealth":source_health,
        "compatibility":compatibility(selected_schema),
        "scope":"read_only_setup_diagnostics",
        "warnings":[
            "Checks describe this executable and only the explicitly selected store. They do not prove installation in a real client, browser compatibility, account access, or complete collection.",
            "Binary architecture may differ from physical hardware under emulation. Verify the release asset against the actual machine before installation.",
            "No collector, subprocess, network request, directory scan, credential read, installation, or background registration is performed.",
            "Setup readiness does not imply fresh or complete data. Collection time and provider data freshness are separate measurements.",
            "Compatibility describes supported local store schemas, not a validated trace bundle or live desktop runtime. wouldUpgrade is null when the selected store could not be read; doctor never imports, migrates, creates or verifies backups."
        ]
    })
}
