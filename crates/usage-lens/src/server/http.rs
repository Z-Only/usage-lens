//! Loopback-only HTTP facade. Local content is confined to explicitly local endpoints.
use crate::core::{CoreError, UsageStore};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("{code}")]
pub struct HttpError {
    pub code: &'static str,
    pub status: StatusCode,
}
impl HttpError {
    pub fn new(code: &'static str, status: StatusCode) -> Self {
        Self { code, status }
    }
}
impl From<CoreError> for HttpError {
    fn from(error: CoreError) -> Self {
        Self::new(error.code(), StatusCode::BAD_REQUEST)
    }
}
fn bad(code: &'static str) -> HttpError {
    HttpError::new(code, StatusCode::BAD_REQUEST)
}
#[derive(Clone)]
struct AppState {
    store: Arc<Mutex<UsageStore>>,
    host: String,
    origin: String,
}
/// Canonical HTTP origin intentionally omits the default :80 port.
pub fn canonical_origin(port: u16) -> String {
    if port == 80 {
        "http://127.0.0.1".to_owned()
    } else {
        format!("http://127.0.0.1:{port}")
    }
}
pub fn validate_host(host: Option<&str>) -> Result<(), HttpError> {
    if host.is_some_and(|h| h != "127.0.0.1") {
        return Err(HttpError::new("loopback_only", StatusCode::BAD_REQUEST));
    }
    Ok(())
}
pub fn router(store: UsageStore, port: u16) -> Router {
    let origin = canonical_origin(port);
    let host = origin.trim_start_matches("http://").to_owned();
    Router::new().fallback(handle).with_state(AppState {
        store: Arc::new(Mutex::new(store)),
        host,
        origin,
    })
}
fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    if headers.get_all(name).iter().count() != 1 {
        return None;
    }
    headers.get(name).and_then(|h| h.to_str().ok())
}
fn security_headers(response: &mut Response) {
    for (key, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        (
            "content-security-policy",
            "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        ),
        ("referrer-policy", "no-referrer"),
        ("cross-origin-resource-policy", "same-origin"),
        ("x-frame-options", "DENY"),
    ] {
        response
            .headers_mut()
            .insert(key, HeaderValue::from_static(value));
    }
}
async fn handle(State(state): State<AppState>, request: Request) -> Response {
    let mut response = match dispatch(state, request).await {
        Ok(response) => response,
        Err(error) => (
            error.status,
            axum::Json(json!({"error":{"code":error.code}})),
        )
            .into_response(),
    };
    security_headers(&mut response);
    response
}
fn validate_request(
    state: &AppState,
    request: &Request,
) -> Result<(String, Option<String>), HttpError> {
    let headers = request.headers();
    if header(headers, "host") != Some(&state.host) {
        return Err(HttpError::new("invalid_host", StatusCode::FORBIDDEN));
    }
    if headers.contains_key("origin") && header(headers, "origin") != Some(&state.origin) {
        return Err(HttpError::new("invalid_origin", StatusCode::FORBIDDEN));
    }
    if header(headers, "sec-fetch-site") == Some("cross-site") {
        return Err(HttpError::new("cross_site_request", StatusCode::FORBIDDEN));
    }
    let raw = request.uri().to_string();
    if !raw.starts_with('/')
        || raw.starts_with("//")
        || raw.len() > 4096
        || raw.bytes().any(|b| b == b'\\' || b < 32)
    {
        return Err(bad("invalid_path"));
    }
    let path = request.uri().path();
    let bytes = path.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            return Err(bad("invalid_path"));
        }
    }
    let decoded = percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .map_err(|_| bad("invalid_path"))?;
    if decoded.split('/').any(|p| p == "." || p == "..")
        || decoded.bytes().any(|b| b == b'\\' || b < 32)
    {
        return Err(bad("invalid_path"));
    }
    if request.method() != Method::GET && request.method() != Method::POST {
        return Err(HttpError::new(
            "method_not_allowed",
            StatusCode::METHOD_NOT_ALLOWED,
        ));
    }
    Ok((path.to_owned(), request.uri().query().map(str::to_owned)))
}
fn query(raw: Option<&str>, allowed: &[&str], required: &[&str]) -> Result<Value, HttpError> {
    let mut result = Map::new();
    for (key, value) in form_urlencoded::parse(raw.unwrap_or("").as_bytes()) {
        if !allowed.contains(&key.as_ref())
            || result.contains_key(key.as_ref())
            || value.chars().count() > 512
        {
            return Err(bad("invalid_query"));
        }
        let parsed = if key == "limit" || key == "maxAgeMs" {
            if value.is_empty() || value.len() > 10 || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad("invalid_query"));
            }
            json!(value.parse::<u64>().map_err(|_| bad("invalid_query"))?)
        } else {
            json!(value)
        };
        result.insert(key.into_owned(), parsed);
    }
    if required.iter().any(|k| {
        result
            .get(*k)
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
    }) {
        return Err(bad("missing_query"));
    }
    Ok(Value::Object(result))
}
fn only_keys(value: &Value, keys: &[&str]) -> Result<(), HttpError> {
    if value
        .as_object()
        .is_none_or(|v| v.keys().any(|k| !keys.contains(&k.as_str())))
    {
        return Err(bad("invalid_body"));
    }
    Ok(())
}
fn json_response(value: Value) -> Response {
    axum::Json(value).into_response()
}
async fn dispatch(state: AppState, request: Request) -> Result<Response, HttpError> {
    let (path, raw_query) = validate_request(&state, &request)?;
    if request.method() == Method::POST {
        if header(request.headers(), "origin") != Some(&state.origin)
            || header(request.headers(), "x-usage-lens-request") != Some("local-ui")
        {
            return Err(HttpError::new("csrf_required", StatusCode::FORBIDDEN));
        }
        if header(request.headers(), "content-type") != Some("application/json") {
            return Err(HttpError::new(
                "json_required",
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ));
        }
        let body =
            tokio::time::timeout(Duration::from_secs(5), to_bytes(request.into_body(), 16384))
                .await
                .map_err(|_| HttpError::new("request_timeout", StatusCode::REQUEST_TIMEOUT))?
                .map_err(|_| HttpError::new("body_too_large", StatusCode::PAYLOAD_TOO_LARGE))?;
        let value: Value = serde_json::from_slice(&body).map_err(|_| bad("invalid_json"))?;
        if !value.is_object() {
            return Err(bad("invalid_body"));
        }
        if raw_query.is_some_and(|q| !q.is_empty()) {
            return Err(bad("invalid_query"));
        }
        let store = state.store.lock().map_err(|_| bad("request_failed"))?;
        let result = match path.as_str() {
            "/api/settings" => {
                only_keys(
                    &value,
                    &["capturePaused", "contentCaptureEnabled", "retentionDays"],
                )?;
                store.update_settings(&value)?
            }
            "/api/delete" => {
                only_keys(&value, &["target", "sourceId", "confirmation"])?;
                if value["confirmation"] != "DELETE"
                    || (value["target"] != "all" && value["target"] != "content")
                    || value.get("sourceId").is_some_and(|v| !v.is_string())
                {
                    return Err(bad("confirmation_required"));
                }
                let mut scope = json!({});
                if let Some(id) = value.get("sourceId") {
                    scope["sourceId"] = id.clone();
                }
                if value["target"] == "all" {
                    store.clear_data(&scope)?
                } else {
                    store.clear_local_content(&scope)?
                }
            }
            "/api/retention" => {
                only_keys(&value, &["confirmation"])?;
                if value["confirmation"] != "APPLY RETENTION" {
                    return Err(bad("confirmation_required"));
                }
                store.apply_retention(&json!({}))?
            }
            _ => return Err(HttpError::new("not_found", StatusCode::NOT_FOUND)),
        };
        return Ok(json_response(result));
    }
    if path.starts_with("/api/") {
        let specs: HashMap<&str, (&[&str], &[&str])> = HashMap::from([
            ("/api/status", (&[][..], &[][..])),
            ("/api/settings", (&[][..], &[][..])),
            (
                "/api/health",
                (&["sourceId", "maxAgeMs"][..], &["sourceId"][..]),
            ),
            (
                "/api/overview",
                (&["sourceId", "maxAgeMs"][..], &["sourceId"][..]),
            ),
            (
                "/api/quota",
                (&["sourceId", "maxAgeMs"][..], &["sourceId"][..]),
            ),
            (
                "/api/daily",
                (
                    &["sourceId", "maxAgeMs", "fromDate", "toDate"][..],
                    &["sourceId", "fromDate", "toDate"][..],
                ),
            ),
            (
                "/api/quota/history",
                (
                    &["sourceId", "maxAgeMs", "cursor", "limit"][..],
                    &["sourceId"][..],
                ),
            ),
            (
                "/api/events/search",
                (
                    &["sourceId", "query", "limit"][..],
                    &["sourceId", "query"][..],
                ),
            ),
            (
                "/api/events/detail",
                (&["sourceId", "eventId"][..], &["sourceId", "eventId"][..]),
            ),
            (
                "/api/events",
                (
                    &[
                        "sourceId",
                        "fromDate",
                        "toDate",
                        "model",
                        "eventType",
                        "cursor",
                        "limit",
                    ][..],
                    &["sourceId"][..],
                ),
            ),
            (
                "/api/skill-summary",
                (
                    &["sourceId", "maxAgeMs", "fromDate", "toDate", "skillName"][..],
                    &["sourceId"][..],
                ),
            ),
            (
                "/api/skills",
                (
                    &[
                        "sourceId", "fromDate", "toDate", "model", "kind", "cursor", "limit",
                    ][..],
                    &["sourceId"][..],
                ),
            ),
            (
                "/api/tools",
                (
                    &["sourceId", "fromDate", "toDate", "model"][..],
                    &["sourceId"][..],
                ),
            ),
            (
                "/api/response-tokens",
                (
                    &["sourceId", "fromDate", "toDate", "model"][..],
                    &["sourceId"][..],
                ),
            ),
            (
                "/api/response-tokens/records",
                (
                    &["sourceId", "fromDate", "toDate", "model", "cursor", "limit"][..],
                    &["sourceId"][..],
                ),
            ),
        ]);
        let (allowed, required) = specs
            .get(path.as_str())
            .ok_or(HttpError::new("not_found", StatusCode::NOT_FOUND))?;
        let input = query(raw_query.as_deref(), allowed, required)?;
        let store = state.store.lock().map_err(|_| bad("request_failed"))?;
        let result = match path.as_str() {
            "/api/status" => store.get_status(),
            "/api/settings" => store.get_settings(),
            "/api/health" => store.get_health(&input),
            "/api/overview" => store.get_overview(&input),
            "/api/quota" => store.get_quota(&input),
            "/api/daily" => store.get_daily_usage(&input),
            "/api/quota/history" => store.get_quota_history(&input),
            "/api/events/search" => store.search_local_details(&input),
            "/api/events/detail" => store.get_local_event_detail(&input),
            "/api/events" => store.get_events(&input),
            "/api/skills" => store.get_skill_evidence(&input),
            "/api/skill-summary" => store.get_skill_summary(&input),
            "/api/tools" => store.get_tool_usage(&input),
            "/api/response-tokens" => store.get_response_token_usage(&input),
            _ => store.get_response_token_records(&input),
        }?;
        return Ok(json_response(result));
    }
    if raw_query.is_some_and(|q| !q.is_empty()) {
        return Err(HttpError::new("not_found", StatusCode::NOT_FOUND));
    }
    let name = if path == "/" {
        "/index.html"
    } else {
        path.as_str()
    };
    let (mime, bytes) =
        embedded_asset(name).ok_or(HttpError::new("not_found", StatusCode::NOT_FOUND))?;
    Ok(([("content-type", mime)], Body::from(bytes)).into_response())
}
/// Only exact build-time allowlisted files are served; there is no runtime filesystem path.
pub fn embedded_asset(path: &str) -> Option<(&'static str, &'static [u8])> {
    let assets: &[(&str, &str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));
    assets
        .iter()
        .find(|(name, _, _)| *name == path)
        .map(|(_, mime, bytes)| (*mime, *bytes))
}
