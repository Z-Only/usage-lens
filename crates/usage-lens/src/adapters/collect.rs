use super::{
    AdapterError,
    launch::launch_account_reader,
    now_iso,
    read_only_rpc::{READ_METHODS, ReadOnlyAppServer},
};
use crate::core::UsageStore;
use serde_json::{Value, json};

fn safe_failure(error: AdapterError, fallback: &'static str) -> &'static str {
    match error.0 {
        "unsupported_method"
        | "rpc_error"
        | "request_timeout"
        | "lifetime_timeout"
        | "auth_handoff_required"
        | "server_request_blocked"
        | "subprocess_closed"
        | "subprocess_error"
        | "stdout_limit"
        | "stderr_limit"
        | "frame_limit"
        | "malformed_response"
        | "unexpected_response_id"
        | "invalid_numeric_encoding"
        | "invalid_payload"
        | "incompatible_auth_mode"
        | "unverified_live_access" => error.0,
        _ => fallback,
    }
}
pub fn check_collection(
    store: &UsageStore,
    source_id: &str,
    accept_startup_risk: bool,
) -> Result<(), AdapterError> {
    if !accept_startup_risk {
        return Err(AdapterError("live_startup_opt_in_required"));
    }
    let sources = store.list_sources().map_err(|e| AdapterError(e.code()))?;
    if !sources
        .as_array()
        .into_iter()
        .flatten()
        .any(|s| s["id"] == source_id && s["mode"] == "live")
    {
        return Err(AdapterError("live_source_required"));
    }
    if store.get_settings().map_err(|e| AdapterError(e.code()))?["capturePaused"] == true {
        return Err(AdapterError("capture_paused"));
    }
    Ok(())
}
pub fn record_startup_failure(
    store: &UsageStore,
    source_id: &str,
    error: AdapterError,
    clock: &impl Fn() -> String,
) -> Result<(), AdapterError> {
    for method in READ_METHODS {
        store.record_failure(&json!({"sourceId":source_id,"method":method,"attemptedAt":clock(),"errorCode":safe_failure(error,"subprocess_error")})).map_err(|e|AdapterError(e.code()))?;
    }
    Ok(())
}
/// Explicit CLI opt-in only; HTTP and MCP never call collection.
pub async fn collect_account(
    store: &UsageStore,
    source_id: &str,
    accept_startup_risk: bool,
) -> Result<Value, AdapterError> {
    check_collection(store, source_id, accept_startup_risk)?;
    let (mut client, version) = match launch_account_reader().await {
        Ok(value) => value,
        Err(error) => {
            record_startup_failure(store, source_id, error, &now_iso)?;
            return Err(error);
        }
    };
    collect_started(store, source_id, &mut client, &version, now_iso).await
}
/// Collection over an already-created peer, also used for hermetic fake-peer tests.
pub async fn collect_started(
    store: &UsageStore,
    source_id: &str,
    client: &mut ReadOnlyAppServer,
    installed_version: &str,
    clock: impl Fn() -> String,
) -> Result<Value, AdapterError> {
    let result = async {
        if let Err(error) = client.initialize().await {
            record_startup_failure(store,source_id,error,&clock)?;
            return Err(error);
        }
        let mut outcomes = Vec::new();
        let mut incompatible = false;
        for method in READ_METHODS {
            let result = async {
                if incompatible { return Err(AdapterError("incompatible_auth_mode")); }
                let raw = match method { "account/read"=>client.read_account().await?,"account/usage/read"=>client.read_usage().await?,_=>client.read_rate_limits().await? };
                store.ingest_observation(&json!({"sourceId":source_id,"method":method,"raw":raw,"observedAt":clock(),"adapterVersion":format!("usage-lens/0.1.0+codex/{installed_version}"),"schemaBaseline":"public-docs-2026-10-02"})).map_err(|_|AdapterError("invalid_payload"))?;
                if method=="account/read" {
                    let overview = store.get_overview(&json!({"sourceId":source_id})).map_err(|_|AdapterError("invalid_payload"))?;
                    incompatible = overview["account"]["status"]=="available" && ["apiKey","amazonBedrock"].contains(&overview["account"]["data"]["type"]["value"].as_str().unwrap_or(""));
                }
                Ok::<(),AdapterError>(())
            }.await;
            match result {
                Ok(())=>outcomes.push(json!({"method":method,"status":"available"})),
                Err(error)=>{
                    let code = safe_failure(error,"invalid_payload");
                    store.record_failure(&json!({"sourceId":source_id,"method":method,"attemptedAt":clock(),"errorCode":code})).map_err(|e|AdapterError(e.code()))?;
                    outcomes.push(json!({"method":method,"status":"unavailable","errorCode":code}));
                }
            }
        }
        Ok(json!({"installedVersion":installed_version,"schemaBaseline":"public-docs-2026-10-02","outcomes":outcomes,"warnings":[
            "Installed method availability is established only by these individual responses; release-schema compatibility has not been independently verified.",
            "Local namespace is not a verified account binding. Recreate the source after changing signed-in accounts.",
            "App-server startup may load local configuration, plugins or credentials and contact Codex services. refreshToken:false is not a guarantee against internal credential refresh."]}))
    }.await;
    client.close().await;
    result
}
