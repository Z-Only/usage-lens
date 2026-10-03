//! Local retained-content presentation. No transport capture or inferred token joins.
use crate::model::Action;
use crate::{
    model::*,
    views::{Ui, click_action},
};
use leptos::prelude::*;
use serde_json::{Value, json};

pub fn retained_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| serde_json::to_string_pretty(value).unwrap_or_default())
}

pub fn event_name(value: &Value, l: Language) -> String {
    match string(value) {
        "user_prompt" => l.text("User message", "用户消息").into(),
        "assistant_visible_message" => l.text("Visible assistant message", "可见助手消息").into(),
        "tool_call" => l.text("Tool call", "工具调用").into(),
        "skill_requested" => l.text("Skill requested", "技能请求").into(),
        "skill_loaded" => l.text("Skill loaded", "技能加载").into(),
        "skill_invoked" => l.text("Skill invoked", "技能调用").into(),
        _ => display_value(value, l),
    }
}

pub fn activity_groups(events: &Value, key: &str) -> Vec<Value> {
    let mut groups = rows(&events[key]);
    if key == "byModel"
        && !groups.iter().any(|group| group["name"].is_null())
        && events["unknownModelCount"].as_str().is_some_and(|count| {
            !count.is_empty() && count != "0" && count.bytes().all(|b| b.is_ascii_digit())
        })
    {
        groups.push(json!({"name":null,"count":events["unknownModelCount"]}));
    }
    groups
}

pub fn activity_breakdown(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let events = &s.data(Slot::Overview)["events"];
    view! {
        <section class="panel activity-breakdown">
            <div class="section-heading">
                <h2>{l.text("Explore recorded activity", "探索活动记录")}</h2>
                <span class="state-label">{l.text("Event counts", "事件数量")}</span>
            </div>
            <p class="muted">{l.text("Open a count to read its local records. These counts are not requests sent, token totals, or quota charges.", "点击数量查看本地记录。这些数量不代表已发送请求、Token 总量或额度扣减。")}</p>
            <div class="breakdown-grid">
                {[("byType", "eventType", "By activity", "按活动"), ("byModel", "model", "By reported model", "按报告模型")].into_iter().map(|(key, field, en, zh)| {
                    let groups = activity_groups(events, key);
                    view! {
                        <div>
                            <h3>{l.text(en, zh)}</h3>
                            {groups.is_empty().then(|| view! { <p class="muted">{l.text("No group evidence", "暂无分组证据")}</p> })}
                            <ul class="breakdown-list">
                                {groups.into_iter().map(|group| {
                                    let name = if field == "eventType" { event_name(&group["name"], l) } else { display_value(&group["name"], l) };
                                    let known = group["name"].as_str().is_some_and(|v| !v.is_empty());
                                    view! {
                                        <li>
                                            {if known { view! {
                                                <button class="breakdown-link" on:click=click_action(ui, Action::Drilldown(field, string(&group["name"]).into()))>
                                                    <span>{name}</span><strong>{integer(&group["count"])}</strong>
                                                </button>
                                            }.into_any() } else { view! {
                                                <div class="breakdown-link unknown"><span>{name}</span><strong>{integer(&group["count"])}</strong></div>
                                            }.into_any() }}
                                        </li>
                                    }
                                }).collect_view()}
                            </ul>
                        </div>
                    }
                }).collect_view()}
            </div>
            <p class="footnote">{l.text("Current source, all retained events. Up to 500 reported model groups; unknown models stay unknown and cannot be selected as an exact-model filter.", "当前来源的全部保留事件。最多显示 500 个报告模型分组；未知模型保持未知，不能作为精确模型筛选项。")}</p>
        </section>
    }.into_any()
}

pub fn search_scope(s: &State) -> AnyView {
    let l = s.language;
    let data = s.data(Slot::Activity);
    let search = &data["search"];
    view! {
        {(!s.applied_filters.content_query.is_empty()).then(|| view! {
            <div class="notice search-scope">
                <strong>{l.text("Retained-content search", "保留内容搜索")}</strong>
                <span>{l.text("Case-sensitive substring search of redacted stored JSON, including field names. At most the latest 10,000 retained-content records in this source are examined after metadata filters. No result does not prove no activity.", "区分大小写，按子串搜索脱敏后存储的 JSON（包括字段名）。元数据筛选后最多检查此来源最新的 10,000 条保留内容记录。无结果不代表没有活动。")}</span>
                <span>{l.text("Applied query", "已应用搜索词")}": "{s.applied_filters.content_query.clone()}</span>
                {(!search.is_null()).then(|| view! {
                    <span>{l.text("Examined", "已检查")}": "{integer(&search["searchedRecordCount"])}" · "{l.text("Matching retained records", "匹配保留记录")}": "{integer(&search["matchedCount"])}</span>
                    {(search["truncated"] == true).then(|| view! { <strong>{l.text("Older retained content is outside this search window", "更早的保留内容不在本次搜索范围内")}</strong> })}
                })}
                <span>{l.text("Changed or deleted retained content can invalidate pagination. Retry starts a fresh search with the submitted filters.", "保留内容发生变化或被删除后，分页可能失效。重试将按已提交的筛选重新搜索。")}</span>
            </div>
        })}
        <p class="footnote">{l.text("Loaded records", "已加载记录")}": "{rows(&data["events"]).len().to_string()}" · "{l.text("Newest observation first. Filters apply only after submission; editing fields does not change the current results or next page.", "按采集时间从新到旧排列。提交后才应用筛选；编辑输入框不会改变当前结果或下一页。")}</p>
    }.into_any()
}

