//! Bundled `default-footer` extension
//! (`packages/coding-agent/extensions/default-footer/src/index.ts`).

use pi_ai::request::CallbackError;
use std::sync::Arc;

use pi_js_compat::{js_format, js_json::JsString, number::to_fixed};
use pi_tui::utils::{truncate_to_width_with, visible_width};

use crate::core::extensions::types::{
    BoxFuture, ExtensionApi, ExtensionContext, ExtensionUiComponent, ExtensionUiFooterContext,
    ExtensionUiFooterFactory, HandlerError,
};
use crate::core::footer_data_provider::{BranchChangeSubscription, FooterDataProvider, FooterSessionOverride};
use crate::core::multi_agent_store::{
    AgentLifecycle, AgentSnapshot, MultiAgentStore, is_active_lifecycle, lifecycle_name,
};
use crate::core::session_manager::KnownSessionEntry;
use crate::modes::interactive::components::footer::{format_cwd_for_footer, format_tokens};
use crate::modes::interactive::theme::{Theme, ThemeColor};
use crate::utils::footer_status::{format_extension_status_text_lines, sanitize_footer_status_text};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DefaultFooterAgentLifecycleCounts {
    pub running: i64,
    pub waiting_for_input: i64,
    pub steering_pending: i64,
}

#[derive(Clone, Default)]
pub struct DefaultFooterExtensionOptions {
    pub multi_agent_store: Option<Arc<MultiAgentStore>>,
}

#[derive(Debug, Clone)]
pub struct DefaultFooterSelectedAgent {
    pub display_name: pi_js_compat::js_json::JsString,
    pub lifecycle: AgentLifecycle,
    pub slot_index: Option<i64>,
}

pub struct DefaultFooterComponentInput {
    pub ctx: ExtensionContext,
    pub footer_data: Arc<FooterDataProvider>,
    pub get_agent_counts: Option<Arc<dyn Fn() -> Option<DefaultFooterAgentLifecycleCounts> + Send + Sync>>,
    pub get_selected_agent: Option<Arc<dyn Fn() -> Option<DefaultFooterSelectedAgent> + Send + Sync>>,
    pub theme: Arc<Theme>,
}

fn format_cost(cost: f64) -> Option<String> {
    if cost == 0.0 || cost.is_nan() {
        return None;
    }
    Some(format!("cost ${}", to_fixed(cost, 2)))
}

fn footer_session(input: &DefaultFooterComponentInput) -> Result<FooterSessionOverride, HandlerError> {
    if let Some(session_override) = input.footer_data.get_session_override() {
        return Ok(session_override);
    }
    let session_manager = input.ctx.session_manager()?;
    let cwd = session_manager.get_cwd();
    Ok(FooterSessionOverride {
        cwd,
        session_manager: Some(session_manager),
        model: input.ctx.model()?,
        thinking_level: input.ctx.get_thinking_level()?.as_str().into(),
        context_usage: input
            .ctx
            .get_context_usage()?
            .map(|usage| crate::core::footer_data_provider::ContextUsage {
                tokens: usage.tokens,
                context_window: usage.context_window,
                percent: usage.percent,
            }),
    })
}

fn format_context_usage(session: &FooterSessionOverride, theme: &Theme) -> JsString {
    let usage = session.context_usage.as_ref();
    let context_window = usage
        .map(|usage| usage.context_window)
        .or_else(|| session.model.as_ref().map(|model| model.context_window))
        .unwrap_or(0.0);
    let context_window_text = format_tokens(context_window);
    let Some(percent) = usage.and_then(|usage| usage.percent) else {
        return theme.fg(ThemeColor::Dim, &format!("ctx ?/{context_window_text}"));
    };
    js_format!(
        "{}{}%{}",
        theme.fg(ThemeColor::Dim, "ctx "),
        to_fixed(percent, 1),
        theme.fg(ThemeColor::Dim, &format!("/{context_window_text}"))
    )
}

