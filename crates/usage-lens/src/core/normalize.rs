//! Allowlisted projections of provider observations. Unknown fields are bounded and discarded.
use super::validation::{
    CoreResult, DAILY_BUCKETS, MAX_SAFE_INTEGER, QUOTA_BUCKETS, RAW_BYTES, check_size, date_label,
    decimal_integer, record,
};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub const SUMMARY_KEYS: [&str; 5] = [
    "lifetimeTokens",
    "peakDailyTokens",
    "longestRunningTurnSec",
    "currentStreakDays",
    "longestStreakDays",
];

pub fn cell(object: &Value, key: &str, parse: impl FnOnce(&Value) -> Option<Value>) -> Value {
    match object.as_object().and_then(|object| object.get(key)) {
        None => json!({"status":"omitted", "value":null}),
        Some(Value::Null) => json!({"status":"not_reported", "value":null}),
        Some(value) => match parse(value) {
            Some(value) => json!({"status":"reported", "value":value}),
            None => json!({"status":"invalid", "value":null}),
        },
    }
}

fn state(object: &Value, key: &str, valid: impl FnOnce(&Value) -> bool) -> &'static str {
    match object.get(key) {
        None => "omitted",
        Some(Value::Null) => "not_reported",
        Some(value) if valid(value) => "reported",
        _ => "invalid",
    }
}

fn text(value: &Value) -> Option<Value> {
    let text = value.as_str()?;
    (text.encode_utf16().count() <= 256 && !text.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}'))
        .then(|| value.clone())
}
fn boolean(value: &Value) -> Option<Value> {
    value.is_boolean().then(|| value.clone())
}
fn percent(value: &Value) -> Option<Value> {
    let number = value.as_f64()?;
    (number.is_finite() && number >= 0.0).then(|| value.clone())
}

/// Exact nonnegative counts, represented as decimal strings in every output.
/// Arbitrary-precision JSON integer lexemes replace the reference's BigInt path.
/// Decimal/exponent number spellings retain its safe-integer requirement.
pub fn count(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => {
            let valid = !s.is_empty()
                && s.len() <= 128
                && (s == "0" || !s.starts_with('0'))
                && s.bytes().all(|b| b.is_ascii_digit());
            valid.then(|| s.clone())
        }
        Value::Number(number) => {
            let s = number.to_string();
            let positive = s.strip_prefix('-').unwrap_or(&s);
            let integer = decimal_integer(positive, 128)?;
            if s.starts_with('-') && integer != "0" {
                return None;
            }
            if s.contains(['.', 'e', 'E'])
                && integer
                    .parse::<i64>()
                    .ok()
                    .is_none_or(|v| v > MAX_SAFE_INTEGER)
            {
                return None;
            }
            Some(integer)
        }
        _ => None,
    }
}

fn count_value(value: &Value) -> Option<Value> {
    count(value).map(Value::String)
}
fn day(value: &Value) -> Option<Value> {
    date_label(value).ok().map(Value::String)
}

fn safe_window(value: &Value) -> Option<Value> {
    value.is_object().then(|| {
        json!({
            "usedPercent":cell(value, "usedPercent", percent),
            "windowDurationMins":cell(value, "windowDurationMins", count_value),
            "resetsAt":cell(value, "resetsAt", count_value),
        })
    })
}
fn safe_credits(value: &Value) -> Option<Value> {
    value.is_object().then(|| {
        json!({
            "hasCredits":cell(value, "hasCredits", boolean),
            "unlimited":cell(value, "unlimited", boolean),
            "balance":cell(value, "balance", text),
        })
    })
}

fn raw_record(raw: &Value) -> CoreResult<()> {
    check_size(raw, RAW_BYTES)?;
    record(raw)?;
    Ok(())
}

pub fn normalize_account(raw: &Value) -> CoreResult<Value> {
    raw_record(raw)?;
    Ok(json!({
        "accountState":state(raw,"account",Value::is_object),
        "type":cell(&raw["account"],"type",text),
        "planType":cell(&raw["account"],"planType",text),
        "requiresOpenaiAuth":cell(raw,"requiresOpenaiAuth",boolean),
    }))
}

