#![recursion_limit = "256"]
use leptos::prelude::*;
use serde_json::{Value, json};
use usage_lens_ui::model::Action;
use usage_lens_ui::{model::*, trace_reader::*, views::*};

fn fixture(key: &str) -> Value {
    serde_json::from_str::<Value>(include_str!("trace-fixtures.json")).unwrap()[key].clone()
}
fn put(s: &mut State, slot: Slot, value: Value) {
    s.remotes.insert(
        slot,
        Remote {
            value,
            ..Remote::default()
        },
    );
}
fn state() -> State {
    let mut s = State::default();
    s.source = "demo".into();
    s.page = Page::Traces;
    for (slot, key) in [(Slot::Status, "status"), (Slot::Overview, "overview")] {
        put(
            &mut s,
            slot,
            serde_json::from_str::<Value>(include_str!("fixtures.json")).unwrap()[key].clone(),
        );
    }
    for (slot, key) in [
        (Slot::Traces, "list"),
        (Slot::TraceSummary, "summary"),
        (Slot::TraceDetail, "detail"),
    ] {
        put(&mut s, slot, fixture(key));
    }
    s
}
fn render_with(s: State, f: impl FnOnce(&State, Ui) -> AnyView) -> String {
    Owner::new().with(|| {
        let signal = RwSignal::new(s.clone());
        let ui = Ui {
            state: signal,
            revision: RwSignal::new(0),
            modal_revision: RwSignal::new(0),
            send: Callback::new(move |action| {
                signal.update(|s| {
                    s.dispatch(action);
                })
            }),
        };
        f(&s, ui).to_html()
    })
}
fn render(s: State) -> String {
    render_with(s, |_, ui| view! { <Dashboard ui /> }.into_any())
}
fn request(s: &mut State, slot: Slot) -> Request {
    s.dispatch(Action::Load(slot, false)).remove(0)
}

#[test]
fn trace_states_preserve_all_missingness_and_statuses_in_both_languages() {
    for l in [Language::English, Language::Chinese] {
        for name in [
            "reported",
            "omitted",
            "not_reported",
            "invalid",
            "completed",
            "failed",
            "cancelled",
            "incomplete",
            "unknown",
        ] {
            assert!(!trace_state(&json!(name), l).is_empty());
        }
        assert_eq!(
            trace_cell(&json!({"state":"reported","value":"high"}), l, false),
            "high"
        );
        assert_eq!(
            trace_cell(
                &json!({"state":"reported","value":"9007199254740993"}),
                l,
                true
            ),
            "9,007,199,254,740,993"
        );
        assert_eq!(
            trace_cell(&json!({"state":"reported","value":null}), l, false),
            trace_state(&json!("reported"), l)
        );
        assert_eq!(trace_cell(&Value::Null, l, true), l.text("Unknown", "未知"));
        assert_eq!(
            trace_cell(&json!({"state":"invalid","value":null}), l, true),
            l.text("Invalid", "无效")
        );
    }
}

