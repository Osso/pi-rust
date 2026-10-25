//! Port of `packages/coding-agent/extensions/goal/src/index.ts`.

use std::process::{Command, Stdio};
use std::sync::Arc;

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::js_json::{JsObject as Map, JsString, JsValue as Value, encode_node_utf8};
use std::sync::OnceLock;

use crate::core::extensions::loader::ExtensionFactory;
use crate::core::extensions::types::{
    AgentEndEvent, BeforeAgentStartEvent, BeforeAgentStartEventResult, BoxFuture, ExtensionApi,
    ExtensionCommandContext, ExtensionContext, HandlerError, InputEvent, NotifyType, SessionStartEvent,
    SessionStartReason, ToolResultEvent, UserMessageContent,
};
use crate::core::session_manager::read_session_header;

use super::completion_scheduling::{
    CompletionSchedulingOptions, CompletionWaitScheduler, create_completion_wait_scheduler,
};
use super::empty_response_scheduling::{EmptyResponseScheduler, create_empty_response_scheduler};
use super::error_status_scheduling::{ErrorStatusScheduler, create_error_status_scheduler};
use super::goal_agent_end_scheduling::{ApplyIdleDecision, ScheduledGoalAgentEndOptions, run_scheduled_goal_agent_end};
use super::goal_args::{ParseGoalArgsResult, ParsedGoalArgs, parse_goal_args};
use super::goal_idle_selection::{GoalIdleSelection, select_goal_for_idle_review};
use super::goal_objective::deduplicate_current_goal_scope;
use super::goal_parsing::parse_goal_json;
use super::goal_presentation::{goal_footer_status, goal_startup_message, goal_system_block, goal_view_message};
use super::goal_review_evidence::{GoalReviewEvidenceController, create_goal_review_evidence_controller};
use super::goal_scheduling::{GoalScheduler, GoalSchedulingOptions, create_goal_scheduler};
use super::goal_state::{
    clear_goal, load_or_migrate_active_goal, load_or_migrate_goal, load_or_migrate_running_goal,
    load_previous_goal_file, mark_goal_complete, pause_goal, resume_goal, save_goal,
};
use super::goal_tool::{ManageGoalAction, ManageGoalParams, register_manage_goal_tool};
use super::goal_types::{
    Goal, GoalEvidenceReview, GoalExtensionOptions, GoalReviewInput, GoalReviewKind, GoalSupervisorResponse,
    ReviewedGoalResponse,
};
use super::rendering::{
    AppendSupervisorStatus, SupervisorStatusController, create_supervisor_status_controller,
    create_supervisor_status_entry_renderer, render_supervisor_message, send_supervisor_instructions,
};
use super::supervisor_review::{review_goal_with_resident_supervisor, with_supervisor_review_status};
use super::wait_countdown::create_wait_countdown_refresher;
use super::wait_status::{append_goal_scheduling_error, create_wait_status_callbacks};

const MAX_OBJECTIVE_CHARS: usize = 10_000;
const USER_PAUSE_REASON: &str = "Paused by user.";
const RESERVED_GOAL_OBJECTIVES: [&str; 7] = ["set", "pause", "resume", "clear", "status", "complete", "continue"];

fn text_result(text: impl Into<JsString>, details: Value) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(details),
        is_error: None,
        terminate: None,
    }
}

fn empty_details() -> Value {
    Value::Object(Map::new())
}

fn update_goal_footer_status(ctx: &ExtensionContext) -> Result<(), HandlerError> {
    let cwd = ctx.cwd()?;
    let session = ctx.session_manager()?;
    let goal = load_or_migrate_active_goal(&cwd, &session)?;
    let ui = ctx.ui()?;
    ui.set_status(
        &"goal".into(),
        goal.as_ref()
            .map(goal_footer_status)
            .map(crate::core::footer_data_provider::ExtensionStatus::Text),
    )?;
    Ok(())
}

/// TS `execFileSync` throws on spawn failure or non-zero exit; both map to `(no branch)`.
fn read_current_branch(cwd: &JsString) -> JsString {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(encode_node_utf8(cwd))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(output) if output.status.success() => {
            JsString::from(String::from_utf8_lossy(&output.stdout).as_ref()).trim()
        }
        _ => "(no branch)".into(),
    }
}

fn session_id_from_session_file(session_file: &JsString) -> Option<JsString> {
    let header = read_session_header(session_file)?;
    header.fields.get("id").and_then(Value::as_string).cloned()
}

fn is_goal_inheritance_event(event: &SessionStartEvent) -> Option<&JsString> {
    if event.reason != SessionStartReason::Fork {
        return None;
    }
    event
        .previous_session_file
        .as_ref()
        .filter(|previous| !previous.is_empty())
}

fn inherit_previous_session_goal(event: &SessionStartEvent, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    let Some(previous_session_file) = is_goal_inheritance_event(event) else {
        return Ok(());
    };
    let session = ctx.session_manager()?;
    if session.is_subagent_session() {
        return Ok(());
    }
    let cwd = ctx.cwd()?;
    if load_or_migrate_goal(&cwd, &session)?.is_some() {
        return Ok(());
    }
    let previous_goal_json = session
        .get_session_goal_json_for_session(previous_session_file)
        .map_err(|error| HandlerError::new(error.message().clone()))?
        .filter(|json| !json.is_empty());
    let previous_goal = match previous_goal_json {
        Some(previous_goal_json) => parse_goal_json(&previous_goal_json),
        None => load_legacy_previous_goal(previous_session_file, &cwd)?,
    };
    if let Some(previous_goal) = previous_goal
        && previous_goal.completed_at.is_none()
    {
        save_goal(&session, &previous_goal)?;
    }
    Ok(())
}

