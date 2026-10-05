//! Bounded, source- and filter-scoped summaries of normalized local trace metadata.
use super::*;

const GROUP_LIMIT: usize = 500;
const REQUEST_KEYS: [&str; 3] = ["model", "reasoningEffort", "serviceTier"];
const STATUSES: [&str; 4] = ["completed", "failed", "cancelled", "incomplete"];
type SettingsKey = [(String, String); 3];

struct BoundedGroups<K, V> {
    values: BTreeMap<K, V>,
    truncated: bool,
}
impl<K: Ord, V: Default> Default for BoundedGroups<K, V> {
    fn default() -> Self {
        Self {
            values: BTreeMap::new(),
            truncated: false,
        }
    }
}
impl<K: Ord, V: Default> BoundedGroups<K, V> {
    fn get(&mut self, key: K) -> Option<&mut V> {
        if !self.values.contains_key(&key) && self.values.len() == GROUP_LIMIT {
            self.truncated = true;
            // The retained threshold only decreases. A key discarded at any point
            // cannot be retained later, so all retained groups have complete counts.
            if self
                .values
                .last_key_value()
                .is_some_and(|(last, _)| &key < last)
            {
                self.values.pop_last();
            } else {
                return None;
            }
        }
        Some(self.values.entry(key).or_default())
    }
}

#[derive(Default)]
struct ThreadTotals {
    total: Totals,
    first: Option<String>,
    last: Option<String>,
    statuses: [u64; 4],
}
impl ThreadTotals {
    fn add(&mut self, attempt: &Value) {
        self.total.add(attempt);
        let started = s(&attempt["startedAt"]);
        if self.first.as_deref().is_none_or(|first| started < first) {
            self.first = Some(started.to_owned());
        }
        if self.last.as_deref().is_none_or(|last| started > last) {
            self.last = Some(started.to_owned());
        }
        for (index, status) in STATUSES.iter().enumerate() {
            self.statuses[index] += u64::from(attempt["status"] == *status);
        }
    }
    fn value(&self, thread: &str) -> Value {
        let mut value = self.total.value();
        value["threadId"] = json!(thread);
        value["firstStartedAt"] = json!(self.first);
        value["lastStartedAt"] = json!(self.last);
        value["statusCounts"] = Value::Object(
            STATUSES
                .iter()
                .zip(self.statuses)
                .map(|(status, count)| ((*status).to_owned(), json!(count.to_string())))
                .collect(),
        );
        value
    }
}

#[derive(Default)]
pub(super) struct TraceInsights {
    threads: BoundedGroups<String, ThreadTotals>,
    settings: BoundedGroups<SettingsKey, Totals>,
    days: BoundedGroups<String, Totals>,
}
impl TraceInsights {
    pub(super) fn add(&mut self, attempt: &Value) {
        if let Some(total) = self.threads.get(s(&attempt["threadId"]).to_owned()) {
            total.add(attempt);
        }
        let settings = REQUEST_KEYS.map(|key| {
            let cell = &attempt["request"][key];
            (s(&cell["state"]).to_owned(), s(&cell["value"]).to_owned())
        });
        if let Some(total) = self.settings.get(settings) {
            total.add(attempt);
        }
        // startedAt is mandatory and normalized on import. Never fall back to
        // importedAt or fill absent dates with fabricated zero-count observations.
        if let Some(total) = self.days.get(s(&attempt["startedAt"])[..10].to_owned()) {
            total.add(attempt);
        }
    }
    pub(super) fn echo(self, result: &mut Value) {
        result["threadsTruncated"] = json!(self.threads.truncated);
        result["requestedSettingsTruncated"] = json!(self.settings.truncated);
        result["daysTruncated"] = json!(self.days.truncated);
        result["byThread"] = json!(
            self.threads
                .values
                .into_iter()
                .map(|(thread, total)| total.value(&thread))
                .collect::<Vec<_>>()
        );
        result["byRequestedSettings"] = json!(
            self.settings
                .values
                .into_iter()
                .map(|(settings, total)| {
                    let mut value = total.value();
                    value["request"] = Value::Object(
                        REQUEST_KEYS
                            .iter()
                            .zip(settings)
                            .map(|(key, (state, text))| {
                                let text = if state == "reported" {
                                    json!(text)
                                } else {
                                    Value::Null
                                };
                                ((*key).to_owned(), json!({"state":state,"value":text}))
                            })
                            .collect(),
                    );
                    value
                })
                .collect::<Vec<_>>()
        );
        result["byDay"] = json!(
            self.days
                .values
                .into_iter()
                .map(|(date, total)| {
                    let mut value = total.value();
                    value["date"] = json!(date);
                    value
                })
                .collect::<Vec<_>>()
        );
    }
}
