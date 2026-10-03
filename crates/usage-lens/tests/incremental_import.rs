//! Synthetic files only. No client configuration, credentials, or real history is opened.
use serde_json::{Value, json};
use std::{fs, process::Command};
use usage_lens::{
    adapters::{
        incremental::import_incremental_rollout,
        rollout::{ROLLOUT_SOURCE_VERSION, parse_incremental_rollout},
    },
    core::UsageStore,
};
const NOW: &str = "2026-10-03T00:00:00.000Z";
fn options(stream: &str) -> Value {
    json!({"sourceId":"synthetic","streamId":stream,"observedAt":NOW,"sourceVersion":ROLLOUT_SOURCE_VERSION})
}
fn row(kind: &str, payload: Value) -> Value {
    json!({"timestamp":NOW,"type":kind,"payload":payload})
}
fn message(id: &str, body: &str) -> Value {
    row(
        "response_item",
        json!({"type":"message","id":id,"role":"user","content":[{"type":"input_text","text":body}]}),
    )
}
fn meta() -> Value {
    row(
        "session_meta",
        json!({"id":"thread","session_id":"session"}),
    )
}
fn call() -> Value {
    row(
        "response_item",
        json!({"type":"function_call","namespace":"skills","name":"read","call_id":"call","arguments":"{\"package\":\"skill://synthetic/test\"}"}),
    )
}
fn output() -> Value {
    row(
        "response_item",
        json!({"type":"function_call_output","call_id":"call","output":json!({"resource":"skill://synthetic/test/SKILL.md","contents":"NEVER PERSIST INSTRUCTIONS","next_cursor":null}).to_string()}),
    )
}
fn token(total: u32) -> Value {
    let counts = json!({"input_tokens":1,"cached_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0,"total_tokens":total});
    row(
        "token_usage_record",
        json!({"thread_id":"thread","session_id":"session","turn_id":"turn","root_turn_id":"root","response_id":"response","usage":counts,"turn_token_usage":counts,"thread_token_usage":counts}),
    )
}
fn bytes(rows: &[Value]) -> Vec<u8> {
    format!(
        "{}\n",
        rows.iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    )
    .into_bytes()
}
fn source(store: &UsageStore) {
    store.create_source(&json!({"id":"synthetic","displayName":"Synthetic","mode":"imported","provider":"synthetic","coverageDescription":"Synthetic only"})).unwrap();
}
fn store() -> UsageStore {
    let store = UsageStore::in_memory().unwrap();
    source(&store);
    store
}
fn import(store: &UsageStore, rows: &[Value], stream: &str) -> Value {
    import_incremental_rollout(store, &bytes(rows), &options(stream)).unwrap()
}
fn checkpoint(store: &UsageStore, stream: &str) -> Value {
    store
        .get_rollout_checkpoint(&json!({"sourceId":"synthetic","streamId":stream}))
        .unwrap()
}
fn events(store: &UsageStore) -> Vec<Value> {
    store
        .get_events(&json!({"sourceId":"synthetic","limit":500}))
        .unwrap()["events"]
        .as_array()
        .unwrap()
        .clone()
}
fn db_version(path: &std::path::Path) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}
fn projection(store: &UsageStore, rows: &[Value], stream: &str) -> Value {
    let mut opts = options(stream);
    opts["captureContent"] = json!(false);
    let mut value = parse_incremental_rollout(&bytes(rows), &opts).unwrap();
    value["importedAt"] = json!(NOW);
    value["expectedCheckpoint"] = checkpoint(store, stream);
    value
}
#[test]
fn append_noop_restart_and_explicit_version_upgrade() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store);
    assert_eq!(db_version(&path), 2);
    assert!(UsageStore::open_read_only(&path).is_ok());
    assert_eq!(db_version(&path), 2);
    let one = vec![meta(), message("one", "first")];
    assert_eq!(import(&store, &one, "stream")["eventsInserted"], "1");
    assert_eq!(db_version(&path), 3);
    drop(store);
    let before = fs::read(&path).unwrap();
    let readonly = UsageStore::open_read_only(&path).unwrap();
    assert_eq!(events(&readonly).len(), 1);
    drop(readonly);
    assert_eq!(fs::read(&path).unwrap(), before);
    let store = UsageStore::open(&path).unwrap();
    assert_eq!(import(&store, &one, "stream")["checkpointAdvanced"], false);
    let mut two = one.clone();
    two.push(message("two", "next"));
    two.push(token(2));
    let added = import(&store, &two, "stream");
    assert_eq!(added["eventsInserted"], "1");
    assert_eq!(added["responseTokensInserted"], "1");
    assert_eq!(events(&store).len(), 2);
    assert_eq!(import(&store, &two, "copy")["eventsInserted"], "0");
    assert_eq!(
        import(&store, &two, "stream")["responseTokensInserted"],
        "0"
    );
}
#[test]
fn pending_tail_is_never_persisted_and_skill_completion_crosses_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store);
    let prefix = bytes(&[meta(), call()]);
    let mut partial = prefix.clone();
    partial.extend_from_slice(b"{\"NEVER PERSIST TAIL");
    partial.extend_from_slice(&[0xf0, 0x9f]);
    let first = import_incremental_rollout(&store, &partial, &options("stream")).unwrap();
    assert_eq!(first["eventsInserted"], "1");
    assert_eq!(first["completeBytes"], prefix.len());
    assert!(first["deferredBytes"].as_u64().unwrap() > 0);
    assert_eq!(
        import(&store, &[meta(), call(), output()], "stream")["eventsInserted"],
        "1"
    );
    assert_eq!(
        events(&store)
            .iter()
            .filter(|v| v["eventType"] == "skill_loaded")
            .count(),
        1
    );
    assert_eq!(
        import(&store, &[meta(), call(), output()], "stream")["eventsInserted"],
        "0"
    );
    let data = String::from_utf8_lossy(&fs::read(path).unwrap()).into_owned();
    assert!(!data.contains("NEVER PERSIST"));
    assert!(!data.contains("toolArguments"));
}
#[test]
fn malformed_complete_line_truncate_regrowth_and_version_changes_are_atomic() {
    let store = store();
    let rows = vec![meta(), message("one", "first")];
    let complete = bytes(&rows);
    import(&store, &rows, "stream");
    let before = checkpoint(&store, "stream");
    for invalid in [
        complete[..complete.len() - 1].to_vec(),
        bytes(&[meta(), message("one", "changed longer value")]),
    ] {
        assert_eq!(
            import_incremental_rollout(&store, &invalid, &options("stream"))
                .unwrap_err()
                .0,
            "rollout_prefix_changed"
        );
        assert_eq!(checkpoint(&store, "stream"), before);
    }
    let mut malformed = complete.clone();
    malformed.extend_from_slice(b"{bad}\n");
    assert_eq!(
        import_incremental_rollout(&store, &malformed, &options("stream"))
            .unwrap_err()
            .0,
        "rollout_invalid_json"
    );
    let mut opts = options("stream");
    opts["sourceVersion"] = json!("unsupported");
    assert_eq!(
        import_incremental_rollout(&store, &complete, &opts)
            .unwrap_err()
            .0,
        "rollout_stream_version_mismatch"
    );
    assert_eq!(checkpoint(&store, "stream"), before);
    assert_eq!(events(&store).len(), 1);
}
#[test]
fn identity_conflicts_across_streams_reject_entire_batch_and_checkpoint() {
    let store = store();
    import(
        &store,
        &[meta(), message("one", "first"), token(2)],
        "stream",
    );
    let changed = [
        meta(),
        message("new", "would insert"),
        message("one", "CHANGED"),
    ];
    assert_eq!(
        import_incremental_rollout(&store, &bytes(&changed), &options("rotation"))
            .unwrap_err()
            .0,
        "rollout_identity_conflict"
    );
    assert_eq!(checkpoint(&store, "rotation"), Value::Null);
    assert_eq!(events(&store).len(), 1);
    assert_eq!(
        import_incremental_rollout(
            &store,
            &bytes(&[meta(), message("new", "would insert"), token(99)]),
            &options("rotation")
        )
        .unwrap_err()
        .0,
        "rollout_identity_conflict"
    );
    assert_eq!(events(&store).len(), 1);
    assert_eq!(
        import(&store, &[meta(), message("other", "safe")], "rotation")["eventsInserted"],
        "1"
    );
}
#[test]
fn content_enable_disable_delete_and_retention_never_backfill() {
    let store = store();
    let mut rows = vec![meta(), message("one", "OLD SECRET"), token(2)];
    import(&store, &rows, "stream");
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    rows.push(message("two", "NEW SECRET"));
    import(&store, &rows, "stream");
    let list = events(&store);
    assert_eq!(list.len(), 2);
    assert_eq!(
        list.iter()
            .filter(|e| store
                .get_local_event_detail(&json!({"sourceId":"synthetic","eventId":e["eventId"]}))
                .unwrap()["contentRetained"]
                == true)
            .count(),
        1
    );
    store
        .clear_local_content(&json!({"sourceId":"synthetic"}))
        .unwrap();
    assert_eq!(import(&store, &rows, "copy")["eventsInserted"], "0");
    for e in events(&store) {
        assert_eq!(
            store
                .get_local_event_detail(&json!({"sourceId":"synthetic","eventId":e["eventId"]}))
                .unwrap()["contentRetained"],
            false
        );
    }
    store
        .apply_retention(&json!({"now":"2027-10-03T00:00:00.000Z"}))
        .unwrap();
    assert!(events(&store).is_empty());
    assert_eq!(import(&store, &rows, "stream")["eventsInserted"], "0");
    assert_eq!(
        import(&store, &rows, "another")["responseTokensInserted"],
        "0"
    );
    assert!(events(&store).is_empty());
    rows.push(message("three", "FUTURE"));
    assert_eq!(import(&store, &rows, "stream")["eventsInserted"], "1");
    store.clear_data(&json!({"sourceId":"synthetic"})).unwrap();
    assert_eq!(checkpoint(&store, "stream"), Value::Null);
    assert_eq!(import(&store, &rows, "stream")["eventsInserted"], "3");
    store.delete_source("synthetic").unwrap();
    source(&store);
    assert_eq!(checkpoint(&store, "stream"), Value::Null);
}
#[test]
fn failure_rolls_back_schema_migration_and_cas_prevents_stale_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store);
    let rows = vec![meta(), message("one", "first")];
    let stale = projection(&store, &rows, "stream");
    let mut bad = stale.clone();
    bad["events"][0]["evidenceType"] = json!("wrong");
    assert_eq!(
        store.import_rollout_incremental(&bad).unwrap_err().code(),
        "invalid_input"
    );
    assert_eq!(db_version(&path), 2);
    assert!(events(&store).is_empty());
    store
        .update_settings(&json!({"capturePaused":true}))
        .unwrap();
    assert_eq!(
        store.import_rollout_incremental(&stale).unwrap_err().code(),
        "capture_paused"
    );
    assert_eq!(db_version(&path), 2);
    store
        .update_settings(&json!({"capturePaused":false}))
        .unwrap();
    store.import_rollout_incremental(&stale).unwrap();
    assert_eq!(
        store.import_rollout_incremental(&stale).unwrap_err().code(),
        "rollout_checkpoint_conflict"
    );
    assert_eq!(events(&store).len(), 1);
}
#[test]
fn concurrent_connections_only_one_expected_checkpoint_can_commit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store);
    let candidate = projection(&store, &[meta(), message("one", "first")], "stream");
    drop(store);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let candidate = candidate.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let store = UsageStore::open(path).unwrap();
                barrier.wait();
                store.import_rollout_incremental(&candidate)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .find_map(|r| r.as_ref().err())
            .unwrap()
            .code(),
        "rollout_checkpoint_conflict"
    );
}
#[test]
fn snapshot_replay_after_incremental_retention_cannot_resurrect_known_ids() {
    let store = store();
    let rows = vec![meta(), message("one", "first"), token(2)];
    import(&store, &rows, "stream");
    store
        .apply_retention(&json!({"now":"2027-10-03T00:00:00.000Z"}))
        .unwrap();
    let opts = json!({"sourceId":"synthetic","observedAt":NOW,"captureContent":false,"sourceVersion":ROLLOUT_SOURCE_VERSION});
    let mut parsed = usage_lens::adapters::rollout::parse_rollout(&bytes(&rows), &opts).unwrap();
    parsed.as_object_mut().unwrap().remove("warnings");
    parsed.as_object_mut().unwrap().remove("recordsSeen");
    parsed["importedAt"] = json!(NOW);
    let result = store.import_rollout(&parsed).unwrap();
    assert_eq!(result["eventsInserted"], "0");
    assert_eq!(result["responseTokensInserted"], "0");
    assert!(events(&store).is_empty());
}
#[test]
fn cli_explicit_path_stream_file_and_version_are_required() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let file = dir.path().join("synthetic.jsonl");
    let store = UsageStore::open(&path).unwrap();
    source(&store);
    fs::write(&file, bytes(&[meta(), message("one", "first")])).unwrap();
    let args = vec![
        "import-rollout-incremental",
        "--db",
        path.to_str().unwrap(),
        "--source",
        "synthetic",
        "--file",
        file.to_str().unwrap(),
        "--stream",
        "stream",
        "--source-version",
        ROLLOUT_SOURCE_VERSION,
    ];
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_usage-lens"))
            .args(args)
            .output()
            .unwrap()
    };
    assert!(run(&args).status.success());
    assert!(run(&args).status.success());
    let mut missing = args.clone();
    missing.truncate(7);
    assert!(!run(&missing).status.success());
    let mut relative = args.clone();
    relative[6] = "relative.jsonl";
    assert!(
        String::from_utf8(run(&relative).stderr)
            .unwrap()
            .contains("absolute_import_path_required")
    );
    assert_eq!(events(&store).len(), 1);
}