fn load_legacy_previous_goal(previous_session_file: &JsString, cwd: &JsString) -> Result<Option<Goal>, HandlerError> {
    match session_id_from_session_file(previous_session_file).filter(|id| !id.is_empty()) {
        Some(previous_session_id) => load_previous_goal_file(cwd, &previous_session_id),
        None => Ok(None),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct SetGoalResult {
    ok: bool,
    message: JsString,
    severity: NotifyType,
    goal: Option<Goal>,
}

fn validate_goal_objective(objective: &JsString) -> Option<SetGoalResult> {
    if RESERVED_GOAL_OBJECTIVES
        .iter()
        .any(|reserved| objective.to_lowercase() == *reserved)
    {
        return Some(SetGoalResult {
            ok: false,
            message: (JsString::from("Objective cannot be a goal control command: ") + objective),
            severity: NotifyType::Error,
            goal: None,
        });
    }
    if objective.len() > MAX_OBJECTIVE_CHARS {
        return Some(SetGoalResult {
            ok: false,
            message: format!("Objective too long ({} > {MAX_OBJECTIVE_CHARS} chars)", objective.len()).into(),
            severity: NotifyType::Error,
            goal: None,
        });
    }
    None
}

fn create_goal(objective: JsString, branch: JsString, created_at: JsString) -> Goal {
    Goal {
        objective,
        branch,
        created_at,
        completed_at: None,
        completion_reason: None,
        continuation_turns: Some(0.0),
        paused_at: None,
        pause_reason: None,
        review_evidence: None,
    }
}

struct SetGoalParams<'a> {
    objective: JsString,
    ctx: &'a ExtensionContext,
    pi: &'a ExtensionApi,
    before_save: Option<&'a (dyn Fn() -> Result<(), HandlerError> + Send + Sync)>,
}

fn set_goal(params: SetGoalParams<'_>) -> Result<SetGoalResult, HandlerError> {
    if let Some(invalid_result) = validate_goal_objective(&params.objective) {
        return Ok(invalid_result);
    }
    if let Some(before_save) = params.before_save {
        before_save()?;
    }
    let cwd = params.ctx.cwd()?;
    let goal = create_goal(
        params.objective,
        read_current_branch(&cwd),
        crate::utils::iso_time::now_iso_string().into(),
    );
    let session = params.ctx.session_manager()?;
    save_goal(&session, &goal)?;
    update_goal_footer_status(params.ctx)?;
    if params.ctx.is_idle()? {
        params
            .pi
            .send_user_message(
                UserMessageContent::Text("Continue working toward the active goal.".to_owned().into()),
                None,
            )
            .map_err(|error| HandlerError::new(error.to_string()))?;
    }
    Ok(SetGoalResult {
        ok: true,
        message: "Goal set — starting work".into(),
        severity: NotifyType::Info,
        goal: Some(goal),
    })
}

fn goal_matches_review(current_goal: Option<&Goal>, reviewed_goal: &Goal) -> bool {
    current_goal.is_some_and(|current| {
        current.created_at == reviewed_goal.created_at
            && current.objective == reviewed_goal.objective
            && current.paused_at == reviewed_goal.paused_at
    })
}

async fn review_goal_set_objective(
    ctx: &ExtensionContext,
    proposed_objective: &JsString,
    review_goal: &GoalEvidenceReview,
) -> Result<Result<JsString, AgentToolResult>, HandlerError> {
    let cwd = ctx.cwd()?;
    let session = ctx.session_manager()?;
    let active_goal = load_or_migrate_active_goal(&cwd, &session)?;
    let mut payload = Map::new();
    if let Some(active_goal) = &active_goal {
        payload.insert("currentObjective".into(), Value::String(active_goal.objective.clone()));
    }
    payload.insert("proposedObjective".into(), Value::String(proposed_objective.to_owned()));
    let reviewed = review_goal(GoalReviewInput {
        kind: GoalReviewKind::GoalSetReview,
        payload,
        ctx: ctx.clone(),
    })
    .await?;
    let current_goal = load_or_migrate_active_goal(&cwd, &session)?;
    let review_still_applies = match &active_goal {
        Some(active_goal) => goal_matches_review(current_goal.as_ref(), active_goal),
        None => current_goal.is_none(),
    };
    if !review_still_applies {
        return Ok(Err(text_result(
            "Goal changed while Supervisor review was in progress; stale decision ignored.",
            empty_details(),
        )));
    }
    match reviewed.decision {
        GoalSupervisorResponse::Set { objective, .. } => Ok(Ok(match &active_goal {
            Some(active_goal) => deduplicate_current_goal_scope(&JsString::from(&objective), &active_goal.objective),
            None => objective,
        })),
        decision => {
            let reason = decision_reason(&decision);
            Ok(Err(text_result(
                JsString::from("Goal not set: ") + &reason,
                empty_details(),
            )))
        }
    }
}

fn decision_reason(decision: &GoalSupervisorResponse) -> JsString {
    match decision {
        GoalSupervisorResponse::Complete { reason, .. }
        | GoalSupervisorResponse::Pause { reason, .. }
        | GoalSupervisorResponse::Wait { reason, .. }
        | GoalSupervisorResponse::Set { reason, .. }
        | GoalSupervisorResponse::Error { reason, .. }
        | GoalSupervisorResponse::Continue { reason, .. } => reason.clone(),
    }
}

fn decision_instructions(decision: &GoalSupervisorResponse) -> JsString {
    match decision {
        GoalSupervisorResponse::Continue { instructions, .. } => instructions.clone(),
        _ => JsString::default(),
    }
}

struct SetGoalActionOptions<'a> {
    ctx: &'a ExtensionContext,
    params: &'a ManageGoalParams,
    pi: &'a ExtensionApi,
    review_goal: &'a GoalEvidenceReview,
    before_goal_save: Option<&'a (dyn Fn() -> Result<(), HandlerError> + Send + Sync)>,
}

async fn run_set_goal_action(options: SetGoalActionOptions<'_>) -> Result<AgentToolResult, HandlerError> {
    let proposed_objective = options
        .params
        .objective
        .as_ref()
        .map(JsString::trim)
        .unwrap_or_default()
        .to_owned();
    if proposed_objective.is_empty() {
        return Ok(text_result("Objective is required.", empty_details()));
    }
    if let Some(invalid_result) = validate_goal_objective(&proposed_objective) {
        return Ok(text_result(invalid_result.message, empty_details()));
    }
    let reviewed_objective =
        match review_goal_set_objective(options.ctx, &proposed_objective, options.review_goal).await? {
            Ok(objective) => objective,
            Err(result) => return Ok(result),
        };
    let result = set_goal(SetGoalParams {
        objective: reviewed_objective.clone(),
        ctx: options.ctx,
        pi: options.pi,
        before_save: options.before_goal_save,
    })?;
    let details = match &result.goal {
        Some(goal) => {
            let mut details = Map::new();
            details.insert("objective".into(), Value::String(goal.objective.clone()));
            Value::Object(details)
        }
        None => empty_details(),
    };
    let text = if result.ok {
        JsString::from("Goal set: ") + &reviewed_objective
    } else {
        result.message
    };
    Ok(text_result(text, details))
}

