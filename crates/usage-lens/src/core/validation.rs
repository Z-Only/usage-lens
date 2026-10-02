//! Bounded, JSON-only validation. Errors never contain provider payloads.
use chrono::{NaiveDate, NaiveDateTime};
use regex::Regex;
use serde_json::{Map, Value};
use std::{fmt, sync::LazyLock};

pub const RAW_BYTES: usize = 2 * 1024 * 1024;
pub const CONTENT_BYTES: usize = 512 * 1024;
pub const BATCH_EVENTS: usize = 1000;
pub const DAILY_BUCKETS: usize = 10000;
pub const QUOTA_BUCKETS: usize = 128;
pub const QUERY_ROWS: i64 = 500;
pub const RANGE_DAYS: i64 = 3660;
pub const MAX_AGE_MS: i64 = 30 * 86400000;
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreError {
    InvalidInput,
    InputTooLarge,
    SourceExists,
    SourceNotFound,
    EventNotFound,
    CapturePaused,
    UnsupportedSchema,
    StoreClosed,
    ResponseTokenConflict,
    ImportedSourceRequired,
    StorageError,
}

impl CoreError {
    pub const fn new(code: &'static str) -> Self {
        // This constructor deliberately cannot carry arbitrary error text.
        match code.as_bytes() {
            b"invalid_input" => Self::InvalidInput,
            b"input_too_large" => Self::InputTooLarge,
            b"source_exists" => Self::SourceExists,
            b"source_not_found" => Self::SourceNotFound,
            b"event_not_found" => Self::EventNotFound,
            b"capture_paused" => Self::CapturePaused,
            b"unsupported_schema" => Self::UnsupportedSchema,
            b"store_closed" => Self::StoreClosed,
            b"response_token_conflict" => Self::ResponseTokenConflict,
            b"imported_source_required" => Self::ImportedSourceRequired,
            _ => Self::StorageError,
        }
    }

    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::InputTooLarge => "input_too_large",
            Self::SourceExists => "source_exists",
            Self::SourceNotFound => "source_not_found",
            Self::EventNotFound => "event_not_found",
            Self::CapturePaused => "capture_paused",
            Self::UnsupportedSchema => "unsupported_schema",
            Self::StoreClosed => "store_closed",
            Self::ResponseTokenConflict => "response_token_conflict",
            Self::ImportedSourceRequired => "imported_source_required",
            Self::StorageError => "storage_error",
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for CoreError {}
pub type CoreResult<T> = Result<T, CoreError>;

pub fn record(value: &Value) -> CoreResult<&Map<String, Value>> {
    value.as_object().ok_or(CoreError::InvalidInput)
}

pub fn exact_keys(value: &Value, allowed: &[&str]) -> CoreResult<()> {
    if record(value)?
        .keys()
        .all(|key| allowed.contains(&key.as_str()))
    {
        Ok(())
    } else {
        Err(CoreError::InvalidInput)
    }
}

pub fn bounded_text(value: &Value, max: usize) -> CoreResult<String> {
    let value = value.as_str().ok_or(CoreError::InvalidInput)?;
    if value.is_empty()
        || value.encode_utf16().count() > max
        || value.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}')
    {
        return Err(CoreError::InvalidInput);
    }
    Ok(value.to_owned())
}

pub fn identifier(value: &Value) -> CoreResult<String> {
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A[A-Za-z0-9][A-Za-z0-9._:@/+-]{0,159}\z").unwrap());
    checked_pattern(value, &PATTERN)
}

pub fn source_id(value: &Value) -> CoreResult<String> {
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A[A-Za-z0-9][A-Za-z0-9._:-]{0,79}\z").unwrap());
    checked_pattern(value, &PATTERN)
}

fn checked_pattern(value: &Value, pattern: &Regex) -> CoreResult<String> {
    match value.as_str() {
        Some(s) if pattern.is_match(s) => Ok(s.to_owned()),
        _ => Err(CoreError::InvalidInput),
    }
}

pub fn date_label(value: &Value) -> CoreResult<String> {
    static PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A[0-9]{4}-[0-9]{2}-[0-9]{2}\z").unwrap());
    let s = checked_pattern(value, &PATTERN)?;
    let date = NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|_| CoreError::InvalidInput)?;
    if date.format("%Y-%m-%d").to_string() != s {
        return Err(CoreError::InvalidInput);
    }
    Ok(s)
}

