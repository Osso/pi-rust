//! Cache-domain values. All user-controlled text retains UTF-16 code units.
use indexmap::IndexMap;
use pi_js_compat::js_json::{self, JsString, JsValue};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
#[serde(transparent)]
pub struct Dictionary<T>(IndexMap<JsString, T>);

impl<T> Default for Dictionary<T> {
    fn default() -> Self {
        Self(IndexMap::new())
    }
}
impl<T> Dictionary<T> {
    pub fn new() -> Self {
        Self::default()
    }
}
impl<T> std::ops::Deref for Dictionary<T> {
    type Target = IndexMap<JsString, T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T> std::ops::DerefMut for Dictionary<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl<T> IntoIterator for Dictionary<T> {
    type Item = (JsString, T);
    type IntoIter = indexmap::map::IntoIter<JsString, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
impl<T: Serialize> Serialize for Dictionary<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let object = self
            .0
            .iter()
            .map(|(key, value)| js_json::to_value(value).map(|value| (key.clone(), value)))
            .collect::<Result<_, _>>()
            .map_err(serde::ser::Error::custom)?;
        JsValue::Object(object).serialize(serializer)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub timestamp: JsString,
    pub session_id: JsString,
    pub project: JsString,
    pub cwd: JsString,
    pub models: Dictionary<Dictionary<ModelUsage>>,
    pub user_msgs: f64,
    pub tool_results: f64,
    pub compaction_count: f64,
    pub compacted_tokens: f64,
    pub model_changes: f64,
    pub thinking_level_count: Dictionary<f64>,
    pub skills: Dictionary<SkillUsage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    pub provider: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<JsString>,
    pub calls: f64,
    pub usage: JsValue,
    pub asst_msgs: f64,
    pub tools: Dictionary<f64>,
    pub languages: Dictionary<LanguageUsage>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct LanguageUsage {
    pub lines: f64,
    pub edits: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SkillUsage {
    pub usage: JsValue,
    pub calls: f64,
}

pub fn number(value: &JsValue, key: &str) -> f64 {
    value.get(key).and_then(JsValue::as_f64).unwrap_or(f64::NAN)
}

pub fn text(value: &JsValue, key: &str) -> JsString {
    value.get(key).and_then(JsValue::as_string).cloned().unwrap_or_default()
}

pub fn merge_usage(left: &JsValue, right: &JsValue) -> JsValue {
    let cost = JsValue::object(["total", "cacheRead", "cacheWrite", "input", "output"].map(|key| {
        (
            key,
            JsValue::Number(number(&left["cost"], key) + number(&right["cost"], key)),
        )
    }));
    let mut result = JsValue::object(
        ["input", "output", "cacheRead", "cacheWrite", "totalTokens"]
            .map(|key| (key, JsValue::Number(number(left, key) + number(right, key)))),
    );
    result
        .as_object_mut()
        .expect("constructed object")
        .insert("cost".into(), cost);
    result
}

pub fn empty_usage() -> JsValue {
    let cost =
        JsValue::object(["total", "cacheRead", "cacheWrite", "input", "output"].map(|key| (key, JsValue::Number(0.0))));
    let mut result = JsValue::object(
        ["input", "output", "cacheRead", "cacheWrite", "totalTokens"].map(|key| (key, JsValue::Number(0.0))),
    );
    result
        .as_object_mut()
        .expect("constructed object")
        .insert("cost".into(), cost);
    result
}

pub fn add_counts(base: &mut Dictionary<f64>, update: Dictionary<f64>) {
    for (key, count) in update {
        *base.entry(key).or_default() += count;
    }
}

pub fn merge_session(base: &mut Session, update: Session) {
    base.user_msgs += update.user_msgs;
    base.tool_results += update.tool_results;
    base.compaction_count += update.compaction_count;
    base.compacted_tokens += update.compacted_tokens;
    base.model_changes += update.model_changes;
    add_counts(&mut base.thinking_level_count, update.thinking_level_count);
    for (name, skill) in update.skills {
        match base.skills.get_mut(&name) {
            None => {
                base.skills.insert(name, skill);
            }
            Some(existing) => {
                existing.usage = merge_usage(&existing.usage, &skill.usage);
                existing.calls += skill.calls;
            }
        }
    }
    for (provider, models) in update.models {
        let target = base.models.entry(provider).or_default();
        for (name, model) in models {
            match target.get_mut(&name) {
                None => {
                    target.insert(name, model);
                }
                Some(existing) => merge_model(existing, model),
            }
        }
    }
}

fn merge_model(base: &mut ModelUsage, update: ModelUsage) {
    base.usage = merge_usage(&base.usage, &update.usage);
    base.calls += update.calls;
    base.asst_msgs += update.asst_msgs;
    add_counts(&mut base.tools, update.tools);
    for (name, language) in update.languages {
        let existing = base.languages.entry(name).or_default();
        existing.lines += language.lines;
        existing.edits += language.edits;
    }
}

pub fn session_value(session: &Session) -> Result<JsValue, JsString> {
    js_json::to_value(session).map_err(|error| error.to_string().into())
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySpend {
    pub date: JsString,
    pub cost: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HourSpend {
    pub hour: f64,
    pub cost: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LangStat {
    pub language: JsString,
    pub lines: f64,
    pub edits: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStat {
    pub provider: JsString,
    pub model: JsString,
    pub cost: f64,
    pub calls: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStat {
    pub project: JsString,
    pub cost: f64,
    pub sessions: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStat {
    pub name: JsString,
    pub count: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStat {
    pub provider: JsString,
    pub cost: f64,
    pub calls: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillStat {
    pub name: JsString,
    pub calls: f64,
    pub sessions: f64,
    pub usage: JsValue,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSummary {
    pub total_cost: f64,
    pub session_count: f64,
    pub total_messages: f64,
    pub total_tokens: f64,
    pub total_input_tokens: f64,
    pub total_output_tokens: f64,
    pub total_cache_read_tokens: f64,
    pub total_cache_write_tokens: f64,
    pub days_active: f64,
    pub avg_cost_per_day: f64,
    pub today_cost: f64,
    pub languages: Vec<LangStat>,
    pub models: Vec<ModelStat>,
    pub projects: Vec<ProjectStat>,
    pub tools: Vec<ToolStat>,
    pub providers: Vec<ProviderStat>,
    pub skills: Vec<SkillStat>,
    pub compaction_count: f64,
    pub compacted_tokens: f64,
    pub model_changes: f64,
    pub thinking_level_count: Dictionary<f64>,
    pub daily_spend: Vec<DaySpend>,
    pub hourly_spend: Vec<HourSpend>,
}