fn run_pause_goal_action(
    ctx: &ExtensionContext,
    reason_input: Option<&JsString>,
    after_goal_change: Option<&(dyn Fn() -> Result<(), HandlerError> + Send + Sync)>,
) -> Result<AgentToolResult, HandlerError> {
    let cwd = ctx.cwd()?;
    let session = ctx.session_manager()?;
    if load_or_migrate_active_goal(&cwd, &session)?.is_none() {
        update_goal_footer_status(ctx)?;
        return Ok(text_result("No active goal to pause.", empty_details()));
    }
    let reason = reason_input.map(JsString::trim).unwrap_or_default();
    if reason.is_empty() {
        return Ok(text_result("Reason is required to pause a goal.", empty_details()));
    }
    let Some(goal) = pause_goal(&cwd, &session, &reason)? else {
        return Ok(text_result("No active goal to pause.", empty_details()));
    };
    update_goal_footer_status(ctx)?;
    if let Some(after_goal_change) = after_goal_change {
        after_goal_change()?;
    }
    let mut details = Map::new();
    details.insert("objective".into(), Value::String(goal.objective));
    details.insert("reason".into(), Value::String(reason.to_owned()));
    Ok(text_result(
        JsString::from("Goal paused: ") + &reason,
        Value::Object(details),
    ))
}

fn run_resume_goal_action(
    ctx: &ExtensionContext,
    pi: &ExtensionApi,
    after_goal_change: Option<&(dyn Fn() -> Result<(), HandlerError> + Send + Sync)>,
) -> Result<AgentToolResult, HandlerError> {
    let cwd = ctx.cwd()?;
    let session = ctx.session_manager()?;
    let goal = resume_goal(&cwd, &session)?;
    update_goal_footer_status(ctx)?;
    let Some(goal) = goal else {
        return Ok(text_result("No paused goal to resume.", empty_details()));
    };
    if let Some(after_goal_change) = after_goal_change {
        after_goal_change()?;
    }
    if ctx.is_idle()? {
        pi.send_user_message(
            UserMessageContent::Text("Continue working toward the active goal.".to_owned().into()),
            None,
        )
        .map_err(|error| HandlerError::new(error.to_string()))?;
    }
    let mut details = Map::new();
    details.insert("objective".into(), Value::String(goal.objective.clone()));
    Ok(text_result(
        JsString::from("Goal resumed: ") + &goal.objective,
        Value::Object(details),
    ))
}

type OnCompletionWait = Arc<
    dyn Fn(Goal, ExtensionContext, JsString, JsString, Option<JsString>) -> BoxFuture<'static, Result<(), HandlerError>>
        + Send
        + Sync,
>;

async fn apply_completion_decision(
    decision: GoalSupervisorResponse,
    active_goal: Goal,
    ctx: &ExtensionContext,
    pi: &ExtensionApi,
    completion_report: &JsString,
    on_wait: &OnCompletionWait,
    append_status: &AppendSupervisorStatus,
) -> Result<AgentToolResult, HandlerError> {
    if matches!(decision, GoalSupervisorResponse::Complete { .. }) {
        let cwd = ctx.cwd()?;
        let session = ctx.session_manager()?;
        let Some(_) = mark_goal_complete(&cwd, &session, completion_report)? else {
            return Ok(text_result("No active goal to complete.", empty_details()));
        };
        update_goal_footer_status(ctx)?;
        return Ok(text_result(
            JsString::from("Goal marked complete: ") + completion_report,
            empty_details(),
        ));
    }
    let reason = decision_reason(&decision);
    let rejection =
        JsString::from("Completion report rejected: ") + &reason + "\n\nSubmitted report:\n" + completion_report;
    let status_message = match &decision {
        GoalSupervisorResponse::Continue { instructions, .. } => rejection + "\n\n" + instructions,
        _ => rejection,
    };
    append_status(ctx.clone(), &status_message, None, decision.display_id())?;
    if matches!(decision, GoalSupervisorResponse::Continue { .. }) {
        let instructions = decision_instructions(&decision);
        send_supervisor_instructions(pi, &instructions, decision.display_id())?;
        let mut details = Map::new();
        details.insert("instructions".into(), Value::String(instructions.clone()));
        return Ok(text_result(
            JsString::from("Goal remains active: ") + &reason,
            Value::Object(details),
        ));
    }
    if matches!(decision, GoalSupervisorResponse::Pause { .. }) {
        return Ok(text_result(
            JsString::from("Goal remains active: ") + &reason,
            empty_details(),
        ));
    }
    if matches!(decision, GoalSupervisorResponse::Wait { .. }) {
        on_wait(
            active_goal,
            ctx.clone(),
            completion_report.to_owned(),
            reason.clone(),
            decision.display_id().cloned(),
        )
        .await?;
        return Ok(text_result(
            JsString::from("Goal remains active: ") + &reason,
            empty_details(),
        ));
    }
    Ok(text_result(
        JsString::from("Goal review failed: ") + &reason,
        empty_details(),
    ))
}

struct CompleteGoalActionOptions<'a> {
    ctx: &'a ExtensionContext,
    completion_report_input: Option<&'a JsString>,
    review_goal: &'a GoalEvidenceReview,
    pi: &'a ExtensionApi,
    on_wait: &'a OnCompletionWait,
    append_status: &'a AppendSupervisorStatus,
    is_review_current: &'a (dyn Fn() -> Result<bool, HandlerError> + Send + Sync),
    consume_review_evidence: &'a (dyn Fn(&ExtensionContext, &Goal, usize) -> Result<(), HandlerError> + Send + Sync),
}

async fn run_complete_goal_action(options: CompleteGoalActionOptions<'_>) -> Result<AgentToolResult, HandlerError> {
    let cwd = options.ctx.cwd()?;
    let session = options.ctx.session_manager()?;
    let Some(active_goal) = load_or_migrate_active_goal(&cwd, &session)? else {
        return Ok(text_result("No active goal to complete.", empty_details()));
    };
    let completion_report = options
        .completion_report_input
        .map(JsString::trim)
        .unwrap_or_default()
        .to_owned();
    if completion_report.is_empty() {
        return Ok(text_result("Completion report is required.", empty_details()));
    }
    let mut payload = Map::new();
    payload.insert("objective".into(), Value::String(active_goal.objective.clone()));
    payload.insert("completionReport".into(), Value::String(completion_report.clone()));
    let reviewed = (options.review_goal)(GoalReviewInput {
        kind: GoalReviewKind::GoalCompletionReview,
        payload,
        ctx: options.ctx.clone(),
    })
    .await?;
    let review_still_applies = (options.is_review_current)()?
        && goal_matches_review(load_or_migrate_active_goal(&cwd, &session)?.as_ref(), &active_goal);
    if !review_still_applies {
        return Ok(text_result(
            "Goal changed or review was canceled; stale decision ignored.",
            empty_details(),
        ));
    }
    let is_error = matches!(reviewed.decision, GoalSupervisorResponse::Error { .. });
    let result = apply_completion_decision(
        reviewed.decision,
        active_goal.clone(),
        options.ctx,
        options.pi,
        &completion_report,
        options.on_wait,
        options.append_status,
    )
    .await?;
    if !is_error {
        (options.consume_review_evidence)(options.ctx, &active_goal, reviewed.evidence_count)?;
    }
    Ok(result)
}

