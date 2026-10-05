//! Local trace projections and directly associated response counters, never wire capture.
use crate::model::Action;
use crate::{
    event_bridge,
    model::*,
    reading::content_section,
    trace_insights::{
        trace_clock_warning, trace_comparison, trace_number, trace_threads, trace_totals,
    },
    views::{Button, Icon, Ui, click_action, more_button, remote_notice, token_name},
};
use leptos::prelude::*;
use serde_json::Value;

pub fn trace_state(value: &Value, l: Language) -> &'static str {
    match string(value) {
        "reported" => l.text("Reported", "已报告"),
        "omitted" => l.text("Omitted", "已省略"),
        "not_reported" => l.text("Not reported", "未报告"),
        "invalid" => l.text("Invalid", "无效"),
        "completed" => l.text("Completed", "已完成"),
        "failed" => l.text("Failed", "失败"),
        "cancelled" => l.text("Cancelled", "已取消"),
        "incomplete" => l.text("Incomplete", "未完成"),
        _ => l.text("Unknown", "未知"),
    }
}

pub fn trace_cell(cell: &Value, l: Language, token: bool) -> String {
    if cell["state"] == "reported" && cell["value"].is_string() {
        if token {
            integer(&cell["value"])
        } else {
            string(&cell["value"]).into()
        }
    } else {
        trace_state(&cell["state"], l).into()
    }
}

pub fn trace_scope(data: &Value, l: Language) -> AnyView {
    view! {
        <p class="trace-scope footnote">
            {if string(&data["fromDate"]).is_empty() { l.text("All retained trace dates", "全部保留追踪日期").to_owned() } else { format!("{} – {} (UTC)", string(&data["fromDate"]), string(&data["toDate"])) }}
        </p>
        <dl class="trace-exact-scope" aria-label=l.text("Submitted exact filters", "已提交的精确筛选")>
            {[("threadId", "Thread ID", "会话 ID"), ("status", "Status", "状态"), ("requestedModel", "Requested model", "请求模型"), ("requestedReasoningEffort", "Requested reasoning effort", "请求推理强度"), ("requestedServiceTier", "Requested service tier", "请求服务档位")].into_iter().filter(|(key, _, _)| !string(&data[key]).is_empty()).map(|(key, en, zh)| view! {
                <div><dt>{l.text(en, zh)}</dt><dd>{if key == "status" { trace_state(&data[key], l).to_owned() } else { string(&data[key]).to_owned() }}</dd></div>
            }).collect_view()}
        </dl>
        <p class="footnote">{l.text("Partial local history. Each record is a trace inference attempt, not a physical HTTP request count; lower-level retries may be combined. No records does not prove no activity.", "仅涵盖部分本地历史。每条记录表示一次追踪推理尝试，不代表物理 HTTP 请求数量；底层重试可能合并。没有记录不能证明没有活动。")}</p>
        <p class="footnote">{l.text("Import warnings describe retained imports across this source, not only the selected dates. Warning lists and metadata groups may be bounded; the returned warnings describe any truncation.", "导入警告涵盖此来源的保留导入记录，并非仅限所选日期。警告列表与元数据分组可能有数量限制；返回的警告会说明截断情况。")}</p>
        {rows(&data["warnings"]).into_iter().map(|warning| view! { <p class="footnote mono">{display_value(&warning, l)}</p> }).collect_view()}
    }.into_any()
}

