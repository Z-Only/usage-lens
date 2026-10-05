//! Bounded, local-only views of matched trace evidence. Never add across dimensions.
use crate::model::Action;
use crate::{
    model::*,
    trace_reader::{trace_cell, trace_state},
    views::{Ui, click_action, token_name},
};
use leptos::prelude::*;
use serde_json::Value;

pub fn trace_number(value: &Value, l: Language) -> String {
    let number = integer(value);
    if number == "—" {
        l.text("Unknown", "未知").into()
    } else {
        number
    }
}

pub fn trace_clock_warning(count: &Value, l: Language) -> AnyView {
    let count = string(count);
    if count.is_empty() || count.bytes().all(|b| b == b'0') {
        return ().into_any();
    }
    view! { <p class="notice trace-clock-anomaly">{l.text("Clock anomaly: recorded completion precedes start. These records stay in their recorded UTC start day; timestamps cannot establish latency or causal order.", "时钟异常：记录的完成时间早于开始时间。这些记录仍归入其记录的 UTC 开始日期；时间戳无法确定延迟或因果顺序。")}</p> }.into_any()
}

pub fn trace_totals(data: &Value, l: Language) -> AnyView {
    view! {
        <dl class="response-totals">{TOKEN_KEYS.into_iter().map(|key| view! {
            <div><dt>{token_name(key, l)}</dt><dd>{trace_number(&data["totals"][key], l)}</dd>
                <dd class="trace-token-coverage">{l.text("Reported records", "已报告记录")}": "{trace_number(&data["tokenCoverage"][key]["reportedCount"], l)}</dd>
            </div>
        }).collect_view()}</dl>
    }.into_any()
}

pub fn trace_threads(data: &Value, l: Language, ui: Ui) -> AnyView {
    let source = string(&data["source"]["id"]);
    view! {
        <section class="trace-insight-section trace-threads trace-thread-summary" aria-labelledby="trace-threads-title">
            <h3 id="trace-threads-title">{l.text("Thread summaries", "会话摘要")}</h3>
            <p class="footnote">{l.text("Each thread includes only records matching the submitted filters, not its complete history. Open a thread to read its recorded timeline. Thread IDs remain local.", "每个会话仅包含匹配已提交筛选的记录，不代表其完整历史。打开会话即可查看其记录时间线。会话 ID 仅用于本地。")}</p>
            {rows(&data["byThread"]).is_empty().then(|| view! { <p class="reader-empty">{l.text("No retained thread evidence in this scope. Missing evidence is not zero usage.", "此范围内没有保留的会话证据。证据缺失不代表零用量。")}</p> })}
            <ul class="trace-thread-cards">{rows(&data["byThread"]).into_iter().map(|row| {
                let thread = string(&row["threadId"]);
                let action = Action::TraceThread { source: source.into(), thread: thread.into() };
                view! {
                    <li class="trace-thread-card">
                        <div class="trace-attempt-head"><h4 class="mono">{display_value(&row["threadId"], l)}</h4>
                            {(!source.is_empty() && valid_trace_thread(thread)).then(|| view! {
                                <button type="button" class="button" aria-label=format!("{} {}", l.text("View thread", "查看会话"), thread) on:click=click_action(ui, action)>{l.text("Open timeline", "打开时间线")}</button>
                            })}
                        </div>
                        <dl class="trace-thread-dates">
                            <div><dt>{l.text("First matched start (UTC)", "首条匹配开始时间（UTC）")}</dt><dd class="mono">{display_value(&row["firstStartedAt"], l)}</dd></div>
                            <div><dt>{l.text("Last matched start (UTC)", "末条匹配开始时间（UTC）")}</dt><dd class="mono">{display_value(&row["lastStartedAt"], l)}</dd></div>
                        </dl>
                        <p class="response-count">{l.text("Inference records", "推理记录")}": "{trace_number(&row["count"], l)}" · "{l.text("With token evidence", "含 Token 证据")}": "{trace_number(&row["tokenAttemptCount"], l)}</p>
                        <dl class="trace-thread-statuses">{TRACE_STATUSES.into_iter().map(|status| view! {
                            <div><dt>{trace_state(&Value::String(status.into()), l)}</dt><dd>{trace_number(&row["statusCounts"][status], l)}</dd></div>
                        }).collect_view()}</dl>
                        <p class="trace-thread-total">{l.text("Direct response total", "直接关联响应总量")}": "<strong>{trace_number(&row["totals"]["totalTokens"], l)}</strong></p>
                        <p class="footnote">{l.text("Clock anomalies", "时钟异常")}": "{trace_number(&row["timestampAnomalyCount"], l)}</p>
                        {trace_clock_warning(&row["timestampAnomalyCount"], l)}
                        <details class="data-details trace-thread-tokens"><summary>{l.text("Token totals and coverage", "Token 总量与覆盖情况")}</summary>{trace_totals(&row, l)}</details>
                    </li>
                }
            }).collect_view()}</ul>
            {(data["threadsTruncated"] == true).then(|| view! { <p class="notice trace-threads-truncated">{l.text("Some threads omitted: only the first 500 thread IDs in lexicographic order are shown. Overall totals include all matching records.", "部分会话已省略：仅按字典顺序展示前 500 个会话 ID。总体总量包含全部匹配记录。")}</p> })}
        </section>
    }.into_any()
}