fn format_agent_counts(counts: Option<DefaultFooterAgentLifecycleCounts>) -> Option<JsString> {
    let counts = counts?;
    let active_count = counts.running + counts.waiting_for_input + counts.steering_pending;
    (active_count > 0).then(|| js_format!("agents {}", active_count.to_string()))
}

fn format_agent_lifecycle(lifecycle: AgentLifecycle) -> String {
    lifecycle_name(lifecycle).replace('_', " ")
}

fn format_selected_agent(agent: Option<DefaultFooterSelectedAgent>) -> Option<JsString> {
    let agent = agent?;
    let slot = agent.slot_index.map(|index| format!(" #{index}")).unwrap_or_default();
    Some(sanitize_footer_status_text(js_format!(
        "selected{} {} {}",
        slot,
        agent.display_name,
        format_agent_lifecycle(agent.lifecycle)
    )))
}

fn home_directory() -> Option<JsString> {
    pi_js_compat::process_env::get("HOME")
        .filter(|home| !home.is_empty())
        .or_else(|| pi_js_compat::process_env::get("USERPROFILE").filter(|home| !home.is_empty()))
        .map(Into::into)
}

fn count_matching_lifecycle(
    mut counts: DefaultFooterAgentLifecycleCounts,
    lifecycle: AgentLifecycle,
) -> DefaultFooterAgentLifecycleCounts {
    match lifecycle {
        AgentLifecycle::Running => counts.running += 1,
        AgentLifecycle::WaitingForInput => counts.waiting_for_input += 1,
        AgentLifecycle::SteeringPending => counts.steering_pending += 1,
        AgentLifecycle::Cancelling | AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::Aborted => {}
    }
    counts
}

pub fn count_default_footer_agents(store: &MultiAgentStore) -> DefaultFooterAgentLifecycleCounts {
    store
        .list_agents()
        .into_iter()
        .fold(DefaultFooterAgentLifecycleCounts::default(), |counts, agent| {
            count_matching_lifecycle(counts, agent.lifecycle)
        })
}

struct SessionUsage {
    input: f64,
    output: f64,
    cost: f64,
}

fn session_usage(session: &FooterSessionOverride) -> SessionUsage {
    let mut usage = SessionUsage {
        input: 0.0,
        output: 0.0,
        cost: 0.0,
    };
    let Some(session_manager) = session.session_manager.as_ref() else {
        return usage;
    };
    let entries = session_manager.get_entries();
    for entry in entries {
        let known = entry.known();
        let Some(KnownSessionEntry::Message(message)) = known.as_deref() else {
            continue;
        };
        let pi_agent_core::agent::AgentMessage::Assistant(assistant) = message.message.as_ref() else {
            continue;
        };
        usage.input += assistant.usage.input;
        usage.output += assistant.usage.output;
        usage.cost += assistant.usage.cost.total;
    }
    usage
}

fn format_stats(input: &DefaultFooterComponentInput) -> Result<JsString, HandlerError> {
    let session = footer_session(input)?;
    let usage = session_usage(&session);
    let executable_name = input.footer_data.get_executable_name();
    let selected_agent = format_selected_agent(input.get_selected_agent.as_ref().and_then(|get| get()));
    let mut parts = vec![
        executable_name.map(|name| input.theme.fg(ThemeColor::Dim, js_format!("[{}]", name))),
        selected_agent.map(|agent| input.theme.fg(ThemeColor::Dim, &agent)),
        format_agent_counts(input.get_agent_counts.as_ref().and_then(|get| get())),
    ];
    parts.extend(usage_parts(input, &usage));
    parts.push(Some(format_context_usage(&session, &input.theme)));

    Ok(JsString::join(parts.into_iter().flatten(), " "))
}

fn usage_parts(input: &DefaultFooterComponentInput, usage: &SessionUsage) -> Vec<Option<JsString>> {
    let mut parts = Vec::new();
    if usage.input > 0.0 {
        parts.push(Some(
            input
                .theme
                .fg(ThemeColor::Dim, &format!("in {}", format_tokens(usage.input))),
        ));
    }
    if usage.output > 0.0 {
        parts.push(Some(
            input
                .theme
                .fg(ThemeColor::Dim, &format!("out {}", format_tokens(usage.output))),
        ));
    }
    if let Some(cost) = format_cost(usage.cost) {
        parts.push(Some(input.theme.fg(ThemeColor::Dim, &cost)));
    }
    parts
}

