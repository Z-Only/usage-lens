//! Explicit UTC token periods are separate from provider quota reset windows.
use crate::model::Action;
use crate::{
    event_bridge,
    model::*,
    views::{Ui, remote_notice, token_name},
};
use leptos::prelude::*;

pub fn token_period_panel(s: &State, ui: Ui) -> AnyView {
    let l = s.language;
    let result = s.data(Slot::TokenPeriod);
    let has_weekly = quota_windows(&s.data(Slot::Overview)["quota"], true)
        .iter()
        .any(|(_, _, window)| {
            reported(&reported(window)["windowDurationMins"]).as_str() == Some("10080")
        });
    view! {
        <section class="panel token-period">
            <div class="section-heading"><h2>{l.text("Tokens alongside quota", "额度与 Token 对照")}</h2><span class="state-label">{l.text("Different measures", "不同指标")}</span></div>
            <p class="muted">{l.text("Quota is the provider-reported allowance percentage above. Choose a separate token period below. A reset timestamp and 10,080-minute duration do not establish the cycle start; no quota cycle is inferred.", "上方额度为提供方报告的使用百分比。下方可独立选择 Token 时间范围。重置时间与 10,080 分钟时长不能确定周期起点；此处不推测额度周期。")}</p>
            {(!has_weekly).then(|| view! { <p class="notice">{l.text("Weekly quota unavailable: no reported 10,080-minute window in this snapshot.", "每周额度不可用：此快照没有报告 10,080 分钟窗口。")}</p> })}
            <form class="filters" on:submit=event_bridge::prevent(ui, Action::ApplyTokenPeriod)>
                <label>{l.text("Token period from (UTC)", "Token 开始日期（UTC）")}
                    <input type="date" name="tokenFrom" prop:value=s.token_period.from.clone() on:input=event_bridge::value(ui, InputAction::Filter("tokenFrom")) />
                </label>
                <label>{l.text("Token period to (UTC)", "Token 结束日期（UTC）")}
                    <input type="date" name="tokenTo" prop:value=s.token_period.to.clone() on:input=event_bridge::value(ui, InputAction::Filter("tokenTo")) />
                </label>
                <button class="button primary" type="submit" disabled=s.source.is_empty()>{l.text("Read token period", "查看 Token 范围")}</button>
            </form>
            <p class="footnote">{l.text("Both UTC dates are required, up to 366 days inclusive. Only recorded occurrence times count; import times are never substituted. The selected dates are not the provider's quota cycle.", "必须填写两个 UTC 日期，含首尾最多 366 天。仅统计记录的发生时间，不以导入时间代替。所选日期不代表提供方的额度周期。")}</p>
            {remote_notice(s, Slot::TokenPeriod, ui)}
            {(s.error(Slot::TokenPeriod) == "invalid_token_period").then(|| view! { <p class="notice error">{l.text("Choose a valid UTC date pair, no more than 366 days inclusive.", "请选择有效的 UTC 起止日期，含首尾不超过 366 天。")}</p> })}
            {if result.is_null() { view! { <p class="reader-empty">{l.text("Token period not selected or unavailable. Missing evidence is not zero usage.", "尚未选择 Token 范围或数据不可用。证据缺失不代表零用量。")}</p> }.into_any() } else { view! {
                <p class="skill-trend-scope"><strong>{l.text("Selected token dates (UTC): ", "所选 Token 日期（UTC）：")}</strong>{string(&result["fromDate"]).to_owned()}" – "{string(&result["toDate"]).to_owned()}</p>
                <p class="response-count"><strong>{integer(&result["responseCount"])}</strong>{l.text("observed completed responses in selected dates", "条已观测完成响应位于所选日期内")}</p>
                <dl class="response-totals">{TOKEN_KEYS.into_iter().map(|key| view! { <div><dt>{token_name(key, l)}</dt><dd>{integer(&result["totals"][key])}</dd></div> }).collect_view()}</dl>
                <div class="table-scroll" tabindex="0" aria-label=l.text("Token period by model", "按模型统计 Token 范围")>
                    <table><thead><tr><th>{l.text("Reported model", "报告模型")}</th><th>{l.text("Responses", "响应")}</th><th>{l.text("Input", "输入")}</th><th>{l.text("Output", "输出")}</th><th>{l.text("Reported total", "报告总量")}</th></tr></thead>
                        <tbody>{rows(&result["byModel"]).into_iter().map(|row| view! { <tr><td>{display_value(&row["model"], l)}</td><td>{integer(&row["responseCount"])}</td><td>{integer(&row["totals"]["inputTokens"])}</td><td>{integer(&row["totals"]["outputTokens"])}</td><td>{integer(&row["totals"]["totalTokens"])}</td></tr> }).collect_view()}</tbody>
                    </table>
                </div>
                {(result["byModelTruncated"] == true).then(|| view! { <p class="notice">{l.text("Model groups truncated; the overall totals still include every matching record", "模型分组已截断；总量仍包含全部匹配记录")}</p> })}
                <dl class="detail-list attribution-unknown"><dt>{l.text("Reasoning effort", "推理强度")}</dt><dd>{l.text("Not recorded for these response tokens", "这些响应 Token 未记录此信息")}</dd><dt>{l.text("Fast / Standard", "Fast / Standard")}</dt><dd>{l.text("Not recorded for these response tokens", "这些响应 Token 未记录此信息")}</dd></dl>
                <p class="footnote">{l.text("All selected responses belong to the unrecorded effort and speed groups. These are the same response totals, not additional usage.", "所选响应均属于未记录推理强度与速度的分组。这些分组共用上述响应总量，并非额外用量。")}</p>
                <p class="notice">{integer(&result["undatedResponseCount"])}" "{l.text("undated response records in this source are excluded. Their position inside or outside the selected period is unknown.", "条来源内缺少发生时间的响应记录已排除。无法判断它们是否位于所选时间范围内。")}</p>
                <details class="data-details token-import-warnings"><summary>{l.text("Source import warning evidence", "来源导入警告证据")}</summary>
                    <p class="footnote">{l.text("Warning codes cover retained imports across this source, not verified coverage of the selected token dates.", "警告代码覆盖此来源的保留导入记录，不代表已验证的所选 Token 日期覆盖情况。")}</p>
                    <ul>{rows(&result["importWarnings"]["codes"]).into_iter().map(|code| view! { <li class="mono">{string(&code).to_owned()}</li> }).collect_view()}</ul>
                    {(result["importWarnings"]["truncated"] == true).then(|| view! { <p class="notice">{l.text("Import warning list truncated; additional warnings are unknown", "导入警告列表已截断；其他警告未知")}</p> })}
                    {rows(&result["importWarnings"]["codes"]).is_empty().then(|| view! { <p class="footnote">{l.text("No retained warning codes returned; this does not prove complete coverage", "未返回保留警告代码；这不能证明覆盖完整")}</p> })}
                </details>
                <p class="footnote">{l.text("Zero matches means no retained dated evidence, not zero historical usage. Up to 500 reported model groups; totals include all matching records. Cached input is part of input; reasoning output is part of output.", "零条匹配仅表示没有保留的带日期证据，不代表历史用量为零。最多显示 500 个报告模型分组；总量包含全部匹配记录。缓存输入属于输入，推理输出属于输出。")}</p>
            }.into_any() }}
            <p class="footnote quota-note">{l.text("A local source name is not a verified account binding. Partial response records and account quota may cover different activity. Tokens cannot determine included allowance, remaining tokens, API cost, or purchased credits. No per-message or effort/speed quota charge is inferred.", "本地来源名称不代表已验证的账户绑定。部分响应记录与账户额度的覆盖范围可能不同。Token 不能确定订阅额度、剩余 Token、API 费用或已购积分；不会推算逐条消息、推理强度或速度档位的额度扣减。")}</p>
        </section>
    }.into_any()
}