pub fn trace_page(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let data = s.data(Slot::Traces);
    view! {
        <section class="panel trace-controls">
            <div class="section-heading"><h2>{l.text("Local trace reader", "本地追踪阅读器")}</h2><span class="state-label">{l.text("Local only", "仅限本地")}</span></div>
            <p class="muted">{l.text("Read explicitly imported inference records, requested settings and directly linked response tokens. Optional visible-text projections stay local; conversational plugin queries never receive them.", "查看明确导入的推理记录、请求设置与直接关联的响应 Token。可选的可见文本投影仅保留在本地；对话插件查询不会接收这些内容。")}</p>
            <form class="filters trace-filters" aria-label=l.text("Trace filters", "追踪筛选") on:submit=event_bridge::prevent(ui, Action::ApplyTraceFilters)>
                <label>{l.text("Trace from (UTC)", "追踪开始日期（UTC）")}<input type="date" name="traceFrom" prop:value=s.trace_filters.from.clone() on:input=event_bridge::value(ui, InputAction::Filter("traceFrom")) /></label>
                <label>{l.text("Trace to (UTC)", "追踪结束日期（UTC）")}<input type="date" name="traceTo" prop:value=s.trace_filters.to.clone() on:input=event_bridge::value(ui, InputAction::Filter("traceTo")) /></label>
                <label>{l.text("Thread ID (exact)", "会话 ID（精确匹配）")}<input type="text" name="traceThread" maxlength="160" autocomplete="off" spellcheck="false" prop:value=s.trace_filters.thread_id.clone() on:input=event_bridge::value(ui, InputAction::Filter("traceThread")) /></label>
                <label><span id="trace-status-label">{l.text("Trace status", "追踪状态")}</span><select aria-labelledby="trace-status-label" name="traceStatus" prop:value=s.trace_filters.status.clone() on:change=event_bridge::value(ui, InputAction::Filter("traceStatus"))>
                    <option value="" selected=s.trace_filters.status.is_empty()>{l.text("All statuses", "全部状态")}</option>
                    {TRACE_STATUSES.into_iter().map(|status| view! { <option value=status selected=s.trace_filters.status == status>{trace_state(&Value::String(status.into()), l)}</option> }).collect_view()}
                </select></label>
                {[("traceModel", "Requested model (exact)", "请求模型（精确匹配）", s.trace_filters.requested_model.clone()), ("traceEffort", "Requested reasoning effort (exact)", "请求推理强度（精确匹配）", s.trace_filters.requested_reasoning_effort.clone()), ("traceTier", "Requested service tier (exact)", "请求服务档位（精确匹配）", s.trace_filters.requested_service_tier.clone())].into_iter().map(|(name, en, zh, value)| view! {
                    <label>{l.text(en, zh)}<input type="text" name=name maxlength="128" autocomplete="off" spellcheck="false" prop:value=value on:input=event_bridge::value(ui, InputAction::Filter(name)) /></label>
                }).collect_view()}
                <label><span id="trace-order-label">{l.text("Timeline order", "时间线顺序")}</span><select aria-labelledby="trace-order-label" name="traceOrder" prop:value=s.trace_order.key() on:change=event_bridge::value(ui, InputAction::Filter("traceOrder"))>
                    {[TraceOrder::NewestFirst, TraceOrder::OldestFirst].into_iter().map(|order| view! { <option value=order.key() selected=s.trace_order == order>{order.label(l)}</option> }).collect_view()}
                </select></label>
                <div class="trace-filter-actions">
                    <button class="button primary" type="submit" disabled=s.source.is_empty()>{l.text("Apply trace filters", "应用追踪筛选")}</button>
                    <button class="button" type="button" disabled=s.source.is_empty() on:click=event_bridge::trace_week(ui)>{l.text("This UTC week", "本 UTC 周")}</button>
                    <Button ui action=Action::ResetTraceFilters text=l.text("Clear all trace filters", "清除全部追踪筛选") disabled=s.source.is_empty() />
                </div>
            </form>
            <p class="footnote">{l.text("Choose both UTC dates or leave both empty. Dates use the recorded start time, including both endpoints. Submit a seven-day range for a weekly breakdown; this is not the provider's quota cycle. Draft edits do not change loaded results or pagination.", "请选择两个 UTC 日期或全部留空。按记录的开始时间筛选，包含起止日期。选择七天范围即可查看周明细；这不代表提供方的额度周期。编辑输入框不会改变已加载结果或分页。")}</p>
            <p class="footnote">{l.text("Exact filters are case-sensitive and preserve spaces. Requested settings match only reported request values, never observed responses or unknown states. The week preset keeps the entered exact filters; thread navigation keeps the submitted filters. Thread IDs stay in this local dashboard.", "精确筛选区分大小写并保留空格。请求设置仅匹配已报告的请求值，不匹配观测响应或未知状态。本周预设保留已输入的精确筛选；会话导航保留已提交的筛选。会话 ID 仅用于此本地仪表盘。")}</p>
            <p class="footnote trace-draft-notice" role="status">{move || ui.state.with(|state| if state.trace_filters != state.applied_trace_filters || state.trace_order != state.applied_trace_order { state.language.text("Unsubmitted edits. Results and pagination use the submitted filters.", "尚有未提交的编辑。结果与分页使用已提交的筛选。") } else { "" })}</p>
            {(s.error(Slot::Traces) == "invalid_trace_date_range").then(|| view! { <p class="notice error">{l.text("Choose a valid date pair, up to 3,661 days inclusive.", "请选择有效的起止日期，含首尾最多 3,661 天。")}</p> })}
            {(s.error(Slot::Traces) == "invalid_trace_filter").then(|| view! { <p class="notice error">{l.text("Use a valid thread ID (1–160 ASCII letters, digits or . _ : @ / + -, starting with a letter or digit), a listed status, and requested values up to 128 UTF-16 units without control characters. Values are not trimmed.", "请使用有效的会话 ID（1–160 个 ASCII 字母、数字或 . _ : @ / + -，以字母或数字开头）、列表中的状态，以及不含控制字符且最多 128 个 UTF-16 单元的请求值。不会删除首尾空格。")}</p> })}
        </section>
        {trace_summary(s, ui)}
        <section class="panel trace-reader trace-timeline" aria-labelledby="trace-timeline-title">
            <div class="section-heading"><h2 id="trace-timeline-title">{l.text("Trace timeline", "追踪时间线")}</h2><span class="state-label">{s.applied_trace_order.label(l)}</span></div>
            <p class="footnote">{l.text("Ordered by recorded start timestamp, then attempt ID for ties. Recorded sequence is unavailable: this is not causal order or request latency. Thread navigation opens oldest first and keeps the submitted exact filters.", "按记录的开始时间排序，同一时间按尝试 ID 排序。没有记录顺序信息：这不代表因果顺序或请求延迟。会话导航按最早在前打开，并保留已提交的精确筛选。")}</p>
            {remote_notice(s, Slot::Traces, ui)}
            {(!data.is_null()).then(|| view! {
                {trace_scope(data, l)}
                {(data["coverage"]["capture"] != "available").then(|| view! { <p class="notice trace-unavailable">{l.text("Trace evidence not captured for this source. Older databases are not migrated by reading this page. No request metadata or token attribution can be recovered from an empty trace result.", "此来源尚未采集追踪证据。读取此页面不会迁移旧版数据库；无法从空追踪结果恢复请求元数据或 Token 归因。")}</p> })}
                <p class="footnote">{l.text("Loaded inference records", "已加载推理记录")}": "{rows(&data["attempts"]).len().to_string()}</p>
                <ol class="trace-attempts">
                    {rows(&data["attempts"]).into_iter().map(|attempt| {
                        let title = format!("{} {}", l.text("View trace", "查看追踪"), string(&attempt["attemptId"]));
                        view! {
                            <li><button class="trace-attempt" aria-label=title on:click=click_action(ui, Action::Select(attempt.clone()))>
                                <span class="trace-attempt-head"><strong>{trace_cell(&attempt["request"]["model"], l, false)}</strong><span class="state-label">{trace_state(&attempt["status"], l)}</span></span>
                                <span class="mono">{display_value(&attempt["startedAt"], l)}</span>
                                <span>{l.text("Requested effort", "请求推理强度")}": "{trace_cell(&attempt["request"]["reasoningEffort"], l, false)}" · "{l.text("Requested tier", "请求服务档位")}": "{trace_cell(&attempt["request"]["serviceTier"], l, false)}</span>
                                <span>{l.text("Direct response total", "直接关联响应总量")}": "{trace_cell(&attempt["tokens"]["totalTokens"], l, true)}</span>
                                {(attempt["timestampAnomaly"] == true).then(|| view! { <span class="trace-clock-anomaly">{l.text("Clock anomaly: recorded completion precedes start", "时钟异常：记录的完成时间早于开始时间")}</span> })}
                                <span class="trace-attempt-id mono">{string(&attempt["attemptId"]).to_owned()}</span>
                            </button>{trace_thread_navigation(&attempt, l, ui)}</li>
                        }
                    }).collect_view()}
                </ol>
                {rows(&data["attempts"]).is_empty().then(|| view! { <p class="reader-empty">{l.text("No retained inference records in this scope. Missing evidence is not zero usage.", "此范围内没有保留的推理记录。证据缺失不代表零用量。")}</p> })}
                {more_button(s, Slot::Traces, ui, "Load more traces", "加载更多追踪")}
            })}
        </section>
    }.into_any()
}

