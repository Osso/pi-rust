//! Pure usage summaries. Clock input is supplied by the command boundary.
use super::data::{self, *};
use pi_js_compat::date::{date_parse_utf16, local_date_fields_in, to_iso_string_value};
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::collections::{BTreeMap, BTreeSet};

const DAY_MS: f64 = 86_400_000.0;

#[derive(Default)]
struct Accumulator {
    summary: StatsSummary,
    languages: Dictionary<LangStat>,
    models: Dictionary<Dictionary<ModelStat>>,
    projects: Dictionary<(f64, BTreeSet<JsString>)>,
    tools: Dictionary<f64>,
    skills: Dictionary<(SkillStat, BTreeSet<JsString>)>,
    sessions: BTreeSet<JsString>,
    dates: BTreeSet<JsString>,
}

pub fn summarize(sessions: &[Session], range: &JsString, filters: &JsValue, now: f64) -> Result<JsValue, JsString> {
    js_json::to_value(&summarize_stats(sessions, range, filters, now)?).map_err(|error| error.to_string().into())
}

pub fn summarize_stats(
    sessions: &[Session],
    range: &JsString,
    filters: &JsValue,
    now: f64,
) -> Result<StatsSummary, JsString> {
    summarize_in_zone(sessions, range, filters, now, &jiff::tz::TimeZone::system())
}

pub fn summarize_in_zone(
    sessions: &[Session],
    range: &JsString,
    filters: &JsValue,
    now: f64,
    zone: &jiff::tz::TimeZone,
) -> Result<StatsSummary, JsString> {
    let today: JsString = to_iso_string_value(now)
        .map_err(|error| JsString::from(error.to_string()))?
        .into();
    let today = today.slice(0, 10);
    let filtered = sessions_in_range(sessions, range, &today)?;
    let project = data::text(filters, "project");
    let project_filtered: Vec<_> = filtered
        .iter()
        .copied()
        .filter(|session| project.is_empty() || session.project == project)
        .collect();
    let mut accumulator = Accumulator::default();
    for session in &project_filtered {
        accumulate_session(&mut accumulator, session, filters, &today);
    }
    let mut result = finish_summary(accumulator);
    result.daily_spend = daily_spend(&filtered, range)?;
    result.hourly_spend = hourly_spend(&project_filtered, range, zone);
    Ok(result)
}

fn sessions_in_range<'a>(
    sessions: &'a [Session],
    range: &JsString,
    today: &JsString,
) -> Result<Vec<&'a Session>, JsString> {
    if range == "All" {
        return Ok(sessions.iter().collect());
    }
    if range == "1d" {
        return Ok(sessions
            .iter()
            .filter(|session| session.timestamp.slice(0, 10) == *today)
            .collect());
    }
    let midnight = pi_js_compat::js_format!("{}T00:00:00Z", today);
    let days = if range == "7d" {
        6.0
    } else if range == "30d" {
        29.0
    } else {
        0.0
    };
    let cutoff = to_iso_string_value(date_parse_utf16(midnight.units()) - days * DAY_MS)
        .map_err(|error| JsString::from(error.to_string()))?;
    let cutoff: JsString = cutoff.into();
    Ok(sessions
        .iter()
        .filter(|session| session.timestamp.slice(0, 10) >= cutoff.slice(0, 10))
        .collect())
}

fn matching_model(name: &JsString, usage: &ModelUsage, filters: &JsValue) -> bool {
    let model = data::text(filters, "model");
    let provider = data::text(filters, "provider");
    (model.is_empty() || *name == model) && (provider.is_empty() || usage.provider == provider)
}

