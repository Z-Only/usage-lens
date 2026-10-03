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
        let key = match r.slot {
            Slot::Overview => "overview",
            Slot::Health => "health",
            _ => "events",
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

#[test]
fn health_request_is_source_scoped_and_loaded_after_status() {
    let mut s = State::default();
    assert!(s.dispatch(Action::Load(Slot::Health, false)).is_empty());
    let status = s.dispatch(Action::Refresh).remove(0);
    let jobs = s.complete(&status, Ok(fixture("status")));
    let health = jobs.iter().find(|r| r.slot == Slot::Health).unwrap();
    assert_eq!(health.path, "/api/health?maxAgeMs=900000&sourceId=demo");
    assert!(health.body.is_none());
    assert!(s.accepts(health));
    s.complete(health, Ok(fixture("health")));
    assert_eq!(s.data(Slot::Health)["stored"]["events"]["count"], "2");
    assert!(!s.loading(Slot::Health));
}
#[test]
fn health_retry_clears_error_and_preserves_last_response() {
    let mut s = loaded();
    let r = request(&mut s, Slot::Health);
    s.complete(&r, Err("health_failed".into()));
    assert_eq!(s.error(Slot::Health), "health_failed");
    assert_eq!(s.data(Slot::Health)["source"]["id"], "demo");
    let retry = request(&mut s, Slot::Health);
    assert!(s.error(Slot::Health).is_empty());
    assert!(s.loading(Slot::Health));
    s.complete(&retry, Ok(fixture("health")));
    assert!(!s.loading(Slot::Health));
}
#[test]
fn health_source_switch_refresh_and_superseded_requests_discard_late_results() {
    for action in [Action::Source("other".into()), Action::Refresh] {
        for result in [Ok(fixture("health")), Err("late_health_failure".into())] {
            let mut s = loaded();
            let r = request(&mut s, Slot::Health);
            s.dispatch(action.clone());
            assert!(!s.accepts(&r));
            s.complete(&r, result);
            assert!(s.error(Slot::Health).is_empty());
            if matches!(action, Action::Source(_)) {
                assert!(s.data(Slot::Health).is_null());
            }
        }
    }
    let mut s = loaded();
    let older = request(&mut s, Slot::Health);
    let newer = request(&mut s, Slot::Health);
    s.complete(&older, Err("older".into()));
    assert!(s.error(Slot::Health).is_empty());
    assert!(s.loading(Slot::Health));
    s.complete(&newer, Ok(fixture("health")));
    assert!(!s.loading(Slot::Health));
}
#[test]
fn health_rejects_mismatched_source_and_no_sources_clear_it() {
    let mut s = loaded();
    let r = request(&mut s, Slot::Health);
    let mut health = fixture("health");
    health["source"]["id"] = json!("other");
    s.complete(&r, Ok(health));
    assert_eq!(s.error(Slot::Health), "source_mismatch");
    assert_eq!(s.data(Slot::Health)["source"]["id"], "demo");
    let r = s.dispatch(Action::Refresh).remove(0);
    let mut status = fixture("status");
    status["sources"] = json!([]);
    s.complete(&r, Ok(status));
    assert!(s.data(Slot::Health).is_null());
    assert!(s.error(Slot::Health).is_empty());
}
#[test]
fn health_labels_preserve_distinct_states_and_exact_or_unknown_counts() {
    for l in [Language::English, Language::Chinese] {
        let mut labels = std::collections::BTreeSet::new();
        for state in [
            "available",
            "missing",
            "observed",
            "unsupported",
            "fresh",
            "stale",
            "future",
            "recent",
            "unknown",
        ] {
            assert!(labels.insert(health_state_text(&json!(state), l)));
        }
        assert_eq!(
            health_state_text(&json!("unexpected"), l),
            l.text("Unknown", "未知")
        );
        assert_eq!(
            health_state_text(&Value::Null, l),
            l.text("Unknown", "未知")
        );
        assert_eq!(health_count(&json!("0"), l), "0");
        assert_eq!(
            health_count(&json!("9007199254740993123456"), l),
            "9,007,199,254,740,993,123,456"
        );
        for value in [Value::Null, json!(0), json!(""), json!("-1"), json!("1.5")] {
            assert_eq!(health_count(&value, l), l.text("Unknown", "未知"));
        }
        for value in [Value::Null, json!(""), json!(123)] {
            assert_eq!(health_timestamp(&value, l), l.text("Unknown", "未知"));
        }
        assert_eq!(
            health_timestamp(&json!("2026-10-02T00:00:00Z"), l),
            "2026-10-02T00:00:00Z"
        );
    }
}

#[test]
fn skill_date_pairs_are_strict_utc_calendar_ranges_bounded_to_366_days() {
    for (from, to, valid) in [
        ("", "", false),
        ("2026-10-01", "", false),
        ("", "2026-10-01", false),
        ("2026-02-30", "2026-03-01", false),
        ("2026-1-01", "2026-01-01", false),
        ("2026-10-03", "2026-10-01", false),
        ("2026-10-01", "2026-10-01", true),
        ("2024-01-01", "2024-12-31", true),
        ("2024-01-01", "2025-01-01", false),
    ] {
        let filters = Filters {
            from: from.into(),
            to: to.into(),
            ..Filters::default()
        };
        assert_eq!(filters.skill_dates_valid(), valid, "{from}..{to}");
    }
}
fn trend_filters(s: &mut State) {
    s.dispatch(Action::Filter("from", "2026-10-01".into()));
    s.dispatch(Action::Filter("to", "2026-10-03".into()));
}
#[test]
fn skill_trends_load_only_paired_dates_and_keep_exact_scope_separate() {
    let mut s = loaded();
    let requests = s.dispatch(Action::Navigate(Page::Skills));
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].slot, Slot::Skills);
    assert!(s.dispatch(Action::ApplyFilters).is_empty());
    assert_eq!(s.error(Slot::SkillSummary), "invalid_skill_date_range");
    trend_filters(&mut s);
    s.dispatch(Action::Filter(
        "skillName",
        " Mixed /中文&kind=invoked ".into(),
    ));
    s.dispatch(Action::Filter("model", "ignored-model".into()));
    s.dispatch(Action::Filter("eventType", "ignored-event".into()));
    let r = s.dispatch(Action::ApplyFilters).remove(0);
    assert_eq!(r.slot, Slot::SkillSummary);
    assert!(r.path.starts_with("/api/skill-summary?"));
    assert!(r.path.contains("fromDate=2026-10-01&toDate=2026-10-03"));
    assert!(
        r.path
            .contains("skillName=%20Mixed%20%2F%E4%B8%AD%E6%96%87%26kind%3Dinvoked%20")
    );
    assert!(r.path.contains("sourceId=demo"));
    assert!(!r.path.contains("ignored"));
    assert!(!r.path.contains("&kind="));
    let mut response = fixture("skillSummary");
    response["skillName"] = json!(s.filters.skill_name);
    s.complete(&r, Ok(response));
    assert_eq!(s.data(Slot::SkillSummary)["unknownOccurredAtCount"], "7");
    let requests = s.dispatch(Action::Navigate(Page::Skills));
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].slot, Slot::SkillSummary);
    let mut empty = State::default();
    empty.page = Page::Skills;
    trend_filters(&mut empty);
    assert!(empty.dispatch(Action::ApplyFilters).is_empty());
}
#[test]
fn skill_filter_edits_reset_and_new_generations_discard_pending_responses() {
    let mut s = loaded();
    s.page = Page::Skills;
    trend_filters(&mut s);
    let old = s.dispatch(Action::ApplyFilters).remove(0);
    let current = s.dispatch(Action::ApplyFilters).remove(0);
    assert!(!s.accepts(&old));
    s.complete(&old, Err("stale".into()));
    s.complete(&current, Ok(fixture("skillSummary")));
    assert_eq!(s.error(Slot::SkillSummary), "");
    for (key, value) in [
        ("from", "2026-10-01"),
        ("to", "2026-10-03"),
        ("skillName", "new"),
    ] {
        let request = s.dispatch(Action::ApplyFilters).remove(0);
        s.dispatch(Action::Filter(key, value.into()));
        assert!(!s.accepts(&request));
        assert!(!s.loading(Slot::SkillSummary));
        assert!(s.data(Slot::SkillSummary).is_null());
        s.complete(&request, Ok(fixture("skillSummary")));
        assert!(s.data(Slot::SkillSummary).is_null());
    }
    let request = s.dispatch(Action::ApplyFilters).remove(0);
    assert!(s.dispatch(Action::ResetFilters).is_empty());
    assert_eq!(s.filters, Filters::default());
    assert!(!s.accepts(&request));
    assert!(
        s.dispatch(Action::Load(Slot::SkillSummary, false))
            .is_empty()
    );
    assert_eq!(s.error(Slot::SkillSummary), "invalid_skill_date_range");
}
#[test]
fn skill_trends_reject_wrong_response_filters_sources_and_source_switch_races() {
    for (field, value, code) in [
        ("fromDate", "2026-10-02", "filter_mismatch"),
        ("toDate", "2026-10-04", "filter_mismatch"),
        ("skillName", "wrong", "filter_mismatch"),
        ("source", "other", "source_mismatch"),
    ] {
        let mut s = loaded();
        s.page = Page::Skills;
        trend_filters(&mut s);
        let request = s.dispatch(Action::ApplyFilters).remove(0);
        let mut response = fixture("skillSummary");
        if field == "source" {
            response["source"]["id"] = json!(value);
        } else {
            response[field] = json!(value);
        }
        s.complete(&request, Ok(response));
        assert_eq!(s.error(Slot::SkillSummary), code);
        assert!(s.data(Slot::SkillSummary).is_null());
        let retry = s
            .dispatch(Action::Load(Slot::SkillSummary, false))
            .remove(0);
        s.complete(&retry, Ok(fixture("skillSummary")));
        assert_eq!(s.error(Slot::SkillSummary), "");
        assert!(!s.data(Slot::SkillSummary).is_null());
        let old = s.dispatch(Action::ApplyFilters).remove(0);
        s.dispatch(Action::Source("other".into()));
        s.complete(&old, Ok(fixture("skillSummary")));
        assert!(s.data(Slot::SkillSummary).is_null());
    }
}
#[test]
fn editing_invalid_dates_clears_previous_trend_and_preserves_activity_filters() {
    let mut s = loaded();
    s.page = Page::Skills;
    trend_filters(&mut s);
    let request = s.dispatch(Action::ApplyFilters).remove(0);
    s.complete(&request, Ok(fixture("skillSummary")));
    s.dispatch(Action::Filter("to", "2026-09-01".into()));
    assert!(s.dispatch(Action::ApplyFilters).is_empty());
    assert!(s.data(Slot::SkillSummary).is_null());
    s.dispatch(Action::Filter("to", "2026-10-03".into()));
    let requests = s.dispatch(Action::Navigate(Page::Activity));
    assert!(
        requests[0]
            .path
            .contains("fromDate=2026-10-01&toDate=2026-10-03")
    );
    assert!(s.data(Slot::SkillSummary).is_null());
}