#[test]
fn trace_navigation_loads_local_endpoints_and_separate_submitted_dates() {
    let mut s = state();
    let jobs = s.dispatch(Action::Navigate(Page::Traces));
    assert_eq!(
        jobs.iter().map(|r| r.slot).collect::<Vec<_>>(),
        vec![Slot::Traces, Slot::TraceSummary]
    );
    assert_eq!(jobs[0].path, "/api/traces?limit=20&sourceId=demo");
    assert_eq!(jobs[1].path, "/api/traces/summary?sourceId=demo");
    s.dispatch(Action::Filter("traceFrom", "2026-10-01".into()));
    s.dispatch(Action::Filter("traceTo", "2026-10-07".into()));
    let jobs = s.dispatch(Action::ApplyTraceFilters);
    for r in &jobs {
        assert!(r.path.contains("fromDate=2026-10-01&toDate=2026-10-07"));
    }
    assert!(s.filters.from.is_empty());
    assert!(s.token_period.from.is_empty());
    s.dispatch(Action::Filter("traceFrom", "2026-10-02".into()));
    for r in jobs {
        let mut result = fixture(if r.slot == Slot::Traces {
            "list"
        } else {
            "summary"
        });
        result["fromDate"] = json!("2026-10-01");
        result["toDate"] = json!("2026-10-07");
        s.complete(&r, Ok(result));
        assert_eq!(s.data(r.slot)["fromDate"], "2026-10-01");
    }
    let more = s.dispatch(Action::Load(Slot::Traces, true)).remove(0);
    assert!(more.path.contains("fromDate=2026-10-01&toDate=2026-10-07"));
    assert!(more.path.contains("cursor=trace-next"));
    assert!(s.dispatch(Action::Load(Slot::Traces, true)).is_empty());
    let mut next = fixture("list");
    next["fromDate"] = json!("2026-10-01");
    next["toDate"] = json!("2026-10-07");
    next["attempts"][0]["attemptId"] = json!("second");
    next["nextCursor"] = Value::Null;
    s.complete(&more, Ok(next));
    assert_eq!(rows(&s.data(Slot::Traces)["attempts"]).len(), 2);
    assert!(s.dispatch(Action::Load(Slot::Traces, true)).is_empty());
    let reset = s.dispatch(Action::ResetTraceFilters);
    assert_eq!(reset.len(), 2);
    assert!(s.trace_filters.from.is_empty());
    assert!(!reset[0].path.contains("fromDate"));
}

#[test]
fn trace_invalid_dates_cannot_restore_old_results_and_retry_revalidates_drafts() {
    let mut s = state();
    let old = request(&mut s, Slot::Traces);
    s.dispatch(Action::Filter("traceFrom", "2026-10-03".into()));
    assert!(s.dispatch(Action::ApplyTraceFilters).is_empty());
    assert!(!s.accepts(&old));
    for slot in [Slot::Traces, Slot::TraceSummary] {
        assert!(s.data(slot).is_null());
        assert_eq!(s.error(slot), "invalid_trace_date_range");
        assert!(s.dispatch(s.retry_action(slot)).is_empty());
    }
    assert!(s.dispatch(Action::Navigate(Page::Traces)).is_empty());
    s.dispatch(Action::Filter("traceTo", "2026-10-02".into()));
    assert!(s.dispatch(Action::ApplyTraceFilters).is_empty());
    s.dispatch(Action::Filter("traceTo", "2026-10-09".into()));
    assert_eq!(s.dispatch(s.retry_action(Slot::TraceSummary)).len(), 2);
    let mut empty = State::default();
    assert!(empty.dispatch(Action::ApplyTraceFilters).is_empty());
    assert!(empty.dispatch(Action::ResetTraceFilters).is_empty());
    assert!(empty.dispatch(Action::Navigate(Page::Traces)).is_empty());
}

#[test]
fn trace_scope_checks_reject_mismatched_filters_and_list_sources() {
    for slot in [Slot::Traces, Slot::TraceSummary] {
        let mut s = state();
        let r = request(&mut s, slot);
        let mut wrong = fixture(if slot == Slot::Traces {
            "list"
        } else {
            "summary"
        });
        wrong["fromDate"] = json!("2026-10-01");
        s.complete(&r, Ok(wrong));
        assert_eq!(s.error(slot), "filter_mismatch");
        let r = request(&mut s, slot);
        let mut wrong = fixture(if slot == Slot::Traces {
            "list"
        } else {
            "summary"
        });
        wrong["toDate"] = json!("2026-10-02");
        s.complete(&r, Ok(wrong));
        assert_eq!(s.error(slot), "filter_mismatch");
    }
    let mut s = state();
    let r = request(&mut s, Slot::Traces);
    let mut wrong = fixture("list");
    wrong["source"]["id"] = json!("other");
    s.complete(&r, Ok(wrong));
    assert_eq!(s.error(Slot::Traces), "source_mismatch");
}

