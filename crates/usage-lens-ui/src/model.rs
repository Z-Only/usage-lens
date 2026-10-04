//! Pure dashboard transitions. A request may commit only while its generation and source still match.
use chrono::{DateTime, Datelike, Days, NaiveDate, Utc};
use num_bigint::BigUint;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const EVENT_TYPES: [&str; 6] = [
    "user_prompt",
    "assistant_visible_message",
    "tool_call",
    "skill_requested",
    "skill_loaded",
    "skill_invoked",
];
pub const TOKEN_KEYS: [&str; 6] = [
    "inputTokens",
    "cachedInputTokens",
    "cacheWriteInputTokens",
    "outputTokens",
    "reasoningOutputTokens",
    "totalTokens",
];
pub const PAGES: [Page; 6] = [
    Page::Overview,
    Page::Activity,
    Page::Traces,
    Page::Quotas,
    Page::Skills,
    Page::Settings,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    Chinese,
}
impl Language {
    pub fn from_preference(value: &str) -> Self {
        if value == "zh" {
            Self::Chinese
        } else {
            Self::English
        }
    }
    pub fn code(self) -> &'static str {
        if self == Self::Chinese { "zh" } else { "en" }
    }
    pub fn html_lang(self) -> &'static str {
        if self == Self::Chinese { "zh-CN" } else { "en" }
    }
    pub fn text(self, en: &'static str, zh: &'static str) -> &'static str {
        if self == Self::Chinese { zh } else { en }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Overview,
    Activity,
    Traces,
    Quotas,
    Skills,
    Settings,
}
impl Page {
    pub fn key(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Activity => "activity",
            Self::Traces => "traces",
            Self::Quotas => "quotas",
            Self::Skills => "skills",
            Self::Settings => "settings",
        }
    }
    pub fn label(self, l: Language) -> &'static str {
        match self {
            Self::Overview => l.text("Overview", "概览"),
            Self::Activity => l.text("Activity", "活动"),
            Self::Traces => l.text("Traces", "追踪"),
            Self::Quotas => l.text("Quotas", "额度"),
            Self::Skills => l.text("Skills", "技能"),
            Self::Settings => l.text("Settings", "设置"),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    Status,
    Overview,
    Health,
    Recent,
    Activity,
    Skills,
    SkillSummary,
    History,
    ResponseUsage,
    ResponseRecords,
    TokenPeriod,
    Traces,
    TraceSummary,
    TraceDetail,
    Detail,
    Mutation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destructive {
    All,
    Content,
    Retention,
}
impl Destructive {
    pub fn required(self) -> &'static str {
        if self == Self::Retention {
            "APPLY RETENTION"
        } else {
            "DELETE"
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filters {
    pub from: String,
    pub to: String,
    pub event_type: String,
    pub model: String,
    pub skill_name: String,
    pub content_query: String,
}
impl Filters {
    pub fn valid(&self) -> bool {
        (self.from.is_empty() && self.to.is_empty())
            || self
                .date_span()
                .is_some_and(|days| (0..=3660).contains(&days))
    }
    pub fn skill_dates_valid(&self) -> bool {
        self.date_span()
            .is_some_and(|days| (0..366).contains(&days))
    }
    fn date_span(&self) -> Option<i64> {
        let parse = |value: &str| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()
                .filter(|date| date.format("%Y-%m-%d").to_string() == value)
        };
        Some((parse(&self.to)? - parse(&self.from)?).num_days())
    }
}
pub const TRACE_STATUSES: [&str; 4] = ["completed", "failed", "cancelled", "incomplete"];

/// Exact local trace scope. Empty draft fields are omitted, never trimmed or inferred.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TraceFilters {
    pub from: String,
    pub to: String,
    pub thread_id: String,
    pub status: String,
    pub requested_model: String,
    pub requested_reasoning_effort: String,
    pub requested_service_tier: String,
}
impl TraceFilters {
    pub fn values(&self) -> [(&'static str, &str); 7] {
        [
            ("fromDate", &self.from),
            ("toDate", &self.to),
            ("threadId", &self.thread_id),
            ("status", &self.status),
            ("requestedModel", &self.requested_model),
            ("requestedReasoningEffort", &self.requested_reasoning_effort),
            ("requestedServiceTier", &self.requested_service_tier),
        ]
    }
    pub fn validation_error(&self) -> Option<&'static str> {
        if !(Filters {
            from: self.from.clone(),
            to: self.to.clone(),
            ..Filters::default()
        })
        .valid()
        {
            return Some("invalid_trace_date_range");
        }
        if (!self.thread_id.is_empty() && !valid_trace_thread(&self.thread_id))
            || (!self.status.is_empty() && !TRACE_STATUSES.contains(&self.status.as_str()))
            || [
                &self.requested_model,
                &self.requested_reasoning_effort,
                &self.requested_service_tier,
            ]
            .into_iter()
            .any(|value| {
                value.encode_utf16().count() > 128
                    || value.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}')
            })
        {
            return Some("invalid_trace_filter");
        }
        None
    }
    pub fn matches(&self, response: &Value) -> bool {
        self.values()
            .into_iter()
            .all(|(key, expected)| string(&response[key]) == expected)
    }
}
pub fn valid_trace_thread(value: &str) -> bool {
    (1..=160).contains(&value.len())
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:@/+-".contains(&b))
}
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub slot: Slot,
    pub generation: u64,
    pub source: String,
    pub path: String,
    pub body: Option<Value>,
    pub append: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Remote {
    pub value: Value,
    pub loading: bool,
    pub error: String,
    pub generation: u64,
}
#[derive(Clone, Debug)]
pub enum Action {
    Refresh,
    Source(String),
    Navigate(Page),
    Load(Slot, bool),
    Select(Value),
    Close,
    Filter(&'static str, String),
    ApplyFilters,
    ApplyTokenPeriod,
    ApplyTraceFilters,
    TraceWeek(String),
    TraceThread { source: String, thread: String },
    ResetTraceFilters,
    ResetFilters,
    Drilldown(&'static str, String),
    SkillKind(String),
    Language(String),
    ToggleTheme,
    ChartRange(usize),
    Setting(&'static str, Value),
    SaveSettings,
    Confirm(Destructive),
    Confirmation(String),
    Perform,
}
#[derive(Clone, Debug)]
pub struct State {
    pub language: Language,
    pub dark: bool,
    pub page: Page,
    pub source: String,
    pub remotes: BTreeMap<Slot, Remote>,
    pub filters: Filters,
    pub applied_filters: Filters,
    pub token_period: Filters,
    pub applied_token_period: Filters,
    pub trace_filters: TraceFilters,
    pub applied_trace_filters: TraceFilters,
    pub kind: String,
    pub selected: Value,
    pub settings: Value,
    pub action: Option<Destructive>,
    pub confirmation: String,
    pub success: bool,
    pub chart_range: usize,
    next_generation: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            language: Language::English,
            dark: false,
            page: Page::Overview,
            source: String::new(),
            remotes: BTreeMap::new(),
            filters: Filters::default(),
            applied_filters: Filters::default(),
            token_period: Filters::default(),
            applied_token_period: Filters::default(),
            trace_filters: TraceFilters::default(),
            applied_trace_filters: TraceFilters::default(),
            kind: "requested".into(),
            selected: Value::Null,
            settings: json!({"capturePaused":false,"contentCaptureEnabled":false,"retentionDays":30}),
            action: None,
            confirmation: String::new(),
            success: false,
            chart_range: 7,
            next_generation: 0,
        }
    }
}
impl State {
    pub fn data(&self, slot: Slot) -> &Value {
        self.remotes
            .get(&slot)
            .map(|r| &r.value)
            .unwrap_or(&Value::Null)
    }
    pub fn loading(&self, slot: Slot) -> bool {
        self.remotes.get(&slot).is_some_and(|r| r.loading)
    }
    pub fn error(&self, slot: Slot) -> &str {
        self.remotes
            .get(&slot)
            .map(|r| r.error.as_str())
            .unwrap_or("")
    }
    pub fn active_source(&self) -> Value {
        rows(&self.data(Slot::Status)["sources"])
            .into_iter()
            .find(|s| string(&s["id"]) == self.source)
            .unwrap_or_default()
    }
    pub fn warnings(&self) -> Vec<String> {
        let mut result = BTreeSet::new();
        for v in [
            &self.data(Slot::Status)["warnings"],
            &self.data(Slot::Overview)["warnings"],
            &self.data(Slot::Overview)["usage"]["warnings"],
            &self.data(Slot::Overview)["quota"]["warnings"],
        ] {
            for w in rows(v) {
                if let Some(s) = w.as_str() {
                    result.insert(s.into());
                }
            }
        }
        result.into_iter().collect()
    }
    pub fn failures(&self) -> Vec<String> {
        ["account", "usage", "quota"]
            .into_iter()
            .filter_map(|k| {
                self.data(Slot::Overview)[k]["lastFailure"]["errorCode"]
                    .as_str()
                    .map(str::to_owned)
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn retry_action(&self, slot: Slot) -> Action {
        match (slot, self.error(slot)) {
            (Slot::Activity, "invalid_date_range") => Action::ApplyFilters,
            (Slot::TokenPeriod, "invalid_token_period") => Action::ApplyTokenPeriod,
            (
                Slot::Traces | Slot::TraceSummary,
                "invalid_trace_date_range" | "invalid_trace_filter",
            ) => Action::ApplyTraceFilters,
            _ => Action::Load(slot, false),
        }
    }
    pub fn busy(&self) -> bool {
        self.loading(Slot::Mutation)
    }
    pub fn confirm_enabled(&self) -> bool {
        !self.busy()
            && self
                .action
                .is_some_and(|a| self.confirmation == a.required())
    }
    fn invalidate(&mut self, slot: Slot) {
        self.remotes.remove(&slot);
    }
    fn clear_source(&mut self) {
        self.remotes
            .retain(|slot, _| *slot == Slot::Status || *slot == Slot::Mutation);
        self.selected = Value::Null;
        self.action = None;
        self.confirmation.clear();
        self.success = false;
    }
    fn request(&mut self, slot: Slot, append: bool) -> Request {
        self.next_generation += 1;
        let cursor = if append {
            string(&self.data(slot)["nextCursor"]).to_owned()
        } else {
            String::new()
        };
        let (endpoint, mut query) = match slot {
            Slot::Status => ("status", vec![]),
            Slot::Overview => ("overview", vec![]),
            Slot::Health => ("health", vec![("maxAgeMs", "900000".into())]),
            Slot::Recent => ("events", vec![("limit", "3".into())]),
            Slot::Activity => (
                if self.applied_filters.content_query.is_empty() {
                    "events"
                } else {
                    "events/search"
                },
                vec![
                    ("limit", "50".into()),
                    ("fromDate", self.applied_filters.from.clone()),
                    ("toDate", self.applied_filters.to.clone()),
                    ("eventType", self.applied_filters.event_type.clone()),
                    ("model", self.applied_filters.model.clone()),
                    ("query", self.applied_filters.content_query.clone()),
                ],
            ),
            Slot::Skills => (
                "skills",
                vec![("kind", self.kind.clone()), ("limit", "100".into())],
            ),
            Slot::SkillSummary => (
                "skill-summary",
                vec![
                    ("fromDate", self.filters.from.clone()),
                    ("toDate", self.filters.to.clone()),
                    ("skillName", self.filters.skill_name.clone()),
                ],
            ),
            Slot::History => ("quota/history", vec![("limit", "20".into())]),
            Slot::ResponseUsage => ("response-tokens", vec![]),
            Slot::ResponseRecords => ("response-tokens/records", vec![("limit", "20".into())]),
            Slot::TokenPeriod => (
                "response-tokens/period",
                vec![
                    ("fromDate", self.applied_token_period.from.clone()),
                    ("toDate", self.applied_token_period.to.clone()),
                ],
            ),
            Slot::Traces | Slot::TraceSummary => (
                if slot == Slot::Traces {
                    "traces"
                } else {
                    "traces/summary"
                },
                self.applied_trace_filters
                    .values()
                    .into_iter()
                    .map(|(key, value)| (key, value.to_owned()))
                    .chain([(
                        "limit",
                        if slot == Slot::Traces {
                            "20".into()
                        } else {
                            String::new()
                        },
                    )])
                    .collect(),
            ),
            Slot::TraceDetail => (
                "traces/detail",
                vec![("attemptId", string(&self.selected["attemptId"]).into())],
            ),
            Slot::Detail => (
                "events/detail",
                vec![("eventId", string(&self.selected["eventId"]).into())],
            ),
            Slot::Mutation => ("settings", vec![]),
        };
        if slot != Slot::Status && slot != Slot::Mutation {
            query.push(("sourceId", self.source.clone()));
        }
        query.push(("cursor", cursor));
        let path = api_path(endpoint, &query);
        let current = self.remotes.entry(slot).or_default();
        current.loading = true;
        current.error.clear();
        current.generation = self.next_generation;
        Request {
            slot,
            generation: self.next_generation,
            source: self.source.clone(),
            path,
            body: None,
            append,
        }
    }
    fn load_page(&mut self) -> Vec<Request> {
        let slot = match self.page {
            Page::Activity => Slot::Activity,
            Page::Traces => Slot::Traces,
            Page::Skills => Slot::Skills,
            Page::Quotas => Slot::History,
            _ => return vec![],
        };
        if self.source.is_empty() {
            vec![]
        } else {
            let mut requests = self.dispatch(Action::Load(slot, false));
            if self.page == Page::Traces {
                requests.extend(self.dispatch(Action::Load(Slot::TraceSummary, false)));
            }
            if self.page == Page::Skills && self.filters.skill_dates_valid() {
                requests.push(self.request(Slot::SkillSummary, false));
            }
            if self.page == Page::Quotas && self.applied_token_period.skill_dates_valid() {
                requests.push(self.request(Slot::TokenPeriod, false));
            }
            requests
        }
    }
    pub fn dispatch(&mut self, action: Action) -> Vec<Request> {
        match action {
            Action::Refresh => {
                for (slot, remote) in &mut self.remotes {
                    if *slot != Slot::Mutation {
                        remote.loading = false;
                    }
                }
                vec![self.request(Slot::Status, false)]
            }
            Action::Source(id) => {
                if self.busy() {
                    return vec![];
                }
                self.source = id;
                self.clear_source();
                vec![self.request(Slot::Status, false)]
            }
            Action::Navigate(page) => {
                if self.busy() {
                    return vec![];
                }
                self.page = page;
                self.selected = Value::Null;
                self.action = None;
                self.invalidate(Slot::Detail);
                self.invalidate(Slot::TraceDetail);
                if page != Page::Traces {
                    for slot in [Slot::Traces, Slot::TraceSummary] {
                        if let Some(remote) = self.remotes.get_mut(&slot) {
                            remote.loading = false;
                        }
                    }
                }
                if page == Page::Activity {
                    self.dispatch(Action::ApplyFilters)
                } else {
                    self.load_page()
                }
            }
            Action::Load(slot, more) => {
                if (slot != Slot::Status && slot != Slot::Mutation && self.source.is_empty())
                    || (more
                        && (self.loading(slot)
                            || string(&self.data(slot)["nextCursor"]).is_empty()))
                {
                    return vec![];
                }
                if slot == Slot::Activity && !self.applied_filters.valid() {
                    self.invalidate(slot);
                    self.remotes.entry(slot).or_default().error = "invalid_date_range".into();
                    return vec![];
                }
                if matches!(slot, Slot::Traces | Slot::TraceSummary)
                    && let Some(error) = self.applied_trace_filters.validation_error()
                {
                    self.invalidate(slot);
                    self.remotes.entry(slot).or_default().error = error.into();
                    return vec![];
                }
                if slot == Slot::TokenPeriod && !self.applied_token_period.skill_dates_valid() {
                    self.invalidate(slot);
                    self.remotes.entry(slot).or_default().error = "invalid_token_period".into();
                    return vec![];
                }
                if slot == Slot::SkillSummary && !self.filters.skill_dates_valid() {
                    self.invalidate(slot);
                    self.remotes.entry(slot).or_default().error = "invalid_skill_date_range".into();
                    return vec![];
                }
                vec![self.request(slot, more)]
            }
            Action::Select(event) => {
                if string(&event["sourceId"]) != self.source {
                    return vec![];
                }
                self.selected = event;
                self.invalidate(Slot::Detail);
                self.invalidate(Slot::TraceDetail);
                let slot = if self.selected["attemptId"].is_string() {
                    Slot::TraceDetail
                } else {
                    Slot::Detail
                };
                vec![self.request(slot, false)]
            }
            Action::Close => {
                if !self.busy() {
                    self.selected = Value::Null;
                    self.action = None;
                    self.confirmation.clear();
                    self.invalidate(Slot::Detail);
                    self.invalidate(Slot::TraceDetail);
                }
                vec![]
            }
            Action::Filter(field, value) => {
                match field {
                    "from" => self.filters.from = value,
                    "to" => self.filters.to = value,
                    "eventType" => self.filters.event_type = value,
                    "model" => self.filters.model = value,
                    "skillName" => self.filters.skill_name = value,
                    "query" => self.filters.content_query = value,
                    "tokenFrom" => self.token_period.from = value,
                    "tokenTo" => self.token_period.to = value,
                    "traceFrom" => self.trace_filters.from = value,
                    "traceTo" => self.trace_filters.to = value,
                    "traceThread" => self.trace_filters.thread_id = value,
                    "traceStatus" => self.trace_filters.status = value,
                    "traceModel" => self.trace_filters.requested_model = value,
                    "traceEffort" => self.trace_filters.requested_reasoning_effort = value,
                    "traceTier" => self.trace_filters.requested_service_tier = value,
                    _ => {}
                }
                if ["from", "to", "skillName"].contains(&field) {
                    self.invalidate(Slot::SkillSummary);
                }
                vec![]
            }
            Action::TraceWeek(now) => {
                let Some(filters) = utc_week(&now) else {
                    return vec![];
                };
                self.trace_filters.from = filters.from;
                self.trace_filters.to = filters.to;
                self.dispatch(Action::ApplyTraceFilters)
            }
            Action::TraceThread { source, thread } => {
                if self.busy()
                    || source.is_empty()
                    || source != self.source
                    || !valid_trace_thread(&thread)
                {
                    return vec![];
                }
                self.page = Page::Traces;
                self.trace_filters = self.applied_trace_filters.clone();
                self.trace_filters.thread_id = thread;
                self.dispatch(Action::ApplyTraceFilters)
            }
            Action::ResetTraceFilters => {
                self.trace_filters = TraceFilters::default();
                self.dispatch(Action::ApplyTraceFilters)
            }
            Action::ApplyTraceFilters => {
                self.selected = Value::Null;
                self.invalidate(Slot::TraceDetail);
                self.applied_trace_filters = self.trace_filters.clone();
                self.invalidate(Slot::Traces);
                self.invalidate(Slot::TraceSummary);
                let mut jobs = self.dispatch(Action::Load(Slot::Traces, false));
                jobs.extend(self.dispatch(Action::Load(Slot::TraceSummary, false)));
                jobs
            }
            Action::ApplyTokenPeriod => {
                if self.source.is_empty() {
                    return vec![];
                }
                self.invalidate(Slot::TokenPeriod);
                self.applied_token_period = self.token_period.clone();
                if !self.token_period.skill_dates_valid() {
                    self.remotes.entry(Slot::TokenPeriod).or_default().error =
                        "invalid_token_period".into();
                    return vec![];
                }
                vec![self.request(Slot::TokenPeriod, false)]
            }
            Action::ApplyFilters => {
                if self.page == Page::Skills {
                    return self.dispatch(Action::Load(Slot::SkillSummary, false));
                }
                if self.source.is_empty() {
                    return vec![];
                }
                self.applied_filters = self.filters.clone();
                if !self.filters.valid() {
                    self.invalidate(Slot::Activity);
                    self.remotes.entry(Slot::Activity).or_default().error =
                        "invalid_date_range".into();
                    vec![]
                } else {
                    self.invalidate(Slot::Activity);
                    vec![self.request(Slot::Activity, false)]
                }
            }
            Action::ResetFilters => {
                self.filters = Filters::default();
                self.invalidate(Slot::SkillSummary);
                self.applied_filters = Filters::default();
                self.invalidate(Slot::Activity);
                if self.page == Page::Skills || self.source.is_empty() {
                    vec![]
                } else {
                    vec![self.request(Slot::Activity, false)]
                }
            }
            Action::Drilldown(field, value) => {
                if self.busy()
                    || self.source.is_empty()
                    || value.is_empty()
                    || !["eventType", "model"].contains(&field)
                {
                    return vec![];
                }
                self.filters = Filters::default();
                self.dispatch(Action::Filter(field, value));
                self.page = Page::Activity;
                self.selected = Value::Null;
                self.action = None;
                self.confirmation.clear();
                self.invalidate(Slot::Detail);
                self.dispatch(Action::ApplyFilters)
            }
            Action::SkillKind(kind) => {
                if !["requested", "loaded", "invoked"].contains(&kind.as_str()) {
                    return vec![];
                }
                self.kind = kind;
                self.invalidate(Slot::Skills);
                vec![self.request(Slot::Skills, false)]
            }
            Action::Language(value) => {
                self.language = Language::from_preference(&value);
                vec![]
            }
            Action::ToggleTheme => {
                self.dark = !self.dark;
                vec![]
            }
            Action::ChartRange(range) => {
                if [7, 30, 90].contains(&range) {
                    self.chart_range = range;
                }
                vec![]
            }
            Action::Setting(key, value) => {
                if ["capturePaused", "contentCaptureEnabled", "retentionDays"].contains(&key)
                    && !self.busy()
                {
                    self.settings[key] = value;
                    self.success = false;
                }
                vec![]
            }
            Action::SaveSettings => {
                if self.busy() {
                    return vec![];
                }
                let days = self.settings["retentionDays"].as_u64().unwrap_or(0);
                if !(1..=3650).contains(&days) {
                    self.remotes.entry(Slot::Mutation).or_default().error =
                        "invalid_retention_days".into();
                    return vec![];
                }
                self.success = false;
                let mut r = self.request(Slot::Mutation, false);
                r.body = Some(self.settings.clone());
                vec![r]
            }
            Action::Confirm(action) => {
                if self.busy() || (self.source.is_empty() && action != Destructive::Retention) {
                    return vec![];
                }
                self.action = Some(action);
                self.confirmation.clear();
                self.invalidate(Slot::Mutation);
                vec![]
            }
            Action::Confirmation(value) => {
                if !self.busy() {
                    self.confirmation = value;
                }
                vec![]
            }
            Action::Perform => {
                if !self.confirm_enabled() {
                    return vec![];
                }
                let action = self.action.expect("validated confirmation");
                let mut r = self.request(Slot::Mutation, false);
                r.path = format!(
                    "/api/{}",
                    if action == Destructive::Retention {
                        "retention"
                    } else {
                        "delete"
                    }
                );
                r.body = Some(if action == Destructive::Retention {
                    json!({"confirmation":self.confirmation})
                } else {
                    json!({"target":if action == Destructive::All {"all"} else {"content"}, "sourceId":self.source, "confirmation":self.confirmation})
                });
                vec![r]
            }
        }
    }
    pub fn accepts(&self, request: &Request) -> bool {
        self.remotes
            .get(&request.slot)
            .is_some_and(|remote| remote.generation == request.generation && remote.loading)
            && (request.slot == Slot::Status
                || request.slot == Slot::Mutation
                || self.source == request.source)
    }
    pub fn complete(&mut self, request: &Request, response: Result<Value, String>) -> Vec<Request> {
        if !self.accepts(request) {
            return vec![];
        }
        let current = self
            .remotes
            .get_mut(&request.slot)
            .expect("validated request");
        current.loading = false;
        let mut value = match response {
            Ok(v) => v,
            Err(error) => {
                current.error = error;
                return vec![];
            }
        };
        if request.slot != Slot::Status && request.slot != Slot::Mutation {
            let response_source = if request.slot == Slot::TraceSummary {
                &request.source
            } else if request.slot == Slot::TraceDetail {
                string(&value["attempt"]["sourceId"])
            } else if request.slot == Slot::Detail {
                string(&value["event"]["sourceId"])
            } else {
                string(&value["source"]["id"])
            };
            if response_source != request.source {
                current.error = "source_mismatch".into();
                return vec![];
            }
            if matches!(request.slot, Slot::Traces | Slot::TraceSummary)
                && !self.applied_trace_filters.matches(&value)
            {
                current.error = "filter_mismatch".into();
                return vec![];
            }
            if request.slot == Slot::TraceDetail
                && string(&value["attempt"]["attemptId"]) != string(&self.selected["attemptId"])
            {
                current.error = "attempt_mismatch".into();
                return vec![];
            }
            if request.slot == Slot::TokenPeriod
                && (string(&value["fromDate"]) != self.applied_token_period.from
                    || string(&value["toDate"]) != self.applied_token_period.to)
            {
                current.error = "filter_mismatch".into();
                return vec![];
            }
            if request.slot == Slot::SkillSummary
                && (string(&value["fromDate"]) != self.filters.from
                    || string(&value["toDate"]) != self.filters.to
                    || string(&value["skillName"]) != self.filters.skill_name)
            {
                current.error = "filter_mismatch".into();
                return vec![];
            }
            if request.slot == Slot::Detail
                && string(&value["event"]["eventId"]) != string(&self.selected["eventId"])
            {
                current.error = "event_mismatch".into();
                return vec![];
            }
        }
        if request.append {
            let key = match request.slot {
                Slot::History => "observations",
                Slot::ResponseRecords => "records",
                Slot::Traces => "attempts",
                _ => "events",
            };
            let mut combined = rows(&current.value[key]);
            combined.extend(rows(&value[key]));
            value[key] = Value::Array(combined);
        }
        current.value = value;
        match request.slot {
            Slot::Status => {
                self.settings = self.data(Slot::Status)["settings"].clone();
                let sources = rows(&self.data(Slot::Status)["sources"]);
                let id = sources
                    .iter()
                    .find(|s| string(&s["id"]) == self.source)
                    .or(sources.first())
                    .map(|s| string(&s["id"]).to_owned())
                    .unwrap_or_default();
                if id != self.source {
                    self.source = id;
                    self.clear_source();
                }
                if self.source.is_empty() {
                    self.clear_source();
                    return vec![];
                }
                let mut next = vec![
                    self.request(Slot::Overview, false),
                    self.request(Slot::Recent, false),
                    self.request(Slot::Health, false),
                ];
                next.extend(self.load_page());
                next
            }
            Slot::Overview
                if string(&self.data(Slot::Overview)["source"]["mode"]) == "imported" =>
            {
                vec![self.request(Slot::ResponseUsage, false)]
            }
            Slot::Mutation => {
                self.action = None;
                self.confirmation.clear();
                self.success = true;
                self.selected = Value::Null;
                self.remotes
                    .retain(|slot, _| *slot == Slot::Status || *slot == Slot::Mutation);
                vec![self.request(Slot::Status, false)]
            }
            _ => vec![],
        }
    }
}

/// Calendar preset from the browser clock; independent of provider quota/reset windows.
pub fn utc_week(now: &str) -> Option<Filters> {
    let date = DateTime::parse_from_rfc3339(now)
        .ok()?
        .with_timezone(&Utc)
        .date_naive();
    let from =
        date.checked_sub_days(Days::new(u64::from(date.weekday().num_days_from_monday())))?;
    let to = from.checked_add_days(Days::new(6))?;
    Some(Filters {
        from: from.to_string(),
        to: to.to_string(),
        ..Filters::default()
    })
}

pub fn string(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}
pub fn rows(value: &Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}
pub fn reported(cell: &Value) -> &Value {
    if cell["status"] == "reported" {
        &cell["value"]
    } else {
        &Value::Null
    }
}
pub fn cell_text(cell: &Value) -> String {
    match reported(cell) {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => "—".into(),
    }
}
pub fn integer(value: &Value) -> String {
    let s = string(value);
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return "—".into();
    }
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
pub fn count_cell(value: &Value) -> String {
    integer(reported(value))
}
pub fn label(value: &Value) -> String {
    string(value).replace('_', " ")
}
pub fn display_value(value: &Value, l: Language) -> String {
    if value.is_null() {
        l.text("Unknown", "未知").into()
    } else if let Some(s) = value.as_str() {
        s.into()
    } else {
        value.to_string()
    }
}
/// Health is a view of stored evidence, never a live access or completeness check.
pub fn health_state_text(value: &Value, l: Language) -> &'static str {
    match string(value) {
        "available" => l.text("Available", "可用"),
        "missing" => l.text("Missing", "缺失"),
        "observed" => l.text("Observed", "已观测"),
        "unsupported" => l.text("Unsupported", "不支持"),
        "fresh" => l.text("Within age threshold", "在时效阈值内"),
        "stale" => l.text("Stale", "已过期"),
        "future" => l.text("Future timestamp", "未来时间戳"),
        "recent" => l.text("Recent", "近期"),
        _ => l.text("Unknown", "未知"),
    }
}
pub fn health_count(value: &Value, l: Language) -> String {
    let count = integer(value);
    if count == "—" {
        l.text("Unknown", "未知").into()
    } else {
        count
    }
}
pub fn health_timestamp(value: &Value, l: Language) -> String {
    value
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(l.text("Unknown", "未知"))
        .into()
}
pub fn api_path(endpoint: &str, query: &[(&str, String)]) -> String {
    let pairs = query
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>();
    format!(
        "/api/{endpoint}{}",
        if pairs.is_empty() {
            String::new()
        } else {
            format!("?{}", pairs.join("&"))
        }
    )
}
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
pub fn chart_buckets(source: &Value, count: usize) -> Vec<Value> {
    let mut map = BTreeMap::new();
    for bucket in rows(reported(&source["dailyUsageBuckets"])) {
        if let Ok(date) =
            NaiveDate::parse_from_str(string(reported(&bucket["startDate"])), "%Y-%m-%d")
        {
            map.insert(date, bucket);
        }
    }
    let (Some(first), Some(last)) = (map.keys().next().copied(), map.keys().next_back().copied())
    else {
        return vec![];
    };
    if count == 0 {
        return vec![];
    }
    let mut date = first.max(
        last.checked_sub_days(Days::new(count.saturating_sub(1) as u64))
            .unwrap_or(first),
    );
    let mut result = vec![];
    loop {
        result.push(map.get(&date).cloned().unwrap_or_else(|| json!({"startDate":{"status":"reported","value":date.to_string()},"tokens":{"status":"not_reported","value":null}})));
        if date == last {
            break;
        }
        // We already stopped at `last`, so a strictly later valid date exists.
        date = date.succ_opt().expect("date precedes final valid date");
    }
    result
}
pub fn unknown_date_buckets(source: &Value) -> Vec<Value> {
    rows(reported(&source["dailyUsageBuckets"]))
        .into_iter()
        .filter(|b| {
            NaiveDate::parse_from_str(string(reported(&b["startDate"])), "%Y-%m-%d").is_err()
        })
        .collect()
}
pub fn bar_percent(cell: &Value, buckets: &[Value]) -> f64 {
    let Some(value) = BigUint::parse_bytes(string(reported(cell)).as_bytes(), 10) else {
        return 0.0;
    };
    let max = buckets
        .iter()
        .filter_map(|b| BigUint::parse_bytes(string(reported(&b["tokens"])).as_bytes(), 10))
        .max()
        .unwrap_or_default();
    if max == BigUint::default() {
        return 0.0;
    }
    let ratio = value * BigUint::from(10000u32) / max;
    ratio.to_string().parse::<f64>().unwrap_or(0.0) / 100.0
}
pub fn quota_windows(observation: &Value, full: bool) -> Vec<(String, &'static str, Value)> {
    let mut result = vec![];
    for bucket in rows(&observation["data"]["buckets"]) {
        for kind in ["primary", "secondary"] {
            let name = if reported(&bucket["limitName"]).is_string() {
                cell_text(&bucket["limitName"])
            } else if bucket["bucketKey"].is_string() {
                string(&bucket["bucketKey"]).into()
            } else {
                "Unknown bucket".into()
            };
            result.push((name, kind, bucket[kind].clone()));
        }
    }
    if !full {
        result.truncate(2);
    }
    result
}
#[derive(Clone, Copy, Debug)]
pub enum InputAction {
    Language,
    Source,
    ChartRange,
    Filter(&'static str),
    RetentionDays,
    Confirmation,
}
impl InputAction {
    pub fn action(self, value: String) -> Action {
        match self {
            Self::Language => Action::Language(value),
            Self::Source => Action::Source(value),
            Self::ChartRange => Action::ChartRange(value.parse().unwrap_or(7)),
            Self::Filter(key) => Action::Filter(key, value),
            Self::RetentionDays => {
                Action::Setting("retentionDays", json!(value.parse::<u32>().unwrap_or(0)))
            }
            Self::Confirmation => Action::Confirmation(value),
        }
    }
}

impl Action {
    /// Draft input must not recreate the focused field. Dialog changes leave their opener mounted.
    pub fn repaint(&self) -> (bool, bool) {
        let modal = !matches!(
            self,
            Self::Filter(..) | Self::Setting(..) | Self::Confirmation(..)
        );
        let main = modal
            && !matches!(
                self,
                Self::Select(..)
                    | Self::Close
                    | Self::Confirm(..)
                    | Self::Load(Slot::Detail | Slot::TraceDetail, _)
            );
        (main, modal)
    }
}
pub fn response_result(ok: bool, data: Value) -> Result<Value, String> {
    if ok {
        Ok(data)
    } else {
        Err(data["error"]["code"]
            .as_str()
            .unwrap_or("request_failed")
            .to_owned())
    }
}
