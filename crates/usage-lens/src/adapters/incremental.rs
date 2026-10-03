//! One-shot append-safe ingestion. Selected file bytes are never remembered or reopened.
use super::{AdapterError, rollout};
use crate::core::{UsageStore, validation};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub fn import_incremental_rollout(
    store: &UsageStore,
    bytes: &[u8],
    options: &Value,
) -> Result<Value, AdapterError> {
    validation::exact_keys(
        options,
        &["sourceId", "streamId", "observedAt", "sourceVersion"],
    )?;
    validation::timestamp(&options["observedAt"])?;
    let source = validation::source_id(&options["sourceId"])?;
    let stream = validation::source_id(&options["streamId"])?;
    let expected = store.get_rollout_checkpoint(&json!({"sourceId":source,"streamId":stream}))?;
    if !expected.is_null() {
        if expected["adapterVersion"] != rollout::INCREMENTAL_ADAPTER_VERSION
            || expected["sourceVersion"] != options["sourceVersion"]
        {
            return Err(AdapterError("rollout_stream_version_mismatch"));
        }
        let offset = expected["completeBytes"]
            .as_u64()
            .ok_or(AdapterError("storage_error"))?;
        let prefix = bytes
            .get(..usize::try_from(offset).map_err(|_| AdapterError("storage_error"))?)
            .ok_or(AdapterError("rollout_prefix_changed"))?;
        if expected["fingerprint"]
            != Sha256::digest(prefix)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        {
            return Err(AdapterError("rollout_prefix_changed"));
        }
    }
    if options["sourceVersion"] != rollout::ROLLOUT_SOURCE_VERSION {
        return Err(AdapterError("rollout_unsupported_source_version"));
    }
    let mut parser_options = options.clone();
    parser_options["captureContent"] = store.get_settings()?["contentCaptureEnabled"].clone();
    // The one-shot import exposes one stable conflict code across parser and store checks.
    // Keep the lower-level parser and snapshot import's more specific contracts unchanged.
    let mut parsed = rollout::parse_incremental_rollout(bytes, &parser_options).map_err(
        |error| match error.0 {
            "rollout_conflicting_event_identity"
            | "rollout_conflicting_response_identity"
            | "rollout_conflicting_call_identity"
            | "rollout_conflicting_output_identity" => AdapterError("rollout_identity_conflict"),
            _ => error,
        },
    )?;
    parsed["expectedCheckpoint"] = expected;
    parsed["importedAt"] = options["observedAt"].clone();
    store
        .import_rollout_incremental(&parsed)
        .map_err(Into::into)
}