pub fn trace_thread_navigation(attempt: &Value, l: Language, ui: Ui) -> AnyView {
    let thread = string(&attempt["threadId"]);
    if !valid_trace_thread(thread) {
        return ().into_any();
    }
    let action = Action::TraceThread {
        source: string(&attempt["sourceId"]).into(),
        thread: thread.into(),
    };
    view! {
        <div class="trace-thread-navigation">
            <span class="mono">{l.text("Thread", "会话")}": "{thread.to_owned()}</span>
            <button type="button" class="button" aria-label=format!("{} {}", l.text("View thread", "查看会话"), thread) on:click=click_action(ui, action)>{l.text("View this thread", "查看此会话")}</button>
        </div>
    }.into_any()
}

pub fn trace_summary(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let data = s.data(Slot::TraceSummary);
    view! {
        <section class="panel trace-summary">
            <div class="section-heading"><h2>{l.text("Trace token breakdown", "追踪 Token 明细")}</h2><span class="state-label">{l.text("Direct response evidence", "直接响应证据")}</span></div>
            {remote_notice(s, Slot::TraceSummary, ui)}
            {(!data.is_null()).then(|| view! {
                {trace_scope(data, l)}
                <p class="response-count"><strong>{integer(&data["attemptCount"])}</strong>{l.text("recorded inference attempts", "条推理尝试记录")}" · "{integer(&data["tokenAttemptCount"])}" "{l.text("with directly associated token records", "条含直接关联 Token 记录")}</p>
                {trace_totals(data, l)}
                <p class="footnote">{l.text("Clock anomalies", "时钟异常")}": "{trace_number(&data["timestampAnomalyCount"], l)}</p>
                {trace_clock_warning(&data["timestampAnomalyCount"], l)}
                <p class="notice">{l.text("Only completed attempts with a directly associated response ID contribute token evidence. Missing, omitted and invalid counters remain unknown; sums include only reported counters. Cached input is part of input and reasoning output is part of output. Groups below repeat the same evidence; do not add groups across dimensions.", "仅完成且直接关联响应 ID 的尝试可提供 Token 证据。缺失、省略和无效计数保持未知；总量仅汇总已报告计数。缓存输入属于输入，推理输出属于输出。下方不同维度重复展示同一证据，不能跨维度相加。")}</p>
                <div class="trace-statuses">{rows(&data["byStatus"]).into_iter().map(|row| view! { <span>{trace_state(&row["status"], l)}": "{integer(&row["count"])}</span> }).collect_view()}</div>
                {trace_threads(data, l, ui)}
                {trace_comparison(data, false, l)}
                {trace_comparison(data, true, l)}
                <details class="data-details trace-individual-dimensions"><summary>{l.text("Individual metadata dimensions", "单项元数据维度")}</summary>
                {[("byRequestedModel", "Requested model", "请求模型"), ("byRequestedReasoningEffort", "Requested reasoning effort", "请求推理强度"), ("byRequestedServiceTier", "Requested service tier", "请求服务档位"), ("byObservedModel", "Observed response model", "观测响应模型"), ("byObservedServiceTier", "Observed response tier", "观测响应档位")].into_iter().map(|(key, en, zh)| trace_groups(&data[key], l.text(en, zh), l)).collect_view()}
                {(data["groupsTruncated"] == true).then(|| view! { <p class="notice trace-groups-truncated">{l.text("Some metadata groups omitted; totals include all matching records", "部分元数据分组已省略；总量包含全部匹配记录")}</p> })}
                </details>
                <p class="footnote">{l.text("Requested settings are separate from observed response metadata. A requested service tier does not prove delivered speed or a Fast / Standard mode. Trace tokens are not combined with account totals and cannot be converted into exact quota charges, remaining tokens or API cost.", "请求设置与观测到的响应元数据分开展示。请求服务档位不能证明实际速度或 Fast / Standard 模式。追踪 Token 不与账户总量合并，也不能换算为精确额度扣减、剩余 Token 或 API 费用。")}</p>
            })}
        </section>
    }.into_any()
}