#[test]
fn existing_snapshot_records_are_tombstoned_on_upgrade_and_conflicts_fail() {
    let store = store();
    let rows = vec![meta(), message("existing", "first"), token(2)];
    let mut opts = options("stream");
    opts["captureContent"] = json!(false);
    let mut snapshot = usage_lens::adapters::rollout::parse_rollout(&bytes(&rows), &opts).unwrap();
    for field in ["warnings", "recordsSeen"] {
        snapshot.as_object_mut().unwrap().remove(field);
    }
    snapshot["importedAt"] = json!(NOW);
    store.import_rollout(&snapshot).unwrap();
    assert_eq!(import(&store, &rows, "stream")["eventsInserted"], "0");
    let mut changed = projection(&store, &rows, "other");
    changed["events"][0]["model"] = json!("different");
    assert_eq!(
        store
            .import_rollout_incremental(&changed)
            .unwrap_err()
            .code(),
        "rollout_identity_conflict"
    );
    store
        .apply_retention(&json!({"now":"2027-10-03T00:00:00.000Z"}))
        .unwrap();
    assert_eq!(
        store.import_rollout(&snapshot).unwrap()["eventsInserted"],
        "0"
    );
    store.clear_data(&json!({})).unwrap();
    assert_eq!(checkpoint(&store, "stream"), Value::Null);
}