#[test]
fn trace_late_results_cannot_reopen_closed_detail_or_replace_newer_source_or_navigation() {
    for dismiss in [
        Action::Close,
        Action::Navigate(Page::Overview),
        Action::Source("other".into()),
        Action::Refresh,
    ] {
        let mut s = state();
        let r = s.dispatch(Action::Select(fixture("attempt"))).remove(0);
        assert_eq!(r.slot, Slot::TraceDetail);
        assert_eq!(
            r.path,
            "/api/traces/detail?attemptId=trace-completed&sourceId=demo"
        );
        s.dispatch(dismiss);
        assert!(!s.accepts(&r));
        s.complete(&r, Ok(fixture("detail")));
        assert!(s.data(Slot::TraceDetail).is_null());
    }
    let mut s = state();
    let old = s.dispatch(Action::Navigate(Page::Traces));
    s.dispatch(Action::Navigate(Page::Overview));
    assert!(old.iter().all(|r| !s.accepts(r)));
    let new = s.dispatch(Action::Navigate(Page::Traces));
    for r in old {
        s.complete(&r, Err("old_error".into()));
    }
    assert!(new.iter().all(|r| s.accepts(r)));
    let old = s.dispatch(Action::Select(fixture("attempt"))).remove(0);
    let mut second = fixture("attempt");
    second["attemptId"] = json!("second");
    let new = s.dispatch(Action::Select(second)).remove(0);
    s.complete(&old, Ok(fixture("detail")));
    assert!(s.accepts(&new));
    s.complete(&new, Ok(fixture("detail")));
    assert_eq!(s.error(Slot::TraceDetail), "attempt_mismatch");
    let r = s.dispatch(Action::Select(fixture("attempt"))).remove(0);
    let mut other = fixture("detail");
    other["attempt"]["sourceId"] = json!("other");
    s.complete(&r, Ok(other));
    assert_eq!(s.error(Slot::TraceDetail), "source_mismatch");
    let r = s.dispatch(Action::Select(fixture("attempt"))).remove(0);
    s.complete(&r, Ok(fixture("detail")));
    assert_eq!(s.data(Slot::TraceDetail)["contentRetained"], true);
    assert_eq!(
        Action::Load(Slot::TraceDetail, false).repaint(),
        (false, true)
    );
}

#[test]
fn bilingual_trace_views_show_requested_observed_and_direct_token_evidence_separately() {
    for l in [Language::English, Language::Chinese] {
        let mut s = state();
        s.language = l;
        let html = render(s.clone());
        for text in [
            l.text("Local trace reader", "本地追踪阅读器"),
            l.text("Requested model", "请求模型"),
            l.text("Observed response model", "观测响应模型"),
            l.text("Requested reasoning effort", "请求推理强度"),
            "requested-model",
            "observed-model",
            "priority",
            "high",
            "9,007,199,254,740,993,323",
            "synthetic_trace_warning",
            l.text("Load more traces", "加载更多追踪"),
        ] {
            assert!(html.contains(text), "missing {text}");
        }
        assert!(html.contains(l.text(
            "not a physical HTTP request count",
            "不代表物理 HTTP 请求数量"
        )));
        assert!(html.contains(l.text(
            "cannot be converted into exact quota charges",
            "不能换算为精确额度扣减"
        )));
        assert!(!html.contains("synthetic visible prompt"));
        s.selected = fixture("attempt");
        let html = render(s);
        for text in [
            l.text("Trace detail", "追踪详情"),
            l.text("Prepared request evidence only", "仅有已准备请求证据"),
            l.text("Redacted visible-text projections", "脱敏可见文本投影"),
            l.text(
                "not full raw requests or wire payloads",
                "不是完整原始请求或网络载荷",
            ),
            "direct-response",
            "[REDACTED]",
            "synthetic_projection_warning",
            l.text("Back to traces", "返回追踪"),
        ] {
            assert!(html.contains(text), "missing {text}");
        }
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
        assert!(!html.contains("<img src=x"));
        assert!(html.contains("data-modal=\"trace\""));
    }
}

