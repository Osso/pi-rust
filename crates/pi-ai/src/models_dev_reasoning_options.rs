//! Verified models.dev reasoning controls; no adapter-specific inference.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::types::{ModelThinkingLevel, ThinkingLevelMap};

/// Effort strings accepted by the pinned models.dev schema (JSON null uses `Option`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelsDevEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Default,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ModelsDevReasoningOption {
    #[serde(rename = "toggle")]
    Toggle,
    #[serde(rename = "effort")]
    Effort { values: Vec<Option<ModelsDevEffort>> },
    #[serde(rename = "budget_tokens")]
    BudgetTokens {
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
}

const SELECTABLE_LEVELS: [(ModelsDevEffort, ModelThinkingLevel, &str); 7] = [
    (ModelsDevEffort::None, ModelThinkingLevel::Off, "none"),
    (ModelsDevEffort::Minimal, ModelThinkingLevel::Minimal, "minimal"),
    (ModelsDevEffort::Low, ModelThinkingLevel::Low, "low"),
    (ModelsDevEffort::Medium, ModelThinkingLevel::Medium, "medium"),
    (ModelsDevEffort::High, ModelThinkingLevel::High, "high"),
    (ModelsDevEffort::Xhigh, ModelThinkingLevel::Xhigh, "xhigh"),
    (ModelsDevEffort::Max, ModelThinkingLevel::Max, "max"),
];

/// Return only verified efforts, with `off` first and unsupported levels explicit null.
/// Toggle/budget controls and efforts without a Pi equivalent do not infer support.
pub fn get_effort_thinking_level_map(options: &[ModelsDevReasoningOption]) -> Option<ThinkingLevelMap> {
    let supported: HashSet<_> = options
        .iter()
        .flat_map(|option| match option {
            ModelsDevReasoningOption::Effort { values } => values.as_slice(),
            _ => &[],
        })
        .copied()
        .collect();
    if !SELECTABLE_LEVELS
        .iter()
        .any(|(effort, _, _)| supported.contains(&Some(*effort)))
    {
        return None;
    }
    Some(
        SELECTABLE_LEVELS
            .iter()
            .map(|(effort, level, name)| {
                let value = supported.contains(&Some(*effort)).then(|| (*name).to_owned());
                (*level, value)
            })
            .collect(),
    )
}
