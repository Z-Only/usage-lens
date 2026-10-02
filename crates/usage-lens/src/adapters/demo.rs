use super::AdapterError;
use crate::core::{CoreError, UsageStore};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};

pub fn seed_demo(store: &UsageStore, now_ms: i64) -> Result<(), AdapterError> {
    if store
        .list_sources()?
        .as_array()
        .is_none_or(|s| !s.is_empty())
    {
        return Err(AdapterError("demo_requires_empty_store"));
    }
    // create_source is deliberately first: a second seed is rejected, never merged.
    store.create_source(&json!({"id":"demo","displayName":"Synthetic local demo","mode":"demo","provider":"synthetic","coverageDescription":"Synthetic fixtures only. Not your account. Real account access and client integration have not been tested."}))?;
    store.update_settings(&json!({"contentCaptureEnabled":true}))?;
    let iso = |ms| {
        DateTime::<Utc>::from_timestamp_millis(ms)
            .unwrap_or_default()
            .to_rfc3339_opts(SecondsFormat::Millis, true)
    };
    let observed_at = iso(now_ms);
    let put = |method: &str, raw: Value, offset: i64| -> Result<(), CoreError> {
        store.ingest_observation(&json!({"sourceId":"demo","method":method,"raw":raw,"observedAt":iso(now_ms-offset),"adapterVersion":"synthetic-demo/0.1.0","schemaBaseline":"public-docs-2026-10-02"}))?;
        Ok(())
    };
    put(
        "account/read",
        json!({"account":{"type":"chatgpt","planType":"pro"},"requiresOpenaiAuth":true}),
        0,
    )?;
    let buckets: Vec<Value> = (0..30).filter(|i| *i != 9 && *i != 18).map(|i| json!({"startDate":&iso(now_ms-(29-i)*86400000)[..10],"tokens":12000+(i*76543)%140000})).collect();
    put(
        "account/usage/read",
        json!({"summary":{"lifetimeTokens":28473019,"peakDailyTokens":412800,"longestRunningTurnSec":784,"currentStreakDays":8,"longestStreakDays":19},"dailyUsageBuckets":buckets}),
        0,
    )?;
    for index in (0..6).rev() {
        put(
            "account/rateLimits/read",
            json!({"rateLimitsByLimitId":{
            "codex":{"limitId":"codex","limitName":"Codex plan","primary":{"usedPercent":68-index*9,"windowDurationMins":300,"resetsAt":now_ms/1000+7200},"secondary":{"usedPercent":42-index*3,"windowDurationMins":10080,"resetsAt":now_ms/1000+172800},"credits":{"hasCredits":false,"unlimited":false,"balance":null},"planType":"pro","normalModelSlug":"plan-default","spendControlReached":false,"rateLimitReachedType":null},
            "reserve":{"limitId":"reserve","limitName":"Reserve allowance","primary":{"usedPercent":12,"windowDurationMins":null,"resetsAt":null},"secondary":null,"credits":null,"planType":"pro"}},"ordinaryUsageAllowed":true,"rateLimitResetCredits":{"availableCount":2,"credits":[]}}),
            index * 3600000,
        )?;
    }
    let specs = [
        json!({"eventType":"user_prompt","evidenceType":"explicit_user_message","content":{"body":"Synthetic request: compare the usage observations and keep missing days unknown."}}),
        json!({"eventType":"tool_call","evidenceType":"explicit_tool_call","toolName":"read_file","content":{"toolArguments":{"file":"example.csv"},"toolResult":"Synthetic fixture: 28 daily rows","files":[{"name":"example.csv","content":"date,tokens\n2026-10-01,42000\n"}]}}),
        json!({"eventType":"skill_requested","evidenceType":"explicit_skill_input","skillName":"spreadsheets"}),
        json!({"eventType":"skill_loaded","evidenceType":"successful_skill_read","skillName":"spreadsheets"}),
        json!({"eventType":"skill_invoked","evidenceType":"explicit_execution_record","skillName":"spreadsheets"}),
        json!({"eventType":"assistant_visible_message","evidenceType":"explicit_assistant_visible_message","content":{"body":"Synthetic response: these observations cover only returned source-date buckets."}}),
        json!({"eventType":"tool_call","evidenceType":"explicit_tool_call","toolName":"web_search"}),
    ];
    for index in 0..56 {
        let mut event = specs[index % specs.len()].clone();
        for (key,value) in json!({"sourceId":"demo","eventId":format!("demo-event-{index}"),"sourceEventId":format!("synthetic-{index}"),"observedAt":observed_at,"occurredAt":iso(now_ms-index as i64*3600000),"collectorVersion":"synthetic-demo/0.1.0","model":if index%5==0 {None} else if index%3==0 {Some("demo-model-b")} else {Some("demo-model-a")},"sessionId":format!("demo-session-{}",index/7),"turnId":format!("demo-turn-{}",index/3)}).as_object().into_iter().flatten() { event[key] = value.clone(); }
        if event["eventType"] == "tool_call" {
            event["status"] = json!(if index % 6 == 0 { "error" } else { "success" });
            event["durationMs"] = json!(240 + index * 37);
        }
        store.ingest_event(&event)?;
    }
    Ok(())
}