/// One comparison table per dimension; totals are already exact server strings.
pub fn trace_comparison(data: &Value, by_day: bool, l: Language) -> AnyView {
    let (key, id, title) = if by_day {
        (
            "byDay",
            "trace-days-title",
            l.text("UTC-day breakdown", "UTC 每日明细"),
        )
    } else {
        (
            "byRequestedSettings",
            "trace-settings-title",
            l.text("Requested settings comparison", "请求设置对比"),
        )
    };
    view! {
        <section class=if by_day { "trace-insight-section trace-groups trace-comparison trace-daily" } else { "trace-insight-section trace-groups trace-comparison trace-settings-comparison" } aria-labelledby=id>
            <h3 id=id>{title}</h3>
            <p class="footnote">{if by_day {
                l.text("Days use recorded start timestamps normalized to UTC, not import time or quota cycles. Missing UTC days are absent evidence, not zero usage; no calendar gaps are filled.", "日期使用归一化为 UTC 的记录开始时间，不是导入时间或额度周期。缺少的 UTC 日期表示证据缺失，不代表零用量；不会填补日历空白。")
            } else {
                l.text("Each row combines requested model, reasoning effort and service tier, preserving reported and unknown states. Requested tiers do not establish delivered Fast / Standard speed.", "每行组合请求模型、推理强度与服务档位，保留已报告和未知状态。请求档位不能确定实际 Fast / Standard 速度。")
            }}</p>
            {if rows(&data[key]).is_empty() { view! { <p class="reader-empty">{l.text("No group evidence", "暂无分组证据")}</p> }.into_any() } else { view! {
                <div class="table-scroll" tabindex="0" aria-label=title><table>
                    <thead><tr>
                        {if by_day { view! { <th scope="col">{l.text("UTC day", "UTC 日期")}</th> }.into_any() } else { view! {
                            <th scope="col">{l.text("Requested model", "请求模型")}</th><th scope="col">{l.text("Requested effort", "请求推理强度")}</th><th scope="col">{l.text("Requested tier", "请求服务档位")}</th>
                        }.into_any() }}
                        <th scope="col">{l.text("Inference records", "推理记录")}</th><th scope="col">{l.text("With token evidence", "含 Token 证据")}</th>
                        <th scope="col">{l.text("Input", "输入")}</th><th scope="col">{l.text("Output", "输出")}</th><th scope="col">{l.text("Reported total", "报告总量")}</th><th scope="col">{l.text("Clock anomalies", "时钟异常")}</th>
                    </tr></thead>
                    <tbody>{rows(&data[key]).into_iter().map(|row| view! {
                        <tr>
                            {if by_day { view! { <th scope="row" class="mono">{display_value(&row["date"], l)}</th> }.into_any() } else { view! {
                                <th scope="row">{trace_cell(&row["request"]["model"], l, false)}</th><td>{trace_cell(&row["request"]["reasoningEffort"], l, false)}</td><td>{trace_cell(&row["request"]["serviceTier"], l, false)}</td>
                            }.into_any() }}
                            <td>{trace_number(&row["count"], l)}</td><td>{trace_number(&row["tokenAttemptCount"], l)}</td>
                            {["inputTokens", "outputTokens", "totalTokens"].into_iter().map(|token| view! {
                                <td>{trace_number(&row["totals"][token], l)}<small>{l.text("Reported records", "已报告记录")}": "{trace_number(&row["tokenCoverage"][token]["reportedCount"], l)}</small></td>
                            }).collect_view()}
                            <td>{trace_number(&row["timestampAnomalyCount"], l)}</td>
                        </tr>
                    }).collect_view()}</tbody>
                </table></div>
            }.into_any() }}
            {(data[if by_day { "daysTruncated" } else { "requestedSettingsTruncated" }] == true).then(|| view! {
                <p class=if by_day { "notice trace-comparison-truncated trace-days-truncated" } else { "notice trace-comparison-truncated trace-settings-truncated" }>{if by_day {
                    l.text("Some days omitted: only the first 500 UTC days in date order are shown. Overall totals include all matching records.", "部分日期已省略：仅按日期顺序展示前 500 个 UTC 日期。总体总量包含全部匹配记录。")
                } else {
                    l.text("Some requested combinations omitted: only the first 500 in state/value lexicographic order are shown. Overall totals include all matching records.", "部分请求组合已省略：仅按状态／值的字典顺序展示前 500 个组合。总体总量包含全部匹配记录。")
                }}</p>
            })}
        </section>
    }.into_any()
}