#[test]
fn content_search_uses_applied_filters_for_pagination_and_rejects_late_results() {
    let mut s = loaded();
    s.dispatch(Action::Navigate(Page::Activity));
    s.dispatch(Action::Filter("query", "literal & 中文".into()));
    s.dispatch(Action::Filter("model", "model-a".into()));
    let search = s.dispatch(Action::ApplyFilters).remove(0);
    assert!(search.path.starts_with("/api/events/search?"));
    assert!(
        search
            .path
            .contains("query=literal%20%26%20%E4%B8%AD%E6%96%87")
    );
    let mut result = fixture("events");
    result["nextCursor"] = json!("search cursor");
    s.complete(&search, Ok(result.clone()));
    s.dispatch(Action::Filter("query", "draft new search".into()));
    s.dispatch(Action::Filter("model", "draft-model".into()));
    let next = s.dispatch(Action::Load(Slot::Activity, true)).remove(0);
    assert!(next.path.contains("model=model-a"));
    assert!(!next.path.contains("draft"));
    assert!(next.path.contains("cursor=search%20cursor"));
    let changed = s.dispatch(Action::ApplyFilters).remove(0);
    assert!(!s.accepts(&next));
    assert!(s.data(Slot::Activity).is_null());
    s.complete(&next, Ok(result));
    assert!(s.data(Slot::Activity).is_null());
    assert!(changed.path.contains("query=draft%20new%20search"));
    let reset = s.dispatch(Action::ResetFilters).remove(0);
    assert!(reset.path.starts_with("/api/events?"));
    assert_eq!(s.applied_filters, Filters::default());
    assert!(!s.accepts(&changed));
}

