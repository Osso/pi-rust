//! Frozen `extensions/agents-status/src/index.ts` tree and status rendering.

use crate::core::multi_agent_store::{AgentCurrentActivity, AgentLifecycle, AgentSnapshot, is_active_lifecycle};
use crate::modes::interactive::theme::{Theme, ThemeColor};
use crate::utils::elapsed_time::format_elapsed_duration;
use pi_js_compat::date::date_parse_utf16;
use pi_js_compat::js_json::JsString;
use pi_js_compat::string::is_js_whitespace_unit;
use pi_tui::utils::{truncate_to_width_with, visible_width};
use std::collections::{HashMap, HashSet};

pub const MAX_AGENT_ROWS: usize = 5;
const MAX_NAME_WIDTH: usize = 24;
const MAX_KIND_WIDTH: usize = 28;
const TERMINAL_LINGER_MS: f64 = 10_000.0;

pub struct AgentRow<'a> {
    pub agent: &'a AgentSnapshot,
    pub depth: usize,
}

fn lifecycle_priority(lifecycle: AgentLifecycle) -> u8 {
    match lifecycle {
        AgentLifecycle::WaitingForInput => 0,
        AgentLifecycle::SteeringPending => 1,
        AgentLifecycle::Running => 2,
        AgentLifecycle::Cancelling => 3,
        AgentLifecycle::Failed => 4,
        AgentLifecycle::Aborted => 5,
        AgentLifecycle::Completed => 6,
    }
}

pub fn collect_agent_rows(agents: &[AgentSnapshot], now_ms: f64) -> Vec<AgentRow<'_>> {
    let visible: Vec<_> = agents
        .iter()
        .filter(|agent| {
            is_active_lifecycle(agent.lifecycle)
                || now_ms - date_parse_utf16(agent.updated_at.units()) < TERMINAL_LINGER_MS
        })
        .collect();
    let visible_ids: HashSet<_> = visible.iter().map(|agent| &agent.id).collect();
    let mut children: HashMap<Option<&JsString>, Vec<&AgentSnapshot>> = HashMap::new();
    for agent in &visible {
        let parent = agent
            .parent_id
            .as_ref()
            .filter(|id| !id.is_empty() && visible_ids.contains(id));
        children.entry(parent).or_default().push(agent);
    }
    for siblings in children.values_mut() {
        siblings.sort_by(|left, right| {
            lifecycle_priority(left.lifecycle)
                .cmp(&lifecycle_priority(right.lifecycle))
                .then_with(|| {
                    pi_js_compat::collator::locale_compare_utf16(right.updated_at.units(), left.updated_at.units())
                })
        });
    }
    let mut rows = Vec::new();
    append_children(&children, None, 0, &mut rows);
    rows
}

fn append_children<'a>(
    children: &HashMap<Option<&JsString>, Vec<&'a AgentSnapshot>>,
    parent: Option<&JsString>,
    depth: usize,
    rows: &mut Vec<AgentRow<'a>>,
) {
    for child in children.get(&parent).into_iter().flatten() {
        rows.push(AgentRow { agent: child, depth });
        append_children(children, Some(&child.id), depth + 1, rows);
    }
}

pub fn describe_agent_kind(agent: &AgentSnapshot) -> JsString {
    match agent.model.as_ref().filter(|model| !model.model_id.is_empty()) {
        Some(model) => agent.agent_type.clone() + " · " + &model.model_id,
        None => agent.agent_type.clone(),
    }
}

fn describe_activity(agent: &AgentSnapshot) -> JsString {
    match agent.lifecycle {
        AgentLifecycle::Running | AgentLifecycle::SteeringPending => match agent.current_activity.as_ref() {
            Some(AgentCurrentActivity::Tool { tool_name, detail, .. }) => {
                match detail.as_ref().filter(|detail| !detail.is_empty()) {
                    Some(detail) => tool_name.clone() + " " + detail,
                    None => tool_name.clone(),
                }
            }
            Some(AgentCurrentActivity::Thinking { .. }) => "thinking".into(),
            None if agent.lifecycle == AgentLifecycle::SteeringPending => "steering pending".into(),
            None => "starting".into(),
        },
        AgentLifecycle::WaitingForInput => "waiting for input".into(),
        AgentLifecycle::Cancelling => "cancelling".into(),
        AgentLifecycle::Completed => "done".into(),
        AgentLifecycle::Aborted => "aborted".into(),
        AgentLifecycle::Failed => agent
            .error
            .as_ref()
            .filter(|error| !error.message.is_empty())
            .map_or_else(|| "failed".into(), |error| JsString::from("failed: ") + &error.message),
    }
}