fn accumulate_session(acc: &mut Accumulator, session: &Session, filters: &JsValue, today: &JsString) {
    let mut has_models = false;
    let mut cost = 0.0;
    for models in session.models.values() {
        for (name, usage) in models
            .iter()
            .filter(|(name, usage)| matching_model(name, usage, filters))
        {
            has_models = true;
            cost += data::number(&usage.usage["cost"], "total");
            accumulate_model(acc, name, usage);
        }
    }
    if session.timestamp.slice(0, 10) == *today {
        acc.summary.today_cost += cost;
    }
    if !has_models {
        return;
    }
    acc.sessions.insert(session.session_id.clone());
    acc.dates.insert(session.timestamp.slice(0, 10));
    acc.summary.total_messages += session.user_msgs + session.tool_results;
    acc.summary.compaction_count += session.compaction_count;
    acc.summary.compacted_tokens += session.compacted_tokens;
    acc.summary.model_changes += session.model_changes;
    data::add_counts(
        &mut acc.summary.thinking_level_count,
        session.thinking_level_count.clone(),
    );
    if !session.project.is_empty() {
        let project = acc.projects.entry(session.project.clone()).or_default();
        project.0 += cost;
        project.1.insert(session.session_id.clone());
    }
    accumulate_skills(acc, session);
}

fn accumulate_model(acc: &mut Accumulator, name: &JsString, model: &ModelUsage) {
    let summary = &mut acc.summary;
    let cost = data::number(&model.usage["cost"], "total");
    summary.total_cost += cost;
    summary.total_tokens += data::number(&model.usage, "totalTokens");
    summary.total_input_tokens += data::number(&model.usage, "input");
    summary.total_output_tokens += data::number(&model.usage, "output");
    summary.total_cache_read_tokens += data::number(&model.usage, "cacheRead");
    summary.total_cache_write_tokens += data::number(&model.usage, "cacheWrite");
    summary.total_messages += model.asst_msgs;
    let item = acc
        .models
        .entry(model.provider.clone())
        .or_default()
        .entry(name.clone())
        .or_insert_with(|| ModelStat {
            provider: model.provider.clone(),
            model: name.clone(),
            ..ModelStat::default()
        });
    item.cost += cost;
    item.calls += model.calls;
    data::add_counts(&mut acc.tools, model.tools.clone());
    for (name, language) in model.languages.iter() {
        let item = acc.languages.entry(name.clone()).or_insert_with(|| LangStat {
            language: name.clone(),
            ..LangStat::default()
        });
        item.lines += language.lines;
        item.edits += language.edits;
    }
}

fn accumulate_skills(acc: &mut Accumulator, session: &Session) {
    for (name, skill) in session.skills.iter() {
        let (item, sessions) = acc.skills.entry(name.clone()).or_insert_with(|| {
            (
                SkillStat {
                    name: name.clone(),
                    usage: data::empty_usage(),
                    ..SkillStat::default()
                },
                BTreeSet::new(),
            )
        });
        item.usage = data::merge_usage(&item.usage, &skill.usage);
        item.calls += skill.calls;
        sessions.insert(session.session_id.clone());
    }
}

fn descending(left: f64, right: f64) -> std::cmp::Ordering {
    right.partial_cmp(&left).unwrap_or(std::cmp::Ordering::Equal)
}

fn finish_summary(acc: Accumulator) -> StatsSummary {
    let mut result = acc.summary;
    result.session_count = acc.sessions.len() as f64;
    result.days_active = acc.dates.len() as f64;
    result.avg_cost_per_day = if result.days_active > 0.0 {
        result.total_cost / result.days_active
    } else {
        0.0
    };
    result.languages = acc.languages.into_iter().map(|(_, item)| item).collect();
    result.languages.sort_by(|a, b| descending(a.lines, b.lines));
    for (provider, models) in acc.models {
        let mut provider_stat = ProviderStat {
            provider,
            ..ProviderStat::default()
        };
        for (_, model) in models {
            provider_stat.cost += model.cost;
            provider_stat.calls += model.calls;
            result.models.push(model);
        }
        result.providers.push(provider_stat);
    }
    result
        .models
        .sort_by(|a, b| descending(a.cost, b.cost).then_with(|| descending(a.calls, b.calls)));
    result
        .providers
        .sort_by(|a, b| descending(a.cost, b.cost).then_with(|| descending(a.calls, b.calls)));
    result.projects = acc
        .projects
        .into_iter()
        .map(|(project, (cost, sessions))| ProjectStat {
            project,
            cost,
            sessions: sessions.len() as f64,
        })
        .collect();
    result
        .projects
        .sort_by(|a, b| descending(a.cost, b.cost).then_with(|| descending(a.sessions, b.sessions)));
    result.tools = acc
        .tools
        .into_iter()
        .map(|(name, count)| ToolStat { name, count })
        .collect();
    result.tools.sort_by(|a, b| descending(a.count, b.count));
    result.skills = acc
        .skills
        .into_iter()
        .map(|(_, (mut item, sessions))| {
            item.sessions = sessions.len() as f64;
            item
        })
        .collect();
    result.skills.sort_by(|a, b| {
        descending(
            data::number(&a.usage["cost"], "total"),
            data::number(&b.usage["cost"], "total"),
        )
        .then_with(|| {
            descending(
                data::number(&a.usage, "totalTokens"),
                data::number(&b.usage, "totalTokens"),
            )
        })
        .then_with(|| descending(a.calls, b.calls))
    });
    result
}