#[test]
fn aggregate_drilldown_resets_unrelated_scope_and_only_fetches_explicit_groups() {
    let mut s = loaded();
    s.filters.from = "2026-10-01".into();
    s.filters.content_query = "previous".into();
    s.selected = fixture("event");
    for (field, value) in [("eventType", "tool_call"), ("model", "model-a")] {
        let r = s.dispatch(Action::Drilldown(field, value.into())).remove(0);
        assert_eq!(s.page, Page::Activity);
        assert!(s.selected.is_null());
        assert!(r.path.contains(&format!("{field}={value}")));
        assert!(!r.path.contains("query="));
        assert!(!r.path.contains("fromDate="));
        assert!(!r.path.contains("cursor="));
    }
    for action in [
        Action::Drilldown("model", "".into()),
        Action::Drilldown("unsupported", "value".into()),
    ] {
        assert!(s.dispatch(action).is_empty());
    }
    let mut empty = State::default();
    for action in [
        Action::Drilldown("model", "a".into()),
        Action::ApplyFilters,
        Action::ResetFilters,
        Action::ApplyTokenPeriod,
    ] {
        assert!(empty.dispatch(action).is_empty());
    }
    s.dispatch(Action::SaveSettings);
    assert!(
        s.dispatch(Action::Drilldown("model", "a".into()))
            .is_empty()
    );
}

