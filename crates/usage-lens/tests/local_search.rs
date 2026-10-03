use axum::{body::Body, http::Request};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use usage_lens::{
    core::UsageStore,
    server::{http, mcp},
};

const NOW: &str = "2026-10-02T12:00:00Z";
fn source(store: &UsageStore, id: &str) {
    store.create_source(&json!({"id":id,"displayName":"Synthetic search","mode":"imported","provider":"synthetic","coverageDescription":"Synthetic tests only"})).unwrap();
}
fn memory() -> UsageStore {
    let store = UsageStore::open(":memory:").unwrap();
    source(&store, "a");
    source(&store, "b");
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    store
}
fn event(id: &str) -> Value {
    json!({"sourceId":"a","eventId":id,"eventType":"user_prompt","evidenceType":"explicit_user_message","observedAt":NOW,"model":"model-a","collectorVersion":"synthetic-test","content":{"body":"Synthetic needle %_ text"}})
}
fn query() -> Value {
    json!({"sourceId":"a","query":"needle"})
}
fn ids(value: &Value) -> Vec<&str> {
    value["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["eventId"].as_str().unwrap())
        .collect()
}
fn cursor(value: &Value) -> Value {
    serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(value["nextCursor"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn assert_invalid(store: &UsageStore, input: &Value) {
    assert_eq!(
        store.search_local_details(input).unwrap_err().code(),
        "invalid_input",
        "{input}"
    );
}

#[test]
fn search_metadata_empty_literal_case_sensitive_and_source_local() {
    let store = memory();
    let empty = store.search_local_details(&query()).unwrap();
    assert_eq!(empty["source"]["id"], "a");
    assert_eq!(empty["events"], json!([]));
    assert!(empty["nextCursor"].is_null());
    assert_eq!(
        empty["search"],
        json!({"scope":"retained_redacted_local_content","candidateLimit":10000,"searchedRecordCount":"0","matchedCount":"0","truncated":false})
    );
    store.ingest_event(&event("retained")).unwrap();
    let mut foreign = event("foreign");
    foreign["sourceId"] = json!("b");
    store.ingest_event(&foreign).unwrap();
    let mut missing = event("missing");
    missing.as_object_mut().unwrap().remove("content");
    store.ingest_event(&missing).unwrap();
    for (term, expected) in [
        ("needle", 1),
        ("Needle", 0),
        ("%_", 1),
        ("%unlikely", 0),
        ("' OR 1=1 --", 0),
        ("不存在", 0),
    ] {
        let found = store
            .search_local_details(&json!({"sourceId":"a","query":term}))
            .unwrap();
        assert_eq!(found["events"].as_array().unwrap().len(), expected);
        assert_eq!(found["search"]["searchedRecordCount"], "1");
        assert!(!found.to_string().contains("Synthetic needle"));
        assert!(
            found["events"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e.get("content").is_none())
        );
    }
    assert_eq!(
        ids(&store
            .search_local_details(&json!({"sourceId":"b","query":"needle"}))
            .unwrap()),
        ["foreign"]
    );
    store.clear_local_content(&json!({"sourceId":"a"})).unwrap();
    assert_eq!(
        store.search_local_details(&query()).unwrap()["search"]["matchedCount"],
        "0"
    );
}

#[test]
fn exact_filters_match_event_dates_and_apply_before_candidate_window() {
    let store = memory();
    for (id, occurred, model, tool) in [
        ("prompt-today", None, "model-a", false),
        (
            "prompt-yesterday",
            Some("2026-10-01T23:59:59Z"),
            "model-a",
            false,
        ),
        ("other-model", None, "model-b", false),
        ("tool-today", None, "model-a", true),
        (
            "tool-yesterday",
            Some("2026-10-01T23:59:59Z"),
            "model-a",
            true,
        ),
    ] {
        let mut e = event(id);
        e["occurredAt"] = json!(occurred);
        e["model"] = json!(model);
        if tool {
            e["eventType"] = json!("tool_call");
            e["evidenceType"] = json!("explicit_tool_call");
            e["toolName"] = json!("synthetic");
        }
        store.ingest_event(&e).unwrap();
    }
    for filters in [
        json!({"fromDate":"2026-10-01","toDate":"2026-10-01"}),
        json!({"eventType":"tool_call"}),
        json!({"model":"model-b"}),
        json!({"fromDate":"2026-10-02","toDate":"2026-10-02","eventType":"user_prompt","model":"model-a"}),
        json!({"model":"unobserved"}),
    ] {
        let mut input = query();
        input
            .as_object_mut()
            .unwrap()
            .extend(filters.as_object().unwrap().clone());
        let found = store.search_local_details(&input).unwrap();
        input.as_object_mut().unwrap().remove("query");
        assert_eq!(found["events"], store.get_events(&input).unwrap()["events"]);
        assert_eq!(
            found["search"]["searchedRecordCount"],
            found["events"].as_array().unwrap().len().to_string()
        );
    }
}

#[test]
fn stable_ties_page_size_changes_and_context_bound_cursors() {
    let store = memory();
    for id in ["a", "b", "c", "d", "e"] {
        store.ingest_event(&event(id)).unwrap();
    }
    let first = store
        .search_local_details(&json!({"sourceId":"a","query":"needle","limit":2}))
        .unwrap();
    assert_eq!(ids(&first), ["e", "d"]);
    assert_eq!(first["search"]["matchedCount"], "5");
    let mut input = json!({"sourceId":"a","query":"needle","limit":1,"cursor":first["nextCursor"]});
    let second = store.search_local_details(&input).unwrap();
    assert_eq!(ids(&second), ["c"]);
    input["cursor"] = second["nextCursor"].clone();
    input["limit"] = json!(500);
    let last = store.search_local_details(&input).unwrap();
    assert_eq!(ids(&last), ["b", "a"]);
    assert!(last["nextCursor"].is_null());
    for change in [
        json!({"sourceId":"b"}),
        json!({"query":"Needle"}),
        json!({"eventType":"user_prompt"}),
        json!({"model":"model-a"}),
        json!({"fromDate":"2026-10-02","toDate":"2026-10-02"}),
    ] {
        let mut wrong = input.clone();
        wrong
            .as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());
        assert_invalid(&store, &wrong);
    }
    // Non-search cursors must never be accepted here, or vice versa.
    let events = store
        .get_events(&json!({"sourceId":"a","limit":1}))
        .unwrap();
    input["cursor"] = events["nextCursor"].clone();
    assert_invalid(&store, &input);
    assert!(
        store
            .get_events(&json!({"sourceId":"a","cursor":first["nextCursor"]}))
            .is_err()
    );
    assert!(
        !String::from_utf8(
            URL_SAFE_NO_PAD
                .decode(first["nextCursor"].as_str().unwrap())
                .unwrap()
        )
        .unwrap()
        .contains("needle")
    );
}

#[test]
fn input_and_cursor_validation_fail_closed_without_echoing_search_text() {
    let store = memory();
    for id in ["a", "b"] {
        store.ingest_event(&event(id)).unwrap();
    }
    for change in [
        json!({"query":""}),
        json!({"query":null}),
        json!({"query":4}),
        json!({"query":"x".repeat(201)}),
        json!({"query":"a\nb"}),
        json!({"unknown":"private"}),
        json!({"limit":0}),
        json!({"limit":501}),
        json!({"limit":1.5}),
        json!({"fromDate":"2026-10-01"}),
        json!({"toDate":"2026-10-02"}),
        json!({"fromDate":"2026-02-30","toDate":"2026-10-02"}),
        json!({"fromDate":"2026-10-03","toDate":"2026-10-02"}),
        json!({"model":null}),
        json!({"eventType":"system"}),
    ] {
        let mut input = query();
        input
            .as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());
        assert_invalid(&store, &input);
    }
    for bad in [
        Value::Null,
        json!(3),
        json!(""),
        json!("a".repeat(1601)),
        json!("a+b"),
        json!("a"),
        json!(URL_SAFE_NO_PAD.encode([255])),
        json!(URL_SAFE_NO_PAD.encode("not json")),
        json!(URL_SAFE_NO_PAD.encode("{}")),
    ] {
        let mut input = query();
        input["cursor"] = bad;
        assert_invalid(&store, &input);
    }
    let first = store
        .search_local_details(&json!({"sourceId":"a","query":"needle","limit":1}))
        .unwrap();
    for (key, bad) in [
        ("extra", json!(true)),
        ("version", json!(2)),
        ("scope", json!("wrong")),
        ("ceiling", json!(0)),
        ("ceiling", json!(null)),
        ("floorAt", json!("bad")),
        ("floorKey", json!("bad key")),
        ("at", json!("bad")),
        ("key", json!("bad key")),
        ("truncated", json!("false")),
        ("window", json!(null)),
        ("window", json!("wrong")),
        ("at", json!("2026-10-01T00:00:00Z")),
    ] {
        let mut changed = cursor(&first);
        changed[key] = bad;
        let mut input = query();
        input["cursor"] = json!(URL_SAFE_NO_PAD.encode(changed.to_string()));
        assert_invalid(&store, &input);
    }
    assert_eq!(
        store
            .search_local_details(&json!({"sourceId":"missing","query":"PRIVATE_CANARY"}))
            .unwrap_err()
            .to_string(),
        "source_not_found"
    );
}

#[test]
fn candidate_cap_and_pinned_boundary_exclude_refills_and_later_ingests() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.sqlite");
    let store = UsageStore::open(&path).unwrap();
    source(&store, "a");
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    for batch in 0..11 {
        let values: Vec<_> = (batch * 1000..((batch + 1) * 1000).min(10003))
            .map(|i| {
                let mut e = event(&format!("record-{i:05}"));
                if i < 3 {
                    e["model"] = json!("older-model");
                }
                e
            })
            .collect();
        store.ingest_events(&json!(values)).unwrap();
    }
    let filtered = store
        .search_local_details(&json!({"sourceId":"a","query":"needle","model":"older-model"}))
        .unwrap();
    assert_eq!(
        ids(&filtered),
        ["record-00002", "record-00001", "record-00000"]
    );
    assert_eq!(filtered["search"]["truncated"], false);
    let first = store
        .search_local_details(&json!({"sourceId":"a","query":"needle","limit":500}))
        .unwrap();
    assert_eq!(first["search"]["searchedRecordCount"], "10000");
    assert_eq!(first["search"]["matchedCount"], "10000");
    assert_eq!(first["search"]["truncated"], true);
    assert_eq!(ids(&first)[0], "record-10002");
    store.ingest_event(&event("record-99999")).unwrap();
    store.ingest_event(&event("record-08500-new")).unwrap();
    let mut backdated = event("record-08500-backdated");
    backdated["observedAt"] = json!("2026-10-01T12:00:00Z");
    store.ingest_event(&backdated).unwrap();
    let mut next = first["nextCursor"].clone();
    let mut seen: Vec<String> = ids(&first).into_iter().map(str::to_owned).collect();
    while !next.is_null() {
        let page = store
            .search_local_details(
                &json!({"sourceId":"a","query":"needle","limit":500,"cursor":next}),
            )
            .unwrap();
        assert_eq!(page["search"]["searchedRecordCount"], "10000");
        assert_eq!(page["search"]["matchedCount"], "10000");
        assert_eq!(page["search"]["truncated"], true);
        seen.extend(ids(&page).into_iter().map(str::to_owned));
        next = page["nextCursor"].clone();
    }
    assert_eq!(seen.len(), 10000);
    assert_eq!(seen.last().unwrap(), "record-00003");
    assert!(seen.windows(2).all(|pair| pair[0] > pair[1]));
    assert!(
        !seen
            .iter()
            .any(|id| id.ends_with("new") || id.ends_with("backdated") || id == "record-99999")
    );
    // Starting over is the explicit refresh operation and includes new records.
    let fresh = store.search_local_details(&query()).unwrap();
    assert_eq!(ids(&fresh)[0], "record-99999");
    // Deleting the floor invalidates the cursor instead of silently refilling
    // the original window with older content or claiming unchanged coverage.
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute(
        "DELETE FROM event_details WHERE event_id='record-00003'",
        [],
    )
    .unwrap();
    assert_invalid(
        &store,
        &json!({"sourceId":"a","query":"needle","cursor":first["nextCursor"]}),
    );
    store.clear_local_content(&json!({"sourceId":"a"})).unwrap();
    assert_invalid(
        &store,
        &json!({"sourceId":"a","query":"needle","cursor":first["nextCursor"]}),
    );
}

#[test]
fn reused_rowids_cannot_substitute_new_content_or_identities() {
    let store = memory();
    for id in ["a", "b", "c"] {
        store.ingest_event(&event(id)).unwrap();
    }
    let first = store
        .search_local_details(&json!({"sourceId":"a","query":"needle","limit":1}))
        .unwrap();
    let continuation = json!({"sourceId":"a","query":"needle","cursor":first["nextCursor"]});
    // Recreated identical observations are observationally equivalent.
    store.clear_data(&json!({"sourceId":"a"})).unwrap();
    for id in ["a", "b", "c"] {
        store.ingest_event(&event(id)).unwrap();
    }
    assert_eq!(
        ids(&store.search_local_details(&continuation).unwrap()),
        ["b", "a"]
    );
    // Reusing every physical rowid and every event ID cannot hide changed text.
    store.clear_data(&json!({"sourceId":"a"})).unwrap();
    for id in ["a", "b", "c"] {
        let mut e = event(id);
        if id == "b" {
            e["content"]["body"] = json!("Changed needle content");
        }
        store.ingest_event(&e).unwrap();
    }
    assert_invalid(&store, &continuation);
    // A different identity within the old window is rejected, too.
    store.clear_data(&json!({"sourceId":"a"})).unwrap();
    for id in ["a", "b-new", "c"] {
        store.ingest_event(&event(id)).unwrap();
    }
    assert_invalid(&store, &continuation);
}

#[test]
fn read_only_search_preserves_persisted_database_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.sqlite");
    let mut store = UsageStore::open(&path).unwrap();
    source(&store, "a");
    store
        .update_settings(&json!({"contentCaptureEnabled":true}))
        .unwrap();
    for id in ["a", "b"] {
        store.ingest_event(&event(id)).unwrap();
    }
    store.close().unwrap();
    let before = std::fs::read(&path).unwrap();
    let readonly = UsageStore::open_read_only(&path).unwrap();
    let first = readonly
        .search_local_details(&json!({"sourceId":"a","query":"needle","limit":1}))
        .unwrap();
    assert_eq!(
        ids(&readonly
            .search_local_details(
                &json!({"sourceId":"a","query":"needle","cursor":first["nextCursor"]})
            )
            .unwrap()),
        ["a"]
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

async fn request(app: axum::Router, path: &str) -> (u16, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .uri(path)
                .header("host", "127.0.0.1:4319")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
#[tokio::test]
async fn http_search_allows_filters_and_pagination_but_not_raw_content_or_mcp_search() {
    let store = memory();
    for id in ["a", "b", "c"] {
        store.ingest_event(&event(id)).unwrap();
    }
    // Local HTTP search remains absent from aggregate-only MCP tools.
    let definitions = mcp::tools_list();
    assert!(!definitions.to_string().contains("search"));
    let app = http::router(store, 4319);
    let base = "/api/events/search?sourceId=a&query=needle&fromDate=2026-10-02&toDate=2026-10-02&eventType=user_prompt&model=model-a&limit=1";
    let (status, first) = request(app.clone(), base).await;
    assert_eq!(status, 200);
    assert_eq!(ids(&first), ["c"]);
    assert_eq!(first["source"]["id"], "a");
    let (status, second) = request(
        app.clone(),
        &format!("{base}&cursor={}", first["nextCursor"].as_str().unwrap()),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(ids(&second), ["b"]);
    assert!(!second.to_string().contains("Synthetic needle"));
    for suffix in ["&unexpected=1", "&model=model-b", "&cursor=bad"] {
        assert_eq!(
            request(app.clone(), &format!("{base}{suffix}")).await.0,
            400
        );
    }
    assert_eq!(
        request(
            app.clone(),
            "/api/events/search?sourceId=a&query=needle&fromDate=2026-10-02"
        )
        .await
        .0,
        400
    );
}

#[tokio::test]
async fn http_continues_maximum_identifier_cursors_with_bounded_query_limits() {
    let store = memory();
    let ids_long = ["a".repeat(160), "b".repeat(160), "c".repeat(160)];
    for id in &ids_long {
        store.ingest_event(&event(id)).unwrap();
    }
    let app = http::router(store, 4319);
    let base = "/api/events/search?sourceId=a&query=needle&limit=1";
    let (status, first) = request(app.clone(), base).await;
    assert_eq!(status, 200);
    let cursor = first["nextCursor"].as_str().unwrap();
    assert!(cursor.len() > 512 && cursor.len() <= 1600);
    let (status, second) = request(app.clone(), &format!("{base}&cursor={cursor}")).await;
    assert_eq!(status, 200);
    assert_eq!(ids(&second), [ids_long[1].as_str()]);
    assert_eq!(
        request(app.clone(), &format!("{base}&cursor={}", "a".repeat(1601)))
            .await
            .0,
        400
    );
    assert_eq!(
        request(
            app,
            &format!("/api/events/search?sourceId=a&query={}", "a".repeat(513))
        )
        .await
        .0,
        400
    );
}
