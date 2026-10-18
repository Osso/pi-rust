//! Resource diagnostic records (`core/diagnostics.ts`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    Extension,
    Skill,
    Prompt,
    Theme,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceCollision {
    pub resource_type: ResourceType,
    /// Skill name, command/tool/flag name, prompt name, or theme name.
    pub name: String,
    pub winner_path: String,
    pub loser_path: String,
    /// For example `npm:foo`, `git:...`, or `local`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winner_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loser_source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceDiagnosticType {
    Warning,
    Error,
    Collision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDiagnostic {
    #[serde(rename = "type")]
    pub diagnostic_type: ResourceDiagnosticType,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision: Option<ResourceCollision>,
}
