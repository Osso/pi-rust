//! Port of `packages/coding-agent/extensions/goal/src/goal-objective.ts`.

use pi_js_compat::string::js_trim;

/// `deduplicateCurrentGoalScope`.
pub fn deduplicate_current_goal_scope(reviewed_objective: &str, current_objective: &str) -> String {
    let Some(first_scope_index) = reviewed_objective.find(current_objective) else {
        return reviewed_objective.to_owned();
    };

    let mut deduplicated = reviewed_objective.to_owned();
    let search_from = first_scope_index + current_objective.len();
    let mut next_scope_index = deduplicated[search_from..]
        .find(current_objective)
        .map(|index| index + search_from);
    while let Some(scope_index) = next_scope_index {
        let separator_start = deduplicated[..scope_index].rfind("; ");
        let removal_start = match separator_start {
            Some(start) if start + 2 == scope_index => start,
            _ => scope_index,
        };
        let tail = scope_index + current_objective.len();
        deduplicated = format!("{}{}", &deduplicated[..removal_start], &deduplicated[tail..]);
        next_scope_index = deduplicated[search_from.min(deduplicated.len())..]
            .find(current_objective)
            .map(|index| index + search_from.min(deduplicated.len()));
    }
    js_trim(&deduplicated).to_owned()
}
