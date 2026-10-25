//! Port of `packages/coding-agent/src/core/usage-totals.ts`.

use pi_ai::types::Usage;

use crate::core::session_manager::SessionEntry;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UsageTotals {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub cost: f64,
}

pub fn create_usage_totals() -> UsageTotals {
    UsageTotals::default()
}

pub fn add_usage_to_totals(totals: &mut UsageTotals, usage: &Usage) {
    totals.input += usage.input;
    totals.output += usage.output;
    totals.cache_read += usage.cache_read;
    totals.cache_write += usage.cache_write;
    totals.cost += usage.cost.total;
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageCostBreakdownEntry {
    pub key: String,
    pub cost: f64,
    pub tokens: f64,
}

/// Group attributable assistant usage by model and all other usage into a separate bucket.
pub fn get_usage_cost_breakdown(entries: &[SessionEntry]) -> Vec<UsageCostBreakdownEntry> {
    let mut totals_by_key: Vec<(String, UsageTotals)> = Vec::new();

    for entry in entries {
        let Some(message) = entry.as_message() else {
            continue;
        };
        let (key, usage): (Option<String>, Option<Usage>) = match message.message.as_ref() {
            pi_agent_core::agent::AgentMessage::Assistant(assistant) => (
                Some(format!(
                    "{}/{}",
                    assistant.provider,
                    assistant.response_model.as_deref().unwrap_or(&assistant.model)
                )),
                Some(assistant.usage.clone()),
            ),
            pi_agent_core::agent::AgentMessage::ToolResult(result) => {
                (Some("Tools/summaries".to_owned()), result.usage.clone())
            }
            _ => (None, None),
        };
        let (Some(key), Some(usage)) = (key, usage) else {
            continue;
        };

        match totals_by_key.iter_mut().find(|(existing, _)| existing == &key) {
            Some((_, totals)) => add_usage_to_totals(totals, &usage),
            None => {
                let mut totals = create_usage_totals();
                add_usage_to_totals(&mut totals, &usage);
                totals_by_key.push((key, totals));
            }
        }
    }

    let mut breakdown: Vec<UsageCostBreakdownEntry> = totals_by_key
        .into_iter()
        .map(|(key, totals)| UsageCostBreakdownEntry {
            key,
            cost: totals.cost,
            tokens: totals.input + totals.output + totals.cache_read + totals.cache_write,
        })
        .filter(|entry| entry.cost > 0.0 || entry.tokens > 0.0)
        .collect();
    breakdown.sort_by(|left, right| right.cost.partial_cmp(&left.cost).unwrap_or(std::cmp::Ordering::Equal));
    breakdown
}