#[test]
fn trace_scope_empty_uncaptured_invalid_loading_and_error_states_are_honest() {
    for l in [Language::English, Language::Chinese] {
        let mut s = state();
        s.language = l;
        let list = s.remotes.get_mut(&Slot::Traces).unwrap();
        list.value["attempts"] = json!([]);
        list.value["nextCursor"] = Value::Null;
        list.value["coverage"]["capture"] = json!("not_captured");
        let summary = s.remotes.get_mut(&Slot::TraceSummary).unwrap();
        for key in [
            "byRequestedModel",
            "byRequestedReasoningEffort",
            "byRequestedServiceTier",
            "byObservedModel",
            "byObservedServiceTier",
        ] {
            summary.value[key] = json!([]);
        }
        let html = render(s.clone());
        assert!(html.contains(l.text("Older databases are not migrated", "不会迁移旧版数据库")));
        assert!(html.contains(l.text("No retained inference records", "没有保留的推理记录")));
        assert!(html.contains(l.text("No group evidence", "暂无分组证据")));
        assert!(!html.contains(l.text("Load more traces", "加载更多追踪")));
        s.dispatch(Action::Filter("traceFrom", "2026-10-03".into()));
        s.dispatch(Action::ApplyTraceFilters);
        let html = render(s.clone());
        assert!(html.contains("invalid_trace_date_range"));
        assert!(html.contains(l.text("Choose a valid date pair", "请选择有效的起止日期")));
        s.dispatch(Action::ResetTraceFilters);
        let html = render(s.clone());
        assert!(html.contains(l.text("Loading local evidence", "正在加载本地证据")));
        let r = request(&mut s, Slot::TraceSummary);
        s.complete(&r, Err("synthetic_failure".into()));
        assert!(render(s).contains("synthetic_failure"));
        let html = render_with(state(), |_, _| {
            trace_scope(&json!({"fromDate":"2026-10-01","toDate":"2026-10-07"}), l)
        });
        assert!(html.contains("2026-10-01 – 2026-10-07 (UTC)"));
    }
}

#[test]
fn trace_projection_is_unavailable_when_content_disabled_deleted_or_missing() {
    for l in [Language::English, Language::Chinese] {
        for content in [
            Value::Null,
            json!({}),
            json!({"requestProjection":[],"responseProjection":""}),
        ] {
            let mut s = state();
            s.language = l;
            s.selected = fixture("attempt");
            let d = &mut s.remotes.get_mut(&Slot::TraceDetail).unwrap().value;
            d["content"] = content.clone();
            let html = render_with(s, trace_detail);
            assert!(!html.contains("synthetic visible prompt"));
            if content.is_null() {
                assert!(
                    html.contains(
                        l.text("No projection retained or available", "投影未保留或不可用")
                    )
                );
            } else if content == json!({}) {
                assert!(html.contains(l.text("Not retained", "未保留")));
            } else {
                assert!(html.contains(l.text("Empty retained text", "保留文本为空")));
            }
        }
        let mut s = state();
        s.language = l;
        s.selected = fixture("attempt");
        s.remotes.get_mut(&Slot::TraceDetail).unwrap().value["contentRetained"] = json!(false);
        let html = render_with(s.clone(), trace_detail);
        assert!(html.contains(l.text("No projection retained or available", "投影未保留或不可用")));
        assert!(!html.contains("synthetic visible prompt"));
        s.remotes.remove(&Slot::TraceDetail);
        let html = render_with(s, trace_detail);
        assert!(html.contains("requested-model"));
        assert!(!html.contains("synthetic visible prompt"));
    }
}

#[test]
fn current_utc_week_preset_uses_monday_sunday_and_handles_utc_day_and_year_boundaries() {
    for (now, from, to) in [
        ("2026-10-03T14:00:00.000Z", "2026-09-28", "2026-10-04"),
        ("2026-10-05T00:00:00Z", "2026-10-05", "2026-10-11"),
        ("2026-10-05T00:15:00+02:00", "2026-09-28", "2026-10-04"),
        ("2027-01-01T00:00:00Z", "2026-12-28", "2027-01-03"),
    ] {
        let filters = utc_week(now).unwrap();
        assert_eq!(filters.from, from);
        assert_eq!(filters.to, to);
        let mut s = state();
        let jobs = s.dispatch(Action::TraceWeek(now.into()));
        assert_eq!(jobs.len(), 2);
        assert!(
            jobs[0]
                .path
                .contains(&format!("fromDate={from}&toDate={to}"))
        );
        assert_eq!(s.trace_filters, s.applied_trace_filters);
    }
    assert!(utc_week("invalid").is_none());
    let mut s = state();
    assert!(s.dispatch(Action::TraceWeek("invalid".into())).is_empty());
    s.remotes.remove(&Slot::Overview);
    let html = render(s);
    assert!(html.contains("Local trace reader"));
    assert!(html.contains("This UTC week"));
}

