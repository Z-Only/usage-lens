use serde_json::{Value, json};
use usage_lens_ui::model::*;
fn fixture(key: &str) -> Value {
    serde_json::from_str::<Value>(include_str!("fixtures.json")).unwrap()[key].clone()
}
fn loaded() -> State {
    let mut s = State::default();
    let r = s.dispatch(Action::Refresh).remove(0);
    let jobs = s.complete(&r, Ok(fixture("status")));
    for r in jobs {
        let key = if r.slot == Slot::Overview {
            "overview"
        } else {
            "events"
        };
        s.complete(&r, Ok(fixture(key)));
    }
    s
}
fn request(s: &mut State, slot: Slot) -> Request {
    s.dispatch(Action::Load(slot, false)).remove(0)
}
#[test]
fn empty_defaults_do_not_invent_evidence() {
    let s = State::default();
    assert!(s.data(Slot::Overview).is_null());
    assert!(!s.loading(Slot::Detail));
    assert_eq!(s.error(Slot::Status), "");
    assert!(s.active_source().is_null());
    assert!(!s.confirm_enabled());
}
#[test]
fn labels_and_language_preferences() {
    for l in [Language::English, Language::Chinese] {
        for p in PAGES {
            assert!(!p.key().is_empty());
            assert!(!p.label(l).is_empty());
        }
        assert!(!l.code().is_empty());
        assert!(!l.html_lang().is_empty());
    }
    assert_eq!(Language::from_preference("zh"), Language::Chinese);
    assert_eq!(Language::from_preference("garbage"), Language::English);
}
#[test]
fn integer_precision_is_decimal_string_only() {
    for (v, want) in [
        (
            json!("9007199254740993123456"),
            "9,007,199,254,740,993,123,456",
        ),
        (json!("0"), "0"),
        (json!("1234"), "1,234"),
        (json!("12.3"), "—"),
        (Value::Null, "—"),
        (json!(1234), "—"),
        (json!(""), "—"),
    ] {
        assert_eq!(integer(&v), want);
    }
}
#[test]
fn cells_preserve_unknown_zero_and_reported_types() {
    assert_eq!(cell_text(&json!({"status":"reported","value":"0"})), "0");
    assert_eq!(cell_text(&json!({"status":"reported","value":0})), "0");
    assert_eq!(
        cell_text(&json!({"status":"reported","value":false})),
        "false"
    );
    for status in ["invalid", "omitted", "not_reported"] {
        let c = json!({"status":status,"value":null});
        assert_eq!(cell_text(&c), "—");
        assert_eq!(count_cell(&c), "—");
    }
    assert_eq!(display_value(&Value::Null, Language::Chinese), "未知");
    assert_eq!(display_value(&json!(false), Language::English), "false");
    assert_eq!(display_value(&json!("exact"), Language::English), "exact");
    assert_eq!(label(&json!("main_read")), "main read");
}
#[test]
fn query_encoding_cannot_change_endpoint_or_scope() {
    assert_eq!(
        api_path(
            "events",
            &[
                ("sourceId", "a&other=1 /中文".into()),
                ("empty", String::new())
            ]
        ),
        "/api/events?sourceId=a%26other%3D1%20%2F%E4%B8%AD%E6%96%87"
    );
    assert_eq!(api_path("status", &[]), "/api/status");
}
#[test]
fn calendar_gaps_are_unknown_not_zero() {
    let v = json!({"dailyUsageBuckets":{"status":"reported","value":[{"startDate":{"status":"reported","value":"2026-10-01"},"tokens":{"status":"reported","value":"0"}},{"startDate":{"status":"reported","value":"2026-10-03"},"tokens":{"status":"reported","value":"100"}}]}});
    let b = chart_buckets(&v, 7);
    assert_eq!(b.len(), 3);
    assert_eq!(b[1]["tokens"]["status"], "not_reported");
    assert_eq!(b[1]["startDate"]["value"], "2026-10-02");
    assert_eq!(chart_buckets(&v, 1).len(), 1);
    assert!(chart_buckets(&v, 0).is_empty());
    assert!(chart_buckets(&Value::Null, 7).is_empty());
}
#[test]
fn invalid_calendar_dates_remain_outside_chart() {
    let v = json!({"dailyUsageBuckets":{"status":"reported","value":[{"startDate":{"status":"reported","value":"2026-02-30"}},{"startDate":{"status":"not_reported","value":null}}]}});
    assert!(chart_buckets(&v, 90).is_empty());
    assert_eq!(unknown_date_buckets(&v).len(), 2);
}
#[test]
fn percentages_use_big_integers_without_precision_loss() {
    let max = json!({"status":"reported","value":"900719925474099312345600"});
    let half = json!({"status":"reported","value":"450359962737049656172800"});
    let buckets = vec![json!({"tokens":max})];
    assert_eq!(bar_percent(&half, &buckets), 50.0);
    assert_eq!(bar_percent(&Value::Null, &buckets), 0.0);
    assert_eq!(bar_percent(&half, &[]), 0.0);
}
#[test]
fn quota_windows_keep_buckets_independent() {
    let q = fixture("overview")["quota"].clone();
    assert_eq!(quota_windows(&q, false).len(), 2);
    assert_eq!(quota_windows(&q, true)[0].0, "Demo plan");
    let mut q = q;
    q["data"]["buckets"][0]["limitName"] = Value::Null;
    assert_eq!(quota_windows(&q, true)[0].0, "plan");
    q["data"]["buckets"][0]["bucketKey"] = Value::Null;
    assert_eq!(quota_windows(&q, true)[0].0, "Unknown bucket");
    assert!(quota_windows(&Value::Null, true).is_empty());
}
#[test]
fn refresh_loads_source_and_separate_evidence() {
    let s = loaded();
    assert_eq!(s.source, "demo");
    assert_eq!(s.data(Slot::Overview)["events"]["total"], "2");
    assert_eq!(s.data(Slot::Recent)["events"].as_array().unwrap().len(), 2);
}
#[test]
fn no_sources_clear_all_old_evidence() {
    let mut s = loaded();
    let r = s.dispatch(Action::Refresh).remove(0);
    let mut status = fixture("status");
    status["sources"] = json!([]);
    assert!(s.complete(&r, Ok(status)).is_empty());
    assert!(s.source.is_empty());
    assert!(s.data(Slot::Overview).is_null());
}
#[test]
fn newer_refresh_wins_and_previous_value_survives_error() {
    let mut s = loaded();
    let old = s.dispatch(Action::Refresh).remove(0);
    let new = s.dispatch(Action::Refresh).remove(0);
    s.complete(&old, Err("stale".into()));
    assert_eq!(s.error(Slot::Status), "");
    s.complete(&new, Err("offline".into()));
    assert_eq!(s.error(Slot::Status), "offline");
    assert!(!s.data(Slot::Overview).is_null());
    assert!(!s.loading(Slot::Status));
}
#[test]
fn refresh_invalidates_existing_detail_work() {
    let mut s = loaded();
    let r = s.dispatch(Action::Select(fixture("event"))).remove(0);
    s.dispatch(Action::Refresh);
    s.complete(&r, Ok(fixture("detail")));
    assert!(s.data(Slot::Detail).is_null());
}
#[test]
fn source_switch_clears_detail_and_ignores_late_success_or_failure() {
    let mut s = loaded();
    let r = s.dispatch(Action::Select(fixture("event"))).remove(0);
    s.dispatch(Action::Source("other".into()));
    assert!(s.selected.is_null());
    assert!(s.data(Slot::Overview).is_null());
    s.complete(&r, Ok(fixture("detail")));
    s.complete(&r, Err("stale".into()));
    assert!(s.data(Slot::Detail).is_null());
    assert_eq!(s.error(Slot::Detail), "");
}
#[test]
fn cross_source_responses_are_rejected() {
    let mut s = loaded();
    let r = request(&mut s, Slot::Activity);
    let mut value = fixture("events");
    value["source"]["id"] = json!("other");
    s.complete(&r, Ok(value));
    assert_eq!(s.error(Slot::Activity), "source_mismatch");
    assert!(s.data(Slot::Activity).is_null());
}
#[test]
fn detail_identity_must_match_both_source_and_event() {
    let mut s = loaded();
    let mut wrong = fixture("event");
    wrong["sourceId"] = json!("other");
    assert!(s.dispatch(Action::Select(wrong)).is_empty());
    let r = s.dispatch(Action::Select(fixture("event"))).remove(0);
    let mut d = fixture("detail");
    d["event"]["eventId"] = json!("wrong");
    s.complete(&r, Ok(d));
    assert_eq!(s.error(Slot::Detail), "event_mismatch");
}
#[test]
fn closing_detail_discards_late_content() {
    let mut s = loaded();
    let r = s.dispatch(Action::Select(fixture("event"))).remove(0);
    s.dispatch(Action::Close);
    s.complete(&r, Ok(fixture("detail")));
    assert!(s.data(Slot::Detail).is_null());
    assert!(s.selected.is_null());
}
#[test]
fn pagination_appends_only_matching_latest_request() {
    for (slot, key, fixture_key) in [
        (Slot::Activity, "events", "events"),
        (Slot::History, "observations", "history"),
        (Slot::ResponseRecords, "records", "responseRecords"),
    ] {
        let mut s = loaded();
        let r = request(&mut s, slot);
        let mut v = fixture(fixture_key);
        v["nextCursor"] = json!("next cursor");
        let count = v[key].as_array().unwrap().len();
        s.complete(&r, Ok(v.clone()));
        let more = s.dispatch(Action::Load(slot, true)).remove(0);
        assert!(more.path.contains("cursor=next%20cursor"));
        assert!(s.dispatch(Action::Load(slot, true)).is_empty());
        v["nextCursor"] = Value::Null;
        s.complete(&more, Ok(v));
        assert_eq!(s.data(slot)[key].as_array().unwrap().len(), count * 2);
        assert!(s.dispatch(Action::Load(slot, true)).is_empty());
    }
}
#[test]
fn date_filters_validate_then_serialize() {
    let mut s = loaded();
    for (key, value) in [
        ("from", "2026-10-03"),
        ("to", "2026-10-01"),
        ("model", "literal model"),
        ("eventType", "tool_call"),
        ("ignored", "unused"),
    ] {
        s.dispatch(Action::Filter(key, value.into()));
    }
    assert!(s.dispatch(Action::ApplyFilters).is_empty());
    assert_eq!(s.error(Slot::Activity), "invalid_date_range");
    s.dispatch(Action::Filter("to", "2026-10-04".into()));
    let r = s.dispatch(Action::ApplyFilters).remove(0);
    assert!(r.path.contains("model=literal%20model"));
    assert!(r.path.contains("eventType=tool_call"));
    let reset = s.dispatch(Action::ResetFilters).remove(0);
    assert!(!reset.path.contains("model="));
    assert_eq!(s.filters, Filters::default());
}
#[test]
fn navigation_loads_only_required_page() {
    let mut s = loaded();
    for (page, slot) in [
        (Page::Activity, Slot::Activity),
        (Page::Skills, Slot::Skills),
        (Page::Quotas, Slot::History),
    ] {
        assert_eq!(s.dispatch(Action::Navigate(page))[0].slot, slot);
    }
    for page in [Page::Settings, Page::Overview] {
        assert!(s.dispatch(Action::Navigate(page)).is_empty());
    }
    let mut empty = State::default();
    assert!(empty.dispatch(Action::Navigate(Page::Activity)).is_empty());
    assert!(
        empty
            .dispatch(Action::Load(Slot::Activity, false))
            .is_empty()
    );
}
#[test]
fn skill_categories_reset_and_remain_distinct() {
    let mut s = loaded();
    for k in ["requested", "loaded", "invoked"] {
        let r = s.dispatch(Action::SkillKind(k.into())).remove(0);
        assert!(r.path.contains(&format!("kind={k}")));
        s.complete(&r, Ok(fixture("skills")));
    }
    assert!(s.dispatch(Action::SkillKind("mentioned".into())).is_empty());
}
#[test]
fn imported_response_counts_are_fetched_separately() {
    let mut s = loaded();
    let r = request(&mut s, Slot::Overview);
    let mut o = fixture("overview");
    o["source"]["mode"] = json!("imported");
    let next = s.complete(&r, Ok(o));
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].slot, Slot::ResponseUsage);
    s.complete(&next[0], Ok(fixture("responseUsage")));
    assert_eq!(s.data(Slot::ResponseUsage)["totals"]["totalTokens"], "1200");
    assert_eq!(
        s.data(Slot::Overview)["usage"]["data"]["summary"]["lifetimeTokens"]["value"],
        "9007199254740993123456"
    );
}
#[test]
fn preferences_and_chart_range_are_validated() {
    let mut s = State::default();
    s.dispatch(Action::Language("zh".into()));
    s.dispatch(Action::ToggleTheme);
    assert_eq!(s.language, Language::Chinese);
    assert!(s.dark);
    for range in [30, 90, 7] {
        s.dispatch(Action::ChartRange(range));
        assert_eq!(s.chart_range, range);
    }
    s.dispatch(Action::ChartRange(0));
    assert_eq!(s.chart_range, 7);
}
#[test]
fn warnings_and_failure_codes_are_deduplicated() {
    let mut s = loaded();
    s.remotes.get_mut(&Slot::Status).unwrap().value["warnings"] = json!(["partial source", 123]);
    let o = &mut s.remotes.get_mut(&Slot::Overview).unwrap().value;
    o["usage"]["warnings"] = json!(["warning"]);
    o["quota"]["warnings"] = json!(["warning"]);
    o["usage"]["lastFailure"] = json!({"errorCode":"rpc_error"});
    o["quota"]["lastFailure"] = json!({"errorCode":"rpc_error"});
    assert_eq!(s.warnings(), vec!["partial source", "warning"]);
    assert_eq!(s.failures(), vec!["rpc_error"]);
}
#[test]
fn settings_validate_and_post_camel_case() {
    let mut s = loaded();
    s.dispatch(Action::Setting("retentionDays", json!(0)));
    assert!(s.dispatch(Action::SaveSettings).is_empty());
    assert_eq!(s.error(Slot::Mutation), "invalid_retention_days");
    s.dispatch(Action::Setting("retentionDays", json!(3651)));
    assert!(s.dispatch(Action::SaveSettings).is_empty());
    s.dispatch(Action::Setting("retentionDays", json!(7)));
    s.dispatch(Action::Setting("capturePaused", json!(true)));
    s.dispatch(Action::Setting("unknown", json!(true)));
    let r = s.dispatch(Action::SaveSettings).remove(0);
    assert_eq!(r.path, "/api/settings");
    assert_eq!(r.body.as_ref().unwrap()["retentionDays"], 7);
    assert!(r.body.as_ref().unwrap().get("unknown").is_none());
    assert_eq!(r.body.as_ref().unwrap()["capturePaused"], true);
}
#[test]
fn destructive_actions_require_exact_separate_confirmation() {
    let mut s = loaded();
    for action in [
        Destructive::Content,
        Destructive::All,
        Destructive::Retention,
    ] {
        s.dispatch(Action::Confirm(action));
        assert!(!s.confirm_enabled());
        assert!(s.dispatch(Action::Perform).is_empty());
        s.dispatch(Action::Confirmation(format!("{} ", action.required())));
        assert!(!s.confirm_enabled());
        s.dispatch(Action::Confirmation(action.required().into()));
        assert!(s.confirm_enabled());
        let r = s.dispatch(Action::Perform).remove(0);
        assert!(s.busy());
        let body = r.body.as_ref().unwrap();
        assert_eq!(body["confirmation"], action.required());
        if action == Destructive::Retention {
            assert_eq!(r.path, "/api/retention");
            assert!(body.get("sourceId").is_none());
        } else {
            assert_eq!(body["sourceId"], "demo");
            assert_eq!(
                body["target"],
                if action == Destructive::All {
                    "all"
                } else {
                    "content"
                }
            );
        }
        let next = s.complete(&r, Ok(json!({})));
        assert_eq!(next[0].slot, Slot::Status);
        assert!(s.success);
        assert!(s.action.is_none());
        assert!(s.data(Slot::Overview).is_null());
    }
}
#[test]
fn mutation_busy_blocks_duplicate_and_scope_changes() {
    let mut s = loaded();
    s.dispatch(Action::Confirm(Destructive::All));
    s.dispatch(Action::Confirmation("DELETE".into()));
    let r = s.dispatch(Action::Perform).remove(0);
    for action in [
        Action::Close,
        Action::Source("other".into()),
        Action::Navigate(Page::Skills),
        Action::Perform,
        Action::SaveSettings,
        Action::Confirm(Destructive::Content),
        Action::Confirmation("bad".into()),
        Action::Setting("retentionDays", json!(10)),
    ] {
        assert!(s.dispatch(action).is_empty());
    }
    assert_eq!(s.source, "demo");
    assert_eq!(s.action, Some(Destructive::All));
    assert_eq!(s.confirmation, "DELETE");
    s.complete(&r, Err("failed".into()));
    assert_eq!(s.error(Slot::Mutation), "failed");
    assert!(!s.busy());
    assert!(s.confirm_enabled());
    s.dispatch(Action::Close);
    assert!(s.action.is_none());
}
#[test]
fn empty_source_allows_global_retention_but_not_source_delete() {
    let mut s = State::default();
    assert!(s.dispatch(Action::Confirm(Destructive::All)).is_empty());
    assert!(s.action.is_none());
    s.dispatch(Action::Confirm(Destructive::Retention));
    assert_eq!(s.action, Some(Destructive::Retention));
}
#[test]
fn duplicate_or_retired_request_cannot_commit_twice() {
    let mut s = loaded();
    let r = request(&mut s, Slot::Activity);
    s.complete(&r, Ok(fixture("events")));
    s.complete(&r, Err("duplicate".into()));
    assert_eq!(s.error(Slot::Activity), "");
    let r = request(&mut s, Slot::Activity);
    s.source = "other".into();
    s.complete(&r, Ok(fixture("events")));
    assert!(s.loading(Slot::Activity));
}
#[test]
fn input_conversion_is_pure_and_reuses_reducers() {
    let mut s = loaded();
    for (input, v) in [
        (InputAction::Language, "zh"),
        (InputAction::ChartRange, "30"),
        (InputAction::Filter("model"), "exact"),
        (InputAction::RetentionDays, "7"),
        (InputAction::Confirmation, "DELETE"),
    ] {
        s.dispatch(input.action(v.into()));
    }
    assert_eq!(s.language, Language::Chinese);
    assert_eq!(s.chart_range, 30);
    assert_eq!(s.filters.model, "exact");
    assert_eq!(s.settings["retentionDays"], 7);
    s.dispatch(InputAction::ChartRange.action("bad".into()));
    assert_eq!(s.chart_range, 7);
    s.dispatch(InputAction::RetentionDays.action("bad".into()));
    assert_eq!(s.settings["retentionDays"], 0);
    s.dispatch(InputAction::Source.action("other".into()));
    assert_eq!(s.source, "other");
}
#[test]
fn input_edits_preserve_focus_and_dialog_changes_preserve_opener() {
    for action in [
        Action::Filter("model", "typing".into()),
        Action::Setting("retentionDays", json!(7)),
        Action::Confirmation("DE".into()),
    ] {
        assert_eq!(action.repaint(), (false, false));
    }
    for action in [
        Action::Select(fixture("event")),
        Action::Close,
        Action::Confirm(Destructive::All),
        Action::Load(Slot::Detail, false),
    ] {
        assert_eq!(action.repaint(), (false, true));
    }
    for action in [
        Action::Refresh,
        Action::Navigate(Page::Settings),
        Action::ToggleTheme,
    ] {
        assert_eq!(action.repaint(), (true, true));
    }
}
#[test]
fn transport_response_errors_are_allowlisted_codes_with_safe_fallback() {
    assert_eq!(
        response_result(true, json!({"x":1})).unwrap(),
        json!({"x":1})
    );
    assert_eq!(
        response_result(false, json!({"error":{"code":"not_found"}})),
        Err("not_found".into())
    );
    assert_eq!(
        response_result(false, json!({"message":"raw server internals"})),
        Err("request_failed".into())
    );
}
