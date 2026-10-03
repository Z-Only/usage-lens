#![recursion_limit = "256"]
use leptos::prelude::*;
use serde_json::{Value, json};
use usage_lens_ui::model::Action;
use usage_lens_ui::{model::*, views::*};
fn fixture(key: &str) -> Value {
    serde_json::from_str::<Value>(include_str!("fixtures.json")).unwrap()[key].clone()
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
fn full() -> State {
    let mut s = State::default();
    s.source = "demo".into();
    for (slot, key) in [
        (Slot::Status, "status"),
        (Slot::Overview, "overview"),
        (Slot::Health, "health"),
        (Slot::Recent, "events"),
        (Slot::Activity, "events"),
        (Slot::Skills, "skills"),
        (Slot::SkillSummary, "skillSummary"),
        (Slot::History, "history"),
        (Slot::ResponseUsage, "responseUsage"),
        (Slot::ResponseRecords, "responseRecords"),
        (Slot::Detail, "detail"),
    ] {
        put(&mut s, slot, fixture(key));
    }
    s
}
fn render_with(s: State, f: impl FnOnce(&State, Ui) -> AnyView) -> String {
    let owner = Owner::new();
    owner.with(|| {
        let state = RwSignal::new(s.clone());
        let ui = Ui {
            state,
            revision: RwSignal::new(0),
            modal_revision: RwSignal::new(0),
            send: Callback::new(move |a| {
                state.update(|s| {
                    s.dispatch(a);
                });
            }),
        };
        f(&s, ui).to_html()
    })
}
fn render(s: State) -> String {
    render_with(s, |_, ui| view! { <Dashboard ui /> }.into_any())
}
#[test]
fn shell_renders_five_views_without_external_assets() {
    let html = render(full());
    for text in [
        "Usage Lens",
        "Main navigation",
        "Overview",
        "Activity",
        "Quotas",
        "Skills",
        "Settings",
        "Skip to content",
        "Local SQLite",
        "Unknown ≠ zero",
        "Source notes",
    ] {
        assert!(html.contains(text), "missing {text}");
    }
    assert!(!html.contains("https://"));
    assert!(!html.contains("<script"));
}
#[test]
fn both_languages_have_complete_main_views() {
    for page in PAGES {
        for language in [Language::English, Language::Chinese] {
            let mut s = full();
            s.page = page;
            s.language = language;
            s.dark = true;
            let html = render(s);
            assert!(html.contains(page.label(language)));
            assert!(html.contains(if language == Language::Chinese {
                "使用浅色主题"
            } else {
                "Use light theme"
            }));
        }
    }
}
#[test]
fn overview_preserves_accepted_composition_and_scope() {
    let html = render(full());
    assert!(html.contains("9,007,199,254,740,993,123,456"));
    assert!(html.contains("演示数据 / Demo data"));
    for text in [
        "metric-band",
        "overview-grid",
        "chart-panel",
        "quota-panel",
        "Recent activity",
        "Freshness: recorded snapshots",
        "Cloud activity may be missing",
        "Account binding unverified",
    ] {
        assert!(html.contains(text));
    }
    assert!(!html.contains("Imported response tokens"));
}
#[test]
fn unavailable_overview_never_substitutes_zero() {
    let mut s = full();
    let o = &mut s.remotes.get_mut(&Slot::Overview).unwrap().value;
    o["usage"] = json!({"status":"unavailable","reason":"not_collected"});
    o["quota"] = o["usage"].clone();
    put(&mut s, Slot::Recent, json!({"events":[]}));
    let h = render(s);
    for text in [
        "Token activity unavailable",
        "Quota unavailable",
        "No recorded events",
        "Usage snapshot not collected",
        "—",
    ] {
        assert!(h.contains(text));
    }
}
#[test]
fn empty_loading_failed_paused_and_stale_states_are_visible() {
    let empty = render(State::default());
    assert!(empty.contains("No local source configured"));
    let mut s = State::default();
    s.dispatch(Action::Refresh);
    let loading = render(s);
    assert!(loading.contains("Reading local evidence"));
    let mut s = full();
    s.remotes.get_mut(&Slot::Status).unwrap().error = "offline".into();
    s.remotes.get_mut(&Slot::Status).unwrap().value["settings"]["capturePaused"] = json!(true);
    s.remotes.get_mut(&Slot::Overview).unwrap().value["usage"]["freshness"]["stale"] = json!(true);
    s.remotes.get_mut(&Slot::Overview).unwrap().value["quota"]["lastFailure"] =
        json!({"errorCode":"rpc_error"});
    let h = render(s);
    for text in [
        "Could not refresh local data",
        "Showing the previous response",
        "Capture is paused",
        "Stale",
        "Latest source read failed",
        "rpc_error",
    ] {
        assert!(h.contains(text));
    }
}
#[test]
fn chart_exact_table_includes_zero_and_unknown() {
    let h = render(full());
    assert!(h.contains("bar zero"));
    assert!(h.contains("bar unknown"));
    assert!(h.contains("not_reported"));
    assert!(h.contains("Exact daily values"));
    assert!(h.contains("Source time zone: unknown"));
}
#[test]
fn unknown_date_bucket_is_in_table_but_not_chart() {
    let mut s = full();
    s.remotes.get_mut(&Slot::Overview).unwrap().value["usage"]["data"]["dailyUsageBuckets"]["value"] = json!([{"startDate":{"status":"invalid","value":null},"tokens":{"status":"reported","value":"5000"}}]);
    let h = render(s);
    assert!(h.contains("cannot be positioned"));
    assert!(h.contains("5,000"));
    assert!(!h.contains("chart-column"));
}
#[test]
fn quota_unknown_windows_no_fill_and_over100_text_preserved() {
    let mut q = fixture("overview")["quota"].clone();
    q["freshness"]["stale"] = json!(true);
    q["data"]["buckets"][0]["primary"] = json!({"status":"not_reported","value":null});
    q["data"]["buckets"][0]["secondary"]["value"]["usedPercent"] =
        json!({"status":"invalid","value":null});
    let h = render_with(full(), |_, _| quota_panel(&q, true, Language::English));
    assert!(h.contains("Window not reported"));
    assert!(h.contains("Used percentage not reported"));
    assert!(!h.contains("<progress"));
    q["data"]["buckets"][0]["secondary"]["value"]["usedPercent"] =
        json!({"status":"reported","value":123.45});
    let h = render_with(full(), |_, _| quota_panel(&q, true, Language::English));
    assert!(h.contains("123.45%"));
    assert!(h.contains("Bars cap at 100%"));
    assert!(h.contains("Resets at · Unix seconds"));
}
#[test]
fn evidence_uses_observation_fallback_unknown_model_and_subtypes() {
    let mut s = full();
    s.remotes.get_mut(&Slot::Recent).unwrap().value["events"][0]["skillEvidenceKind"] =
        json!("main_read");
    let h = render(s);
    for text in [
        "Collection time",
        "Unknown",
        "main read",
        "explicit tool call",
        "View event event-1",
    ] {
        assert!(h.contains(text));
    }
}
#[test]
fn activity_has_all_filters_and_pagination() {
    let mut s = full();
    s.page = Page::Activity;
    let r = s.remotes.get_mut(&Slot::Activity).unwrap();
    r.value["nextCursor"] = json!("next");
    r.loading = true;
    r.error = "offline".into();
    let h = render(s);
    for text in [
        "Apply filters",
        "Reset",
        "from",
        "to",
        "eventType",
        "Exact model name",
        "Partial history",
        "Load more records",
        "offline",
        "Loading local evidence",
    ] {
        assert!(h.contains(text));
    }
}
#[test]
fn skill_main_read_and_injection_are_separate_groups() {
    let mut s = full();
    s.page = Page::Skills;
    s.kind = "loaded".into();
    s.remotes.get_mut(&Slot::Overview).unwrap().value["events"]["skills"] = json!([{"kind":"loaded","name":"spreadsheets","evidenceKind":"main_read","count":"2"},{"kind":"loaded","name":"spreadsheets","evidenceKind":"instruction_injection","count":"1"},{"kind":"loaded","name":null,"count":"0"}]);
    let h = render(s);
    for text in [
        "main read",
        "instruction injection",
        "not unique skill executions",
        "Unknown skill",
        "Evidence subtype unknown",
    ] {
        assert!(h.contains(text));
    }
    assert!(h.contains("aria-pressed=\"true\""));
}
#[test]
fn absent_skill_evidence_explains_unknown_not_zero() {
    let mut s = full();
    s.page = Page::Skills;
    put(
        &mut s,
        Slot::Skills,
        json!({"status":"unavailable","events":[]}),
    );
    let h = render(s);
    assert!(h.contains("No direct evidence collected"));
    assert!(h.contains("Usage is unknown, not zero"));
    assert!(h.contains("not an invocation"));
}
#[test]
fn history_has_collection_time_and_independent_snapshot_panels() {
    let mut s = full();
    s.page = Page::Quotas;
    s.remotes.get_mut(&Slot::History).unwrap().value["nextCursor"] = json!("next");
    let h = render(s.clone());
    assert!(h.contains("Snapshot history"));
    assert!(h.contains("changes between samples are unknown"));
    assert!(h.contains("Load older snapshots"));
    assert!(h.contains("history-row"));
    s.remotes.get_mut(&Slot::History).unwrap().value["observations"] = json!([]);
    assert!(render(s).contains("No quota snapshots recorded"));
}
#[test]
fn imported_response_region_never_combines_account_totals() {
    let mut s = full();
    s.remotes.get_mut(&Slot::Overview).unwrap().value["source"]["mode"] = json!("imported");
    let h = render(s);
    for text in [
        "Imported response tokens",
        "not added to account totals",
        "9,007,199,254,740,993,123,456",
        "1,200",
        "Cached input · subset",
        "Reasoning output · subset",
        "not recomputed",
        "Possible overlapping fork history",
        "responseId",
        "thread-1",
        "Import time",
    ] {
        assert!(h.contains(text));
    }
}
#[test]
fn imported_zero_is_no_evidence_not_zero_history() {
    let mut s = full();
    s.remotes.get_mut(&Slot::ResponseUsage).unwrap().value["responseCount"] = json!("0");
    s.remotes.remove(&Slot::ResponseRecords);
    let h = render_with(s, imported);
    assert!(h.contains("Actual historical usage is unknown"));
    assert!(h.contains("Inspect response records"));
    assert!(!h.contains("response-totals"));
}
#[test]
fn response_record_loading_empty_error_and_pagination() {
    let mut s = full();
    let r = s.remotes.get_mut(&Slot::ResponseRecords).unwrap();
    r.value["records"] = json!([]);
    r.value["nextCursor"] = json!("next");
    r.error = "record_failed".into();
    r.loading = true;
    let h = render_with(s, imported);
    for text in [
        "No response records in this selection",
        "record_failed",
        "Load more responses",
        "Loading local evidence",
    ] {
        assert!(h.contains(text));
    }
}
#[test]
fn source_details_preserve_capabilities_and_unknown_identities() {
    let mut s = full();
    let o = &mut s.remotes.get_mut(&Slot::Overview).unwrap().value;
    o["account"] = json!({"status":"available","data":{"type":{"status":"reported","value":"chatgpt"},"planType":{"status":"reported","value":"pro"}},"freshness":{"sourceAsOf":"2026-10-01T00:00:00Z"}});
    o["events"]["byModel"] = json!([{"name":null,"count":"0"}]);
    o["events"]["tools"] = json!([{"name":null,"count":"9007199254740993123456"}]);
    s.remotes.get_mut(&Slot::Status).unwrap().value["capabilities"] = json!([{"sourceId":"demo","method":"events","state":"partial","reason":"Local hook records only"},{"sourceId":"other","method":"secret","state":"available","reason":"WRONG SOURCE"}]);
    let h = render_with(s, |s, _| source_details(s));
    for text in [
        "chatgpt",
        "Unknown model",
        "Unknown tool",
        "9,007,199,254,740,993,123,456",
        "Local hook records only",
        "2026-10-01T00:00:00Z",
    ] {
        assert!(h.contains(text));
    }
    assert!(!h.contains("WRONG SOURCE"));
}
#[test]
fn settings_disclose_local_privacy_and_scope() {
    let mut s = full();
    s.page = Page::Settings;
    s.success = true;
    let h = render(s);
    for text in [
        "Collection &amp; privacy",
        "Pause capture",
        "Retain local content",
        "best-effort",
        "Save settings",
        "Local data updated",
        "Apply retention to all sources",
        "Delete source records",
    ] {
        assert!(h.contains(text), "missing {text}");
    }
}
#[test]
fn confirmation_shows_permanent_scope_and_saved_not_draft_retention() {
    let mut s = full();
    s.settings["retentionDays"] = json!(7);
    for action in [
        Destructive::Retention,
        Destructive::Content,
        Destructive::All,
    ] {
        s.action = Some(action);
        let h = render_with(s.clone(), modal);
        assert!(h.contains("permanently removes"));
        assert!(h.contains(action.required()));
        assert!(h.contains("Confirm permanent deletion"));
        assert!(h.contains("aria-modal=\"true\""));
        if action == Destructive::Retention {
            assert!(h.contains("ALL sources"));
            assert!(h.contains("30"));
            assert!(h.contains("collection/import timestamps"));
        } else {
            assert!(h.contains("Synthetic local demo (demo)"));
        }
    }
}
#[test]
fn confirmation_busy_errors_are_visible_and_controls_disable() {
    let mut s = full();
    s.action = Some(Destructive::All);
    s.confirmation = "DELETE".into();
    s.remotes.insert(
        Slot::Mutation,
        Remote {
            loading: true,
            error: "mutation_failed".into(),
            ..Remote::default()
        },
    );
    let h = render_with(s, modal);
    assert!(h.contains("Working…"));
    assert!(h.contains("mutation_failed"));
    assert!(h.contains("disabled"));
}
#[test]
fn event_content_is_escaped_text_only() {
    let mut s = full();
    s.selected = fixture("event");
    let h = render_with(s, modal);
    assert!(h.contains("&lt;img src=x onerror=alert(1)&gt;"));
    assert!(!h.contains("<img"));
    assert!(h.contains("Local-only content"));
    assert!(h.contains("Plugin queries never receive message bodies"));
    assert!(h.contains("Best-effort redaction"));
    assert!(h.contains("content-body"));
}
#[test]
fn event_missing_retention_and_error_states() {
    let mut s = full();
    s.selected = fixture("event");
    let d = s.remotes.get_mut(&Slot::Detail).unwrap();
    d.value["contentRetained"] = json!(false);
    d.value["content"] = Value::Null;
    d.error = "detail_failed".into();
    d.loading = true;
    let h = render_with(s, modal);
    assert!(h.contains("Content was not retained"));
    assert!(h.contains("detail_failed"));
    assert!(!h.contains("content-body"));
}
#[test]
fn click_callback_routes_to_native_action_reducer() {
    let owner = Owner::new();
    owner.with(|| {
        let state = RwSignal::new(State::default());
        let ui = Ui {
            state,
            revision: RwSignal::new(0),
            modal_revision: RwSignal::new(0),
            send: Callback::new(move |a| {
                state.update(|s| {
                    s.dispatch(a);
                })
            }),
        };
        click_action::<()>(ui, Action::ToggleTheme)(());
        assert!(state.get_untracked().dark);
        ui.send(Action::Language("zh".into()));
        assert_eq!(ui.text("English", "中文"), "中文");
    });
}
#[test]
fn icons_all_have_safe_builtin_paths() {
    for name in [
        "overview", "activity", "quotas", "skills", "settings", "refresh", "moon", "sun", "clock",
        "storage", "close", "arrow", "unknown",
    ] {
        assert!(!icon_path(name).is_empty());
    }
    assert_eq!(icon_path("unknown"), icon_path("info"));
}
#[test]
fn source_css_keeps_responsive_theme_and_focus_design() {
    let css = include_str!("../public/styles.css");
    for text in [
        "grid-template-columns: 208px",
        "max-width: 760px",
        "data-theme=\"dark\"",
        "focus-visible",
        "prefers-reduced-motion",
        "overflow-x: auto",
        "dialog::backdrop",
    ] {
        assert!(css.contains(text));
    }
    assert!(!css.contains("https://"));
}
#[test]
fn configured_source_fetch_failure_is_not_a_missing_source() {
    let mut s = full();
    s.remotes.get_mut(&Slot::Overview).unwrap().value = Value::Null;
    s.remotes.get_mut(&Slot::Overview).unwrap().error = "read_failed".into();
    let html = render(s);
    assert!(html.contains("Could not refresh local data"));
    assert!(html.contains("read_failed"));
    assert!(!html.contains("No local source configured"));
}
#[test]
fn scrollable_tables_contain_accessible_labels_without_hiding_page_content() {
    let css = include_str!("../public/styles.css");
    let table_rule = css
        .split(".table-scroll {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(table_rule.contains("position: relative"));
    assert!(table_rule.contains("overflow-x: auto"));
    let body_rule = css
        .split("body {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(!body_rule.contains("overflow"));
    let quota_unit_rule = css
        .split(".quota-value small {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(quota_unit_rule.contains("margin-inline-start: 0.35em"));
    assert!(render(full()).contains("<span class=\"sr-only\">Details</span>"));
}

#[test]
fn health_is_visible_on_overview_and_settings_without_needing_overview_success() {
    for page in [Page::Overview, Page::Settings] {
        for l in [Language::English, Language::Chinese] {
            let mut s = full();
            s.page = page;
            s.language = l;
            s.remotes.remove(&Slot::Overview);
            s.remotes.entry(Slot::Overview).or_default().error = "overview_unavailable".into();
            let html = render(s);
            assert!(html.contains(l.text("Collection health &amp; coverage", "采集健康与覆盖")));
            assert!(html.contains("account/usage/read"));
        }
    }
    assert!(!render(State::default()).contains("collection-health-title"));
    let mut s = full();
    s.page = Page::Activity;
    assert!(!render(s).contains("collection-health-title"));
}
#[test]
fn health_keeps_exact_counts_scope_and_unknown_history_in_both_languages() {
    for l in [Language::English, Language::Chinese] {
        let mut s = full();
        s.language = l;
        let health = &mut s.remotes.get_mut(&Slot::Health).unwrap().value;
        health["stored"]["events"]["count"] = json!("9007199254740993123456");
        health["stored"]["skills"]["count"] = json!("0");
        health["stored"]["responseTokens"]["count"] = Value::Null;
        health["stored"]["imports"]["lastImportedAt"] = Value::Null;
        health["stored"]["events"]["unknownOccurredAtCount"] = json!("9007199254740993123456");
        let h = render_with(s, collection_health);
        for text in [
            l.text("Stored records only", "仅限已存储记录"),
            l.text("No live account access check", "未检查实时账户访问"),
            l.text(
                "Zero records does not prove zero historical usage",
                "零记录不能证明历史用量为零",
            ),
            l.text("Skill evidence", "技能证据"),
            l.text("Response token records", "响应 Token 记录"),
            l.text("Occurrence time unknown", "发生时间未知"),
            l.text("Missing records: Unknown", "缺失记录：未知"),
            l.text("History before collection: Unknown", "采集之前的历史：未知"),
            "9,007,199,254,740,993,123,456",
            "2026-10-02T00:00:00Z",
        ] {
            assert!(h.contains(text), "missing {text}");
        }
        assert!(!h.contains("<progress"));
        assert!(!h.contains('%'));
        assert!(!h.contains("9007199254740993000000"));
    }
}
#[test]
fn health_missing_available_unsupported_and_error_remain_independent() {
    for l in [Language::English, Language::Chinese] {
        let mut s = full();
        s.language = l;
        let h = &mut s.remotes.get_mut(&Slot::Health).unwrap().value;
        h["observations"][0]["availability"] = json!("missing");
        h["observations"][0]["capability"] = json!("unknown");
        h["observations"][0]["freshness"] = json!({"state":"unknown"});
        h["observations"][1]["capability"] = json!("unsupported");
        h["observations"][1]["freshness"]["state"] = json!("stale");
        h["observations"][1]["lastFailure"] = json!({"errorCode":"method_not_found","attemptedAt":"2026-10-02T00:04:00Z","state":"recent","atOrAfterLatestObservation":true});
        h["observations"][2]["freshness"]["state"] = json!("future");
        let html = render_with(s, collection_health);
        for text in [
            l.text("Missing", "缺失"),
            l.text("Available", "可用"),
            l.text("Unsupported", "不支持"),
            l.text("Unknown", "未知"),
            l.text("Stale", "已过期"),
            l.text("Future timestamp", "未来时间戳"),
            l.text("Last recorded error", "最近记录的错误"),
            l.text(
                "freshness cannot be treated as current",
                "不可将其视为最新状态",
            ),
            l.text(
                "At or after the retained snapshot, if any",
                "等于或晚于保留的快照（如有）",
            ),
            "method_not_found",
            "2026-10-02T00:04:00Z",
        ] {
            assert!(html.contains(text), "missing {text}");
        }
    }
}
#[test]
fn health_prior_error_is_not_claimed_to_be_latest_attempt_failure() {
    for l in [Language::English, Language::Chinese] {
        for (relation, label) in [
            (
                json!(false),
                l.text("Earlier than the retained snapshot", "早于保留的快照"),
            ),
            (
                Value::Null,
                l.text(
                    "Relation to retained snapshot: Unknown",
                    "与保留快照的先后关系：未知",
                ),
            ),
        ] {
            let mut s = full();
            s.language = l;
            s.remotes.get_mut(&Slot::Health).unwrap().value["observations"][0]["lastFailure"] = json!({"errorCode":"read_error","state":"stale","atOrAfterLatestObservation":relation});
            let html = render_with(s, collection_health);
            assert!(html.contains(label));
            assert!(html.contains(l.text("Last recorded error", "最近记录的错误")));
            assert!(!html.contains("Latest attempt failed"));
        }
    }
}
#[test]
fn health_loading_error_retry_and_retained_response_are_explicit() {
    for l in [Language::English, Language::Chinese] {
        let mut s = full();
        s.language = l;
        s.remotes.remove(&Slot::Health);
        let html = render_with(s.clone(), collection_health);
        assert!(html.contains(l.text("Collection health is unknown", "采集健康状态未知")));
        assert!(html.contains(l.text("Retry health", "重试健康检查")));
        s.remotes.entry(Slot::Health).or_default().loading = true;
        let html = render_with(s.clone(), collection_health);
        assert!(html.contains(l.text("Reading collection health", "正在读取采集健康状态")));
        assert!(html.contains("aria-busy=\"true\""));
        assert!(html.contains("disabled"));
        s.remotes.get_mut(&Slot::Health).unwrap().loading = false;
        s.remotes.get_mut(&Slot::Health).unwrap().error = "health_request_failed".into();
        let html = render_with(s.clone(), collection_health);
        assert!(html.contains(l.text("Could not read collection health", "无法读取采集健康状态")));
        assert!(html.contains("health_request_failed"));
        assert!(!html.contains(l.text(
            "Showing the previous health response",
            "正在显示上次健康响应"
        )));
        s.remotes.get_mut(&Slot::Health).unwrap().value = fixture("health");
        let html = render_with(s, collection_health);
        assert!(html.contains(l.text(
            "Showing the previous health response",
            "正在显示上次健康响应"
        )));
        assert!(html.contains("account/read"));
    }
}
#[test]
fn health_capture_setting_and_notes_are_safe_and_have_explicit_scope() {
    for l in [Language::English, Language::Chinese] {
        for (value, label) in [
            (json!(true), l.text("Paused", "已暂停")),
            (json!(false), l.text("Not paused", "未暂停")),
            (Value::Null, l.text("Unknown", "未知")),
        ] {
            let mut s = full();
            s.language = l;
            let h = &mut s.remotes.get_mut(&Slot::Health).unwrap().value;
            h["settings"]["capturePaused"] = value;
            h["warnings"] = json!(["<img src=x onerror=alert(1)>"]);
            let html = render_with(s, collection_health);
            assert!(html.contains(label));
            assert!(html.contains(l.text("All sources", "所有来源")));
            assert!(html.contains(l.text(
                "Not paused does not confirm that a collector is running",
                "未暂停不代表采集器正在运行"
            )));
            assert!(html.contains(l.text("Collection health notes", "采集健康说明")));
            assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
            assert!(!html.contains("<img"));
        }
    }
}

#[test]
fn daily_skill_trends_preserve_exact_counts_independent_states_and_missing_days() {
    for language in [Language::English, Language::Chinese] {
        let mut s = full();
        s.page = Page::Skills;
        s.language = language;
        let h = render(s);
        for text in [
            "2026-10-01",
            "2026-10-03",
            "9,007,199,254,740,993,123,456",
            "scope=\"row\"",
            "scope=\"col\"",
            "tabindex=\"0\"",
            "skill-trend-totals",
        ] {
            assert!(h.contains(text), "missing {text}");
        }
        assert!(!h.contains("<th scope=\"row\">2026-10-02"));
        let phrases = if language == Language::English {
            [
                "Daily skill evidence",
                "Loaded · main read",
                "Loaded · instruction injection",
                "Loaded · unknown subtype",
                "outside the dated totals",
                "omitted days are unknown",
                "All retained evidence below",
            ]
        } else {
            [
                "每日技能证据",
                "加载 · 主文件读取",
                "加载 · 指令注入",
                "加载 · 未知子类型",
                "不计入日期范围合计",
                "未列出的日期为未知",
                "下方为全部保留证据",
            ]
        };
        for phrase in phrases {
            assert!(h.contains(phrase), "missing {phrase}");
        }
        assert!(!h.contains("<canvas"));
        assert!(!h.contains("success rate"));
    }
}
#[test]
fn daily_skill_trends_have_prompt_loading_validation_error_retry_and_empty_states() {
    for language in [Language::English, Language::Chinese] {
        let mut s = full();
        s.language = language;
        s.remotes.remove(&Slot::SkillSummary);
        let h = render_with(s.clone(), skill_trends);
        assert!(h.contains(language.text("Apply a date range", "应用日期范围")));
        assert!(!h.contains("skill-trend-totals"));
        s.remotes.entry(Slot::SkillSummary).or_default().loading = true;
        let h = render_with(s.clone(), skill_trends);
        assert!(h.contains("aria-busy=\"true\""));
        assert!(h.contains(language.text("Loading local evidence", "正在加载本地证据")));
        assert!(!h.contains(language.text("Apply a date range", "应用日期范围")));
        for code in ["invalid_skill_date_range", "offline", "filter_mismatch"] {
            s.remotes.get_mut(&Slot::SkillSummary).unwrap().error = code.into();
            let h = render_with(s.clone(), skill_trends);
            assert!(h.contains("role=\"alert\""));
            assert!(h.contains(language.text("Retry", "重试")));
            assert!(h.contains(if code == "invalid_skill_date_range" {
                language.text("Choose a valid UTC date pair", "请选择有效的 UTC 起止日期")
            } else {
                language.text("Could not load daily evidence", "无法加载每日证据")
            }));
        }
        let mut response = fixture("skillSummary");
        response["daily"] = json!([]);
        response["totals"] = Value::Null;
        put(&mut s, Slot::SkillSummary, response);
        let h = render_with(s, skill_trends);
        assert!(h.contains(language.text(
            "No dated evidence in this range",
            "此范围内没有带日期的证据"
        )));
        assert!(h.contains("—"));
        assert!(!h.contains("<table"));
    }
}
#[test]
fn daily_skill_trends_escape_exact_skill_warnings_and_show_truncation_without_fake_days() {
    let mut s = full();
    let response = &mut s.remotes.get_mut(&Slot::SkillSummary).unwrap().value;
    response["skillName"] = json!("<img src=x onerror=alert(1)>");
    response["warnings"] = json!(["<script>bad()</script>"]);
    response["skillsTruncated"] = json!(true);
    let h = render_with(s, skill_trends);
    assert!(h.contains("&lt;img"));
    assert!(h.contains("&lt;script&gt;"));
    assert!(!h.contains("<script>"));
    assert!(h.contains("limited to 500 entries"));
    assert!(h.contains("daily totals include all matching evidence"));
}