#[test]
fn trace_replay_protection_is_disclosed_for_all_destructive_actions() {
    for l in [Language::English, Language::Chinese] {
        for action in [
            Destructive::All,
            Destructive::Content,
            Destructive::Retention,
        ] {
            let mut s = state();
            s.language = l;
            s.action = Some(action);
            let html = render(s);
            assert!(html.contains(l.text("Trace replay protection", "追踪重放保护")));
            assert!(html.contains(l.text("bundle fingerprints", "数据包指纹")));
        }
    }
}

#[test]
fn trace_group_truncation_is_visible_without_claiming_partial_overall_totals() {
    for l in [Language::English, Language::Chinese] {
        let mut s = state();
        s.language = l;
        let warning = l.text(
            "Some metadata groups omitted; totals include all matching records",
            "部分元数据分组已省略；总量包含全部匹配记录",
        );
        assert!(!render(s.clone()).contains(warning));
        s.remotes.get_mut(&Slot::TraceSummary).unwrap().value["groupsTruncated"] = json!(true);
        assert!(render(s.clone()).contains(warning));
        s.remotes.get_mut(&Slot::TraceSummary).unwrap().value["groupsTruncated"] = json!(false);
        assert!(!render(s).contains(warning));
    }
}

fn exact_filters() -> TraceFilters {
    TraceFilters {
        from: "2026-10-01".into(),
        to: "2026-10-07".into(),
        thread_id: "thread/A+B:@-_.1".into(),
        status: "completed".into(),
        requested_model: " Requested 模型 + ".into(),
        requested_reasoning_effort: "high".into(),
        requested_service_tier: "priority".into(),
    }
}
fn scoped_fixture(slot: Slot, filters: &TraceFilters) -> Value {
    let mut value = fixture(if slot == Slot::Traces {
        "list"
    } else {
        "summary"
    });
    for (key, text) in filters.values() {
        value[key] = if text.is_empty() {
            Value::Null
        } else {
            json!(text)
        };
    }
    value
}

#[test]
fn exact_trace_filters_share_list_summary_scope_and_keep_drafts_out_of_cursors() {
    let mut s = state();
    for (key, value) in [
        ("traceFrom", "2026-10-01"),
        ("traceTo", "2026-10-07"),
        ("traceThread", "thread/A+B:@-_.1"),
        ("traceStatus", "completed"),
        ("traceModel", " Requested 模型 + "),
        ("traceEffort", "high"),
        ("traceTier", "priority"),
    ] {
        assert!(s.dispatch(Action::Filter(key, value.into())).is_empty());
        assert_eq!(Action::Filter(key, value.into()).repaint(), (false, false));
    }
    assert_eq!(s.trace_filters, exact_filters());
    assert_eq!(s.applied_trace_filters, TraceFilters::default());
    let jobs = s.dispatch(Action::ApplyTraceFilters);
    for r in &jobs {
        assert!(
            r.path.contains("threadId=thread%2FA%2BB%3A%40-_.1"),
            "{}",
            r.path
        );
        assert!(r.path.contains("status=completed"));
        assert!(
            r.path
                .contains("requestedModel=%20Requested%20%E6%A8%A1%E5%9E%8B%20%2B%20")
        );
        assert!(
            r.path
                .contains("requestedReasoningEffort=high&requestedServiceTier=priority")
        );
        assert!(!r.path.contains("cursor="));
    }
    assert_eq!(s.applied_trace_filters, exact_filters());
    s.dispatch(Action::Filter("traceThread", "draft-thread".into()));
    s.dispatch(Action::Filter("traceStatus", "failed".into()));
    s.dispatch(Action::Filter("traceModel", "draft-model".into()));
    s.dispatch(Action::Filter("traceEffort", "low".into()));
    s.dispatch(Action::Filter("traceTier", "draft-tier".into()));
    for r in jobs {
        s.complete(&r, Ok(scoped_fixture(r.slot, &exact_filters())));
        assert_eq!(s.error(r.slot), "");
        assert_eq!(s.data(r.slot)["requestedModel"], " Requested 模型 + ");
    }
    let more = s.dispatch(Action::Load(Slot::Traces, true)).remove(0);
    assert!(more.path.contains("cursor=trace-next"));
    assert!(more.path.contains("status=completed"));
    assert!(!more.path.contains("draft"));
    let next = s.dispatch(Action::ApplyTraceFilters);
    assert!(!s.accepts(&more));
    s.complete(&more, Ok(scoped_fixture(Slot::Traces, &exact_filters())));
    assert!(s.data(Slot::Traces).is_null());
    assert!(next.iter().all(|r| !r.path.contains("cursor=")));
    assert!(
        next.iter()
            .all(|r| r.path.contains("threadId=draft-thread&status=failed"))
    );
    assert_eq!(s.filters, Filters::default());
    assert_eq!(s.token_period, Filters::default());
}

