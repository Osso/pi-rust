//! Port of `packages/coding-agent/extensions/goal/src/goal-args.ts`.

use pi_js_compat::{js_json::JsString, string::is_js_whitespace_unit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedGoalArgs {
    View,
    Pause,
    Resume,
    Clear,
    Set { objective: JsString },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseGoalArgsResult {
    Parsed(ParsedGoalArgs),
    Error(String),
}

fn unsupported_goal_flag(parts: &[JsString]) -> Option<String> {
    let flag = parts.iter().find(|part| part.starts_with("--"))?;
    if flag == "--token-budget" || flag.starts_with("--token-budget=") {
        return Some("/goal --token-budget is no longer supported".to_owned());
    }
    if flag == "--wall-clock-minutes" || flag.starts_with("--wall-clock-minutes=") {
        return Some("/goal --wall-clock-minutes is no longer supported".to_owned());
    }
    Some("Goal flags are no longer supported".to_owned())
}

fn split_js_whitespace(value: &JsString) -> Vec<JsString> {
    let mut parts = Vec::new();
    let mut start = None;
    for (index, unit) in value.units().iter().enumerate() {
        if is_js_whitespace_unit(*unit) {
            if let Some(start) = start.take() {
                parts.push(value.slice(start, index));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(start) = start {
        parts.push(value.slice(start, value.len()));
    }
    parts
}

fn is_standalone_control_action(action: &JsString, objective_parts: &[JsString]) -> bool {
    let is_control_action = action == "pause" || action == "resume" || action == "clear";
    is_control_action && objective_parts.is_empty()
}

/// `parseGoalArgs`.
pub fn parse_goal_args(args: impl Into<JsString>) -> ParseGoalArgsResult {
    let parts = split_js_whitespace(&args.into().trim());
    if let Some(flag_error) = unsupported_goal_flag(&parts) {
        return ParseGoalArgsResult::Error(flag_error);
    }
    let Some((action, objective_parts)) = parts.split_first() else {
        return ParseGoalArgsResult::Parsed(ParsedGoalArgs::View);
    };
    if action == "set" {
        let objective = JsString::join(objective_parts.iter(), " ");
        if objective.is_empty() {
            return ParseGoalArgsResult::Error("Use /goal set <objective> to set a goal".to_owned());
        }
        return ParseGoalArgsResult::Parsed(ParsedGoalArgs::Set { objective });
    }
    if is_standalone_control_action(action, objective_parts) {
        let parsed = match action {
            value if value == "pause" => ParsedGoalArgs::Pause,
            value if value == "resume" => ParsedGoalArgs::Resume,
            _ => ParsedGoalArgs::Clear,
        };
        return ParseGoalArgsResult::Parsed(parsed);
    }
    ParseGoalArgsResult::Error("Use /goal set <objective> to set a goal".to_owned())
}