fn run_clear_goal_action(
    ctx: &ExtensionContext,
    after_goal_change: Option<&(dyn Fn() -> Result<(), HandlerError> + Send + Sync)>,
) -> Result<AgentToolResult, HandlerError> {
    let cwd = ctx.cwd()?;
    let session = ctx.session_manager()?;
    let cleared = clear_goal(&cwd, &session)?;
    update_goal_footer_status(ctx)?;
    if cleared && let Some(after_goal_change) = after_goal_change {
        after_goal_change()?;
    }
    let message = if cleared { "Goal cleared" } else { "No active goal" };
    Ok(text_result(message, empty_details()))
}

fn run_goal_status_action(ctx: &ExtensionContext) -> Result<AgentToolResult, HandlerError> {
    let cwd = ctx.cwd()?;
    let session = ctx.session_manager()?;
    let goal = load_or_migrate_active_goal(&cwd, &session)?;
    let message = match &goal {
        Some(goal) => goal_view_message(goal),
        None => "No active goal — use /goal set <objective>".into(),
    };
    let details = match &goal {
        Some(goal) => {
            let mut details = Map::new();
            details.insert("objective".into(), Value::String(goal.objective.clone()));
            Value::Object(details)
        }
        None => empty_details(),
    };
    Ok(text_result(message, details))
}

type ConsumeReviewEvidence = Arc<dyn Fn(&ExtensionContext, &Goal, usize) -> Result<(), HandlerError> + Send + Sync>;

type ClearSessionSchedules = Arc<dyn Fn(&JsString) + Send + Sync>;

struct ManageGoalContext {
    ctx: ExtensionContext,
    params: ManageGoalParams,
    pi: ExtensionApi,
    review_goal: GoalEvidenceReview,
    consume_review_evidence: ConsumeReviewEvidence,
    on_completion_wait: OnCompletionWait,
    append_status: AppendSupervisorStatus,
    is_completion_review_current: Arc<dyn Fn() -> Result<bool, HandlerError> + Send + Sync>,
    before_goal_save: Option<Arc<dyn Fn() -> Result<(), HandlerError> + Send + Sync>>,
}

async fn manage_goal(context: ManageGoalContext) -> Result<AgentToolResult, HandlerError> {
    let before_goal_save = context
        .before_goal_save
        .as_deref()
        .map(|callback| callback as &(dyn Fn() -> Result<(), HandlerError> + Send + Sync));
    let action = context.params.action;
    match action {
        Some(ManageGoalAction::Set) => {
            run_set_goal_action(SetGoalActionOptions {
                ctx: &context.ctx,
                params: &context.params,
                pi: &context.pi,
                review_goal: &context.review_goal,
                before_goal_save,
            })
            .await
        }
        Some(ManageGoalAction::Pause) => {
            run_pause_goal_action(&context.ctx, context.params.reason.as_ref(), before_goal_save)
        }
        Some(ManageGoalAction::Resume) => run_resume_goal_action(&context.ctx, &context.pi, before_goal_save),
        Some(ManageGoalAction::Complete) => {
            run_complete_goal_action(CompleteGoalActionOptions {
                ctx: &context.ctx,
                completion_report_input: context.params.completion_report.as_ref(),
                review_goal: &context.review_goal,
                pi: &context.pi,
                on_wait: &context.on_completion_wait,
                append_status: &context.append_status,
                is_review_current: context.is_completion_review_current.as_ref(),
                consume_review_evidence: context.consume_review_evidence.as_ref(),
            })
            .await
        }
        Some(ManageGoalAction::Clear) => run_clear_goal_action(&context.ctx, before_goal_save),
        Some(ManageGoalAction::Status) | None => run_goal_status_action(&context.ctx),
    }
}

fn complete_goal_from_idle_decision(
    goal: &Goal,
    reason: &JsString,
    ctx: &ExtensionContext,
) -> Result<(), HandlerError> {
    let cwd = ctx.cwd()?;
    let session = ctx.session_manager()?;
    mark_goal_complete(&cwd, &session, reason)?;
    update_goal_footer_status(ctx)?;
    let ui = ctx.ui()?;
    ui.notify(
        &(JsString::from("Goal complete: ") + &goal.objective),
        Some(NotifyType::Info),
    )?;
    Ok(())
}

fn continue_goal_from_idle_decision(
    goal: &Goal,
    instructions: &JsString,
    ctx: &ExtensionContext,
    pi: &ExtensionApi,
    display_id: Option<&JsString>,
) -> Result<(), HandlerError> {
    let session = ctx.session_manager()?;
    let continuation_turns = goal.continuation_turns.unwrap_or(0.0);
    save_goal(
        &session,
        &Goal {
            continuation_turns: Some(continuation_turns + 1.0),
            ..goal.clone()
        },
    )?;
    send_supervisor_instructions(pi, instructions, display_id)
}

async fn apply_goal_idle_decision(
    decision: GoalSupervisorResponse,
    goal: Goal,
    ctx: &ExtensionContext,
    pi: &ExtensionApi,
    on_wait: &(dyn Fn(JsString) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync),
    append_status: &AppendSupervisorStatus,
) -> Result<(), HandlerError> {
    match decision {
        GoalSupervisorResponse::Complete { reason, .. } => {
            complete_goal_from_idle_decision(&goal, &JsString::from(&reason), ctx)
        }
        GoalSupervisorResponse::Pause { reason, .. } => {
            append_status(ctx.clone(), &(JsString::from("Goal waiting: ") + &reason), None, None)?;
            Ok(())
        }
        GoalSupervisorResponse::Wait { reason, .. } => on_wait(JsString::from("Waiting: ") + &reason).await,
        GoalSupervisorResponse::Error { reason, .. } => on_wait(JsString::from("Goal review failed: ") + &reason).await,
        GoalSupervisorResponse::Continue {
            instructions,
            display_id,
            ..
        } => continue_goal_from_idle_decision(&goal, &instructions, ctx, pi, display_id.as_ref()),
        GoalSupervisorResponse::Set { reason, .. } => {
            append_status(
                ctx.clone(),
                &(JsString::from("Goal review failed: unexpected set decision: ") + &reason),
                None,
                None,
            )?;
            Ok(())
        }
    }
}