pub fn timestamp(value: &Value) -> CoreResult<String> {
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\A[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]{1,3})?Z\z")
            .unwrap()
    });
    let s = checked_pattern(value, &PATTERN)?;
    // Chrono accepts leap seconds, whereas the reference JavaScript Date does not.
    if &s[17..19] == "60" {
        return Err(CoreError::InvalidInput);
    }
    let parsed = NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.fZ")
        .map_err(|_| CoreError::InvalidInput)?;
    if parsed.format("%Y-%m-%dT%H:%M:%S").to_string() != s[..19] {
        return Err(CoreError::InvalidInput);
    }
    Ok(parsed.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
}

pub fn nullable_identifier(value: &Value) -> CoreResult<Option<String>> {
    if value.is_null() {
        Ok(None)
    } else {
        identifier(value).map(Some)
    }
}

pub fn enum_value(value: &Value, values: &[&str]) -> CoreResult<String> {
    match value.as_str() {
        Some(s) if values.contains(&s) => Ok(s.to_owned()),
        _ => Err(CoreError::InvalidInput),
    }
}

pub fn integer(value: &Value, min: i64, max: i64) -> CoreResult<i64> {
    let number = value.as_number().ok_or(CoreError::InvalidInput)?;
    let text = number.to_string();
    let (negative, magnitude) = match text.strip_prefix('-') {
        Some(v) => (true, v),
        None => (false, text.as_str()),
    };
    let magnitude = decimal_integer(magnitude, 16).ok_or(CoreError::InvalidInput)?;
    let mut number: i64 = magnitude.parse().map_err(|_| CoreError::InvalidInput)?;
    if negative {
        number = -number;
    }
    if !(-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&number) || number < min || number > max {
        return Err(CoreError::InvalidInput);
    }
    Ok(number)
}

/// Convert an unsigned JSON numeric spelling to an exact whole decimal number.
/// Bounded before allocation, including exponent spellings. No floating point.
pub(crate) fn decimal_integer(input: &str, max_digits: usize) -> Option<String> {
    let (mantissa, exponent) = match input.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i64>().ok()?),
        None => (input, 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty()
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let mut digits = format!("{whole}{fraction}");
    if digits.bytes().all(|b| b == b'0') {
        return Some("0".to_owned());
    }
    let shift = exponent.checked_sub(i64::try_from(fraction.len()).ok()?)?;
    if shift < 0 {
        let remove = usize::try_from(shift.checked_neg()?).ok()?;
        if remove >= digits.len() || !digits[digits.len() - remove..].bytes().all(|b| b == b'0') {
            return None;
        }
        digits.truncate(digits.len() - remove);
    }
    let stripped = digits.trim_start_matches('0');
    let append = if shift > 0 {
        usize::try_from(shift).ok()?
    } else {
        0
    };
    if stripped.len().checked_add(append)? > max_digits {
        return None;
    }
    let mut result = stripped.to_owned();
    result.extend(std::iter::repeat_n('0', append));
    Some(result)
}

pub fn check_size(value: &Value, max_bytes: usize) -> CoreResult<()> {
    fn visit(
        value: &Value,
        depth: usize,
        nodes: &mut usize,
        size: &mut usize,
        max: usize,
    ) -> CoreResult<()> {
        *nodes += 1;
        if depth > 32 || *nodes > 100_000 {
            return Err(CoreError::InputTooLarge);
        }
        match value {
            Value::String(s) => *size = size.saturating_add(s.len() + 2),
            Value::Number(n) => *size = size.saturating_add(n.to_string().len() + 1),
            Value::Bool(b) => *size = size.saturating_add(if *b { 5 } else { 6 }),
            Value::Null => *size = size.saturating_add(5),
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    *size = size.saturating_add(index.to_string().len() + 4);
                    visit(item, depth + 1, nodes, size, max)?;
                }
            }
            Value::Object(items) => {
                for (key, item) in items {
                    *size = size.saturating_add(key.len() + 4);
                    visit(item, depth + 1, nodes, size, max)?;
                }
            }
        }
        if *size > max {
            Err(CoreError::InputTooLarge)
        } else {
            Ok(())
        }
    }
    visit(value, 0, &mut 0, &mut 0, max_bytes)
}

pub fn date_range(from: &Value, to: &Value) -> CoreResult<(String, String)> {
    let from = date_label(from)?;
    let to = date_label(to)?;
    let start =
        NaiveDate::parse_from_str(&from, "%Y-%m-%d").map_err(|_| CoreError::InvalidInput)?;
    let end = NaiveDate::parse_from_str(&to, "%Y-%m-%d").map_err(|_| CoreError::InvalidInput)?;
    if !(0..=RANGE_DAYS).contains(&(end - start).num_days()) {
        return Err(CoreError::InvalidInput);
    }
    Ok((from, to))
}
