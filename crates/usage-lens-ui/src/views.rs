//! Escaped Leptos DOM views, shared unchanged by browser CSR and native SSR tests.
use crate::event_bridge;
use crate::model::Action;
use crate::model::*;
use crate::reading::*;
use leptos::prelude::*;
use serde_json::Value;

#[derive(Clone, Copy)]
pub struct Ui {
    pub state: RwSignal<State>,
    pub revision: RwSignal<u64>,
    pub modal_revision: RwSignal<u64>,
    pub send: Callback<Action>,
}
impl Ui {
    pub fn text(self, en: &'static str, zh: &'static str) -> &'static str {
        self.state.with(|s| s.language.text(en, zh))
    }
    pub fn send(self, action: Action) {
        self.send.run(action);
    }
}
pub fn click_action<E>(ui: Ui, action: Action) -> impl Fn(E) {
    move |_| ui.send(action.clone())
}
pub fn icon_path(name: &str) -> &'static str {
    match name {
        "overview" => "m3 10 9-7 9 7M5 9v12h5v-7h4v7h5V9",
        "activity" => "M7 3h10v18H7zM10 7h4M10 11h4M10 15h4",
        "quotas" => "M5 21V13M12 21V7M19 21V3",
        "skills" => "m3 7 9-5 9 5-9 5-9-5m0 5 9 5 9-5m-18 5 9 5 9-5",
        "settings" => {
            "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2"
        }
        "refresh" => "M20 7v5h-5M4 17v-5h5M5 8a8 8 0 0 1 13-3l2 3M4 16l2 3a8 8 0 0 0 13-3",
        "moon" => "M20 15A9 9 0 0 1 9 4a9 9 0 1 0 11 11Z",
        "sun" => {
            "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1 1m12 12 1 1M5 19l1-1M18 6l1-1"
        }
        "clock" => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18M12 7v5l4 2",
        "storage" => "M4 6c0-4 16-4 16 0s-16 4-16 0v12c0 4 16 4 16 0V6",
        "close" => "m6 6 12 12M6 18 18 6",
        "arrow" => "M4 12h16m-6-6 6 6-6 6",
        _ => "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18M12 11v6M12 7v1",
    }
}
#[component]
pub fn Icon(#[prop(into)] name: String) -> impl IntoView {
    view! {
        <svg
            class="icon"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.7"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <path d=icon_path(&name) />
        </svg>
    }
}
#[component]
pub fn Button(
    ui: Ui,
    action: Action,
    text: &'static str,
    #[prop(default = "button")] class: &'static str,
    #[prop(default = false)] disabled: bool,
) -> impl IntoView {
    view! {
        <button type="button" class=class disabled=disabled on:click=click_action(ui, action)>
            {text}
        </button>
    }
}
#[component]
pub fn Dashboard(ui: Ui) -> impl IntoView {
    view! {
        <a class="skip-link" href="#main-content">
            {move || ui.text("Skip to content", "跳转到内容")}
        </a>
        <div class="app-shell">
            <aside class="sidebar">
                <div class="brand">
                    <span class="lens-mark" aria-hidden="true"></span>
                    <div>
                        <strong>"Usage Lens"</strong>
                        <small>{move || ui.text("Local workspace", "本地工作区")}</small>
                    </div>
                </div>
                <nav aria-label=move || {
                    ui.text("Main navigation", "主导航")
                }>
                    {PAGES
                        .into_iter()
                        .map(|page| {
                            view! {
                                <button
                                    aria-current=move || {
                                        ui.state.with(|s| (s.page == page).then_some("page"))
                                    }
                                    disabled=move || ui.state.with(State::busy)
                                    on:click=click_action(ui, Action::Navigate(page))
                                >
                                    <Icon name=page.key() />
                                    <span>{move || ui.state.with(|s| page.label(s.language))}</span>
                                </button>
                            }
                        })
                        .collect_view()}
                </nav>
                <div class="storage-note">
                    <span>
                        <Icon name="storage" />
                        "Local SQLite"
                    </span>
                    <p>
                        {move || {
                            ui.text(
                                "Records are saved on this device.",
                                "记录保存在此设备。",
                            )
                        }}
                    </p>
                </div>
            </aside>
            <div class="workspace">
                <header class="topbar">
                    <span>
                        {move || {
                            ui.text(
                                "Local evidence · Source scope is shown",
                                "本地证据 · 明确标注来源范围",
                            )
                        }}
                    </span>
                    <div class="utility-controls">
                        <label class="sr-only" for="language">
                            {move || ui.text("Language", "语言")}
                        </label>
                        <select
                            id="language"
                            prop:value=move || ui.state.with(|s| s.language.code())
                            on:change=event_bridge::value(ui, InputAction::Language)
                        >
                            <option value="en">"English"</option>
                            <option value="zh">"简体中文"</option>
                        </select>
                        <button
                            class="icon-button theme-button"
                            aria-label=move || {
                                ui.state
                                    .with(|s| {
                                        if s.dark {
                                            s.language.text("Use light theme", "使用浅色主题")
                                        } else {
                                            s.language.text("Use dark theme", "使用深色主题")
                                        }
                                    })
                            }
                            on:click=click_action(ui, Action::ToggleTheme)
                        >
                            {move || {
                                view! {
                                    <Icon name=if ui.state.with(|s| s.dark) {
                                        "sun"
                                    } else {
                                        "moon"
                                    } />
                                }
                            }}
                        </button>
                    </div>
                </header>
                <main id="main-content" tabindex="-1">
                    {move || {
                        ui.revision.get();
                        content(&ui.state.get_untracked(), ui)
                    }}
                </main>
            </div>
        </div>
        {move || {
            ui.modal_revision.get();
            modal(&ui.state.get_untracked(), ui)
        }}
    }
}
pub fn content(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let status = s.data(Slot::Status);
    let overview = s.data(Slot::Overview);
    let active = s.active_source();
    let refresh_error = [
        s.error(Slot::Status),
        s.error(Slot::Overview),
        s.error(Slot::Recent),
    ]
    .into_iter()
    .find(|v| !v.is_empty())
    .unwrap_or("")
    .to_owned();
    let has_refresh_error = !refresh_error.is_empty();
    let loading = s.loading(Slot::Status) || s.loading(Slot::Overview);
    let sources = rows(&status["sources"]);
    let warnings = s.warnings();
    let failures = s.failures();
    view! {
        <div class="page-heading">
            <div>
                <h1>
                    {if s.page == Page::Overview {
                        l.text("Your usage, in focus", "用量，一目了然")
                    } else {
                        s.page.label(l)
                    }}
                </h1>
                <p>
                    {l
                        .text(
                            "A clear view of what your local records can tell you.",
                            "清晰了解本地记录所能呈现的事实。",
                        )}
                </p>
            </div>
            <button
                class="button primary refresh-button"
                disabled=loading
                aria-label=l.text("Refresh", "刷新")
                on:click=click_action(ui, Action::Refresh)
            >
                <Icon name="refresh" />
                {if loading {
                    l.text("Refreshing…", "刷新中…")
                } else {
                    l.text("Refresh", "刷新")
                }}
            </button>
        </div>
        {(!sources.is_empty())
            .then(|| {
                view! {
                    <div class="source-selector">
                        <label for="source">{l.text("Source", "来源")}</label>
                        <select
                            id="source"
                            prop:value=s.source.clone()
                            disabled=s.busy()
                            on:change=event_bridge::value(ui, InputAction::Source)
                        >
                            {sources
                                .into_iter()
                                .map(|v| {
                                    view! {
                                        <option
                                            value=string(&v["id"]).to_owned()
                                            selected=string(&v["id"]) == s.source
                                        >
                                            {format!(
                                                "{} · {}",
                                                string(&v["displayName"]),
                                                string(&v["mode"]),
                                            )}
                                        </option>
                                    }
                                })
                                .collect_view()}
                        </select>
                        <span class="footnote">
                            {l.text("Account binding unverified", "账户归属未经验证")}
                        </span>
                    </div>
                }
            })}
        {(active["mode"] == "demo")
            .then(|| {
                view! {
                    <div class="notice demo-notice" role="note">
                        <Icon name="info" />
                        <strong>"演示数据 / Demo data"</strong>
                        <span>
                            {l
                                .text(
                                    "Synthetic records for preview. Not your account usage.",
                                    "用于预览的合成记录，不是您的账户用量。",
                                )}
                        </span>
                    </div>
                }
            })}
        {(status["settings"]["capturePaused"] == true)
            .then(|| {
                view! {
                    <div class="notice" role="status">
                        {l
                            .text(
                                "Capture is paused. Existing records remain available.",
                                "采集已暂停，现有记录仍可查看。",
                            )}
                    </div>
                }
            })}
        {(!refresh_error.is_empty())
            .then(|| {
                view! {
                    <div class="notice error" role="alert">
                        <strong>
                            {l.text("Could not refresh local data.", "无法刷新本地数据。")}
                        </strong>
                        {refresh_error}
                        {(!overview.is_null())
                            .then_some(
                                l
                                    .text(
                                        "Showing the previous response.",
                                        "正在显示上次响应。",
                                    ),
                            )}
                        <Button
                            ui
                            action=Action::Refresh
                            text=l.text("Retry", "重试")
                            class="text-button"
                        />
                    </div>
                }
            })}
        {(!failures.is_empty())
            .then(|| {
                view! {
                    <div class="notice" role="status">
                        {l
                            .text(
                                "Latest source read failed; a retained snapshot may be shown.",
                                "最新来源读取失败；可能正在显示保留快照。",
                            )}
                        {failures.join(" · ")}
                    </div>
                }
            })}
        {(loading && overview.is_null())
            .then(|| {
                view! {
                    <p class="loading-state" role="status">
                        {l.text("Reading local evidence…", "正在读取本地证据…")}
                    </p>
                }
            })}
        {if s.page == Page::Settings && !status.is_null() {
            settings(s, ui)
        } else if !overview.is_null() {
            match s.page {
                Page::Overview => overview_view(s, ui),
                Page::Activity => activity(s, ui),
                Page::Skills => skills(s, ui),
                Page::Quotas => quotas(s, ui),
                Page::Settings => ().into_any(),
            }
        } else if !loading && !has_refresh_error {
            view! {
                <section class="panel empty-state">
                    <Icon name="storage" />
                    <h2>{l.text("No local source configured", "尚未配置本地来源")}</h2>
                    <p>
                        {l
                            .text(
                                "Import a supported local record or configure the local collector to begin. The dashboard does not sign in or read your account automatically.",
                                "导入支持的本地记录或配置本地采集器即可开始。本面板不会自动登录或读取账户。",
                            )}
                    </p>
                </section>
            }
                .into_any()
        } else {
            ().into_any()
        }}
        {(!s.source.is_empty() && matches!(s.page, Page::Overview | Page::Settings))
            .then(|| collection_health(s, ui))}
        {(s.page == Page::Overview && !overview.is_null()).then(|| source_details(s))}
        {(!warnings.is_empty())
            .then(|| {
                view! {
                    <details class="source-warnings">
                        <summary>
                            {format!(
                                "{} ({})",
                                l.text("Source notes", "来源说明"),
                                warnings.len(),
                            )}
                        </summary>
                        <ul>
                            {warnings.into_iter().map(|w| view! { <li>{w}</li> }).collect_view()}
                        </ul>
                    </details>
                }
            })}
        <footer class="app-footer">
            {l
                .text(
                    "Local evidence, with its limits intact.",
                    "本地证据，清楚标注其边界。",
                )}<span>{l.text("Unknown ≠ zero", "未知 ≠ 零")}</span>
        </footer>
    }.into_any()
}
pub fn overview_view(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let o = s.data(Slot::Overview);
    let usage = &o["usage"];
    view! {
        <div class="metric-band">
            <div class="metric">
                <p>{l.text("Account recorded tokens", "账户已记录 Token")}</p>
                <strong>{count_cell(&usage["data"]["summary"]["lifetimeTokens"])}</strong>
                <span>
                    {l.text("Source-reported lifetime total", "来源报告的累计总量")}
                </span>
            </div>
            <div class="metric">
                <p>{l.text("Observed activity", "已观测活动")}</p>
                <strong>{integer(&o["events"]["total"])}</strong>
                <span>{l.text("Locally recorded events", "本地已记录事件")}</span>
            </div>
            <div class="metric">
                <p>{l.text("Data coverage", "数据覆盖")}</p>
                <strong>{l.text("Partial", "部分")}</strong>
                <span>{l.text("Completeness is unknown", "完整性未知")}</span>
            </div>
        </div>
        {(o["source"]["mode"] == "imported").then(|| imported(s, ui))}
        <div class="overview-grid">{chart(s, ui)}{quota_panel(&o["quota"], false, l)}</div>
        {activity_breakdown(s, ui)}
        <section class="panel">
            <div class="section-heading">
                <h2>{l.text("Recent activity", "最近活动")}</h2>
                <button
                    class="text-button"
                    on:click=click_action(ui, Action::Navigate(Page::Activity))
                >
                    {l.text("View all activity", "查看全部活动")}
                    <Icon name="arrow" />
                </button>
            </div>
            {evidence(
                &rows(&s.data(Slot::Recent)["events"]).into_iter().take(3).collect::<Vec<_>>(),
                l,
                ui,
            )}
        </section>
        <section class="provenance-band">
            <Icon name="clock" />
            <div>
                <strong>
                    {l.text("Freshness: recorded snapshots", "新鲜度：已记录快照")}
                </strong>
                {if usage["status"] == "available" {
                    view! {
                        <p>
                            {l.text("Collected", "采集于")}": "
                            <span class="mono">
                                {string(&usage["freshness"]["observedAt"]).to_owned()}
                            </span>
                            {(usage["freshness"]["stale"] == true)
                                .then(|| {
                                    view! {
                                        <span class="state-label warning">
                                            {l.text("Stale", "已过期")}
                                        </span>
                                    }
                                })}
                        </p>
                    }
                        .into_any()
                } else {
                    view! {
                        <p>{l.text("Usage snapshot not collected", "尚未采集用量快照")}</p>
                    }
                        .into_any()
                }}
            </div>
            <div>
                <strong>
                    {l.text("Cloud activity may be missing", "云端活动可能缺失")}
                </strong>
                <p>{string(&o["source"]["coverageDescription"]).to_owned()}</p>
            </div>
        </section>
    }
    .into_any()
}
pub fn collection_health(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let health = s.data(Slot::Health);
    let loading = s.loading(Slot::Health) || s.loading(Slot::Status);
    let error = s.error(Slot::Health).to_owned();
    view! {
        <section class="panel health-panel" aria-labelledby="collection-health-title" aria-busy=loading.to_string()>
            <div class="section-heading">
                <h2 id="collection-health-title">{l.text("Collection health & coverage", "采集健康与覆盖")}</h2>
                <Button ui action=Action::Load(Slot::Health, false)
                    text=l.text("Retry health", "重试健康检查") class="text-button" disabled=loading />
            </div>
            <p class="footnote">
                {l.text("Selected source · Stored records only. No live account access check. Unknown never means complete or zero usage.",
                    "所选来源 · 仅限已存储记录。未检查实时账户访问。未知不代表完整，也不代表零用量。")}
            </p>
            {loading.then(|| view! {
                <p class="loading-state" role="status">{l.text("Reading collection health…", "正在读取采集健康状态…")}</p>
            })}
            {(!error.is_empty()).then(|| view! {
                <div class="notice error" role="alert">
                    <strong>{l.text("Could not read collection health.", "无法读取采集健康状态。")}</strong>
                    {error}
                    {(!health.is_null()).then_some(l.text("Showing the previous health response; it may be out of date.", "正在显示上次健康响应；其状态可能已过时。"))}
                </div>
            })}
            {if health.is_null() {
                view! { <p class="health-empty">{l.text("Collection health is unknown until a response is available.", "收到响应之前，采集健康状态未知。")}</p> }.into_any()
            } else {
                view! {
                    <div class="health-summary">
                        <p><strong>{l.text("Capture setting · All sources", "采集设置 · 所有来源")}</strong>": "
                            {match health["settings"]["capturePaused"].as_bool() {
                                Some(true) => l.text("Paused", "已暂停"),
                                Some(false) => l.text("Not paused", "未暂停"),
                                None => l.text("Unknown", "未知"),
                            }}
                        </p>
                        <p>{l.text("Local health read at", "本地健康读取时间")}": "
                            <span class="mono">{health_timestamp(&health["checkedAt"], l)}</span>
                        </p>
                        <p class="footnote">{l.text("Not paused does not confirm that a collector is running. Pausing leaves existing records available.", "未暂停不代表采集器正在运行。暂停后，现有记录仍可查看。")}</p>
                    </div>
                    <p class="footnote">{l.text("Capture age threshold (milliseconds)", "采集时效阈值（毫秒）")}": "{display_value(&health["maxAgeMs"], l)}
                        " · "{l.text("Based on collection time, not source freshness", "基于采集时间，不代表来源数据时效")}
                    </p>
                    <div class="health-methods">
                        {rows(&health["observations"]).iter().map(|observation| health_method(observation, l)).collect_view()}
                    </div>
                    <h3>{l.text("Retained evidence counts", "保留证据数量")}</h3>
                    <div class="health-counts">
                        {[
                            ("events", l.text("Events", "事件"), "lastCapturedAt"),
                            ("skills", l.text("Skill evidence", "技能证据"), "lastCapturedAt"),
                            ("responseTokens", l.text("Response token records", "响应 Token 记录"), "lastCapturedAt"),
                            ("imports", l.text("Imports", "导入"), "lastImportedAt"),
                        ].into_iter().map(|(key, title, timestamp)| {
                            let record = &health["stored"][key];
                            view! {
                                <div class="health-count">
                                    <h4>{title}</h4>
                                    <strong>{health_count(&record["count"], l)}</strong>
                                    <p>{l.text("Latest recorded capture / import", "最近记录的采集 / 导入时间")}": "
                                        <span class="mono">{health_timestamp(&record[timestamp], l)}</span>
                                    </p>
                                    {(key != "imports").then(|| view! {
                                        <p>{l.text("Occurrence time unknown", "发生时间未知")}": "{health_count(&record["unknownOccurredAtCount"], l)}</p>
                                    })}
                                </div>
                            }
                        }).collect_view()}
                    </div>
                    <p class="footnote">{l.text("Counts describe retained records, not unique executions or total usage. Skill records can overlap events; these counts must not be added together. Zero records does not prove zero historical usage.", "数量仅描述保留记录，不代表唯一执行次数或总用量。技能记录可能与事件重叠；这些数量不可相加。零记录不能证明历史用量为零。")}</p>
                    <div class="health-gaps">
                        <strong>{l.text("Historical coverage remains partial", "历史覆盖仍不完整")}</strong>
                        <p>{l.text("Missing records: Unknown · History before collection: Unknown", "缺失记录：未知 · 采集之前的历史：未知")}</p>
                        <p class="footnote">{l.text("Recent capture timestamps describe when records were retained here, not whether the underlying source is current or complete.", "近期采集时间仅说明记录何时被保留，不能证明底层来源是最新或完整的。")}</p>
                    </div>
                    {(!rows(&health["warnings"]).is_empty()).then(|| view! {
                        <details class="source-warnings">
                            <summary>{l.text("Collection health notes", "采集健康说明")}</summary>
                            <ul>{rows(&health["warnings"]).iter().map(|warning| view! { <li>{string(warning).to_owned()}</li> }).collect_view()}</ul>
                        </details>
                    })}
                }.into_any()
            }}
        </section>
    }.into_any()
}
pub fn health_method(observation: &Value, l: Language) -> AnyView {
    let freshness = &observation["freshness"];
    let failure = &observation["lastFailure"];
    view! {
        <div class="health-method">
            <h3 class="mono">{string(&observation["method"]).to_owned()}</h3>
            <dl>
                <dt>{l.text("Stored snapshot", "已存储快照")}</dt>
                <dd>{health_state_text(&observation["availability"], l)}</dd>
                <dt>{l.text("Recorded capability", "已记录能力")}</dt>
                <dd>{health_state_text(&observation["capability"], l)}</dd>
                <dt>{l.text("Capture freshness", "采集时效")}</dt>
                <dd class=if ["stale", "future"].contains(&string(&freshness["state"])) { "health-warning" } else { "" }>
                    {health_state_text(&freshness["state"], l)}
                </dd>
                <dt>{l.text("Latest snapshot capture", "最近快照采集")}</dt>
                <dd class="mono">{health_timestamp(&freshness["observedAt"], l)}</dd>
                <dt>{l.text("Source as of", "来源数据截至")}</dt>
                <dd class="mono">{health_timestamp(&freshness["sourceAsOf"], l)}</dd>
            </dl>
            {(freshness["state"] == "future").then(|| view! {
                <p class="footnote">{l.text("Capture is ahead of the local clock; freshness cannot be treated as current.", "采集时间晚于本地时钟；不可将其视为最新状态。")}</p>
            })}
            {(!failure.is_null()).then(|| view! {
                <div class="health-failure">
                    <strong>{l.text("Last recorded error", "最近记录的错误")}</strong>
                    <p>{health_timestamp(&failure["errorCode"], l)}" · "{health_state_text(&failure["state"], l)}</p>
                    <p class="mono">{health_timestamp(&failure["attemptedAt"], l)}</p>
                    <p>{match failure["atOrAfterLatestObservation"].as_bool() {
                        Some(false) => l.text("Earlier than the retained snapshot", "早于保留的快照"),
                        Some(true) => l.text("At or after the retained snapshot, if any", "等于或晚于保留的快照（如有）"),
                        None => l.text("Relation to retained snapshot: Unknown", "与保留快照的先后关系：未知"),
                    }}</p>
                </div>
            })}
        </div>
    }.into_any()
}
pub fn chart(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let usage = &s.data(Slot::Overview)["usage"]["data"];
    let buckets = chart_buckets(usage, s.chart_range);
    let unknown = unknown_date_buckets(usage);
    let mut table = buckets.clone();
    table.extend(unknown.clone());
    view! {
        <section class="panel chart-panel">
            <div class="section-heading">
                <h2>{l.text("Token activity", "Token 活动")}</h2>
                <select
                    aria-label=l.text("Chart range", "图表范围")
                    prop:value=s.chart_range.to_string()
                    on:change=event_bridge::value(ui, InputAction::ChartRange)
                >
                    {[7, 30, 90]
                        .into_iter()
                        .map(|days| {
                            view! {
                                <option value=days.to_string() selected=days == s.chart_range>
                                    {if l == Language::Chinese {
                                        format!("最近 {days} 天")
                                    } else {
                                        format!("Latest {days} days")
                                    }}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
            </div>
            {if buckets.is_empty() {
                view! {
                    <div class="empty-state">
                        <h3>{l.text("Token activity unavailable", "暂无 Token 活动数据")}</h3>
                        <p>
                            {l
                                .text(
                                    "No daily buckets were reported by this source.",
                                    "该来源未提供每日统计。",
                                )}
                        </p>
                    </div>
                }
                    .into_any()
            } else {
                view! {
                    <div class="chart-scroll">
                        <div
                            class="chart"
                            role="img"
                            aria-label=l
                                .text(
                                    "Recorded daily token activity; exact values in the table below",
                                    "已记录每日 Token 活动；精确值见下方表格",
                                )
                        >
                            {buckets
                                .iter()
                                .map(|b| {
                                    let date = cell_text(&b["startDate"]);
                                    view! {
                                        <div class="chart-column">
                                            <div class="bar-slot">
                                                <div
                                                    class=if reported(&b["tokens"]).is_null() {
                                                        "bar unknown"
                                                    } else if reported(&b["tokens"]) == "0" {
                                                        "bar zero"
                                                    } else {
                                                        "bar"
                                                    }
                                                    style=format!(
                                                        "height:{}%",
                                                        bar_percent(&b["tokens"], &buckets),
                                                    )
                                                    title=format!("{}: {}", date, count_cell(&b["tokens"]))
                                                ></div>
                                            </div>
                                            <span>{date.get(5..10).unwrap_or(&date).to_owned()}</span>
                                        </div>
                                    }
                                })
                                .collect_view()}
                        </div>
                    </div>
                }
                    .into_any()
            }}
            <p class="footnote">
                {l
                    .text(
                        "Source-reported values. Missing records are not zero. Source time zone: unknown.",
                        "来源报告值。缺失记录不等于零。来源时区：未知。",
                    )}
            </p>
            {(!unknown.is_empty())
                .then(|| {
                    view! {
                        <p class="footnote">
                            {l
                                .text(
                                    "Some buckets have no valid calendar date and cannot be positioned on the chart.",
                                    "部分统计没有有效日历日期，无法在图表中定位。",
                                )}
                        </p>
                    }
                })}
            {(!table.is_empty())
                .then(|| {
                    view! {
                        <details class="data-details">
                            <summary>{l.text("Exact daily values", "每日精确值")}</summary>
                            <div class="table-scroll">
                                <table>
                                    <thead>
                                        <tr>
                                            <th>
                                                {l.text("Source calendar date", "来源日历日期")}
                                            </th>
                                            <th>"Tokens"</th>
                                            <th>{l.text("Status", "状态")}</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {table
                                            .into_iter()
                                            .map(|b| {
                                                view! {
                                                    <tr>
                                                        <td>{cell_text(&b["startDate"])}</td>
                                                        <td class="mono">{count_cell(&b["tokens"])}</td>
                                                        <td>{string(&b["tokens"]["status"]).to_owned()}</td>
                                                    </tr>
                                                }
                                            })
                                            .collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        </details>
                    }
                })}
        </section>
    }.into_any()
}
pub fn quota_panel(o: &Value, full: bool, l: Language) -> AnyView {
    let windows = quota_windows(o, full);
    let over = windows.iter().any(|(_, _, w)| {
        reported(&reported(w)["usedPercent"])
            .as_f64()
            .is_some_and(|v| v > 100.0)
    });
    view! {
        <section class="panel quota-panel">
            <div class="section-heading">
                <h2>{l.text("Quota windows", "额度窗口")}</h2>
                {(o["freshness"]["stale"] == true)
                    .then(|| {
                        view! {
                            <span class="state-label warning">{l.text("Stale", "已过期")}</span>
                        }
                    })}
            </div>
            {windows
                .is_empty()
                .then(|| {
                    view! {
                        <div class="empty-state compact">
                            <h3>{l.text("Quota unavailable", "额度数据不可用")}</h3>
                            <p>
                                {l
                                    .text(
                                        "This source has not reported a quota snapshot.",
                                        "该来源尚未报告额度快照。",
                                    )}
                            </p>
                        </div>
                    }
                })}
            {windows
                .into_iter()
                .map(|(name, kind, w)| {
                    let v = reported(&w);
                    let percent = reported(&v["usedPercent"]);
                    view! {
                        <div class="quota-window">
                            <div class="quota-label">
                                <strong>{name.clone()}</strong>
                                <span>
                                    {if kind == "primary" {
                                        l.text("Primary", "主窗口")
                                    } else {
                                        l.text("Secondary", "次窗口")
                                    }}
                                </span>
                            </div>
                            {if !v.is_null() {
                                view! {
                                    {(reported(&v["windowDurationMins"]).as_str() == Some("10080")).then(|| view! { <span class="state-label weekly-label">{l.text("Reported weekly window", "报告的每周窗口")}</span> })}
                                    <div class="quota-value">
                                        <span>
                                            {cell_text(&v["windowDurationMins"])}" "
                                            {l.text("minute window", "分钟窗口")}
                                        </span>
                                        <strong>
                                            {if percent.is_number() {
                                                format!("{}%", percent)
                                            } else {
                                                "—".into()
                                            }}<small>{l.text("used", "已用")}</small>
                                        </strong>
                                    </div>
                                    {if percent.is_number() {
                                        view! {
                                            <progress
                                                max="100"
                                                value=percent.to_string()
                                                aria-label=format!(
                                                    "{} {} {}",
                                                    name,
                                                    kind,
                                                    l.text("used percent", "已用百分比"),
                                                )
                                            ></progress>
                                        }
                                            .into_any()
                                    } else {
                                        view! {
                                            <p class="footnote">
                                                {l
                                                    .text(
                                                        "Used percentage not reported",
                                                        "未报告使用百分比",
                                                    )}
                                            </p>
                                        }
                                            .into_any()
                                    }}
                                    {full
                                        .then(|| {
                                            view! {
                                                <p class="footnote">
                                                    {l
                                                        .text(
                                                            "Resets at · Unix seconds",
                                                            "重置时间 · Unix 秒",
                                                        )}": "{cell_text(&v["resetsAt"])}
                                                </p>
                                            }
                                        })}
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <p class="footnote">
                                        {l.text("Window not reported", "未报告此窗口")}" · "
                                        {string(&w["status"]).to_owned()}
                                    </p>
                                }
                                    .into_any()
                            }}
                        </div>
                    }
                })
                .collect_view()}
            {(o["status"] == "available")
                .then(|| {
                    view! {
                        <p class="footnote">
                            {l.text("Collected", "采集于")}": "
                            <span class="mono">
                                {string(&o["freshness"]["observedAt"]).to_owned()}
                            </span>
                        </p>
                    }
                })}
            {over
                .then(|| {
                    view! {
                        <p class="footnote">
                            {l
                                .text(
                                    "Bars cap at 100%; reported percentages above 100% are preserved in text.",
                                    "进度条最高显示 100%；超过 100% 的来源报告值仍以文字保留。",
                                )}
                        </p>
                    }
                })}
            <p class="footnote quota-note">
                {l
                    .text(
                        "Windows are independent. Remaining tokens cannot be inferred from percentages.",
                        "各窗口独立，不能根据百分比推算剩余 Token。",
                    )}
            </p>
        </section>
    }.into_any()
}
pub fn evidence(events: &[Value], l: Language, ui: Ui) -> AnyView {
    if events.is_empty() {
        return view! {
            <div class="empty-state compact">
                <Icon name="activity" />
                <h3>{l.text("No recorded events", "暂无事件记录")}</h3>
                <p>
                    {l
                        .text(
                            "Missing records do not mean there was no activity.",
                            "没有记录不代表没有发生过活动。",
                        )}
                </p>
            </div>
        }
        .into_any();
    }
    view! {
        <div
            class="table-scroll"
            tabindex="0"
            aria-label=l.text("Recorded activity table", "已记录活动表格")
        >
            <table>
                <thead>
                    <tr>
                        <th>{l.text("Time · as recorded", "时间 · 原始记录")}</th>
                        <th>{l.text("Model", "模型")}</th>
                        <th>{l.text("Activity", "活动")}</th>
                        <th>{l.text("Evidence", "证据")}</th>
                        <th>
                            <span class="sr-only">{l.text("Details", "详情")}</span>
                        </th>
                    </tr>
                </thead>
                <tbody>
                    {events
                        .iter()
                        .map(|e| {
                            view! {
                                <tr>
                                    <td class="mono small">
                                        {string(
                                                e
                                                    .get("occurredAt")
                                                    .filter(|v| !v.is_null())
                                                    .unwrap_or(&e["observedAt"]),
                                            )
                                            .to_owned()}
                                        {e["occurredAt"]
                                            .is_null()
                                            .then(|| {
                                                view! {
                                                    <small>{l.text("Collection time", "采集时间")}</small>
                                                }
                                            })}
                                    </td>
                                    <td>{display_value(&e["model"], l)}</td>
                                    <td>{event_name(&e["eventType"], l)}</td>
                                    <td class="muted capitalize small">
                                        {label(&e["evidenceType"])}
                                        {(!e["skillEvidenceKind"].is_null())
                                            .then(|| {
                                                view! { <small>{label(&e["skillEvidenceKind"])}</small> }
                                            })}
                                    </td>
                                    <td>
                                        <button
                                            class="icon-button"
                                            aria-label=format!(
                                                "{} {}",
                                                l.text("View event", "查看事件"),
                                                string(&e["eventId"]),
                                            )
                                            on:click=click_action(ui, Action::Select(e.clone()))
                                        >
                                            <Icon name="activity" />
                                        </button>
                                    </td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }.into_any()
}
pub fn remote_notice(s: &State, slot: Slot, ui: Ui) -> AnyView {
    let l = s.language;
    let retry = s.retry_action(slot);
    view! {
        {(!s.error(slot).is_empty())
            .then(|| {
                view! {
                    <div class="notice error" role="alert">
                        {s.error(slot).to_owned()}
                        <Button
                            ui
                            action=retry
                            text=l.text("Retry", "重试")
                            class="text-button"
                        />
                    </div>
                }
            })}
        {s
            .loading(slot)
            .then(|| {
                view! {
                    <p role="status" class="muted">
                        {l.text("Loading local evidence…", "正在加载本地证据…")}
                    </p>
                }
            })}
    }
    .into_any()
}
pub fn more_button(s: &State, slot: Slot, ui: Ui, en: &'static str, zh: &'static str) -> AnyView {
    (!string(&s.data(slot)["nextCursor"]).is_empty())
        .then(|| {
            view! {
                <Button
                    ui
                    action=Action::Load(slot, true)
                    text=s.language.text(en, zh)
                    disabled=s.loading(slot)
                    class="button load-more"
                />
            }
        })
        .into_any()
}
pub fn activity(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    view! {
        <section class="panel">
            <div class="section-heading">
                <h2>{l.text("Recorded activity", "活动记录")}</h2>
                <span class="state-label">{l.text("Partial history", "部分历史")}</span>
            </div>
            <p class="muted">
                {l
                    .text(
                        "Explicit local records only. Token attribution per event is not provided.",
                        "仅显示本地明确记录。未提供事件级 Token 归因。",
                    )}
            </p>
            <form class="filters" on:submit=event_bridge::prevent(ui, Action::ApplyFilters)>
                <label class="content-query-filter">
                    {l.text("Search retained content", "搜索保留内容")}
                    <input type="search" name="query" maxlength="200" prop:value=s.filters.content_query.clone()
                        placeholder=l.text("Exact text · local content only", "精确文本 · 仅本地内容")
                        on:input=event_bridge::value(ui, InputAction::Filter("query")) />
                </label>
                <label>
                    {l.text("From", "开始日期")}
                    <input
                        type="date"
                        name="from"
                        prop:value=s.filters.from.clone()
                        on:input=event_bridge::value(ui, InputAction::Filter("from"))
                    />
                </label>
                <label>
                    {l.text("To", "结束日期")}
                    <input
                        type="date"
                        name="to"
                        prop:value=s.filters.to.clone()
                        on:input=event_bridge::value(ui, InputAction::Filter("to"))
                    />
                </label>
                <label>
                    <span id="activity-event-type-label">{l.text("Event type", "事件类型")}</span>
                    <select
                        aria-labelledby="activity-event-type-label"
                        name="eventType"
                        prop:value=s.filters.event_type.clone()
                        on:change=event_bridge::value(ui, InputAction::Filter("eventType"))
                    >
                        <option value="">{l.text("All event types", "所有事件")}</option>
                        {EVENT_TYPES
                            .into_iter()
                            .map(|e| {
                                view! {
                                    <option value=e selected=s.filters.event_type == e>
                                        {e.replace('_', " ")}
                                    </option>
                                }
                            })
                            .collect_view()}
                    </select>
                </label>
                <label>
                    {l.text("Model", "模型")}
                    <input
                        name="model"
                        prop:value=s.filters.model.clone()
                        placeholder=l.text("Exact model name", "精确模型名")
                        on:input=event_bridge::value(ui, InputAction::Filter("model"))
                    />
                </label>
                <button class="button primary" type="submit" disabled=s.loading(Slot::Activity)>
                    {l.text("Apply filters", "应用筛选")}
                </button>
                <Button ui action=Action::ResetFilters text=l.text("Reset", "重置") />
            </form>
            <p class="footnote">
                {l
                    .text(
                        "Filters use recorded timestamps. Source time zone is not inferred.",
                        "筛选依据已记录时间戳，不推测来源时区。",
                    )}
            </p>
            {search_scope(s)}
            {remote_notice(s, Slot::Activity, ui)}
            {(!s.data(Slot::Activity).is_null())
                .then(|| evidence(&rows(&s.data(Slot::Activity)["events"]), l, ui))}
            {more_button(s, Slot::Activity, ui, "Load more records", "加载更多记录")}
        </section>
    }
    .into_any()
}
const SKILL_TREND_COLUMNS: [(&str, &str, &str); 6] = [
    ("requested", "Requested", "请求"),
    ("loaded", "Loaded", "加载"),
    ("invoked", "Invoked", "调用"),
    ("mainRead", "Loaded · main read", "加载 · 主文件读取"),
    (
        "instructionInjection",
        "Loaded · instruction injection",
        "加载 · 指令注入",
    ),
    ("unknown", "Loaded · unknown subtype", "加载 · 未知子类型"),
];
fn skill_trend_count(value: &Value, key: &str) -> String {
    integer(value.get(key).unwrap_or(&value["loadedEvidence"][key]))
}
pub fn skill_trends(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    view! {
        <section class="panel skill-trends" aria-labelledby="skill-trends-title">
            <div class="section-heading">
                <h2 id="skill-trends-title">{l.text("Daily skill evidence", "每日技能证据")}</h2>
                <span class="state-label">{l.text("Partial retained history", "部分保留历史")}</span>
            </div>
            <p class="muted">
                {l.text("Separate evidence counts by UTC occurrence date. Missing days remain unknown; records are not successful or unique executions.", "按 UTC 发生日期分别统计证据。缺失日期仍为未知；记录不代表成功或唯一执行次数。")}
            </p>
            <form class="filters" on:submit=event_bridge::prevent(ui, Action::ApplyFilters)>
                <label>
                    {l.text("From (UTC)", "开始日期（UTC）")}
                    <input type="date" name="from" prop:value=s.filters.from.clone()
                        on:input=event_bridge::value(ui, InputAction::Filter("from")) />
                </label>
                <label>
                    {l.text("To (UTC)", "结束日期（UTC）")}
                    <input type="date" name="to" prop:value=s.filters.to.clone()
                        on:input=event_bridge::value(ui, InputAction::Filter("to")) />
                </label>
                <label class="skill-name-filter">
                    {l.text("Exact skill name (optional)", "精确技能名（可选）")}
                    <input name="skillName" prop:value=s.filters.skill_name.clone()
                        placeholder=l.text("All skill names", "所有技能名")
                        on:input=event_bridge::value(ui, InputAction::Filter("skillName")) />
                </label>
                <button class="button primary" type="submit" disabled=s.source.is_empty()>
                    {l.text("Apply filters", "应用筛选")}
                </button>
                <Button ui action=Action::ResetFilters text=l.text("Reset", "重置") />
            </form>
            <p class="footnote">
                {l.text("Choose both dates, up to 366 days inclusive. Dates are shared with Activity; its model and event-type filters do not apply here. Import time is never used.", "请同时选择起止日期，含首尾最多 366 天。日期与活动页共用；模型及事件类型筛选不适用于此处。不会使用导入时间。")}
            </p>
            {move || ui.state.with(|state| skill_trend_result(state, ui))}
        </section>
    }.into_any()
}
pub fn skill_trend_result(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let slot = Slot::SkillSummary;
    let result = s.data(slot);
    if !s.error(slot).is_empty() {
        return view! {
            <div class="notice error" role="alert">
                <span>{if s.error(slot) == "invalid_skill_date_range" {
                    l.text("Choose a valid UTC date pair, no more than 366 days inclusive.", "请选择有效的 UTC 起止日期，含首尾不超过 366 天。")
                } else {
                    l.text("Could not load daily evidence.", "无法加载每日证据。")
                }}</span>
                <small>{s.error(slot).to_owned()}</small>
                <Button ui action=Action::Load(slot, false) text=l.text("Retry", "重试") class="text-button" />
            </div>
        }.into_any();
    }
    view! {
        <div class="skill-trend-result" aria-busy=s.loading(slot).to_string()>
            {remote_notice(s, slot, ui)}
            {if result.is_null() {
                (!s.loading(slot)).then(|| view! {
                    <p class="muted">{l.text("Apply a date range to read daily evidence.", "应用日期范围以查看每日证据。")}</p>
                }).into_any()
            } else {
                let daily = rows(&result["daily"]);
                view! {
                    <p class="skill-trend-scope">
                        <strong>{l.text("Selected range (UTC): ", "所选范围（UTC）：")}</strong>
                        {string(&result["fromDate"]).to_owned()}" – "{string(&result["toDate"]).to_owned()}
                        <span>{if result["skillName"].is_null() {
                            l.text("All skill names", "所有技能名").to_owned()
                        } else { string(&result["skillName"]).to_owned() }}</span>
                    </p>
                    <h3 class="skill-trend-label">{l.text("Evidence records in selected range", "所选范围内的证据记录")}</h3>
                    <dl class="response-totals skill-trend-totals">
                        {SKILL_TREND_COLUMNS.into_iter().map(|(key, en, zh)| view! {
                            <div><dt>{l.text(en, zh)}</dt><dd>{skill_trend_count(&result["totals"], key)}</dd></div>
                        }).collect_view()}
                    </dl>
                    {if daily.is_empty() {
                        view! {
                            <div class="empty-state compact">
                                <h3>{l.text("No dated evidence in this range", "此范围内没有带日期的证据")}</h3>
                                <p>{l.text("No matching retained records does not prove no skill usage.", "没有匹配的保留记录，不代表没有使用技能。")}</p>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="table-scroll" tabindex="0" role="region" aria-label=l.text("Daily skill evidence table", "每日技能证据表")>
                                <table class="skill-trend-table">
                                    <caption>{l.text("Recorded days only · UTC; omitted days are unknown", "仅显示有记录的 UTC 日期；未列出的日期为未知")}</caption>
                                    <thead><tr><th scope="col">{l.text("Date (UTC)", "日期（UTC）")}</th>
                                        {SKILL_TREND_COLUMNS.into_iter().map(|(_, en, zh)| view! { <th scope="col">{l.text(en, zh)}</th> }).collect_view()}
                                    </tr></thead>
                                    <tbody>{daily.into_iter().map(|day| view! {
                                        <tr><th scope="row">{string(&day["date"]).to_owned()}</th>
                                            {SKILL_TREND_COLUMNS.into_iter().map(|(key, _, _)| view! { <td>{skill_trend_count(&day, key)}</td> }).collect_view()}
                                        </tr>
                                    }).collect_view()}</tbody>
                                </table>
                            </div>
                        }.into_any()
                    }}
                    <p class="footnote skill-unknown-time">
                        <strong>{l.text("Unknown occurrence time: ", "发生时间未知：")}{integer(&result["unknownOccurredAtCount"])}</strong>
                        " "{l.text("Across all retained records for this source and exact skill filter, outside the dated totals. These records cannot be assigned to a day.", "范围为此来源及精确技能筛选下的全部保留记录，不计入日期范围合计。这些记录无法归入某一天。")}
                    </p>
                    {(result["skillsTruncated"] == true).then(|| view! {
                        <p class="footnote">{l.text("The skill-name summary is limited to 500 entries; daily totals include all matching evidence.", "技能名汇总最多列出 500 项；每日合计包含全部匹配证据。")}</p>
                    })}
                    {rows(&result["warnings"]).into_iter().map(|warning| view! {
                        <p class="footnote">{string(&warning).to_owned()}</p>
                    }).collect_view()}
                }.into_any()
            }}
        </div>
    }.into_any()
}
pub fn skills(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let counts = rows(&s.data(Slot::Overview)["events"]["skills"]);
    let result = s.data(Slot::Skills);
    view! {
        {skill_trends(s, ui)}
        <section class="panel">
            <div class="section-heading">
                <h2>{l.text("Skill evidence", "技能证据")}</h2>
                <span class="state-label">{l.text("Direct evidence only", "仅直接证据")}</span>
            </div>
            <p class="muted">
                {l
                    .text(
                        "Requested, loaded and invoked are different states. Evidence records can overlap in forked history and are not unique skill executions.",
                        "请求、加载与调用是不同状态。分叉历史中的证据记录可能重叠，不代表唯一技能执行次数。",
                    )}
            </p>
            <p class="footnote">{l.text("All retained evidence below; daily trend filters do not change these records.", "下方为全部保留证据；每日趋势筛选不会改变这些记录。")}</p>
            <div class="segmented" aria-label=l.text("Skill evidence state", "技能证据状态")>
                {[
                    ("requested", "Requested", "请求"),
                    ("loaded", "Loaded", "加载"),
                    ("invoked", "Invoked", "调用"),
                ]
                    .into_iter()
                    .map(|(kind, en, zh)| {
                        view! {
                            <button
                                aria-pressed=(s.kind == kind).to_string()
                                on:click=click_action(ui, Action::SkillKind(kind.into()))
                            >
                                {l.text(en, zh)}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
            <dl class="skill-counts">
                {counts
                    .into_iter()
                    .filter(|v| string(&v["kind"]) == s.kind)
                    .map(|v| {
                        view! {
                            <div>
                                <dt>
                                    {if v["name"].is_null() {
                                        l.text("Unknown skill", "未知技能").to_owned()
                                    } else {
                                        string(&v["name"]).into()
                                    }}
                                    <small>
                                        {if v["evidenceKind"].is_null() {
                                            l.text("Evidence subtype unknown", "证据子类型未知")
                                                .into()
                                        } else {
                                            label(&v["evidenceKind"])
                                        }}
                                    </small>
                                </dt>
                                <dd>
                                    {integer(&v["count"])}
                                    <small>{l.text("evidence records", "条证据记录")}</small>
                                </dd>
                            </div>
                        }
                    })
                    .collect_view()}
            </dl>
            {remote_notice(s, Slot::Skills, ui)}
            {if result["status"] == "unavailable" {
                view! {
                    <div class="empty-state">
                        <h3>
                            {l.text("No direct evidence collected", "尚未采集直接证据")}
                        </h3>
                        <p>
                            {l
                                .text(
                                    "Usage is unknown, not zero. A skill mentioned in text is not an invocation.",
                                    "使用情况未知，不代表零。文本提及技能不等于调用。",
                                )}
                        </p>
                    </div>
                }
                    .into_any()
            } else if !result.is_null() {
                evidence(&rows(&result["events"]), l, ui)
            } else {
                ().into_any()
            }}
        </section>
    }.into_any()
}
pub fn quotas(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let history = s.data(Slot::History);
    let observations = rows(&history["observations"]);
    view! {
        {quota_panel(&s.data(Slot::Overview)["quota"], true, l)}
        {crate::token_period::token_period_panel(s, ui)}
        <section class="panel">
            <div class="section-heading">
                <h2>{l.text("Snapshot history", "快照历史")}</h2>
                <Button
                    ui
                    action=Action::Load(Slot::History, false)
                    text=l.text("Refresh history", "刷新历史")
                    class="text-button"
                    disabled=s.loading(Slot::History)
                />
            </div>
            <p class="muted">
                {l
                    .text(
                        "Only recorded snapshots are shown. Quota changes between samples are unknown.",
                        "仅显示已记录快照。采样之间的额度变化未知。",
                    )}
            </p>
            {remote_notice(s, Slot::History, ui)}
            {(!history.is_null() && observations.is_empty())
                .then(|| {
                    view! {
                        <p class="empty-state">
                            {l.text("No quota snapshots recorded", "暂无额度快照")}
                        </p>
                    }
                })}
            {observations
                .into_iter()
                .map(|o| {
                    view! {
                        <details class="history-row">
                            <summary>
                                <span class="mono">
                                    {string(&o["freshness"]["observedAt"]).to_owned()}
                                </span>
                                <span>{l.text("Collection time", "采集时间")}</span>
                            </summary>
                            {quota_panel(&o, true, l)}
                        </details>
                    }
                })
                .collect_view()}
            {more_button(s, Slot::History, ui, "Load older snapshots", "加载更早快照")}
        </section>
    }.into_any()
}
pub fn token_name(key: &str, l: Language) -> &'static str {
    match key {
        "inputTokens" => l.text("Input tokens", "输入 Token"),
        "cachedInputTokens" => l.text("Cached input · subset", "缓存输入 · 子集"),
        "cacheWriteInputTokens" => l.text("Cache-write input · reported", "缓存写入输入 · 报告值"),
        "outputTokens" => l.text("Output tokens", "输出 Token"),
        "reasoningOutputTokens" => l.text("Reasoning output · subset", "推理输出 · 子集"),
        _ => l.text("Source-reported total", "来源报告总量"),
    }
}
pub fn imported(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let summary = s.data(Slot::ResponseUsage);
    let records = s.data(Slot::ResponseRecords);
    view! {
        <section class="panel imported-tokens">
            <div class="section-heading">
                <h2>{l.text("Imported response tokens", "导入的响应 Token")}</h2>
                <span class="state-label">
                    {l.text("Separate local evidence", "独立本地证据")}
                </span>
            </div>
            <p class="muted">
                {l
                    .text(
                        "Completed-response counters from this local source. These are not added to account totals and cannot determine quota or cost.",
                        "来自此本地来源的已完成响应计数。不会与账户总量相加，也不能据此推算额度或费用。",
                    )}
            </p>
            {remote_notice(s, Slot::ResponseUsage, ui)}
            {(!summary.is_null())
                .then(|| {
                    view! {
                        <p class="response-count">
                            <strong>{integer(&summary["responseCount"])}</strong>
                            {l.text("completed-response records", "条已完成响应记录")}
                        </p>
                        {if summary["responseCount"] == "0" {
                            view! {
                                <p class="footnote">
                                    {l
                                        .text(
                                            "No completed-response token evidence was imported. Actual historical usage is unknown.",
                                            "尚未导入已完成响应的 Token 证据。实际历史用量未知。",
                                        )}
                                </p>
                            }
                                .into_any()
                        } else {
                            view! {
                                <dl class="response-totals">
                                    {TOKEN_KEYS
                                        .into_iter()
                                        .map(|k| {
                                            view! {
                                                <div>
                                                    <dt>{token_name(k, l)}</dt>
                                                    <dd class="mono">{integer(&summary["totals"][k])}</dd>
                                                </div>
                                            }
                                        })
                                        .collect_view()}
                                </dl>
                            }
                                .into_any()
                        }}
                        <p class="footnote">
                            {l
                                .text(
                                    "Cached input is a subset of input; reasoning output is a subset of output. Source totals are preserved, not recomputed. Missing responses and overlap in forked histories are unknown.",
                                    "缓存输入是输入的子集，推理输出是输出的子集。总量保留来源原值，不重新计算。缺失响应及分叉历史的重叠情况未知。",
                                )}
                        </p>
                        {(!rows(&summary["byModel"]).is_empty())
                            .then(|| {
                                view! {
                                    <details class="data-details">
                                        <summary>
                                            {l
                                                .text(
                                                    "Response counters by reported model",
                                                    "按报告模型统计响应",
                                                )}
                                        </summary>
                                        <div class="table-scroll">
                                            <table>
                                                <thead>
                                                    <tr>
                                                        <th>{l.text("Model", "模型")}</th>
                                                        <th>{l.text("Responses", "响应")}</th>
                                                        <th>{l.text("Input", "输入")}</th>
                                                        <th>{l.text("Output", "输出")}</th>
                                                        <th>{l.text("Reported total", "报告总量")}</th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {rows(&summary["byModel"])
                                                        .into_iter()
                                                        .map(|r| {
                                                            view! {
                                                                <tr>
                                                                    <td>{display_value(&r["model"], l)}</td>
                                                                    <td>{integer(&r["responseCount"])}</td>
                                                                    <td>{integer(&r["totals"]["inputTokens"])}</td>
                                                                    <td>{integer(&r["totals"]["outputTokens"])}</td>
                                                                    <td>{integer(&r["totals"]["totalTokens"])}</td>
                                                                </tr>
                                                            }
                                                        })
                                                        .collect_view()}
                                                </tbody>
                                            </table>
                                        </div>
                                    </details>
                                }
                            })}
                        {rows(&summary["warnings"])
                            .into_iter()
                            .map(|w| view! { <p class="footnote">{string(&w).to_owned()}</p> })
                            .collect_view()}
                        {records
                            .is_null()
                            .then(|| {
                                view! {
                                    <Button
                                        ui
                                        action=Action::Load(Slot::ResponseRecords, false)
                                        text=l
                                            .text("Inspect response records", "查看响应记录")
                                        class="button response-record-button"
                                        disabled=s.loading(Slot::ResponseRecords)
                                    />
                                }
                            })}
                    }
                })}
            {remote_notice(s, Slot::ResponseRecords, ui)}
            {(!records.is_null())
                .then(|| {
                    view! {
                        <div class="response-records">
                            {rows(&records["records"])
                                .is_empty()
                                .then(|| {
                                    view! {
                                        <p class="footnote">
                                            {l
                                                .text(
                                                    "No response records in this selection",
                                                    "当前范围没有响应记录",
                                                )}
                                        </p>
                                    }
                                })}
                            {rows(&records["records"])
                                .into_iter()
                                .map(|r| {
                                    view! {
                                        <details class="history-row">
                                            <summary>
                                                <span class="mono">
                                                    {string(
                                                            r
                                                                .get("occurredAt")
                                                                .filter(|v| !v.is_null())
                                                                .unwrap_or(&r["importedAt"]),
                                                        )
                                                        .to_owned()}
                                                </span>
                                                <span>
                                                    {if r["occurredAt"].is_null() {
                                                        l.text("Import time", "导入时间")
                                                    } else {
                                                        l.text("Reported event time", "报告的事件时间")
                                                    }}
                                                </span>
                                                <strong>
                                                    {integer(&r["usage"]["totalTokens"])}" tokens"
                                                </strong>
                                            </summary>
                                            <dl class="detail-list">
                                                {[
                                                    "responseId",
                                                    "threadId",
                                                    "sessionId",
                                                    "turnId",
                                                    "rootTurnId",
                                                ]
                                                    .into_iter()
                                                    .map(|k| {
                                                        view! {
                                                            <dt>{k}</dt>
                                                            <dd class="mono">{string(&r[k]).to_owned()}</dd>
                                                        }
                                                    })
                                                    .collect_view()}<dt>{l.text("Model", "模型")}</dt>
                                                <dd>{display_value(&r["model"], l)}</dd>
                                                {TOKEN_KEYS
                                                    .into_iter()
                                                    .map(|k| {
                                                        view! {
                                                            <dt>{token_name(k, l)}</dt>
                                                            <dd class="mono">{integer(&r["usage"][k])}</dd>
                                                        }
                                                    })
                                                    .collect_view()}<dt>{l.text("Evidence", "证据")}</dt>
                                                <dd>{string(&r["provenance"]["semantics"]).to_owned()}</dd>
                                                <dt>{l.text("Imported at", "导入于")}</dt>
                                                <dd class="mono">{string(&r["importedAt"]).to_owned()}</dd>
                                            </dl>
                                        </details>
                                    }
                                })
                                .collect_view()}
                            {more_button(
                                s,
                                Slot::ResponseRecords,
                                ui,
                                "Load more responses",
                                "加载更多响应",
                            )}
                        </div>
                    }
                })}
        </section>
    }.into_any()
}
pub fn source_details(s: &State) -> AnyView {
    let l = s.language;
    let o = s.data(Slot::Overview);
    let caps = rows(&s.data(Slot::Status)["capabilities"])
        .into_iter()
        .filter(|v| string(&v["sourceId"]) == s.source)
        .collect::<Vec<_>>();
    view! {
        <details class="panel evidence-details">
            <summary>
                {l.text("Source details & recorded breakdowns", "来源详情与已记录明细")}
            </summary>
            <div class="detail-sections">
                <section>
                    <h3>{l.text("Reported account summary", "来源报告的账户概要")}</h3>
                    <p class="footnote">
                        {l
                            .text(
                                "Account namespace is unverified. Cross-surface coverage is unknown.",
                                "账户命名空间未经验证，跨端覆盖情况未知。",
                            )}
                    </p>
                    <dl class="detail-list">
                        <dt>{l.text("Account type", "账户类型")}</dt>
                        <dd>{cell_text(&o["account"]["data"]["type"])}</dd>
                        <dt>{l.text("Plan", "套餐")}</dt>
                        <dd>{cell_text(&o["account"]["data"]["planType"])}</dd>
                        {[
                            ("peakDailyTokens", "Peak daily tokens", "每日 Token 峰值"),
                            (
                                "longestRunningTurnSec",
                                "Longest turn · seconds",
                                "最长回合 · 秒",
                            ),
                            (
                                "currentStreakDays",
                                "Current streak · days",
                                "当前连续活跃 · 天",
                            ),
                            (
                                "longestStreakDays",
                                "Longest streak · days",
                                "最长连续活跃 · 天",
                            ),
                        ]
                            .into_iter()
                            .map(|(k, en, zh)| {
                                view! {
                                    <dt>{l.text(en, zh)}</dt>
                                    <dd>{count_cell(&o["usage"]["data"]["summary"][k])}</dd>
                                }
                            })
                            .collect_view()}
                    </dl>
                </section>
                <section>
                    <h3>{l.text("Recorded events by model", "按模型记录的事件")}</h3>
                    <p class="footnote">
                        {l
                            .text(
                                "Event counts, not token attribution.",
                                "事件数量，不代表 Token 归因。",
                            )}
                    </p>
                    <dl class="detail-list">
                        {rows(&o["events"]["byModel"])
                            .into_iter()
                            .map(|r| {
                                view! {
                                    <dt>
                                        {if r["name"].is_null() {
                                            l.text("Unknown model", "未知模型").into()
                                        } else {
                                            string(&r["name"]).to_owned()
                                        }}
                                    </dt>
                                    <dd>{integer(&r["count"])}</dd>
                                }
                            })
                            .collect_view()}
                    </dl>
                    <p class="footnote">
                        {l.text("Unknown model events", "模型未知的事件")}": "
                        {integer(&o["events"]["unknownModelCount"])}
                    </p>
                    <h3>{l.text("Recorded tool calls", "已记录工具调用")}</h3>
                    <dl class="detail-list">
                        {rows(&o["events"]["tools"])
                            .into_iter()
                            .map(|r| {
                                view! {
                                    <dt>
                                        {if r["name"].is_null() {
                                            l.text("Unknown tool", "未知工具").into()
                                        } else {
                                            string(&r["name"]).to_owned()
                                        }}
                                    </dt>
                                    <dd>{integer(&r["count"])}</dd>
                                }
                            })
                            .collect_view()}
                    </dl>
                </section>
            </div>
            <div class="table-scroll">
                <table>
                    <thead>
                        <tr>
                            <th>{l.text("Method", "方法")}</th>
                            <th>{l.text("Collected at", "采集于")}</th>
                            <th>{l.text("Source as of", "来源截至时间")}</th>
                            <th>{l.text("Adapter", "适配器")}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {["account", "usage", "quota"]
                            .into_iter()
                            .map(|k| {
                                let v = &o[k];
                                if v["status"] == "available" {
                                    view! {
                                        <tr>
                                            <td class="mono">
                                                {string(&v["provenance"]["method"]).to_owned()}
                                            </td>
                                            <td class="mono">
                                                {string(&v["freshness"]["observedAt"]).to_owned()}
                                            </td>
                                            <td class="mono">
                                                {if v["freshness"]["sourceAsOf"].is_null() {
                                                    l.text("Not provided", "未提供").into()
                                                } else {
                                                    string(&v["freshness"]["sourceAsOf"]).to_owned()
                                                }}
                                            </td>
                                            <td class="mono">
                                                {string(&v["provenance"]["adapterVersion"]).to_owned()}
                                            </td>
                                        </tr>
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <tr>
                                            <td>{k}</td>
                                            <td colspan="3">
                                                {l.text("Unavailable", "不可用")}" · "
                                                {string(
                                                        v["lastFailure"].get("errorCode").unwrap_or(&v["reason"]),
                                                    )
                                                    .to_owned()}
                                            </td>
                                        </tr>
                                    }
                                        .into_any()
                                }
                            })
                            .collect_view()}
                    </tbody>
                </table>
            </div>
            <h3 class="capability-heading">{l.text("Source capabilities", "来源能力")}</h3>
            {if caps.is_empty() {
                view! {
                    <p class="footnote">
                        {l.text("Capability information unavailable", "能力信息不可用")}
                    </p>
                }
                    .into_any()
            } else {
                view! {
                    <ul class="capability-list">
                        {caps
                            .into_iter()
                            .map(|c| {
                                view! {
                                    <li>
                                        <span class="mono">{string(&c["method"]).to_owned()}</span>
                                        <span class="state-label">
                                            {string(&c["state"]).to_owned()}
                                        </span>
                                        <span>{string(&c["reason"]).to_owned()}</span>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ul>
                }
                    .into_any()
            }}
        </details>
    }.into_any()
}
pub fn settings(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    view! {
        <section class="panel settings-panel">
            <h2>{l.text("Collection & privacy", "采集与隐私")}</h2>
            <p class="muted">
                {l
                    .text(
                        "These controls apply to the shared local collector and database.",
                        "这些控制项适用于共享本地采集器和数据库。",
                    )}
            </p>
            <form on:submit=event_bridge::prevent(ui, Action::SaveSettings)>
                <label class="setting-row">
                    <span>
                        <strong>{l.text("Pause capture", "暂停采集")}</strong>
                        <small>
                            {l
                                .text(
                                    "Keep existing records while pausing all new capture and imports.",
                                    "保留现有记录，暂停所有新采集和导入。",
                                )}
                        </small>
                    </span>
                    <input
                        type="checkbox"
                        role="switch"
                        prop:checked=s.settings["capturePaused"] == true
                        on:change=event_bridge::checked(ui, "capturePaused")
                    />
                </label>
                <label class="setting-row">
                    <span>
                        <strong>{l.text("Retain local content", "保留本地内容")}</strong>
                        <small>
                            {l
                                .text(
                                    "Store permitted message bodies and tool details locally. Off by default.",
                                    "在本地保留获准的消息正文和工具详情。默认关闭。",
                                )}
                        </small>
                    </span>
                    <input
                        type="checkbox"
                        role="switch"
                        prop:checked=s.settings["contentCaptureEnabled"] == true
                        on:change=event_bridge::checked(ui, "contentCaptureEnabled")
                    />
                </label>
                <p class="notice">
                    {l
                        .text(
                            "Redaction is best-effort, not a guarantee. Do not capture authentication secrets. Local content is excluded from plugin responses.",
                            "脱敏尽力而为，不能保证完备。请勿采集认证密钥。本地内容不会包含在插件响应中。",
                        )}
                </p>
                <label class="setting-row">
                    <span>
                        <strong>{l.text("Retention period", "保留期限")}</strong>
                        <small>
                            {l
                                .text(
                                    "Days to keep. Apply below to prune existing records.",
                                    "保留天数。在下方应用以清理现有记录。",
                                )}
                        </small>
                    </span>
                    <input
                        name="retentionDays"
                        type="number"
                        min="1"
                        max="3650"
                        required
                        prop:value=s.settings["retentionDays"].to_string()
                        on:input=event_bridge::value(ui, InputAction::RetentionDays)
                    />
                </label>
                {(!s.error(Slot::Mutation).is_empty() && s.action.is_none())
                    .then(|| {
                        view! {
                            <div class="notice error" role="alert">
                                {s.error(Slot::Mutation).to_owned()}
                            </div>
                        }
                    })}
                {s
                    .success
                    .then(|| {
                        view! {
                            <p class="success" role="status">
                                {l.text("Local data updated", "本地数据已更新")}
                            </p>
                        }
                    })}
                <button class="button primary" type="submit" disabled=s.busy()>
                    {if s.busy() {
                        l.text("Saving…", "保存中…")
                    } else {
                        l.text("Save settings", "保存设置")
                    }}
                </button>
            </form>
        </section>
        <section class="panel settings-panel">
            <h2>{l.text("Manage local records", "管理本地记录")}</h2>
            <p class="muted">
                {l
                    .text(
                        "Destructive actions require a separate typed confirmation. Deleting content keeps event metadata.",
                        "破坏性操作需要单独输入确认。删除内容会保留事件元数据。",
                    )}
            </p>
            <div class="manage-actions">
                <Button
                    ui
                    action=Action::Confirm(Destructive::Retention)
                    text=l
                        .text("Apply retention to all sources", "对所有来源应用保留规则")
                    disabled=s.busy()
                />
                <Button
                    ui
                    action=Action::Confirm(Destructive::Content)
                    text=l.text("Delete local content", "删除本地内容")
                    class="button danger-outline"
                    disabled=s.source.is_empty() || s.busy()
                />
                <Button
                    ui
                    action=Action::Confirm(Destructive::All)
                    text=l.text("Delete source records", "删除来源记录")
                    class="button danger-outline"
                    disabled=s.source.is_empty() || s.busy()
                />
            </div>
        </section>
    }.into_any()
}
pub fn modal(s: &State, ui: Ui) -> AnyView {
    if let Some(action) = s.action {
        confirmation(s, action, ui)
    } else if !s.selected.is_null() {
        event_detail(s, ui)
    } else {
        ().into_any()
    }
}
pub fn confirmation(s: &State, action: Destructive, ui: Ui) -> AnyView {
    let l = s.language;
    let source = s.active_source();
    view! {
        <dialog
            class="confirm-dialog"
            data-modal="confirmation"
            aria-modal="true"
            aria-labelledby="confirm-title"
            on:cancel=event_bridge::cancel(ui)
        >
            <h2 id="confirm-title">
                {match action {
                    Destructive::Retention => {
                        l.text(
                            "Apply retention to all sources?",
                            "对所有来源应用保留规则？",
                        )
                    }
                    Destructive::Content => {
                        l.text("Delete retained content?", "删除已保留内容？")
                    }
                    Destructive::All => {
                        l.text("Delete this source’s records?", "删除此来源记录？")
                    }
                }}
            </h2>
            {if action == Destructive::Retention {
                view! {
                    <p class="destructive-scope">
                        {l
                            .text(
                                "Scope: ALL sources in the local database.",
                                "范围：本地数据库中的所有来源。",
                            )}" "{l.text("Saved retention setting:", "已保存的保留期限：")}
                        " "{s.data(Slot::Status)["settings"]["retentionDays"].to_string()}" "
                        {l
                            .text(
                                "days. This removes older recorded observations, events, attempts, import-deduplication history and response-token records using collection/import timestamps, not the time an activity occurred.",
                                "天。依据采集／导入时间删除过期快照、事件、尝试、导入去重历史与响应 Token 记录，而不是依据活动发生时间。",
                            )}
                    </p>
                }
                    .into_any()
            } else {
                view! {
                    <p class="destructive-scope">
                        {l.text("Selected source:", "选定来源：")}
                        <strong>
                            {format!("{} ({})", string(&source["displayName"]), s.source)}
                        </strong><br />
                        {if action == Destructive::Content {
                            l.text(
                                "Removes retained message bodies, tool arguments/results and file contents. Event metadata remains.",
                                "删除保留的消息正文、工具参数／结果与文件内容，保留事件元数据。",
                            )
                        } else {
                            l.text(
                                "Removes recorded observations, events, attempts, retained content, import-deduplication history and response-token records for this source. Source configuration and settings remain.",
                                "删除此来源的快照、事件、尝试、保留内容、导入去重历史与响应 Token 记录，保留来源配置与设置。",
                            )
                        }}
                    </p>
                }
                    .into_any()
            }}
            <p>
                {l
                    .text(
                        "This permanently removes matching data from the local database. This cannot be undone from this app.",
                        "此操作会永久删除本地数据库中的匹配数据，无法在本应用内撤销。",
                    )}
            </p>
            <form on:submit=event_bridge::prevent(ui, Action::Perform)>
                <label>
                    {l.text("Type to confirm", "输入以下内容确认")}": "
                    <strong>{action.required()}</strong>
                    <input
                        name="confirmation"
                        autocomplete="off"
                        prop:value=s.confirmation.clone()
                        disabled=s.busy()
                        on:input=event_bridge::value(ui, InputAction::Confirmation)
                    />
                </label>
                {(!s.error(Slot::Mutation).is_empty())
                    .then(|| {
                        view! {
                            <p class="notice error" role="alert">
                                {s.error(Slot::Mutation).to_owned()}
                            </p>
                        }
                    })}
                <div class="dialog-actions">
                    <Button
                        ui
                        action=Action::Close
                        text=l.text("Cancel", "取消")
                        disabled=s.busy()
                    />
                    <button
                        class="button danger"
                        type="submit"
                        disabled=move || !ui.state.with(State::confirm_enabled)
                    >
                        {if s.busy() {
                            l.text("Working…", "处理中…")
                        } else {
                            l.text("Confirm permanent deletion", "确认永久删除")
                        }}
                    </button>
                </div>
            </form>
        </dialog>
    }.into_any()
}
pub fn event_detail(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let d = s.data(Slot::Detail);
    let fields = s
        .selected
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    view! {
        <dialog
            class="event-drawer"
            data-modal="event"
            closedby="any"
            aria-modal="true"
            aria-labelledby="event-title"
            on:cancel=event_bridge::cancel(ui)
        >
            <div class="section-heading">
                <h2 id="event-title">{l.text("Event detail", "事件详情")}</h2>
                <button
                    class="icon-button"
                    aria-label=l.text("Close event detail", "关闭事件详情")
                    on:click=click_action(ui, Action::Close)
                >
                    <Icon name="close" />
                </button>
            </div>
            <p class="notice">
                {l
                    .text(
                        "Local-only content. Plugin queries never receive message bodies.",
                        "内容仅限本地。插件查询不会接收消息正文。",
                    )}
            </p>
            {remote_notice(s, Slot::Detail, ui)}
            {(!d.is_null())
                .then(|| {
                    view! {
                        {retained_content(d, l)}
                        {rows(&d["warnings"])
                            .into_iter()
                            .map(|w| view! { <p class="footnote">{string(&w).to_owned()}</p> })
                            .collect_view()}
                        <p class="footnote">
                            {l
                                .text(
                                    "Secret redaction is best-effort. Authentication secrets are not collection targets.",
                                    "敏感信息脱敏尽力而为，不能保证完备。认证密钥不属于采集目标。",
                                )}
                        </p>
                    }
                })}
            {event_context(&s.selected, l)}
            <details class="data-details event-metadata"><summary>{l.text("Recorded metadata", "已记录元数据")}</summary>
                <dl class="detail-list">
                    {fields.into_iter().map(|(k, v)| view! { <dt>{k}</dt><dd class="mono">{display_value(&v, l)}</dd> }).collect_view()}
                </dl>
            </details>
        </dialog>
    }.into_any()
}