struct GoalExtensionRuntime {
    empty_response_scheduler: EmptyResponseScheduler<Goal>,
    error_status_scheduler: ErrorStatusScheduler,
    scheduler: GoalScheduler<Goal, ReviewedGoalResponse>,
    completion_scheduler: CompletionWaitScheduler,
    status: Arc<SupervisorStatusController>,
    apply_decision: ApplyIdleDecision,
    clear_goal_schedules: Arc<dyn Fn(&JsString) + Send + Sync>,
    clear_all_goal_schedules: Arc<dyn Fn() + Send + Sync>,
}

fn same_running_goal(ctx: &ExtensionContext, goal: &Goal) -> Result<bool, HandlerError> {
    let cwd = ctx.cwd()?;
    let session = ctx.session_manager()?;
    let active_goal = load_or_migrate_running_goal(&cwd, &session)?;
    Ok(active_goal.is_some_and(|active| active.created_at == goal.created_at && active.objective == goal.objective))
}

fn inject_goal_context(
    event: &BeforeAgentStartEvent,
    ctx: &ExtensionContext,
) -> Result<Option<BeforeAgentStartEventResult>, HandlerError> {
    let cwd = ctx.cwd()?;
    let session = ctx.session_manager()?;
    let Some(goal) = load_or_migrate_running_goal(&cwd, &session)? else {
        return Ok(None);
    };
    Ok(Some(BeforeAgentStartEventResult {
        message: None,
        system_prompt: Some(event.system_prompt.clone() + "\n\n" + goal_system_block(&goal)),
    }))
}

fn report_goal_scheduling_error(append: &AppendSupervisorStatus, error: HandlerError, ctx: ExtensionContext) {
    append_goal_scheduling_error(append, &error.message, ctx)
        .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message));
}

fn create_completion_scheduler(
    pi: &ExtensionApi,
    review_goal: &GoalEvidenceReview,
    evidence: &GoalReviewEvidenceController,
    status: &Arc<SupervisorStatusController>,
) -> CompletionWaitScheduler {
    let evidence = evidence.clone();
    let pi_for_continue = pi.clone();
    let clear_status = status.clone();
    let append_status = status.append_callback();
    let errors = append_status.clone();
    create_completion_wait_scheduler(CompletionSchedulingOptions {
        pi: pi.clone(),
        review_goal: review_goal.clone(),
        consume_review_evidence: Arc::new(move |ctx, goal, count| evidence.consume(&ctx, &goal, count)),
        is_same_goal: Arc::new(|ctx, waiting| {
            let active = load_or_migrate_active_goal(&ctx.cwd()?, &ctx.session_manager()?)?;
            Ok(active.is_some_and(|active| goal_matches_review(Some(&active), &waiting.goal)))
        }),
        on_complete: Arc::new(|waiting, ctx| {
            let goal = mark_goal_complete(&ctx.cwd()?, &ctx.session_manager()?, &waiting.completion_report)?;
            update_goal_footer_status(&ctx)?;
            if let Some(goal) = goal {
                ctx.ui()?.notify(
                    &(JsString::from("Goal complete: ") + &goal.objective),
                    Some(NotifyType::Info),
                )?;
            }
            Ok(())
        }),
        on_continue: Arc::new(move |instructions, display_id| {
            send_supervisor_instructions(&pi_for_continue, instructions, display_id)
        }),
        on_status: append_status,
        on_clear_status: Arc::new(move |session_id| clear_status.clear_session(session_id)),
        on_error: Arc::new(move |error, ctx| report_goal_scheduling_error(&errors, error, ctx)),
    })
}

fn create_idle_goal_scheduler(
    pi: &ExtensionApi,
    review_goal: &GoalEvidenceReview,
    evidence: &GoalReviewEvidenceController,
    status: &Arc<SupervisorStatusController>,
    clear_schedules: Arc<dyn Fn(&JsString) + Send + Sync>,
) -> (GoalScheduler<Goal, ReviewedGoalResponse>, ApplyIdleDecision) {
    let scheduler_slot = Arc::new(OnceLock::<GoalScheduler<Goal, ReviewedGoalResponse>>::new());
    let apply_slot = scheduler_slot.clone();
    let apply_evidence = evidence.clone();
    let apply_status = status.clone();
    let pi_for_apply = pi.clone();
    let apply_decision: ApplyIdleDecision = Arc::new(move |reviewed, goal, ctx, terminal_turn| {
        let slot = apply_slot.clone();
        let evidence = apply_evidence.clone();
        let status = apply_status.clone();
        let pi = pi_for_apply.clone();
        let clear = clear_schedules.clone();
        Box::pin(async move {
            clear(&ctx.session_manager()?.get_session_id());
            let scheduler = slot
                .get()
                .ok_or_else(|| HandlerError::new("Idle scheduler is not initialized"))?
                .clone();
            let display_id = reviewed.decision.display_id().cloned();
            let append = status.append_callback();
            let grouped: AppendSupervisorStatus =
                Arc::new(move |ctx, message, review_at, _| append(ctx, message, review_at, display_id.as_ref()));
            let on_wait = {
                let grouped = grouped.clone();
                let ctx = ctx.clone();
                let goal = goal.clone();
                move |message: JsString| {
                    let scheduler = scheduler.clone();
                    let ctx = ctx.clone();
                    let goal = goal.clone();
                    let turn = terminal_turn.clone();
                    let wake_ctx = ctx.clone();
                    let status = status.clone();
                    let callbacks = create_wait_status_callbacks(
                        grouped.clone(),
                        ctx.clone(),
                        message,
                        Arc::new(move || {
                            status.clear_session(&wake_ctx.session_manager()?.get_session_id());
                            Ok(())
                        }),
                    );
                    Box::pin(async move {
                        scheduler
                            .wait_for_agents_or_schedule_review(ctx, goal, turn, callbacks)
                            .await
                    }) as BoxFuture<'static, Result<(), HandlerError>>
                }
            };
            let is_error = matches!(reviewed.decision, GoalSupervisorResponse::Error { .. });
            apply_goal_idle_decision(reviewed.decision, goal.clone(), &ctx, &pi, &on_wait, &grouped).await?;
            if !is_error {
                evidence.consume(&ctx, &goal, reviewed.evidence_count)?;
            }
            Ok(())
        })
    });
    let review = review_goal.clone();
    let append = status.append_callback();
    let scheduler = create_goal_scheduler(GoalSchedulingOptions {
        pi: pi.clone(),
        review_goal: Arc::new(move |ctx, goal: Goal, _turn, wake_evidence| {
            let review = review.clone();
            Box::pin(async move {
                let mut payload = Map::new();
                payload.insert("objective".into(), Value::String(goal.objective));
                if let Some(evidence) = wake_evidence {
                    payload.insert("wakeEvidence".into(), evidence);
                }
                review(GoalReviewInput {
                    kind: GoalReviewKind::GoalIdleReview,
                    payload,
                    ctx,
                })
                .await
            })
        }),
        apply_decision: apply_decision.clone(),
        is_same_running_goal: Arc::new(|ctx, goal| same_running_goal(&ctx, &goal)),
        report_error: Arc::new(move |error, ctx| report_goal_scheduling_error(&append, error, ctx)),
    });
    let _ = scheduler_slot.set(scheduler.clone());
    (scheduler, apply_decision)
}