#[test]
fn every_exact_trace_filter_echo_is_required_for_both_list_and_summary() {
    for slot in [Slot::Traces, Slot::TraceSummary] {
        for (key, _) in exact_filters().values() {
            for replacement in [Value::Null, json!("wrong")] {
                let mut s = state();
                s.applied_trace_filters = exact_filters();
                let r = request(&mut s, slot);
                let mut wrong = scoped_fixture(slot, &exact_filters());
                wrong[key] = replacement;
                s.complete(&r, Ok(wrong));
                assert_eq!(s.error(slot), "filter_mismatch", "{slot:?} {key}");
                assert_ne!(s.data(slot)["threadId"], "thread/A+B:@-_.1");
            }
        }
        let mut s = state();
        let r = request(&mut s, slot);
        let mut wrong = scoped_fixture(slot, &TraceFilters::default());
        wrong["threadId"] = json!("unexpected-thread");
        s.complete(&r, Ok(wrong));
        assert_eq!(s.error(slot), "filter_mismatch");
    }
}

#[test]
fn trace_filter_validation_matches_exact_identifier_status_and_utf16_metadata_contract() {
    for value in ["a", "Thread.1_2:@/+-", &"a".repeat(160)] {
        assert!(valid_trace_thread(value));
        let filters = TraceFilters {
            thread_id: value.into(),
            ..TraceFilters::default()
        };
        assert_eq!(filters.validation_error(), None);
    }
    for value in [
        "",
        "-start",
        " space",
        "trailing ",
        "line\n",
        "a?b",
        "a&b",
        "模型",
        &"a".repeat(161),
    ] {
        assert!(!valid_trace_thread(value));
        if !value.is_empty() {
            let filters = TraceFilters {
                thread_id: value.into(),
                ..TraceFilters::default()
            };
            assert_eq!(filters.validation_error(), Some("invalid_trace_filter"));
        }
    }
    for status in TRACE_STATUSES {
        let filters = TraceFilters {
            status: status.into(),
            ..TraceFilters::default()
        };
        assert_eq!(filters.validation_error(), None);
    }
    for status in ["Completed", "unknown", "reported", "failed ", " "] {
        let filters = TraceFilters {
            status: status.into(),
            ..TraceFilters::default()
        };
        assert_eq!(filters.validation_error(), Some("invalid_trace_filter"));
    }
    for field in ["traceModel", "traceEffort", "traceTier"] {
        for value in [
            String::new(),
            " ".into(),
            "  Exact + 模型  ".into(),
            "a".repeat(128),
            "😀".repeat(64),
            "\u{80}".into(),
        ] {
            let mut s = state();
            s.dispatch(Action::Filter(field, value.clone()));
            assert_eq!(
                s.trace_filters.validation_error(),
                None,
                "{field}: {value:?}"
            );
            assert_eq!(s.dispatch(Action::ApplyTraceFilters).len(), 2);
        }
        for value in [
            "a".repeat(129),
            "😀".repeat(65),
            "a\0b".into(),
            "a\u{1f}b".into(),
            "a\u{7f}b".into(),
            "a\nb".into(),
        ] {
            let mut s = state();
            let old = request(&mut s, Slot::Traces);
            s.dispatch(Action::Filter(field, value.clone()));
            assert!(s.dispatch(Action::ApplyTraceFilters).is_empty());
            assert!(!s.accepts(&old));
            for slot in [Slot::Traces, Slot::TraceSummary] {
                assert_eq!(s.error(slot), "invalid_trace_filter", "{field}: {value:?}");
                assert!(s.data(slot).is_null());
            }
            s.dispatch(Action::Filter(field, "fixed".into()));
            assert_eq!(s.dispatch(s.retry_action(Slot::TraceSummary)).len(), 2);
        }
    }
}