fn format_right_side(input: &DefaultFooterComponentInput) -> Result<JsString, HandlerError> {
    let session = footer_session(input)?;
    let model_name = session
        .model
        .as_ref()
        .map(|model| model.id.clone())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| "no-model".to_owned());
    let effort_suffix = match session.model.as_ref() {
        Some(model) if model.reasoning => js_format!(" • effort {}", session.thinking_level),
        _ => JsString::default(),
    };
    if input.footer_data.get_available_provider_count() > 1.0 && session.model.is_some() {
        let provider = session
            .model
            .as_ref()
            .map(|model| model.provider.as_str())
            .unwrap_or_default();
        return Ok(js_format!("({}) {}{}", provider, model_name, effort_suffix));
    }
    Ok(js_format!("{}{}", model_name, effort_suffix))
}

fn truncate(text: &JsString, width: usize, ellipsis: &JsString) -> JsString {
    truncate_to_width_with(text, width, ellipsis, false)
}

fn format_stats_line(input: &DefaultFooterComponentInput, width: usize) -> Result<JsString, HandlerError> {
    let mut left = format_stats(input)?;
    let right = input.theme.fg(ThemeColor::Dim, &format_right_side(input)?);
    if visible_width(&left) + 2 + visible_width(&right) > width {
        let max_left = width.saturating_sub(visible_width(&right) + 2);
        left = truncate(&left, max_left, &input.theme.fg(ThemeColor::Dim, "..."));
    }
    let padding_width = width
        .saturating_sub(visible_width(&left) + visible_width(&right))
        .max(2);
    let line = js_format!("{}{}{}", left, " ".repeat(padding_width), right);
    Ok(truncate(&line, width, &input.theme.fg(ThemeColor::Dim, "...")))
}

fn format_pwd_line(input: &DefaultFooterComponentInput, width: usize) -> Result<JsString, HandlerError> {
    let session = footer_session(input)?;
    let mut pwd = format_cwd_for_footer(&session.cwd, home_directory().as_ref());
    let branch = if input.footer_data.get_session_override().is_some() {
        None
    } else {
        input.footer_data.get_git_branch()
    };
    if let Some(branch) = branch {
        pwd = js_format!("{} ({})", pwd, branch);
    }
    if let Some(session_manager) = session.session_manager.as_ref()
        && let Some(session_name) = session_manager.get_session_name()
    {
        pwd = js_format!("{} • {}", pwd, session_name);
    }
    Ok(truncate(
        &input.theme.fg(ThemeColor::Dim, &pwd),
        width,
        &input.theme.fg(ThemeColor::Dim, "..."),
    ))
}

fn selected_agent_slot_index(store: &MultiAgentStore, selected_agent: &AgentSnapshot) -> Option<i64> {
    if let Some(slot) = selected_agent.slot.as_ref() {
        return Some(slot.index);
    }
    store
        .list_agents()
        .iter()
        .position(|agent| agent.id == selected_agent.id)
        .map(|index| index as i64 + 1)
}

fn selected_agent_for_footer(store: &MultiAgentStore) -> Option<DefaultFooterSelectedAgent> {
    let selected_agent_id = store.get_selected_agent_id()?;
    let selected_agent = store.get_agent(&selected_agent_id)?;
    if !is_active_lifecycle(selected_agent.lifecycle) {
        return None;
    }
    Some(DefaultFooterSelectedAgent {
        display_name: selected_agent.display_name.clone(),
        lifecycle: selected_agent.lifecycle,
        slot_index: selected_agent_slot_index(store, &selected_agent),
    })
}