#[test]
fn checkpoint_commit_failure_rolls_back_rows_replays_and_progress() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store);
    let mut rows = vec![meta(), message("one", "first")];
    import(&store, &rows, "stream");
    let before = checkpoint(&store, "stream");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_checkpoint BEFORE UPDATE ON rollout_checkpoints BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    rows.push(message("two", "second"));
    assert_eq!(
        import_incremental_rollout(&store, &bytes(&rows), &options("stream"))
            .unwrap_err()
            .0,
        "storage_error"
    );
    assert_eq!(checkpoint(&store, "stream"), before);
    assert_eq!(events(&store).len(), 1);
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM rollout_replays WHERE kind='event'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    db.execute_batch("DROP TRIGGER fail_checkpoint").unwrap();
    assert_eq!(import(&store, &rows, "stream")["eventsInserted"], "1");
}

#[test]
fn strict_projection_shapes_positions_and_versions_fail_before_upgrade() {
    let store = store();
    let valid = projection(
        &store,
        &[meta(), message("one", "first"), token(2)],
        "stream",
    );
    let cases = [
        ("fingerprint", json!("bad")),
        ("fingerprint", json!(null)),
        ("adapterVersion", json!("future")),
        ("sourceVersion", json!("future")),
        ("completeBytes", json!(-1)),
        ("completeLines", json!(20001)),
        ("deferredBytes", json!(8 * 1024 * 1024)),
        ("recordsSeen", json!(20001)),
        ("warnings", json!(null)),
        ("warnings", json!([null])),
        ("warnings", json!(vec!["warning"; 101])),
        ("events", json!(null)),
        ("responseTokens", json!(null)),
        ("eventPositions", json!([1])),
        ("responsePositions", json!([1])),
        ("eventPositions", json!([])),
        ("responsePositions", json!(null)),
        ("replayRecords", json!(null)),
        ("replayRecords", json!([])),
        ("expectedCheckpoint", json!({"unknown":0})),
        ("extra", json!(true)),
    ];
    for (field, value) in cases {
        let mut invalid = valid.clone();
        invalid[field] = value;
        assert!(
            store.import_rollout_incremental(&invalid).is_err(),
            "{field}"
        );
        assert_eq!(store.get_status().unwrap()["schemaVersion"], 2);
    }
    for (field, value) in [
        ("digest", json!("bad")),
        ("identity", json!("bad identity")),
        ("kind", json!("unknown")),
        ("line", json!(0)),
        ("extra", json!(true)),
    ] {
        let mut invalid = valid.clone();
        invalid["replayRecords"][0][field] = value;
        assert!(store.import_rollout_incremental(&invalid).is_err());
    }
    let mut duplicate = valid.clone();
    let entry = duplicate["replayRecords"][0].clone();
    duplicate["replayRecords"]
        .as_array_mut()
        .unwrap()
        .push(entry);
    assert_eq!(
        store.import_rollout_incremental(&duplicate).unwrap()["eventsInserted"],
        "1"
    );
    let mut live_opts = options("live");
    live_opts["sourceId"] = json!("live");
    store.create_source(&json!({"id":"live","displayName":"Live","mode":"live","provider":"synthetic","coverageDescription":"Synthetic only"})).unwrap();
    assert_eq!(
        import_incremental_rollout(&store, &bytes(&[meta()]), &live_opts)
            .unwrap_err()
            .0,
        "imported_source_required"
    );
    let mut bad_options = options("x");
    bad_options["extra"] = json!(true);
    assert_eq!(
        import_incremental_rollout(&store, b"", &bad_options)
            .unwrap_err()
            .0,
        "invalid_input"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn explicit_incremental_file_reader_rejects_symlink_fifo_and_directory() {
    use std::os::unix::fs::symlink;
    use usage_lens::cli::read_bounded_file;
    let dir = tempfile::tempdir().unwrap();
    let selected = dir.path().join("selected.jsonl");
    let link = dir.path().join("link.jsonl");
    let fifo = dir.path().join("pipe");
    fs::write(&selected, bytes(&[meta()])).unwrap();
    symlink(&selected, &link).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    for path in [&link, &fifo, dir.path()] {
        assert_eq!(
            read_bounded_file(path, 8 * 1024 * 1024)
                .await
                .unwrap_err()
                .0,
            "import_file_required"
        );
    }
}

#[test]
fn canonical_source_event_aliases_cannot_resurrect_content_after_adoption_and_retention() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store);
    let rows = vec![meta(), message("stable", "OLD-CONTENT-CANARY")];
    let parsed = projection(&store, &rows, "one");
    let mut aliased = parsed["events"][0].clone();
    let canonical = aliased["eventId"].clone();
    aliased["eventId"] = json!("external-alias");
    store.ingest_event(&aliased).unwrap();
    assert_eq!(import(&store, &rows, "one")["eventsInserted"], "0");
    assert_eq!(events(&store)[0]["eventId"], "external-alias");
    let db = rusqlite::Connection::open(&path).unwrap();
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM record_tombstones WHERE source_id='synthetic'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 3);
    // Adoption must check immutable projected metadata, even when another local ID owns the row.
    let mut mismatch = parsed.clone();
    mismatch["streamId"] = json!("conflicting");
    mismatch["events"][0]["model"] = json!("changed");
    assert_eq!(
        store
            .import_rollout_incremental(&mismatch)
            .unwrap_err()
            .code(),
        "rollout_identity_conflict"
    );
    assert_eq!(checkpoint(&store, "conflicting"), Value::Null);
    store
        .apply_retention(&json!({"now":"2027-10-03T00:00:00.000Z"}))
        .unwrap();
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    assert_eq!(import(&store, &rows, "two")["eventsInserted"], "0");
    assert!(events(&store).is_empty());
    assert_eq!(
        store
            .get_local_event_detail(&json!({"sourceId":"synthetic","eventId":canonical}))
            .unwrap_err()
            .code(),
        "event_not_found"
    );
    // A newly supplied alternative local ID is also remembered when its canonical alias is suppressed.
    aliased["eventId"] = json!("third-alias");
    aliased["content"] = json!({"body":"OLD-CONTENT-CANARY"});
    assert_eq!(store.ingest_event(&aliased).unwrap()["inserted"], false);
    let mut removed_alias = aliased.clone();
    removed_alias["sourceEventId"] = Value::Null;
    assert_eq!(
        store.ingest_event(&removed_alias).unwrap_err().code(),
        "rollout_identity_conflict"
    );
    // A conflicting alias in a batch rolls back earlier inserts and their replay metadata.
    let mut fresh = aliased.clone();
    fresh["eventId"] = json!("fresh");
    fresh["sourceEventId"] = json!("fresh-source-id");
    let mut changed = aliased;
    changed["model"] = json!("changed");
    assert_eq!(
        store
            .ingest_events(&json!([fresh, changed]))
            .unwrap_err()
            .code(),
        "rollout_identity_conflict"
    );
    assert!(events(&store).is_empty());
    let fresh_count: i64 = db
        .query_row(
            "SELECT count(*) FROM record_tombstones WHERE identity='fresh'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fresh_count, 0);
    drop(db);
    let data = String::from_utf8_lossy(&fs::read(path).unwrap()).into_owned();
    assert!(!data.contains("OLD-CONTENT-CANARY"));
}