pub fn trace_groups(groups: &Value, title: &'static str, l: Language) -> AnyView {
    view! {
        <details class="data-details trace-groups" open>
            <summary>{title}</summary>
            {if rows(groups).is_empty() { view! { <p class="muted">{l.text("No group evidence", "暂无分组证据")}</p> }.into_any() } else { view! {
                <div class="table-scroll" tabindex="0" aria-label=title><table><thead><tr><th>{title}</th><th>{l.text("Inference records", "推理记录")}</th><th>{l.text("Input", "输入")}</th><th>{l.text("Output", "输出")}</th><th>{l.text("Reported total", "报告总量")}</th></tr></thead>
                    <tbody>{rows(groups).into_iter().map(|group| view! {
                        <tr><th scope="row">{trace_cell(&group, l, false)}</th><td>{integer(&group["count"])}</td>
                            {["inputTokens", "outputTokens", "totalTokens"].into_iter().map(|key| view! { <td>{integer(&group["totals"][key])}<small>{l.text("Reported records", "已报告记录")}": "{integer(&group["tokenCoverage"][key]["reportedCount"])}</small></td> }).collect_view()}
                        </tr>
                    }).collect_view()}</tbody>
                </table></div>
            }.into_any() }}
        </details>
    }.into_any()
}

pub fn trace_detail(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let data = s.data(Slot::TraceDetail);
    let attempt = if data.is_null() {
        &s.selected
    } else {
        &data["attempt"]
    };
    view! {
        <dialog class="event-drawer trace-drawer" data-modal="trace" closedby="any" aria-modal="true" aria-labelledby="trace-title" on:cancel=event_bridge::cancel(ui)>
            <div class="section-heading"><h2 id="trace-title">{l.text("Trace detail", "追踪详情")}</h2><button class="icon-button" aria-label=l.text("Close trace detail", "关闭追踪详情") on:click=click_action(ui, Action::Close)><Icon name="close" /></button></div>
            <p class="notice transmission-evidence"><strong>{l.text("Prepared request evidence only", "仅有已准备请求证据")}</strong><br />{l.text("A prepared record does not prove the request was sent or accepted. WebSocket warmup may reconstruct logical input. Redacted visible-text projections are not full raw requests or wire payloads; system/developer instructions, hidden reasoning, credentials and unclassified user-role content are excluded. Redaction is best-effort.", "已准备记录不能证明请求已发送或被接受。WebSocket 预热可能重建逻辑输入。脱敏可见文本投影不是完整原始请求或网络载荷；系统／开发者指令、隐藏推理、凭据及未分类的 user 角色内容均不在采集范围。脱敏尽力而为，不能保证完备。")}</p>
            {remote_notice(s, Slot::TraceDetail, ui)}
            {trace_thread_navigation(attempt, l, ui)}
            {(attempt["timestampAnomaly"] == true).then(|| view! { <p class="notice trace-clock-anomaly">{l.text("Clock anomaly: recorded completion precedes start. Do not interpret these timestamps as request latency or causal order.", "时钟异常：记录的完成时间早于开始时间。请勿将这些时间戳解释为请求延迟或因果顺序。")}</p> })}
            <section class="reader-context"><h3>{l.text("Requested settings", "请求设置")}</h3><dl class="detail-list">
                {[("model", "Model", "模型"), ("reasoningEffort", "Reasoning effort", "推理强度"), ("serviceTier", "Service tier", "服务档位")].into_iter().map(|(key, en, zh)| view! { <dt>{l.text(en, zh)}</dt><dd>{trace_cell(&attempt["request"][key], l, false)}</dd> }).collect_view()}
            </dl><h3>{l.text("Observed response metadata", "观测响应元数据")}</h3><dl class="detail-list">
                {[("model", "Model", "模型"), ("serviceTier", "Service tier", "服务档位")].into_iter().map(|(key, en, zh)| view! { <dt>{l.text(en, zh)}</dt><dd>{trace_cell(&attempt["observed"][key], l, false)}</dd> }).collect_view()}
                <dt>{l.text("Status", "状态")}</dt><dd>{trace_state(&attempt["status"], l)}</dd>
            </dl><p class="footnote">{l.text("Requested tier is not evidence of delivered Fast / Standard speed.", "请求档位不能证明实际 Fast / Standard 速度。")}</p></section>
            <section class="reader-context"><h3>{l.text("Directly associated response tokens", "直接关联响应 Token")}</h3>
                <dl class="detail-list">{TOKEN_KEYS.into_iter().map(|key| view! { <dt>{token_name(key, l)}</dt><dd>{trace_cell(&attempt["tokens"][key], l, true)}</dd> }).collect_view()}</dl>
                <p class="footnote">{l.text("Only directly associated completed-response counters. No token estimate for failed, cancelled or incomplete attempts; unknown is not zero.", "仅展示直接关联的已完成响应计数。不估算失败、已取消或未完成尝试的 Token；未知不等于零。")}</p>
            </section>
            {(!data.is_null()).then(|| view! {
                <div class="message-reader trace-projections"><h3>{l.text("Redacted visible-text projections", "脱敏可见文本投影")}</h3>
                    {if data["contentRetained"] != true || data["content"].is_null() { view! { <p class="reader-empty">{l.text("No projection retained or available. Content may not have been captured or may have been deleted. Enabling retention now cannot recover earlier content.", "投影未保留或不可用。内容可能未采集或已删除。现在开启保留不会恢复此前内容。")}</p> }.into_any() } else { view! {
                        {[("requestProjection", "Prepared request projection", "已准备请求投影"), ("responseProjection", "Visible response projection", "可见响应投影")].into_iter().map(|(key, en, zh)| {
                            if data["content"][key].is_null() { view! { <p class="reader-empty">{l.text(en, zh)}": "{l.text("Not retained", "未保留")}</p> }.into_any() } else { content_section(l.text(en, zh), &data["content"][key], l) }
                        }).collect_view()}
                    }.into_any() }}
                    <p class="footnote">{l.text("Local-only content. MCP and conversational Skill queries remain aggregate-only and do not return these projections or correlation IDs.", "内容仅限本地。MCP 与对话 Skill 查询仍只返回汇总数据，不返回这些投影或关联 ID。")}</p>
                </div>
                {rows(&data["warnings"]).into_iter().map(|warning| view! { <p class="footnote mono">{display_value(&warning, l)}</p> }).collect_view()}
            })}
            <details class="data-details trace-identifiers"><summary>{l.text("Local correlation and evidence", "本地关联与证据")}</summary><dl class="detail-list">
                {[("sourceId", "Source", "来源"), ("bundleId", "Bundle", "数据包"), ("attemptId", "Inference attempt", "推理尝试"), ("threadId", "Thread", "会话"), ("turnId", "Turn", "轮次"), ("inferenceId", "Inference", "推理"), ("responseId", "Response ID", "响应 ID"), ("upstreamRequestId", "Upstream request ID", "上游请求 ID"), ("startedAt", "Started (UTC)", "开始时间（UTC）"), ("completedAt", "Completed (UTC)", "完成时间（UTC）")].into_iter().map(|(key, en, zh)| view! { <dt>{l.text(en, zh)}</dt><dd class="mono">{display_value(&attempt[key], l)}</dd> }).collect_view()}
            </dl>{content_section(l.text("Recorded evidence flags", "已记录证据标记"), &attempt["evidence"], l)}</details>
            <Button ui action=Action::Close text=l.text("Back to traces", "返回追踪") />
        </dialog>
    }.into_any()
}