#[test]
fn thread_navigation_uses_submitted_scope_and_cancels_older_detail_list_summary() {
    let mut s = state();
    s.trace_filters = exact_filters();
    let old = s.dispatch(Action::ApplyTraceFilters);
    let detail = s.dispatch(Action::Select(fixture("attempt"))).remove(0);
    s.dispatch(Action::Filter("traceTo", "2025-01-01".into()));
    s.dispatch(Action::Filter("traceStatus", "failed".into()));
    s.dispatch(Action::Filter("traceModel", "draft-only".into()));
    let action = Action::TraceThread {
        source: "demo".into(),
        thread: "synthetic-thread".into(),
    };
    assert_eq!(action.repaint(), (true, true));
    let jobs = s.dispatch(action);
    let mut expected = exact_filters();
    expected.thread_id = "synthetic-thread".into();
    assert_eq!(s.trace_filters, expected);
    assert_eq!(s.applied_trace_filters, expected);
    assert!(s.selected.is_null());
    assert!(s.data(Slot::TraceDetail).is_null());
    assert!(!s.accepts(&detail));
    for r in old {
        assert!(!s.accepts(&r));
        s.complete(&r, Ok(scoped_fixture(r.slot, &exact_filters())));
    }
    s.complete(&detail, Ok(fixture("detail")));
    assert!(s.selected.is_null());
    assert!(jobs.iter().all(|r| s.accepts(r)
        && r.path.contains("threadId=synthetic-thread")
        && !r.path.contains("cursor=")));
    for r in jobs {
        s.complete(&r, Ok(scoped_fixture(r.slot, &expected)));
    }
    let html = render(s);
    assert!(html.contains("synthetic-thread"));
    assert!(!html.contains("draft-only"));
}

#[test]
fn invalid_cross_source_and_busy_thread_navigation_cannot_retarget_evidence() {
    for (source, thread) in [
        ("other", "thread"),
        ("", "thread"),
        ("demo", ""),
        ("demo", "not valid"),
    ] {
        let mut s = state();
        s.selected = fixture("attempt");
        let before = s.selected.clone();
        assert!(
            s.dispatch(Action::TraceThread {
                source: source.into(),
                thread: thread.into()
            })
            .is_empty()
        );
        assert_eq!(s.selected, before);
        assert_eq!(s.trace_filters, TraceFilters::default());
    }
    let mut s = state();
    s.remotes.insert(
        Slot::Mutation,
        Remote {
            loading: true,
            ..Remote::default()
        },
    );
    assert!(
        s.dispatch(Action::TraceThread {
            source: "demo".into(),
            thread: "thread".into()
        })
        .is_empty()
    );
    assert_eq!(s.trace_filters, TraceFilters::default());
    let mut empty = State::default();
    assert!(
        empty
            .dispatch(Action::TraceThread {
                source: "".into(),
                thread: "thread".into()
            })
            .is_empty()
    );
}