fn create_goal_extension_runtime(
    pi: &ExtensionApi,
    review_goal: &GoalEvidenceReview,
    evidence: &GoalReviewEvidenceController,
    status: Arc<SupervisorStatusController>,
) -> GoalExtensionRuntime {
    let append_status = status.append_callback();
    let empty_response_scheduler =
        create_empty_response_scheduler(super::empty_response_scheduling::EmptyResponseSchedulingOptions {
            pi: pi.clone(),
            is_same_running_goal: Arc::new(|ctx, goal| same_running_goal(&ctx, &goal)),
        });
    let error_status_scheduler =
        create_error_status_scheduler(super::error_status_scheduling::ErrorStatusSchedulingOptions {
            on_status: {
                let append_status = Arc::clone(&append_status);
                Arc::new(move |ctx, message| append_status(ctx, &JsString::from(message), None, None))
            },
        });
    let clear_slot: Arc<std::sync::Mutex<Option<ClearSessionSchedules>>> = Arc::new(std::sync::Mutex::new(None));
    let clear_slot_for_idle = Arc::clone(&clear_slot);
    let (scheduler, apply_decision) = create_idle_goal_scheduler(
        pi,
        review_goal,
        evidence,
        &status,
        Arc::new(move |session_id| {
            let callback = clear_slot_for_idle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            if let Some(callback) = callback {
                callback(session_id);
            }
        }),
    );
    let completion_scheduler = create_completion_scheduler(pi, review_goal, evidence, &status);
    let empty_for_clear = empty_response_scheduler.clone();
    let error_for_clear = error_status_scheduler.clone();
    let scheduler_for_clear = scheduler.clone();
    let completion_for_clear = completion_scheduler.clone();
    let status_for_clear = Arc::clone(&status);
    let clear_goal_schedules: ClearSessionSchedules = Arc::new(move |session_id| {
        empty_for_clear.clear_session(session_id);
        error_for_clear.clear_session(session_id);
        scheduler_for_clear.clear_session(session_id);
        completion_for_clear.clear_session(session_id);
        status_for_clear.clear_session(session_id);
    });
    *clear_slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::clone(&clear_goal_schedules));

    let empty_for_all = empty_response_scheduler.clone();
    let error_for_all = error_status_scheduler.clone();
    let scheduler_for_all = scheduler.clone();
    let completion_for_all = completion_scheduler.clone();
    let status_for_all = Arc::clone(&status);
    let clear_all_goal_schedules: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        empty_for_all.clear_all();
        error_for_all.clear_all();
        scheduler_for_all.clear_all();
        completion_for_all.clear_all();
        status_for_all.clear_all();
    });

    GoalExtensionRuntime {
        empty_response_scheduler,
        error_status_scheduler,
        scheduler,
        completion_scheduler,
        status,
        apply_decision,
        clear_goal_schedules,
        clear_all_goal_schedules,
    }
}

fn register_manage_goal(
    pi: &ExtensionApi,
    review_goal: &GoalEvidenceReview,
    evidence: &GoalReviewEvidenceController,
    runtime: &GoalExtensionRuntime,
) -> Result<(), HandlerError> {
    let runtime_scheduler = runtime.completion_scheduler.clone();
    let runtime_status = Arc::clone(&runtime.status);
    let runtime_clear = Arc::clone(&runtime.clear_goal_schedules);
    let review_goal = Arc::clone(review_goal);
    let evidence = evidence.clone();
    let pi_for_tool = pi.clone();
    register_manage_goal_tool(
        pi,
        Arc::new(move |params: ManageGoalParams, ctx: ExtensionContext| {
            let review_goal = Arc::clone(&review_goal);
            let evidence = evidence.clone();
            let pi = pi_for_tool.clone();
            let is_completion_review_current = { runtime_scheduler.create_review_guard(&ctx) };
            let on_completion_wait: OnCompletionWait = {
                let scheduler = runtime_scheduler.clone();
                Arc::new(move |goal, wait_ctx, completion_report, status_reason, display_id| {
                    let scheduler = scheduler.clone();
                    Box::pin(async move {
                        scheduler
                            .wait(goal, wait_ctx, &completion_report, &status_reason, display_id.as_ref())
                            .await
                    })
                })
            };
            let append_status = runtime_status.append_callback();
            let before_goal_save: Arc<dyn Fn() -> Result<(), HandlerError> + Send + Sync> = {
                let clear = Arc::clone(&runtime_clear);
                let ctx = ctx.clone();
                Arc::new(move || {
                    clear(&ctx.session_manager()?.get_session_id());
                    Ok(())
                })
            };
            let consume = {
                let evidence = evidence.clone();
                Arc::new(move |ctx: &ExtensionContext, goal: &Goal, count: usize| evidence.consume(ctx, goal, count))
                    as Arc<dyn Fn(&ExtensionContext, &Goal, usize) -> Result<(), HandlerError> + Send + Sync>
            };
            Box::pin(async move {
                let is_completion_review_current = Arc::from(is_completion_review_current?);
                manage_goal(ManageGoalContext {
                    ctx,
                    params,
                    pi,
                    review_goal,
                    consume_review_evidence: consume,
                    on_completion_wait,
                    append_status,
                    is_completion_review_current,
                    before_goal_save: Some(before_goal_save),
                })
                .await
            }) as BoxFuture<'static, Result<AgentToolResult, HandlerError>>
        }),
    )
}

