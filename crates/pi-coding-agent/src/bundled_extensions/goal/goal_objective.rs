//! Port of `packages/coding-agent/extensions/goal/src/goal-objective.ts`.

use pi_js_compat::js_json::JsString;

/// `deduplicateCurrentGoalScope`.
pub fn deduplicate_current_goal_scope(reviewed_objective: &JsString, current_objective: &JsString) -> JsString {
    let Some(first_scope_index) = reviewed_objective.find(current_objective) else {
        return reviewed_objective.to_owned();
    };

    let mut deduplicated = reviewed_objective.to_owned();
    let search_from = first_scope_index + current_objective.len();
    let mut next_scope_index = deduplicated.find_from(current_objective, search_from);
    while let Some(scope_index) = next_scope_index {
        let removal_start = if deduplicated.slice(0, scope_index).ends_with("; ") {
            scope_index - 2
        } else {
            scope_index
        };
        let tail = scope_index + current_objective.len();
        deduplicated.replace_range(removal_start..tail, "");
        next_scope_index = deduplicated.find_from(current_objective, search_from.min(deduplicated.len()));
    }
    deduplicated.trim()
}