fn elapsed_ms(agent: &AgentSnapshot, now_ms: f64) -> f64 {
    let created_at = date_parse_utf16(agent.created_at.units());
    if !is_active_lifecycle(agent.lifecycle) {
        return date_parse_utf16(agent.updated_at.units()) - created_at;
    }
    let started_at = match &agent.current_activity {
        Some(AgentCurrentActivity::Thinking { started_at } | AgentCurrentActivity::Tool { started_at, .. }) => {
            started_at
        }
        None => &agent.created_at,
    };
    now_ms - date_parse_utf16(started_at.units())
}

fn lifecycle_icon(lifecycle: AgentLifecycle, theme: &Theme) -> JsString {
    let (color, icon) = match lifecycle {
        AgentLifecycle::Running => (ThemeColor::Accent, "●"),
        AgentLifecycle::WaitingForInput | AgentLifecycle::SteeringPending => (ThemeColor::Warning, "◐"),
        AgentLifecycle::Cancelling | AgentLifecycle::Aborted => (ThemeColor::Muted, "○"),
        AgentLifecycle::Completed => (ThemeColor::Success, "✓"),
        AgentLifecycle::Failed => (ThemeColor::Error, "✗"),
    };
    theme.fg(color, icon)
}

fn single_line(text: &JsString) -> JsString {
    let mut units = Vec::new();
    let mut pending_space = false;
    for unit in text.units() {
        if is_js_whitespace_unit(*unit) {
            pending_space = !units.is_empty();
            continue;
        }
        if pending_space {
            units.push(32);
            pending_space = false;
        }
        units.push(*unit);
    }
    JsString::from_utf16(units)
}

fn pad_column(text: &JsString, width: usize) -> JsString {
    let truncated = truncate_to_width_with(single_line(text), width, "…", false);
    let padding = width.saturating_sub(visible_width(&truncated));
    truncated + " ".repeat(padding)
}