#[test]
fn utc_week_preserves_exact_drafts_and_reset_clears_every_submitted_filter() {
    let mut s = state();
    s.trace_filters = exact_filters();
    let jobs = s.dispatch(Action::TraceWeek("2026-10-03T14:00:00Z".into()));
    assert_eq!(s.trace_filters.from, "2026-09-28");
    assert_eq!(s.trace_filters.to, "2026-10-04");
    assert_eq!(s.trace_filters.thread_id, exact_filters().thread_id);
    assert_eq!(
        s.trace_filters.requested_model,
        exact_filters().requested_model
    );
    for r in &jobs {
        assert!(
            r.path
                .contains("requestedReasoningEffort=high&requestedServiceTier=priority")
        );
    }
    let reset = s.dispatch(Action::ResetTraceFilters);
    assert_eq!(s.trace_filters, TraceFilters::default());
    assert_eq!(s.applied_trace_filters, TraceFilters::default());
    assert_eq!(reset[0].path, "/api/traces?limit=20&sourceId=demo");
    assert_eq!(reset[1].path, "/api/traces/summary?sourceId=demo");
    assert!(jobs.iter().all(|r| !s.accepts(r)));
}

#[test]
fn exact_filter_controls_submitted_scopes_and_navigation_render_safely_in_both_languages() {
    for l in [Language::English, Language::Chinese] {
        let mut s = state();
        s.language = l;
        s.trace_filters = exact_filters();
        s.applied_trace_filters = exact_filters();
        for slot in [Slot::Traces, Slot::TraceSummary] {
            put(&mut s, slot, scoped_fixture(slot, &exact_filters()));
        }
        let html = render(s.clone());
        for text in [
            l.text("Thread ID (exact)", "会话 ID（精确匹配）"),
            l.text("Trace status", "追踪状态"),
            l.text("Requested model (exact)", "请求模型（精确匹配）"),
            l.text(
                "Requested reasoning effort (exact)",
                "请求推理强度（精确匹配）",
            ),
            l.text("Requested service tier (exact)", "请求服务档位（精确匹配）"),
            l.text("Apply trace filters", "应用追踪筛选"),
            l.text("Clear all trace filters", "清除全部追踪筛选"),
            l.text("View this thread", "查看此会话"),
            l.text("match only reported request values", "仅匹配已报告的请求值"),
        ] {
            assert!(html.contains(text), "missing {text}");
        }
        let status_label = format!(
            "<span id=\"trace-status-label\">{}</span>",
            l.text("Trace status", "追踪状态")
        );
        assert!(html.contains(&status_label));
        assert!(html.contains("aria-labelledby=\"trace-status-label\" name=\"traceStatus\""));
        assert!(html.contains("maxlength=\"160\""));
        assert_eq!(html.matches("maxlength=\"128\"").count(), 3);
        assert_eq!(html.matches("class=\"trace-exact-scope\"").count(), 2);
        assert!(html.contains("thread/A+B:@-_.1"));
        assert!(!html.contains(l.text("Unsubmitted edits", "尚有未提交的编辑")));
        s.dispatch(Action::Filter(
            "traceModel",
            "<script>draft-only</script>".into(),
        ));
        let html = render(s.clone());
        assert!(html.contains(l.text("Unsubmitted edits", "尚有未提交的编辑")));
        assert!(!html.contains("<script>draft-only</script>"));
        let mut scope = scoped_fixture(Slot::Traces, &exact_filters());
        scope["requestedModel"] = json!("<img onerror=alert(1)>");
        let html = render_with(s.clone(), |_, _| trace_scope(&scope, l));
        assert!(html.contains("&lt;img onerror=alert(1)&gt;"));
        assert!(!html.contains("<img"));
        s.selected = fixture("attempt");
        assert!(
            render_with(s.clone(), trace_detail)
                .contains(l.text("View thread synthetic-thread", "查看会话 synthetic-thread"))
        );
        s.dispatch(Action::Filter("traceThread", "invalid thread".into()));
        s.dispatch(Action::ApplyTraceFilters);
        let html = render(s);
        assert!(html.contains("invalid_trace_filter"));
        assert!(html.contains(l.text("Values are not trimmed", "不会删除首尾空格")));
        for thread in [Value::Null, json!(""), json!("bad thread")] {
            let mut attempt = fixture("attempt");
            attempt["threadId"] = thread;
            assert!(
                !render_with(state(), |_, ui| trace_thread_navigation(&attempt, l, ui))
                    .contains("<button")
            );
        }
        let mut no_source = state();
        no_source.source.clear();
        let html = render_with(no_source, trace_page);
        assert!(html.contains("disabled"));
    }
}