fn register_session_goal_handlers(
    pi: &ExtensionApi,
    evidence: &GoalReviewEvidenceController,
    runtime: &GoalExtensionRuntime,
) -> Result<(), String> {
    let status = Arc::clone(&runtime.status);
    let pi_for_start = pi.clone();
    pi.on_session_start(move |event: SessionStartEvent, ctx: ExtensionContext| {
        let status = Arc::clone(&status);
        let pi = pi_for_start.clone();
        Box::pin(async move {
            inherit_previous_session_goal(&event, &ctx)?;
            let cwd = ctx.cwd()?;
            let session = ctx.session_manager()?;
            let goal = load_or_migrate_active_goal(&cwd, &session)?;
            update_goal_footer_status(&ctx)?;
            status.restore(ctx.clone())?;
            if let Some(goal) = &goal
                && goal.paused_at.is_none()
            {
                let _ = pi.request_resume_continuation();
            }
            if let Some(goal) = &goal {
                let ui = ctx.ui()?;
                ui.notify(&goal_startup_message(goal), Some(NotifyType::Info))?;
            }
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;
    let clear_all = Arc::clone(&runtime.clear_all_goal_schedules);
    pi.on_session_shutdown(move |_event, _ctx| {
        let clear_all = Arc::clone(&clear_all);
        Box::pin(async move {
            clear_all();
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;
    let evidence_for_input = evidence.clone();
    let clear_for_input = Arc::clone(&runtime.clear_goal_schedules);
    pi.on_input(move |event: InputEvent, ctx: ExtensionContext| {
        let evidence = evidence_for_input.clone();
        let clear = Arc::clone(&clear_for_input);
        Box::pin(async move {
            if let Ok(session) = ctx.session_manager() {
                clear(&session.get_session_id());
            }
            evidence.append_input(&event, &ctx)?;
            Ok(None)
        }) as BoxFuture<'static, Result<Option<crate::core::extensions::types::InputEventResult>, HandlerError>>
    })?;
    let evidence_for_tool = evidence.clone();
    pi.on_tool_result(move |event: ToolResultEvent, ctx: ExtensionContext| {
        let evidence = evidence_for_tool.clone();
        Box::pin(async move {
            evidence.append_tool_result(&event, &ctx)?;
            Ok(None)
        })
            as BoxFuture<'static, Result<Option<crate::core::extensions::types::ToolResultEventResult>, HandlerError>>
    })?;
    Ok(())
}

fn register_agent_goal_handlers(
    pi: &ExtensionApi,
    review_goal: &GoalEvidenceReview,
    runtime: &GoalExtensionRuntime,
) -> Result<(), String> {
    let error_scheduler = runtime.error_status_scheduler.clone();
    pi.on_agent_start(move |_event, ctx: ExtensionContext| {
        let error_scheduler = error_scheduler.clone();
        Box::pin(async move {
            if let Ok(session) = ctx.session_manager() {
                error_scheduler.clear_session(&session.get_session_id());
            }
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;

    let review_goal = Arc::clone(review_goal);
    let scheduler = runtime.scheduler.clone();
    let empty_response_scheduler = runtime.empty_response_scheduler.clone();
    let apply_decision = Arc::clone(&runtime.apply_decision);
    let status = Arc::clone(&runtime.status);
    let error_scheduler = runtime.error_status_scheduler.clone();
    pi.on_agent_end(move |event: AgentEndEvent, ctx: ExtensionContext| {
        let review_goal = Arc::clone(&review_goal);
        let scheduler = scheduler.clone();
        let empty_response_scheduler = empty_response_scheduler.clone();
        let apply_decision = Arc::clone(&apply_decision);
        let status = Arc::clone(&status);
        let error_scheduler = error_scheduler.clone();
        Box::pin(async move {
            if event.session_continuation.is_some() {
                return Ok(None);
            }
            let select_goal = {
                let ctx = ctx.clone();
                let empty_response_scheduler = empty_response_scheduler.clone();
                let error_scheduler = error_scheduler.clone();
                let status = Arc::clone(&status);
                let event = event.clone();
                move || {
                    let empty_for_retry = empty_response_scheduler.clone();
                    let error_for_status = error_scheduler.clone();
                    let status_for_report = Arc::clone(&status);
                    let ctx_for_report = ctx.clone();
                    let has_pending_messages = || ctx.has_pending_messages().unwrap_or(false);
                    select_goal_for_idle_review(GoalIdleSelection {
                        event: &event,
                        has_pending_messages: &has_pending_messages,
                        select_goal: || {
                            let cwd = ctx.cwd()?;
                            let session = ctx.session_manager()?;
                            load_or_migrate_running_goal(&cwd, &session)
                        },
                        clear_retry: || {
                            if let Ok(session) = ctx.session_manager() {
                                empty_for_retry.clear_session(&session.get_session_id());
                            }
                        },
                        schedule_retry: |goal| empty_for_retry.schedule(ctx.clone(), goal),
                        schedule_error_status: |message: &str| error_for_status.schedule(ctx.clone(), &message.into()),
                        report_skipped: |message: &str| {
                            let _ = status_for_report.append(ctx_for_report.clone(), message, None, None);
                        },
                    })
                }
            };
            let select_goal: Arc<dyn Fn() -> Result<Option<Goal>, HandlerError> + Send + Sync> = Arc::new(select_goal);
            run_scheduled_goal_agent_end(ScheduledGoalAgentEndOptions {
                event,
                ctx,
                review_goal,
                scheduler,
                empty_response_scheduler,
                apply_decision,
                select_goal,
                is_same_goal: Arc::new(|ctx, goal| same_running_goal(&ctx, goal)),
            })
            .await?;
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;

    let clear_for_before = Arc::clone(&runtime.clear_goal_schedules);
    pi.on_before_agent_start(move |event: BeforeAgentStartEvent, ctx: ExtensionContext| {
        let clear = Arc::clone(&clear_for_before);
        Box::pin(async move {
            if let Ok(session) = ctx.session_manager() {
                clear(&session.get_session_id());
            }
            inject_goal_context(&event, &ctx)
        }) as BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>>
    })?;
    Ok(())
}

fn register_goal_command(pi: &ExtensionApi, runtime: &GoalExtensionRuntime) -> Result<(), JsString> {
    let pi_for_command = pi.clone();
    let clear = Arc::clone(&runtime.clear_goal_schedules);
    pi.register_command(
        "goal",
        Some("Set, view, pause, resume, or clear the objective for a long-running task (/goal set <objective> | /goal | /goal pause | /goal resume | /goal clear)".into()),
        None,
        Arc::new(move |args: JsString, ctx: ExtensionCommandContext| {
            let pi = pi_for_command.clone();
            let clear = Arc::clone(&clear);
            Box::pin(async move {
                handle_goal_command(&args, &ctx, &pi, clear.as_ref())
            }) as BoxFuture<'static, Result<(), HandlerError>>
        }),
    )
}

fn handle_goal_command(
    args: &JsString,
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    clear_retry: &(dyn Fn(&JsString) + Send + Sync),
) -> Result<(), HandlerError> {
    match parse_goal_args(args) {
        ParseGoalArgsResult::Error(error) => {
            ctx.ui()?.notify(&error.into(), Some(NotifyType::Error))?;
            Ok(())
        }
        ParseGoalArgsResult::Parsed(action) => match action {
            ParsedGoalArgs::View => show_goal(ctx),
            ParsedGoalArgs::Pause => pause_goal_from_command(ctx, clear_retry),
            ParsedGoalArgs::Resume => resume_goal_from_command(ctx, pi, clear_retry),
            ParsedGoalArgs::Clear => clear_goal_from_command(ctx, clear_retry),
            ParsedGoalArgs::Set { objective } => set_goal_from_command(objective, ctx, pi, clear_retry),
        },
    }
}

fn pause_goal_from_command(
    ctx: &ExtensionCommandContext,
    clear_retry: &(dyn Fn(&JsString) + Send + Sync),
) -> Result<(), HandlerError> {
    let goal = pause_goal(&ctx.cwd()?, &ctx.session_manager()?, USER_PAUSE_REASON)?;
    if goal.is_some() {
        clear_retry(&ctx.session_manager()?.get_session_id());
    }
    let message = if goal.is_some() {
        format!("Goal paused: {USER_PAUSE_REASON}")
    } else {
        "No active goal to pause".to_owned()
    };
    ctx.ui()?.notify(&message.into(), Some(NotifyType::Info))?;
    update_goal_footer_status(ctx)
}

fn resume_goal_from_command(
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    clear_retry: &(dyn Fn(&JsString) + Send + Sync),
) -> Result<(), HandlerError> {
    let goal = resume_goal(&ctx.cwd()?, &ctx.session_manager()?)?;
    if goal.is_some() {
        clear_retry(&ctx.session_manager()?.get_session_id());
    }
    let message = match &goal {
        Some(goal) => JsString::from("Goal resumed: ") + &goal.objective,
        None => "No paused goal to resume".into(),
    };
    ctx.ui()?.notify(&message, Some(NotifyType::Info))?;
    update_goal_footer_status(ctx)?;
    if goal.is_some() && ctx.is_idle()? {
        pi.send_user_message(
            UserMessageContent::Text("Continue working toward the active goal.".into()),
            None,
        )?;
    }
    Ok(())
}

fn clear_goal_from_command(
    ctx: &ExtensionCommandContext,
    clear_retry: &(dyn Fn(&JsString) + Send + Sync),
) -> Result<(), HandlerError> {
    let cleared = clear_goal(&ctx.cwd()?, &ctx.session_manager()?)?;
    if cleared {
        clear_retry(&ctx.session_manager()?.get_session_id());
    }
    let message = if cleared { "Goal cleared" } else { "No active goal" };
    ctx.ui()?.notify(&message.into(), Some(NotifyType::Info))?;
    update_goal_footer_status(ctx)
}

fn set_goal_from_command(
    objective: JsString,
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    clear_retry: &(dyn Fn(&JsString) + Send + Sync),
) -> Result<(), HandlerError> {
    let result = set_goal(SetGoalParams {
        objective,
        ctx,
        pi,
        before_save: Some(&|| {
            clear_retry(&ctx.session_manager()?.get_session_id());
            Ok(())
        }),
    })?;
    if result.ok && !ctx.is_idle()? {
        pi.send_user_message(
            UserMessageContent::Text("Continue working toward the active goal.".into()),
            Some(crate::core::extensions::types::SendUserMessageOptions {
                deliver_as: Some(crate::core::extensions::types::UserMessageDelivery::FollowUp),
                expand_prompt_templates: None,
            }),
        )?;
    }
    ctx.ui()?.notify(&result.message, Some(result.severity))?;
    Ok(())
}

fn show_goal(ctx: &ExtensionCommandContext) -> Result<(), HandlerError> {
    let cwd = ctx.cwd()?;
    let session = ctx.session_manager()?;
    let goal = load_or_migrate_active_goal(&cwd, &session)?;
    let message = match &goal {
        Some(goal) => goal_view_message(goal),
        None => "No active goal — use /goal set <objective>".into(),
    };
    let ui = ctx.ui()?;
    ui.notify(&JsString::from(&message), Some(NotifyType::Info))?;
    Ok(())
}

/// `goalExtension`.
pub fn install_goal_extension(pi: &ExtensionApi, options: GoalExtensionOptions) -> Result<(), HandlerError> {
    let refresher = create_wait_countdown_refresher();
    let status = create_supervisor_status_controller(pi, refresher.clone());
    let review_goal = options
        .review_goal
        .unwrap_or_else(|| Arc::new(|input| review_goal_with_resident_supervisor(input)));
    let supervisor_review = with_supervisor_review_status(status.append_callback(), review_goal);
    let evidence = create_goal_review_evidence_controller(supervisor_review);
    pi.register_entry_renderer(
        super::rendering::SUPERVISOR_STATUS_TYPE,
        create_supervisor_status_entry_renderer(refresher),
    )
    .map_err(HandlerError::new)?;
    pi.register_message_renderer("supervisor", Arc::new(render_supervisor_message))
        .map_err(HandlerError::new)?;
    let runtime = create_goal_extension_runtime(pi, &evidence.review, &evidence, status);
    register_manage_goal(pi, &evidence.review, &evidence, &runtime)?;
    register_session_goal_handlers(pi, &evidence, &runtime).map_err(HandlerError::new)?;
    register_agent_goal_handlers(pi, &evidence.review, &runtime).map_err(HandlerError::new)?;
    register_goal_command(pi, &runtime).map_err(HandlerError::new)?;
    Ok(())
}

/// Registered factory for `<first-party:goal>`.
pub fn goal_extension_factory(options: GoalExtensionOptions) -> ExtensionFactory {
    Arc::new(move |pi| {
        let result = install_goal_extension(pi, options.clone());
        Box::pin(async move { result })
    })
}
