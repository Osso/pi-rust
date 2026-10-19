//! Port of `packages/coding-agent/extensions/goal/src/goal-presentation.ts`.

use pi_js_compat::string::js_trim;

use super::goal_types::Goal;

const MISSING_PAUSE_REASON: &str = "No pause reason recorded";

fn pause_reason(goal: &Goal) -> &str {
    goal.pause_reason
        .as_deref()
        .map(js_trim)
        .filter(|reason| !reason.is_empty())
        .unwrap_or(MISSING_PAUSE_REASON)
}

/// `goalFooterStatus`.
pub fn goal_footer_status(goal: &Goal) -> String {
    if goal.paused_at.is_some() {
        format!("goal paused: {}", pause_reason(goal))
    } else {
        format!("goal: {}", goal.objective)
    }
}

/// `goalStartupMessage`.
pub fn goal_startup_message(goal: &Goal) -> String {
    if goal.paused_at.is_some() {
        format!("Paused goal: {}", pause_reason(goal))
    } else {
        format!("Active goal: {}", goal.objective)
    }
}

/// `goalViewMessage`.
pub fn goal_view_message(goal: &Goal) -> String {
    if goal.paused_at.is_some() {
        format!("Goal paused: {}", pause_reason(goal))
    } else {
        format!("Goal: {}", goal.objective)
    }
}

/// `goalSystemBlock`.
pub fn goal_system_block(goal: &Goal) -> String {
    [
        "<goal>".to_owned(),
        format!("Long-running objective: {}", goal.objective),
        format!("(set on {} at {})", goal.branch, goal.created_at),
        format!("Continuation turns used: {}", format_continuation_turns(goal)),
        String::new(),
        "Keep working toward this objective across turns until it is achieved.".to_owned(),
        "When it is achieved, call the manage_goal tool with action \"complete\".".to_owned(),
        "Do not call manage_goal with action set for this objective; it is already active.".to_owned(),
        "If you cannot make further progress, say what is blocking it rather than stopping silently.".to_owned(),
        "</goal>".to_owned(),
    ]
    .join("\n")
}

fn format_continuation_turns(goal: &Goal) -> String {
    match goal.continuation_turns {
        Some(turns) => pi_js_compat::number::js_number_to_string(turns),
        None => "0".to_owned(),
    }
}