fn widest_column(rows: &[AgentRow<'_>], describe: impl Fn(&AgentSnapshot) -> JsString, cap: usize) -> usize {
    rows.iter()
        .map(|row| visible_width(single_line(&describe(row.agent))))
        .max()
        .unwrap_or(0)
        .min(cap)
}

fn render_agent_row(row: &AgentRow<'_>, columns: (usize, usize), width: usize, now_ms: f64, theme: &Theme) -> JsString {
    let indent = if row.depth > 0 {
        "  ".repeat(row.depth - 1) + "└ "
    } else {
        String::new()
    };
    let name = pad_column(&row.agent.display_name, columns.0);
    let kind = theme.fg(ThemeColor::Dim, pad_column(&describe_agent_kind(row.agent), columns.1));
    let left =
        JsString::from(" ") + indent + lifecycle_icon(row.agent.lifecycle, theme) + " " + name + "  " + kind + "  ";
    let elapsed = theme.fg(ThemeColor::Dim, format_elapsed_duration(elapsed_ms(row.agent, now_ms)));
    let activity_width = width.saturating_sub(visible_width(&left) + visible_width(&elapsed) + 2);
    let activity = truncate_to_width_with(single_line(&describe_activity(row.agent)), activity_width, "…", false);
    let padding = width
        .saturating_sub(visible_width(&left) + visible_width(&activity) + visible_width(&elapsed))
        .max(2);
    truncate_to_width_with(
        left + theme.fg(ThemeColor::Muted, activity) + " ".repeat(padding) + elapsed,
        width,
        "…",
        false,
    )
}

pub fn render_agents_status(agents: &[AgentSnapshot], width: usize, now_ms: f64, theme: &Theme) -> Vec<JsString> {
    let rows = collect_agent_rows(agents, now_ms);
    if rows.is_empty() {
        return Vec::new();
    }
    let shown = &rows[..rows.len().min(MAX_AGENT_ROWS)];
    let columns = (
        widest_column(shown, |agent| agent.display_name.clone(), MAX_NAME_WIDTH),
        widest_column(shown, describe_agent_kind, MAX_KIND_WIDTH),
    );
    let mut lines: Vec<_> = shown
        .iter()
        .map(|row| render_agent_row(row, columns, width, now_ms, theme))
        .collect();
    if rows.len() > shown.len() {
        lines.push(truncate_to_width_with(
            theme.fg(ThemeColor::Dim, format!(" +{} more", rows.len() - shown.len())),
            width,
            "…",
            false,
        ));
    }
    lines
}

#[derive(Clone, Default)]
pub struct AgentsStatusExtensionOptions {
    pub store: Option<std::sync::Arc<crate::core::multi_agent_store::MultiAgentStore>>,
}

pub fn agents_status_extension(
    pi: &crate::core::extensions::types::ExtensionApi,
    options: &AgentsStatusExtensionOptions,
) -> Result<(), crate::core::extensions::types::HandlerError> {
    let Some(store) = options.store.clone() else {
        return Ok(());
    };
    pi.on_session_start(
        move |_: crate::core::extensions::types::SessionStartEvent,
              ctx: crate::core::extensions::types::ExtensionContext|
              -> pi_ai::request::BoxFuture<
            'static,
            Result<Option<()>, crate::core::extensions::types::HandlerError>,
        > {
            let store = std::sync::Arc::clone(&store);
            Box::pin(async move {
                ctx.ui()?.set_widget_component(
                    &"agents-status".into(),
                    Some(std::sync::Arc::new(move |input| {
                        create_agents_status_component(std::sync::Arc::clone(&store), input.tui, input.theme)
                    })),
                    None,
                );
                Ok(None)
            })
        },
    )
    .map_err(crate::core::extensions::types::HandlerError::from)
}

struct AgentsStatusState {
    store: std::sync::Arc<crate::core::multi_agent_store::MultiAgentStore>,
    tui: crate::core::extensions::types::ExtensionUiTui,
    had_rows: std::sync::atomic::AtomicBool,
}

impl AgentsStatusState {
    fn request_render_while_visible(self: &std::sync::Arc<Self>) {
        let has_rows = !collect_agent_rows(&self.store.list_agents(), pi_js_compat::date::js_now_millis()).is_empty();
        let had_rows = self.had_rows.swap(has_rows, std::sync::atomic::Ordering::SeqCst);
        if has_rows || had_rows {
            self.tui.request_component_render(std::sync::Arc::as_ptr(self) as usize);
        }
    }
}

struct AgentsStatusResources {
    timer: tokio::task::JoinHandle<()>,
    subscription: crate::core::multi_agent_store::AgentSubscription,
}

struct AgentsStatusComponent {
    state: std::sync::Arc<AgentsStatusState>,
    theme: std::sync::Arc<Theme>,
    resources: std::sync::Mutex<Option<AgentsStatusResources>>,
}

impl crate::core::extensions::types::ExtensionUiComponent for AgentsStatusComponent {
    fn render(&self, width: usize) -> Vec<JsString> {
        render_agents_status(
            &self.state.store.list_agents(),
            width,
            pi_js_compat::date::js_now_millis(),
            &self.theme,
        )
    }
    fn render_identity(&self) -> usize {
        std::sync::Arc::as_ptr(&self.state) as usize
    }
    fn dispose(&self) {
        if let Some(resources) = self.resources.lock().expect("agents status resources").take() {
            resources.timer.abort();
            drop(resources.subscription);
        }
    }
    fn invalidate(&self) {}
}
impl Drop for AgentsStatusComponent {
    fn drop(&mut self) {
        crate::core::extensions::types::ExtensionUiComponent::dispose(self);
    }
}

pub fn create_agents_status_component(
    store: std::sync::Arc<crate::core::multi_agent_store::MultiAgentStore>,
    tui: crate::core::extensions::types::ExtensionUiTui,
    theme: std::sync::Arc<Theme>,
) -> Box<dyn crate::core::extensions::types::ExtensionUiComponent> {
    let state = std::sync::Arc::new(AgentsStatusState {
        store: std::sync::Arc::clone(&store),
        tui,
        had_rows: std::sync::atomic::AtomicBool::new(false),
    });
    let updates = std::sync::Arc::clone(&state);
    let subscription = store.subscribe_agent_updates(move |_, _| {
        updates.request_render_while_visible();
        Ok(())
    });
    let ticks = std::sync::Arc::clone(&state);
    let runtime = tokio::runtime::Handle::try_current().unwrap_or_else(|error| {
        crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.to_string())
    });
    let timer = runtime.spawn(async move {
        let refresh = std::time::Duration::from_secs(1);
        let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + refresh, refresh);
        loop {
            timer.tick().await;
            ticks.request_render_while_visible();
        }
    });
    Box::new(AgentsStatusComponent {
        state,
        theme,
        resources: std::sync::Mutex::new(Some(AgentsStatusResources { timer, subscription })),
    })
}
