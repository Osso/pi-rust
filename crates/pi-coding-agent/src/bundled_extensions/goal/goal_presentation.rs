//! Port of `packages/coding-agent/extensions/goal/src/goal-presentation.ts`.

use pi_js_compat::js_json::JsString;

use super::goal_types::Goal;

const MISSING_PAUSE_REASON: &str = "No pause reason recorded";

fn pause_reason(goal: &Goal) -> JsString {
    goal.pause_reason
        .as_ref()
        .map(JsString::trim)
        .filter(|reason| !reason.is_empty())
        .unwrap_or_else(|| MISSING_PAUSE_REASON.into())
}

/// `goalFooterStatus`.
pub fn goal_footer_status(goal: &Goal) -> JsString {
    if goal.paused_at.is_some() {
        JsString::from("goal paused: ") + pause_reason(goal)
    } else {
        JsString::from("goal: ") + &goal.objective
    }
}

/// `goalStartupMessage`.
pub fn goal_startup_message(goal: &Goal) -> JsString {
    if goal.paused_at.is_some() {
        JsString::from("Paused goal: ") + pause_reason(goal)
    } else {
        JsString::from("Active goal: ") + &goal.objective
    }
}

/// `goalViewMessage`.
pub fn goal_view_message(goal: &Goal) -> JsString {
    if goal.paused_at.is_some() {
        JsString::from("Goal paused: ") + pause_reason(goal)
    } else {
        JsString::from("Goal: ") + &goal.objective
    }
}

/// `goalSystemBlock`.
pub fn goal_system_block(goal: &Goal) -> JsString {
    JsString::join(
        [
            JsString::from("<goal>"),
            JsString::from("Long-running objective: ") + &goal.objective,
            format!("(set on {} at {})", goal.branch, goal.created_at).into(),
            JsString::from("Continuation turns used: ") + format_continuation_turns(goal),
            JsString::default(),
            "Keep working toward this objective across turns until it is achieved.".into(),
            "When it is achieved, call the manage_goal tool with action \"complete\".".into(),
            "Do not call manage_goal with action set for this objective; it is already active.".into(),
            "If you cannot make further progress, say what is blocking it rather than stopping silently.".into(),
            "</goal>".into(),
        ],
        "\n",
    )
}

fn format_continuation_turns(goal: &Goal) -> JsString {
    match goal.continuation_turns {
        Some(turns) => pi_js_compat::number::js_number_to_string(turns).into(),
        None => "0".into(),
    }
}