fn extension_status_lines(input: &DefaultFooterComponentInput, width: usize) -> Result<Vec<JsString>, CallbackError> {
    Ok(
        format_extension_status_text_lines(&input.footer_data.get_extension_statuses()?)
            .into_iter()
            .map(|line| truncate(&line, width, &input.theme.fg(ThemeColor::Dim, "...")))
            .collect(),
    )
}

pub struct DefaultFooterComponent {
    input: DefaultFooterComponentInput,
    subscriptions: std::sync::Mutex<Vec<BranchChangeSubscription>>,
}

impl DefaultFooterComponent {
    fn new(input: DefaultFooterComponentInput) -> Self {
        let subscription = input.footer_data.on_branch_change(|| Ok(()));
        Self {
            input,
            subscriptions: std::sync::Mutex::new(vec![subscription]),
        }
    }

    fn add_branch_change_subscription(&self, subscription: BranchChangeSubscription) {
        self.subscriptions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(subscription);
    }

    pub fn render_checked(&self, width: usize) -> Result<Vec<JsString>, CallbackError> {
        let mut lines = vec![
            format_pwd_line(&self.input, width)?,
            format_stats_line(&self.input, width)?,
        ];
        lines.extend(extension_status_lines(&self.input, width)?);
        Ok(lines)
    }
}

impl ExtensionUiComponent for DefaultFooterComponent {
    fn render(&self, width: usize) -> Vec<JsString> {
        self.render_checked(width)
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error))
    }

    fn dispose(&self) {
        self.subscriptions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

pub fn create_default_footer_component(input: DefaultFooterComponentInput) -> DefaultFooterComponent {
    DefaultFooterComponent::new(input)
}

#[cfg(test)]
mod raw_text_tests {
    use super::*;

    #[test]
    fn selected_agent_label_retains_unmatched_units() {
        for raw in [
            JsString::from_utf16(vec![0xd800]),
            JsString::from_utf16(vec![0xdc00]),
            JsString::from("😀"),
        ] {
            let result = format_selected_agent(Some(DefaultFooterSelectedAgent {
                display_name: raw.clone(),
                lifecycle: AgentLifecycle::Running,
                slot_index: Some(2),
            }))
            .unwrap();
            assert_eq!(result, JsString::from("selected #2 ") + &raw + " running");
        }
    }
}

fn build_footer_factory(ctx: ExtensionContext, store: Option<Arc<MultiAgentStore>>) -> ExtensionUiFooterFactory {
    Arc::new(
        move |footer: ExtensionUiFooterContext| -> Box<dyn ExtensionUiComponent> {
            let tui = footer.tui.clone();
            let unsubscribe = footer.footer_data.on_branch_change(move || {
                tui.request_render();
                Ok(())
            });
            let get_agent_counts = store.clone().map(|store| {
                let store = Arc::clone(&store);
                Arc::new(move || Some(count_default_footer_agents(&store)))
                    as Arc<dyn Fn() -> Option<DefaultFooterAgentLifecycleCounts> + Send + Sync>
            });
            let get_selected_agent = store.clone().map(|store| {
                let store = Arc::clone(&store);
                Arc::new(move || selected_agent_for_footer(&store))
                    as Arc<dyn Fn() -> Option<DefaultFooterSelectedAgent> + Send + Sync>
            });
            let component = create_default_footer_component(DefaultFooterComponentInput {
                ctx: ctx.clone(),
                footer_data: Arc::new(footer.footer_data.clone()),
                get_agent_counts,
                get_selected_agent,
                theme: Arc::clone(&footer.theme),
            });
            component.add_branch_change_subscription(unsubscribe);
            Box::new(component)
        },
    )
}

pub fn default_footer_extension(
    pi: &ExtensionApi,
    options: &DefaultFooterExtensionOptions,
) -> Result<(), HandlerError> {
    let store = options.multi_agent_store.clone();
    pi.on_session_start(
        move |_event, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let store = store.clone();
            Box::pin(async move {
                let factory = build_footer_factory(ctx.clone(), store);
                ctx.ui()?.set_default_footer(Some(factory));
                Ok(None)
            })
        },
    )?;
    Ok(())
}
