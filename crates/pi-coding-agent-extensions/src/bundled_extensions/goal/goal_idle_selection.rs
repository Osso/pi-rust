//! Port of `packages/coding-agent/extensions/goal/src/goal-idle-selection.ts`.

use pi_ai::types::StopReason;

use crate::core::extensions::types::{AgentEndEvent, HandlerError};

use super::goal_turn::{did_last_assistant_abort, did_last_assistant_return_empty, find_last_assistant_message};
use super::goal_types::Goal;

pub struct GoalIdleSelection<'a, FSelect, FClear, FScheduleRetry, FScheduleError, FReport> {
    pub event: &'a AgentEndEvent,
    pub has_pending_messages: &'a dyn Fn() -> bool,
    pub select_goal: FSelect,
    pub clear_retry: FClear,
    pub schedule_retry: FScheduleRetry,
    pub schedule_error_status: FScheduleError,
    pub report_skipped: FReport,
}

/// `selectGoalForIdleReview`.
pub fn select_goal_for_idle_review<FSelect, FClear, FScheduleRetry, FScheduleError, FReport>(
    options: GoalIdleSelection<FSelect, FClear, FScheduleRetry, FScheduleError, FReport>,
) -> Result<Option<Goal>, HandlerError>
where
    FSelect: FnOnce() -> Result<Option<Goal>, HandlerError>,
    FClear: FnOnce(),
    FScheduleRetry: FnOnce(Goal),
    FScheduleError: FnOnce(&str) -> Result<(), HandlerError>,
    FReport: FnOnce(&str),
{
    let Some(goal) = (options.select_goal)()? else {
        return Ok(None);
    };
    let has_pending_messages = options.has_pending_messages;
    if has_pending_messages() {
        (options.clear_retry)();
        return Ok(None);
    }
    if did_last_assistant_abort(options.event) {
        (options.clear_retry)();
        (options.report_skipped)("Goal continuation skipped: the model turn was aborted.");
        return Ok(None);
    }
    if find_last_assistant_message(options.event).is_some_and(|message| message.stop_reason == StopReason::Error) {
        (options.clear_retry)();
        (options.schedule_error_status)("Goal continuation skipped: the model turn ended with an error.")?;
        return Ok(None);
    }
    if did_last_assistant_return_empty(options.event) {
        (options.schedule_retry)(goal);
        return Ok(None);
    }
    (options.clear_retry)();
    Ok(Some(goal))
}