#[test]
fn preincremental_capture_off_adoption_checks_metadata_without_inventing_old_body_hashes() {
    let store = store();
    let initial = projection(
        &store,
        &[meta(), message("stable", "UNRETAINED ORIGINAL")],
        "one",
    );
    let mut original = initial["events"][0].clone();
    original["content"] = json!({"body":"UNRETAINED ORIGINAL"});
    store.ingest_event(&original).unwrap();
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    assert_eq!(
        import(
            &store,
            &[
                meta(),
                message("stable", "DIFFERENT FIRST INCREMENTAL BODY")
            ],
            "one"
        )["eventsInserted"],
        "0"
    );
    assert_eq!(
        store
            .get_local_event_detail(&json!({"sourceId":"synthetic","eventId":original["eventId"]}))
            .unwrap()["contentRetained"],
        false
    );
    assert_eq!(
        import_incremental_rollout(
            &store,
            &bytes(&[meta(), message("stable", "LATER CHANGED BODY")]),
            &options("two")
        )
        .unwrap_err()
        .0,
        "rollout_identity_conflict"
    );
}

#[test]
fn incremental_import_normalizes_only_identity_conflicts_without_changing_snapshot_contracts() {
    let mut changed_call = call();
    changed_call["payload"]["arguments"] = json!("{\"package\":\"skill://synthetic/changed\"}");
    let mut changed_output = output();
    changed_output["payload"]["output"] = json!("changed output");
    let cases = [
        (
            message("duplicate", "one"),
            message("duplicate", "two"),
            "rollout_conflicting_event_identity",
        ),
        (token(2), token(3), "rollout_conflicting_response_identity"),
        (call(), changed_call, "rollout_conflicting_call_identity"),
        (
            output(),
            changed_output,
            "rollout_conflicting_output_identity",
        ),
    ];
    for (first, second, parser_error) in cases {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        let file = dir.path().join("synthetic.jsonl");
        let store = UsageStore::open(&path).unwrap();
        source(&store);
        let data = bytes(&[meta(), first, second]);
        let mut parser_options = options("stream");
        parser_options["captureContent"] = json!(false);
        assert_eq!(
            parse_incremental_rollout(&data, &parser_options)
                .unwrap_err()
                .0,
            parser_error
        );
        let snapshot = usage_lens::adapters::rollout::parse_rollout(&data, &parser_options);
        if parser_error == "rollout_conflicting_event_identity" {
            // Snapshot message projection deliberately preserves its existing last-wins behavior.
            assert_eq!(snapshot.unwrap()["events"].as_array().unwrap().len(), 1);
        } else {
            assert_eq!(snapshot.unwrap_err().0, parser_error);
        }
        assert_eq!(
            import_incremental_rollout(&store, &data, &options("stream"))
                .unwrap_err()
                .0,
            "rollout_identity_conflict"
        );
        assert_eq!(db_version(&path), 2);
        assert_eq!(checkpoint(&store, "stream"), Value::Null);
        fs::write(&file, &data).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_usage-lens"))
            .args([
                "import-rollout-incremental",
                "--db",
                path.to_str().unwrap(),
                "--source",
                "synthetic",
                "--file",
                file.to_str().unwrap(),
                "--stream",
                "stream",
                "--source-version",
                ROLLOUT_SOURCE_VERSION,
            ])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert_eq!(
            String::from_utf8(result.stderr).unwrap(),
            "Usage Lens: rollout_identity_conflict\n"
        );
        // The same conflict appended after an accepted checkpoint also leaves progress untouched.
        import(&store, &[meta()], "stream");
        let before = checkpoint(&store, "stream");
        assert_eq!(
            import_incremental_rollout(&store, &data, &options("stream"))
                .unwrap_err()
                .0,
            "rollout_identity_conflict"
        );
        assert_eq!(checkpoint(&store, "stream"), before);
        assert!(events(&store).is_empty());
    }
    let store = store();
    for (data, code) in [
        (b"{bad}\n".as_slice(), "rollout_invalid_json"),
        (b"\xff\n".as_slice(), "rollout_invalid_utf8"),
    ] {
        assert_eq!(
            import_incremental_rollout(&store, data, &options("other"))
                .unwrap_err()
                .0,
            code
        );
    }
}