fn session_cost(session: &Session) -> f64 {
    // Preserve the source's addition order, including provider subtotals where used.
    let mut total = 0.0;
    for models in session.models.values() {
        for model in models.values() {
            total += data::number(&model.usage["cost"], "total");
        }
    }
    total
}

fn daily_spend(sessions: &[&Session], range: &JsString) -> Result<Vec<DaySpend>, JsString> {
    let mut dates = BTreeMap::<JsString, f64>::new();
    for session in sessions {
        *dates.entry(session.timestamp.slice(0, 10)).or_default() += session_cost(session);
    }
    if range == "All" {
        return Ok(dates.into_iter().map(|(date, cost)| DaySpend { date, cost }).collect());
    }
    let Some(first) = dates.keys().next() else {
        return Ok(Vec::new());
    };
    let last = dates.keys().next_back().expect("nonempty dates");
    let midnight = |date: &JsString| date_parse_utf16(pi_js_compat::js_format!("{}T00:00:00Z", date).units());
    let mut day = midnight(first);
    let end = midnight(last);
    let mut result = Vec::new();
    while day <= end {
        let date: JsString = to_iso_string_value(day)
            .map_err(|error| JsString::from(error.to_string()))?
            .into();
        let date = date.slice(0, 10);
        let cost = dates.get(&date).copied().unwrap_or(0.0);
        result.push(DaySpend { date, cost });
        day += DAY_MS;
    }
    Ok(result)
}

fn hourly_spend(sessions: &[&Session], range: &JsString, zone: &jiff::tz::TimeZone) -> Vec<HourSpend> {
    if range != "1d" || sessions.is_empty() {
        return Vec::new();
    }
    let mut costs = [0.0; 24];
    for session in sessions {
        let Some(date) = local_date_fields_in(date_parse_utf16(session.timestamp.units()) as i64, zone) else {
            continue;
        };
        for models in session.models.values() {
            let cost = models
                .values()
                .fold(0.0, |total, model| total + data::number(&model.usage["cost"], "total"));
            costs[date.hours as usize] += cost;
        }
    }
    costs
        .into_iter()
        .enumerate()
        .map(|(hour, cost)| HourSpend {
            hour: hour as f64,
            cost,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_js_compat::js_json;
    #[test]
    fn matches_frozen_summaries_for_all_ranges_and_filters() -> Result<(), JsString> {
        let fixture =
            js_json::parse_lossless(include_str!("fixtures/domain.json")).map_err(|error| error.into_message())?;
        let sessions = fixture["logs"]
            .as_array()
            .expect("logs")
            .iter()
            .filter(|log| !log["expected"].is_null())
            .map(|log| js_json::from_value::<Session>(&log["expected"]))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| JsString::from(error.to_string()))?;
        let now = pi_js_compat::date::date_parse("2026-10-25T12:00:00Z");
        for case in fixture["summaries"].as_array().expect("summaries") {
            let stats = summarize_in_zone(
                &sessions,
                case["range"].as_string().expect("range"),
                &case["filters"],
                now,
                &jiff::tz::TimeZone::UTC,
            )?;
            let actual = js_json::to_value(&stats).map_err(|error| JsString::from(error.to_string()))?;
            assert_eq!(actual, case["expected"]);
        }
        Ok(())
    }
}
