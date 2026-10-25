//! Bundled `effort` extension (`packages/coding-agent/extensions/effort/src/index.ts`).

use std::sync::{Arc, Mutex};

use indexmap::IndexSet;

use pi_agent_core::harness::ThinkingLevel;
use pi_js_compat::js_json::{JsString, JsValue};

use crate::core::agent_session::thinking_level_from_model;
use crate::core::extensions::types::{
    BeforeAgentStartEvent, BeforeAgentStartEventResult, BoxFuture, ExtensionApi, ExtensionCommandContext,
    ExtensionContext, HandlerError, NotifyType, SessionTreeEvent, ThinkingLevelSelectEvent,
};
use crate::core::session_manager::KnownSessionEntry;
use crate::modes::interactive::theme::ThemeColor;

const DELEGATION_MODE_ENTRY: &str = "multi-agent-mode";
const DELEGATION_STATUS_KEY: &str = "multi-agent-mode";
const POLICY_START: &str = "<multi_agent_mode>";
const POLICY_END: &str = "</multi_agent_mode>";

const PROACTIVE_POLICY: &str = "Proactive multi-agent delegation is active. Use sub-agents when parallel work would materially improve speed or quality. This mode remains active until a later multi-agent mode message changes it.";
const SUBAGENT_TOOLS: [&str; 10] = [
    "spawn_agent",
    "list_agents",
    "attach_session_agent",
    "wait_agent",
    "close_agent",
    "steer_agent",
    "agent_viewer",
    "send_agent_message",
    "contact_parent",
    "answer_question",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiAgentMode {
    Proactive,
    Disabled,
}

impl MultiAgentMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proactive => "proactive",
            Self::Disabled => "disabled",
        }
    }

    const fn display_name(self) -> &'static str {
        match self {
            Self::Proactive => "active",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DelegationState {
    mode: Arc<Mutex<MultiAgentMode>>,
    disabled_tools: Arc<Mutex<Vec<String>>>,
}

impl Default for DelegationState {
    fn default() -> Self {
        Self {
            mode: Arc::new(Mutex::new(MultiAgentMode::Proactive)),
            disabled_tools: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl DelegationState {
    pub fn new() -> Self {
        Self::default()
    }

    fn mode(&self) -> MultiAgentMode {
        *self.mode.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn set_mode(&self, mode: MultiAgentMode) {
        *self.mode.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = mode;
    }
}

fn is_child_runtime(ctx: &ExtensionContext) -> Result<bool, HandlerError> {
    Ok(ctx.multi_agent_agent_id()?.is_some() || ctx.multi_agent_requires_agent_id()?)
}

pub fn parse_multi_agent_mode(value: impl Into<JsString>) -> Option<MultiAgentMode> {
    match value.into() {
        value if value == "proactive" => Some(MultiAgentMode::Proactive),
        value if value == "disabled" => Some(MultiAgentMode::Disabled),
        _ => None,
    }
}

fn get_effort_levels(ctx: &ExtensionCommandContext) -> Result<Option<Vec<ThinkingLevel>>, HandlerError> {
    let Some(model) = ctx.model()? else {
        return Ok(None);
    };
    Ok(Some(
        pi_ai::models::get_supported_thinking_levels(&model)
            .into_iter()
            .map(thinking_level_from_model)
            .collect(),
    ))
}

pub fn format_effort_levels(levels: &[ThinkingLevel]) -> String {
    levels.iter().map(|level| level.as_str()).collect::<Vec<_>>().join(", ")
}

pub fn find_selected_effort(levels: &[ThinkingLevel], effort: impl Into<JsString>) -> Option<ThinkingLevel> {
    let requested = effort.into().to_lowercase();
    levels.iter().copied().find(|level| requested == level.as_str())
}

fn clear_editor(ctx: &ExtensionCommandContext) -> Result<(), HandlerError> {
    ctx.ui()?.set_editor_text(&JsString::default())?;
    Ok(())
}

fn restore_delegation_mode(state: &DelegationState, ctx: &ExtensionContext) -> Result<(), HandlerError> {
    state.set_mode(MultiAgentMode::Proactive);
    let session_manager = ctx.session_manager()?;
    for entry in session_manager.get_branch(None) {
        let known = entry.known();
        let Some(KnownSessionEntry::Custom(custom)) = known.as_deref() else {
            continue;
        };
        if custom.custom_type != DELEGATION_MODE_ENTRY {
            continue;
        }
        let Some(mode) = custom
            .data
            .as_ref()
            .and_then(|data| data.get("mode"))
            .and_then(JsValue::as_string)
            .and_then(parse_multi_agent_mode)
        else {
            continue;
        };
        state.set_mode(mode);
    }
    Ok(())
}

fn persist_delegation_mode(pi: &ExtensionApi, state: &DelegationState) -> Result<(), HandlerError> {
    pi.append_entry(
        &DELEGATION_MODE_ENTRY.into(),
        Some(serde_json::json!({ "mode": state.mode().as_str() }).into()),
    )
}

fn update_delegation_tools(pi: &ExtensionApi, state: &DelegationState) -> Result<(), HandlerError> {
    let active = pi.get_active_tools()?;
    if state.mode() == MultiAgentMode::Disabled {
        let removed: Vec<String> = active
            .iter()
            .filter(|name| SUBAGENT_TOOLS.contains(&name.as_str()))
            .cloned()
            .collect();
        {
            let mut disabled = state
                .disabled_tools
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let union: IndexSet<String> = disabled.iter().cloned().chain(removed.iter().cloned()).collect();
            *disabled = union.into_iter().collect();
        }
        if !removed.is_empty() {
            pi.set_active_tools(
                active
                    .into_iter()
                    .filter(|name| !SUBAGENT_TOOLS.contains(&name.as_str()))
                    .collect(),
            )?;
        }
        return Ok(());
    }
    let disabled = state
        .disabled_tools
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if !disabled.is_empty() {
        let restored: IndexSet<String> = active.into_iter().chain(disabled).collect();
        pi.set_active_tools(restored.into_iter().collect())?;
        state
            .disabled_tools
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
    Ok(())
}

fn update_delegation_status(ctx: &ExtensionContext, state: &DelegationState) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    // `ctx.ui.theme` throws before `initTheme`; surface it like the source.
    let theme =
        crate::modes::interactive::theme::try_current_theme().map_err(|error| HandlerError::new(error.to_string()))?;
    let prefix = theme.fg(ThemeColor::Dim, "multi-agent: ");
    ui.set_status(
        &DELEGATION_STATUS_KEY.into(),
        Some(crate::core::footer_data_provider::ExtensionStatus::Text(
            prefix + state.mode().display_name(),
        )),
    )?;
    Ok(())
}

pub fn policy_text(mode: MultiAgentMode) -> String {
    if mode == MultiAgentMode::Disabled {
        String::new()
    } else {
        format!("{POLICY_START}{PROACTIVE_POLICY}{POLICY_END}")
    }
}

pub fn remove_delegation_policy(system_prompt: &JsString) -> JsString {
    let mut prompt = system_prompt.clone();
    let mut offset = 0;
    while let Some(start) = prompt.find_from(POLICY_START, offset) {
        let Some(end) = prompt.find_from(POLICY_END, start + POLICY_START.len()) else {
            break;
        };
        let mut begin = start;
        while begin > 0 && prompt.units()[begin - 1] == 10 {
            begin -= 1;
        }
        let mut end = end + POLICY_END.len();
        while end < prompt.len() && prompt.units()[end] == 10 {
            end += 1;
        }
        prompt.replace_range(begin..end, "");
        offset = begin;
    }
    prompt.trim_end()
}

pub fn inject_delegation_policy(system_prompt: &JsString, mode: MultiAgentMode) -> JsString {
    let base_prompt = remove_delegation_policy(system_prompt);
    if mode == MultiAgentMode::Disabled {
        return base_prompt;
    }
    let policy = JsString::from(policy_text(mode));
    if base_prompt.is_empty() {
        policy
    } else {
        base_prompt + "\n\n" + policy
    }
}

fn apply_delegation_mode(
    pi: &ExtensionApi,
    ctx: &ExtensionContext,
    state: &DelegationState,
    mode: MultiAgentMode,
) -> Result<(), HandlerError> {
    state.set_mode(mode);
    persist_delegation_mode(pi, state)?;
    update_delegation_tools(pi, state)?;
    update_delegation_status(ctx, state)
}

fn set_delegation_mode(
    pi: &ExtensionApi,
    ctx: &ExtensionCommandContext,
    state: &DelegationState,
    mode: MultiAgentMode,
) -> Result<(), HandlerError> {
    if mode == MultiAgentMode::Disabled && ctx.get_thinking_level()? == ThinkingLevel::Ultra {
        ctx.set_thinking_level(ThinkingLevel::Max)?;
    }
    apply_delegation_mode(pi, ctx, state, mode)?;
    ctx.ui()?.notify(
        &format!("Multi-agent mode: {}", mode.display_name()).into(),
        Some(NotifyType::Info),
    )?;
    clear_editor(ctx)
}

fn show_invalid_mode(ctx: &ExtensionCommandContext, requested_mode: &JsString) -> Result<(), HandlerError> {
    ctx.ui()?.notify(
        &(JsString::from("Invalid multi-agent mode \"") + requested_mode + "\". Available: proactive, disabled"),
        Some(NotifyType::Warning),
    )?;
    clear_editor(ctx)
}

async fn select_delegation_mode(ctx: &ExtensionCommandContext) -> Result<Option<MultiAgentMode>, HandlerError> {
    let selected = ctx
        .ui()?
        .select(
            &"Select multi-agent mode".into(),
            &["active".into(), "disabled".into()],
            None,
        )
        .await
        .map_err(HandlerError::from)?;
    Ok(selected.and_then(|mode| {
        if mode == "active" {
            Some(MultiAgentMode::Proactive)
        } else {
            parse_multi_agent_mode(mode)
        }
    }))
}

pub async fn handle_delegation_command(
    args: &JsString,
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    state: &DelegationState,
) -> Result<(), HandlerError> {
    if is_child_runtime(ctx)? {
        ctx.ui()?.notify(
            &"Multi-agent mode is controlled by the main thread".into(),
            Some(NotifyType::Warning),
        )?;
        clear_editor(ctx)?;
        return Ok(());
    }

    let requested_mode = args.trim().to_lowercase();
    let mode = if requested_mode.is_empty() {
        select_delegation_mode(ctx).await?
    } else {
        parse_multi_agent_mode(&requested_mode)
    };
    let Some(mode) = mode else {
        if !requested_mode.is_empty() {
            show_invalid_mode(ctx, &requested_mode)?;
        } else {
            clear_editor(ctx)?;
        }
        return Ok(());
    };
    set_delegation_mode(pi, ctx, state, mode)
}

fn restore_and_update_delegation_mode(
    pi: &ExtensionApi,
    state: &DelegationState,
    ctx: &ExtensionContext,
) -> Result<(), HandlerError> {
    if is_child_runtime(ctx)? {
        return Ok(());
    }
    restore_delegation_mode(state, ctx)?;
    update_delegation_tools(pi, state)?;
    update_delegation_status(ctx, state)
}

fn set_effort(
    pi: &ExtensionApi,
    ctx: &ExtensionCommandContext,
    state: &DelegationState,
    effort: ThinkingLevel,
) -> Result<(), HandlerError> {
    let activates_proactive_delegation = effort == ThinkingLevel::Ultra && !is_child_runtime(ctx)?;
    if activates_proactive_delegation {
        apply_delegation_mode(pi, ctx, state, MultiAgentMode::Proactive)?;
    }
    ctx.set_thinking_level(effort)?;
    let label = if activates_proactive_delegation {
        "ultra (max + active)".to_owned()
    } else {
        ctx.get_thinking_level()?.as_str().to_owned()
    };
    ctx.ui()?
        .notify(&format!("Effort: {label}").into(), Some(NotifyType::Info))?;
    clear_editor(ctx)
}

fn show_invalid_effort(
    ctx: &ExtensionCommandContext,
    requested_effort: &JsString,
    levels: &[ThinkingLevel],
) -> Result<(), HandlerError> {
    ctx.ui()?.notify(
        &(JsString::from("Invalid effort \"") + requested_effort + "\". Available: " + format_effort_levels(levels)),
        Some(NotifyType::Warning),
    )?;
    Ok(())
}

async fn select_effort(
    ctx: &ExtensionCommandContext,
    levels: &[ThinkingLevel],
) -> Result<Option<ThinkingLevel>, HandlerError> {
    let options: Vec<JsString> = levels.iter().map(|level| level.as_str().into()).collect();
    let selected = ctx
        .ui()?
        .select(&"Select effort".into(), &options, None)
        .await
        .map_err(HandlerError::from)?;
    Ok(selected.and_then(|effort| find_selected_effort(levels, effort)))
}

pub async fn handle_effort_command(
    args: &JsString,
    ctx: &ExtensionCommandContext,
    pi: &ExtensionApi,
    state: &DelegationState,
) -> Result<(), HandlerError> {
    let Some(levels) = get_effort_levels(ctx)? else {
        ctx.ui()?
            .notify(&"No model selected".into(), Some(NotifyType::Warning))?;
        clear_editor(ctx)?;
        return Ok(());
    };

    let requested_effort = args.trim();
    let selected_effort = if requested_effort.is_empty() {
        select_effort(ctx, &levels).await?
    } else {
        find_selected_effort(&levels, &requested_effort)
    };
    let Some(selected_effort) = selected_effort else {
        if !requested_effort.is_empty() {
            show_invalid_effort(ctx, &requested_effort, &levels)?;
        }
        clear_editor(ctx)?;
        return Ok(());
    };
    set_effort(pi, ctx, state, selected_effort)
}

/// Bundled `effort` extension.
pub fn effort_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let state = DelegationState::new();
    register_delegation_control(pi, state.clone())?;

    pi.register_command(
        "effort",
        Some("Set model effort level (depends on selected model)".into()),
        None,
        {
            let state = state.clone();
            let pi = pi.clone();
            Arc::new(move |args, ctx| {
                let state = state.clone();
                let pi = pi.clone();
                Box::pin(async move { handle_effort_command(&args, &ctx, &pi, &state).await })
            })
        },
    )
    .map_err(HandlerError::new)?;
    Ok(())
}

fn register_delegation_control(pi: &ExtensionApi, state: DelegationState) -> Result<(), HandlerError> {
    pi.register_command("multi-agent", Some("Set multi-agent delegation mode".into()), None, {
        let state = state.clone();
        let pi = pi.clone();
        Arc::new(move |args, ctx| {
            let state = state.clone();
            let pi = pi.clone();
            Box::pin(async move { handle_delegation_command(&args, &ctx, &pi, &state).await })
        })
    })
    .map_err(HandlerError::new)?;

    pi.on_session_start({
        let state = state.clone();
        let pi = pi.clone();
        move |_event, ctx| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = state.clone();
            let pi = pi.clone();
            Box::pin(async move {
                restore_and_update_delegation_mode(&pi, &state, &ctx)?;
                Ok(None)
            })
        }
    })
    .map_err(HandlerError::new)?;

    pi.on_session_tree({
        let state = state.clone();
        let pi = pi.clone();
        move |_event: SessionTreeEvent, ctx| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = state.clone();
            let pi = pi.clone();
            Box::pin(async move {
                restore_and_update_delegation_mode(&pi, &state, &ctx)?;
                Ok(None)
            })
        }
    })
    .map_err(HandlerError::new)?;

    pi.on_thinking_level_select({
        let state = state.clone();
        let pi = pi.clone();
        move |event: ThinkingLevelSelectEvent, ctx| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let state = state.clone();
            let pi = pi.clone();
            Box::pin(async move {
                if is_child_runtime(&ctx)?
                    || event.level != ThinkingLevel::Ultra
                    || state.mode() == MultiAgentMode::Proactive
                {
                    return Ok(None);
                }
                apply_delegation_mode(&pi, &ctx, &state, MultiAgentMode::Proactive)?;
                Ok(None)
            })
        }
    })
    .map_err(HandlerError::new)?;

    pi.on_before_agent_start({
        let state = state.clone();
        move |event: BeforeAgentStartEvent,
              ctx|
              -> BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>> {
            let state = state.clone();
            Box::pin(async move {
                if is_child_runtime(&ctx)? {
                    return Ok(None);
                }
                Ok(Some(BeforeAgentStartEventResult {
                    message: None,
                    system_prompt: Some(inject_delegation_policy(&event.system_prompt, state.mode())),
                }))
            })
        }
    })
    .map_err(HandlerError::new)?;
    Ok(())
}

/// Registers the extension factory under `<first-party:effort>`.
pub fn register(registry: &mut crate::core::extensions::loader::ExtensionRegistry) {
    registry.register(
        "<first-party:effort>",
        Arc::new(|pi| {
            let result = effort_extension(pi);
            Box::pin(async move { result })
        }),
    );
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    #[test]
    fn delegation_policy_removal_retains_surrounding_raw_units() {
        let head = JsString::from_utf16(vec![0xd800]);
        let tail = JsString::from_utf16(vec![0xdc00]);
        let prompt = head.clone() + "\n" + policy_text(MultiAgentMode::Proactive) + "\n" + &tail;
        assert_eq!(remove_delegation_policy(&prompt), head + tail);
    }
}