#[test]
fn invalid_activity_submission_invalidates_inflight_results() {
    let mut s = loaded();
    let r = s.dispatch(Action::ApplyFilters).remove(0);
    s.dispatch(Action::Filter("from", "2026-10-03".into()));
    s.dispatch(Action::Filter("to", "2026-10-01".into()));
    assert!(s.dispatch(Action::ApplyFilters).is_empty());
    assert!(!s.accepts(&r));
}

#[test]
fn token_period_dates_are_explicit_separate_and_match_response_scope() {
    let mut s = loaded();
    assert!(
        s.dispatch(Action::Load(Slot::TokenPeriod, false))
            .is_empty()
    );
    assert_eq!(s.error(Slot::TokenPeriod), "invalid_token_period");
    assert!(s.dispatch(Action::ApplyTokenPeriod).is_empty());
    s.dispatch(Action::Filter("tokenFrom", "2026-10-01".into()));
    s.dispatch(Action::Filter("tokenTo", "2026-10-03".into()));
    assert!(s.filters.from.is_empty());
    let r = s.dispatch(Action::ApplyTokenPeriod).remove(0);
    assert!(r.path.starts_with("/api/response-tokens/period?"));
    assert!(r.path.contains("fromDate=2026-10-01&toDate=2026-10-03"));
    s.dispatch(Action::Filter("tokenFrom", "2026-10-02".into()));
    let result = json!({"source":{"id":"demo"},"fromDate":"2026-10-01","toDate":"2026-10-03","responseCount":"0"});
    s.complete(&r, Ok(result.clone()));
    assert_eq!(s.data(Slot::TokenPeriod)["fromDate"], "2026-10-01");
    let retry = s.dispatch(Action::Load(Slot::TokenPeriod, false)).remove(0);
    assert_eq!(retry.path, r.path);
    let mut wrong = result;
    wrong["toDate"] = json!("2026-10-04");
    s.complete(&retry, Ok(wrong));
    assert_eq!(s.error(Slot::TokenPeriod), "filter_mismatch");
    let jobs = s.dispatch(Action::Navigate(Page::Quotas));
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[1].slot, Slot::TokenPeriod);
    assert!(jobs[1].path.contains("fromDate=2026-10-01"));
    s.dispatch(Action::Filter("tokenTo", "2026-01-01".into()));
    assert!(s.dispatch(Action::ApplyTokenPeriod).is_empty());
    assert!(s.data(Slot::TokenPeriod).is_null());
}