pub fn content_section(title: &'static str, value: &Value, l: Language) -> AnyView {
    let text = retained_text(value);
    view! {
        <section class="reader-section">
            <h3>{title}</h3>
            {if text.is_empty() { view! { <p class="muted">{l.text("Empty retained text", "保留文本为空")}</p> }.into_any() } else { view! { <pre class="content-body" tabindex="0">{text}</pre> }.into_any() }}
        </section>
    }.into_any()
}

pub fn retained_content(d: &Value, l: Language) -> AnyView {
    let content = &d["content"];
    view! {
        <div class="message-reader">
            <h3>{l.text("Retained content", "保留内容")}</h3>
            {if d["contentRetained"] != true || content.is_null() { view! {
                <div class="reader-empty">
                    <strong>{l.text("Content unavailable", "内容不可用")}</strong>
                    <p>{l.text("Content was not retained, is unavailable, or has been deleted. This record cannot tell which occurred.", "内容未保留、不可用或已删除。此记录无法区分具体原因。")}</p>
                    <p>{l.text("Enabling capture now does not recover earlier content. Capture is off by default.", "现在开启采集不会恢复此前内容。默认关闭内容采集。")}</p>
                </div>
            }.into_any() } else { view! {
                {[ ("body", "Message body", "消息正文"), ("toolArguments", "Tool arguments", "工具参数"), ("toolResult", "Tool result", "工具结果") ].into_iter().filter_map(|(key, en, zh)| content.get(key).map(|v| content_section(l.text(en, zh), v, l))).collect_view()}
                {rows(&content["files"]).into_iter().map(|file| view! {
                    <details class="data-details retained-file"><summary>{l.text("Retained file", "保留文件")}": "{display_value(&file["name"], l)}</summary>
                        {content_section(l.text("File text", "文件文本"), &file["content"], l)}
                    </details>
                }).collect_view()}
                {content.as_object().is_some_and(|c| c.is_empty()).then(|| view! { <p class="muted">{l.text("No allowed content fields were retained", "未保留允许的内容字段")}</p> })}
                <details class="data-details retained-json"><summary>{l.text("Inspect retained JSON", "查看保留 JSON")}</summary><pre class="content-body" tabindex="0">{retained_text(content)}</pre></details>
            }.into_any() }}
        </div>
    }.into_any()
}

pub fn event_context(event: &Value, l: Language) -> AnyView {
    view! {
        <section class="reader-context">
            <div class="section-heading"><h3>{event_name(&event["eventType"], l)}</h3><span class="state-label">{l.text("Local record", "本地记录")}</span></div>
            <dl class="detail-list">
                {[("model", "Reported model", "报告模型"), ("reasoningEffort", "Reported reasoning effort", "报告推理强度"), ("toolName", "Tool name", "工具名称"), ("status", "Reported status", "报告状态"), ("sessionId", "Recorded session", "记录会话"), ("turnId", "Recorded turn", "记录轮次")].into_iter().map(|(key, en, zh)| view! {
                    <dt>{l.text(en, zh)}</dt><dd class="mono">{display_value(&event[key], l)}</dd>
                }).collect_view()}
                <dt>{l.text("Fast / Standard", "Fast / Standard")}</dt><dd>{l.text("Unknown · not recorded", "未知 · 未记录")}</dd>
            </dl>
        </section>
        <section class="transmission-evidence notice">
            <strong>{l.text("Actual remote-send evidence unavailable", "缺少实际远端发送证据")}</strong>
            <p>{l.text("A retained message or tool record does not prove the exact request body sent to a remote service. This source has no wire payload, delivery receipt, or direct response-token link for this event.", "保留的消息或工具记录不能证明发往远端服务的精确请求正文。此来源没有该事件的网络请求正文、送达回执或直接对应的响应 Token 关系。")}</p>
            <p>{l.text("System/developer instructions and hidden reasoning are excluded. Tool arguments/results describe retained local evidence only. Missing fields stay unknown.", "系统／开发者指令及隐藏推理不在采集范围内。工具参数／结果仅代表保留的本地证据。缺失字段保持未知。")}</p>
        </section>
    }.into_any()
}