pub fn normalize_usage(raw: &Value) -> CoreResult<Value> {
    raw_record(raw)?;
    let summary: Map<String, Value> = SUMMARY_KEYS
        .iter()
        .map(|key| ((*key).to_owned(), cell(&raw["summary"], key, count_value)))
        .collect();
    Ok(json!({
        "summaryState":state(raw,"summary",Value::is_object),
        "summary":summary,
        "dailyUsageBuckets":cell(raw,"dailyUsageBuckets",|value| {
            let values = value.as_array()?;
            if values.len() > DAILY_BUCKETS { return None; }
            let mut days: HashMap<String,usize> = HashMap::new();
            for bucket in values {
                if let Ok(label) = date_label(&bucket["startDate"]) {
                    *days.entry(label).or_default() += 1;
                }
            }
            Some(Value::Array(values.iter().map(|bucket| {
                let mut start_date = cell(bucket,"startDate",day);
                if let Some(date) = start_date["value"].as_str()
                    && days.get(date).copied().unwrap_or_default() > 1
                {
                    start_date = json!({"status":"invalid","value":null});
                }
                json!({
                    "startDate":start_date,
                    "tokens":cell(bucket,"tokens",count_value),
                    "sourceTimezone":null,"timezoneStatus":"not_documented","model":null,
                })
            }).collect()))
        }),
        "semantics":"account_token_activity",
        "coverage":{
            "completeness":"unknown","missingDays":"unknown",
            "perModel":"not_provided","exactRemainingTokens":"not_provided",
        },
    }))
}

fn normalize_bucket(raw: &Value, key: Option<&str>, source_view: &str) -> Value {
    let limit_id = cell(raw, "limitId", text);
    let matches = if source_view == "multi" && limit_id["status"] == "reported" {
        Some(key == limit_id["value"].as_str())
    } else {
        None
    };
    json!({
        "bucketKey":key,"sourceView":source_view,
        "status":if raw.is_object() {"reported"} else {"invalid"},
        "limitId":limit_id,"keyMatchesLimitId":matches,
        "limitName":cell(raw,"limitName",text),
        "normalModelSlug":cell(raw,"normalModelSlug",text),
        "primary":cell(raw,"primary",safe_window),
        "secondary":cell(raw,"secondary",safe_window),
        "credits":cell(raw,"credits",safe_credits),
        "planType":cell(raw,"planType",text),
        "rateLimitReachedType":cell(raw,"rateLimitReachedType",text),
        "spendControlReached":cell(raw,"spendControlReached",boolean),
    })
}

pub fn normalize_rate_limits(raw: &Value) -> CoreResult<Value> {
    raw_record(raw)?;
    let multi = raw["rateLimitsByLimitId"].as_object();
    let valid_multi = multi.is_some_and(|m| {
        m.len() <= QUOTA_BUCKETS && m.keys().all(|k| k.encode_utf16().count() <= 256)
    });
    // An invalid present multi-map is not permission to substitute the legacy mirror.
    let fallback = raw["rateLimitsByLimitId"].is_null() && raw["rateLimits"].is_object();
    let buckets: Vec<Value> = if valid_multi {
        multi
            .unwrap()
            .iter()
            .map(|(key, value)| normalize_bucket(value, Some(key), "multi"))
            .collect()
    } else if fallback {
        vec![normalize_bucket(&raw["rateLimits"], None, "legacy")]
    } else {
        vec![]
    };
    Ok(json!({
        "buckets":buckets,
        "bucketView":if valid_multi {"multi"} else if fallback {"legacy_fallback"} else {"unavailable"},
        "multiBucketState":state(raw,"rateLimitsByLimitId", |_| valid_multi),
        "legacyBucketState":state(raw,"rateLimits",Value::is_object),
        "ordinaryUsageAllowed":cell(raw,"ordinaryUsageAllowed",boolean),
        "rateLimitResetCredits":cell(raw,"rateLimitResetCredits",|value| {
            value.is_object().then(|| json!({
                "availableCount":cell(value,"availableCount",count_value),
                "detailsState":state(value,"credits",Value::is_array),
                "detailRowsReturned":value["credits"].as_array().map(|rows| rows.len().to_string()),
            }))
        }),
        "coverage":{
            "completeness":"unknown","exactRemainingTokens":"not_provided","affordability":"not_inferred",
        },
    }))
}