#[test]
fn validation_error_retry_revalidates_drafts_instead_of_reloading_older_evidence() {
    let mut s = loaded();
    let r = s.dispatch(Action::ApplyFilters).remove(0);
    s.complete(&r, Ok(fixture("events")));
    s.dispatch(Action::Filter("from", "2026-10-03".into()));
    s.dispatch(Action::Filter("to", "2026-10-01".into()));
    s.dispatch(Action::ApplyFilters);
    assert!(s.dispatch(s.retry_action(Slot::Activity)).is_empty());
    assert_eq!(s.error(Slot::Activity), "invalid_date_range");
    assert!(s.data(Slot::Activity).is_null());
    s.dispatch(Action::Filter("to", "2026-10-03".into()));
    let retry = s.dispatch(s.retry_action(Slot::Activity)).remove(0);
    assert!(retry.path.contains("fromDate=2026-10-03&toDate=2026-10-03"));
    s.dispatch(Action::Filter("tokenFrom", "2026-10-01".into()));
    s.dispatch(Action::Filter("tokenTo", "2026-10-03".into()));
    s.dispatch(Action::ApplyTokenPeriod);
    s.dispatch(Action::Filter("tokenTo", "2026-01-01".into()));
    s.dispatch(Action::ApplyTokenPeriod);
    assert!(s.dispatch(s.retry_action(Slot::TokenPeriod)).is_empty());
    assert_eq!(s.error(Slot::TokenPeriod), "invalid_token_period");
    assert_eq!(s.dispatch(Action::Navigate(Page::Quotas)).len(), 1);
    assert!(s.data(Slot::TokenPeriod).is_null());
    assert!(matches!(
        s.retry_action(Slot::Health),
        Action::Load(Slot::Health, false)
    ));
}

#[test]
fn invalid_submitted_activity_cannot_reload_old_scope_on_refresh_or_source_switch() {
    let mut s = loaded();
    s.page = Page::Activity;
    s.dispatch(Action::ApplyFilters);
    s.dispatch(Action::Filter("from", "2026-10-03".into()));
    s.dispatch(Action::Filter("to", "2026-10-01".into()));
    s.dispatch(Action::ApplyFilters);
    assert!(s.dispatch(Action::Load(Slot::Activity, false)).is_empty());
    let refresh = s.dispatch(Action::Refresh).remove(0);
    let jobs = s.complete(&refresh, Ok(fixture("status")));
    assert!(jobs.iter().all(|r| r.slot != Slot::Activity));
    assert_eq!(s.error(Slot::Activity), "invalid_date_range");
    let switch = s.dispatch(Action::Source("other".into())).remove(0);
    let jobs = s.complete(&switch, Ok(fixture("status")));
    assert!(jobs.iter().all(|r| r.slot != Slot::Activity));
    assert!(s.data(Slot::Activity).is_null());
}

#[test]
fn activity_date_validation_requires_canonical_paired_dates_and_api_bound() {
    for (from, to, valid) in [
        ("", "", true),
        ("2026-10-01", "", false),
        ("", "2026-10-01", false),
        ("2026-02-30", "2026-03-01", false),
        ("2026-1-01", "2026-10-01", false),
        ("2000-01-01", "2026-10-01", false),
        ("2026-10-01", "2026-10-01", true),
    ] {
        let f = Filters {
            from: from.into(),
            to: to.into(),
            ..Filters::default()
        };
        assert_eq!(f.valid(), valid, "{from} {to}");
    }
}
