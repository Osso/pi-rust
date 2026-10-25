//! Ported from `packages/coding-agent/src/modes/interactive/components/agent-switcher.ts`.

use std::rc::Rc;

use pi_js_compat::{js_format, js_json::JsString};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, Container, component};
use pi_tui::utils::{truncate_to_width_with, visible_width};

use super::super::theme::{ThemeBg, ThemeColor, with_theme};
use super::keybinding_hints::{key_hint, raw_key_hint};
use crate::core::multi_agent_store::{AgentLifecycle, AgentSnapshot, is_active_lifecycle};

const MAIN_THREAD_AGENT_ID: &str = "main";
const MAX_VISIBLE_AGENTS: usize = 10;

struct AgentSwitcherRow {
    agent: AgentSnapshot,
    tree_prefix: JsString,
}

fn lifecycle_name(lifecycle: AgentLifecycle) -> &'static str {
    match lifecycle {
        AgentLifecycle::Running => "running",
        AgentLifecycle::WaitingForInput => "waiting_for_input",
        AgentLifecycle::SteeringPending => "steering_pending",
        AgentLifecycle::Cancelling => "cancelling",
        AgentLifecycle::Completed => "completed",
        AgentLifecycle::Failed => "failed",
        AgentLifecycle::Aborted => "aborted",
    }
}

fn format_lifecycle(lifecycle: AgentLifecycle) -> JsString {
    JsString::from(lifecycle_name(lifecycle)).replace("_", " ")
}

fn format_agent_id(agent_id: &JsString) -> JsString {
    if agent_id.len() <= 12 {
        return agent_id.to_owned();
    }
    agent_id.slice(0, 11) + "…"
}

fn build_agent_rows(agents: &[AgentSnapshot]) -> Vec<AgentSwitcherRow> {
    let visible_agent_ids: std::collections::HashSet<&JsString> = agents.iter().map(|agent| &agent.id).collect();
    let mut children_by_parent_id: Vec<(JsString, Vec<AgentSnapshot>)> = Vec::new();
    let mut roots: Vec<AgentSnapshot> = Vec::new();

    for agent in agents {
        let parent = agent
            .parent_id
            .as_ref()
            .filter(|parent_id| visible_agent_ids.contains(parent_id));
        let Some(parent_id) = parent else {
            roots.push(agent.clone());
            continue;
        };
        match children_by_parent_id.iter_mut().find(|(id, _)| id == parent_id) {
            Some((_, siblings)) => siblings.push(agent.clone()),
            None => children_by_parent_id.push((parent_id.to_owned(), vec![agent.clone()])),
        }
    }

    fn append_agent(
        rows: &mut Vec<AgentSwitcherRow>,
        children_by_parent_id: &[(JsString, Vec<AgentSnapshot>)],
        agent: &AgentSnapshot,
        prefix: &JsString,
        is_last_sibling: bool,
        is_root: bool,
    ) {
        let tree_prefix = if is_root {
            JsString::default()
        } else {
            js_format!("{}{}", prefix, if is_last_sibling { "└─ " } else { "├─ " })
        };
        rows.push(AgentSwitcherRow {
            agent: agent.clone(),
            tree_prefix,
        });
        let children = children_by_parent_id
            .iter()
            .find(|(id, _)| *id == agent.id)
            .map(|(_, children)| children.as_slice())
            .unwrap_or_default();
        let child_prefix = if is_root {
            JsString::default()
        } else {
            js_format!("{}{}", prefix, if is_last_sibling { "   " } else { "│  " })
        };
        for (child_index, child) in children.iter().enumerate() {
            append_agent(
                rows,
                children_by_parent_id,
                child,
                &child_prefix,
                child_index == children.len() - 1,
                false,
            );
        }
    }

    let mut rows = Vec::new();
    for (root_index, root) in roots.iter().enumerate() {
        append_agent(
            &mut rows,
            &children_by_parent_id,
            root,
            &JsString::default(),
            root_index == roots.len() - 1,
            true,
        );
    }
    rows
}

struct AgentSwitcherList {
    all_agents: Vec<AgentSnapshot>,
    on_cancel: Rc<dyn Fn()>,
    on_inactive_select: Rc<dyn Fn(AgentSnapshot)>,
    on_select: Rc<dyn Fn(JsString)>,
    selected_agent_id: Option<JsString>,
    selected_index: usize,
    show_closed_agents: bool,
}

impl AgentSwitcherList {
    fn new(
        agents: Vec<AgentSnapshot>,
        selected_agent_id: Option<JsString>,
        on_select: Rc<dyn Fn(JsString)>,
        on_cancel: Rc<dyn Fn()>,
        on_inactive_select: Rc<dyn Fn(AgentSnapshot)>,
    ) -> Self {
        let active_agents: Vec<AgentSnapshot> = agents
            .iter()
            .filter(|agent| is_active_lifecycle(agent.lifecycle))
            .cloned()
            .collect();
        let selected_agent_index = build_agent_rows(&active_agents)
            .iter()
            .position(|row| Some(&row.agent.id) == selected_agent_id.as_ref())
            .map(|index| index + 1)
            .unwrap_or(0);
        Self {
            all_agents: agents,
            on_cancel,
            on_inactive_select,
            on_select,
            selected_agent_id,
            selected_index: selected_agent_index,
            show_closed_agents: false,
        }
    }

    fn agents(&self) -> Vec<AgentSnapshot> {
        if self.show_closed_agents {
            self.all_agents.clone()
        } else {
            self.all_agents
                .iter()
                .filter(|agent| is_active_lifecycle(agent.lifecycle))
                .cloned()
                .collect()
        }
    }

    fn agent_rows(&self) -> Vec<AgentSwitcherRow> {
        build_agent_rows(&self.agents())
    }

    fn render_main_thread_line(&self, width: usize, index: usize) -> JsString {
        let is_selected = index == self.selected_index;
        let cursor = if is_selected {
            with_theme(|theme| theme.fg(ThemeColor::Accent, "› "))
        } else {
            JsString::from("  ")
        };
        let left_text = js_format!("{}{}", cursor, with_theme(|theme| theme.bold("Main thread")));
        let right_text = with_theme(|theme| theme.fg(ThemeColor::Muted, "current"));
        let spacing = width
            .saturating_sub(visible_width(&left_text) + visible_width(&right_text))
            .max(1);
        let mut line = left_text + &" ".repeat(spacing) + &right_text;
        if is_selected {
            line = with_theme(|theme| theme.bg(ThemeBg::SelectedBg, &line));
        }
        truncate_to_width_with(&line, width, "", false)
    }

    fn render_agent_line(&self, row: &AgentSwitcherRow, index: usize, width: usize) -> JsString {
        let agent = &row.agent;
        let is_selected = index == self.selected_index;
        let is_current = Some(&agent.id) == self.selected_agent_id.as_ref() && is_active_lifecycle(agent.lifecycle);
        let cursor = if is_selected {
            with_theme(|theme| theme.fg(ThemeColor::Accent, "› "))
        } else {
            JsString::from("  ")
        };
        let slot_label = agent
            .slot
            .as_ref()
            .map_or_else(|| JsString::from("--"), |slot| js_format!("#{}", slot.index));
        let row_label = js_format!("{}.", index + 1);
        let selected_marker = if is_current { " selected" } else { "" };
        let inactive_marker = if is_active_lifecycle(agent.lifecycle) {
            ""
        } else {
            " inactive"
        };
        let lifecycle = format_lifecycle(agent.lifecycle);
        let left_text = js_format!(
            "{}{}{}",
            cursor,
            with_theme(|theme| theme.fg(
                ThemeColor::Muted,
                &js_format!("{}{} {} ", &row.tree_prefix, slot_label, row_label)
            )),
            with_theme(|theme| theme.bold(&agent.display_name))
        );
        let id_text = with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!(" {}", format_agent_id(&agent.id))));
        let right_text = with_theme(|theme| {
            theme.fg(
                ThemeColor::Muted,
                &js_format!(
                    "{}{} {}{}",
                    lifecycle,
                    inactive_marker,
                    &agent.agent_type,
                    selected_marker
                ),
            )
        });
        let left_width = visible_width(&left_text) + visible_width(&id_text);
        let spacing = width.saturating_sub(left_width + visible_width(&right_text)).max(1);
        let mut line = left_text + &id_text + &" ".repeat(spacing) + &right_text;
        if is_selected {
            line = with_theme(|theme| theme.bg(ThemeBg::SelectedBg, &line));
        }
        truncate_to_width_with(&line, width, "", false)
    }

    fn confirm_selection(&self) {
        if self.selected_index == 0 {
            (self.on_select)(MAIN_THREAD_AGENT_ID.into());
            return;
        }
        let rows = self.agent_rows();
        let Some(row) = rows.get(self.selected_index - 1) else {
            return;
        };
        if !is_active_lifecycle(row.agent.lifecycle) {
            (self.on_inactive_select)(row.agent.clone());
            return;
        }
        (self.on_select)(row.agent.id.clone());
    }
}

impl Component for AgentSwitcherList {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = self.render_visible_rows(width);
        let row_count = self.agent_rows().len() + 1;
        if row_count > MAX_VISIBLE_AGENTS {
            let scroll_text = js_format!("  ({}/{})", self.selected_index + 1, row_count);
            lines.push(with_theme(|theme| {
                theme.fg(
                    ThemeColor::Muted,
                    truncate_to_width_with(&scroll_text, width, "", false),
                )
            }));
        }
        let hidden_count = self
            .all_agents
            .iter()
            .filter(|agent| !is_active_lifecycle(agent.lifecycle))
            .count();
        if hidden_count > 0 {
            let plural = if hidden_count == 1 { "" } else { "s" };
            let status = if self.show_closed_agents {
                js_format!("  Showing {} closed agent{}", hidden_count, plural)
            } else {
                js_format!("  {} closed agent{} hidden", hidden_count, plural)
            };
            lines.push(with_theme(|theme| {
                theme.fg(ThemeColor::Dim, truncate_to_width_with(&status, width, "", false))
            }));
        }
        lines
    }

    fn handle_input(&mut self, key_data: &JsString) {
        let keybindings = get_keybindings();
        if keybindings.matches(key_data, "tui.select.up") {
            self.selected_index = self.selected_index.saturating_sub(1);
            return;
        }
        if keybindings.matches(key_data, "tui.select.down") {
            self.selected_index = (self.selected_index + 1).min(self.agent_rows().len());
            return;
        }
        if keybindings.matches(key_data, "tui.select.pageUp") {
            self.selected_index = self.selected_index.saturating_sub(MAX_VISIBLE_AGENTS);
            return;
        }
        if keybindings.matches(key_data, "tui.select.pageDown") {
            self.selected_index = (self.selected_index + MAX_VISIBLE_AGENTS).min(self.agent_rows().len());
            return;
        }
        if keybindings.matches(key_data, "app.agent.toggleClosed") {
            self.show_closed_agents = !self.show_closed_agents;
            self.selected_index = self.selected_index.min(self.agent_rows().len());
            return;
        }
        if keybindings.matches(key_data, "tui.select.confirm") {
            self.confirm_selection();
            return;
        }
        if keybindings.matches(key_data, "tui.select.cancel") {
            (self.on_cancel)();
        }
    }

    fn invalidate(&mut self) {}
}

impl AgentSwitcherList {
    fn render_visible_rows(&self, width: usize) -> Vec<JsString> {
        let rows = self.agent_rows();
        let row_count = rows.len() + 1;
        let start_index = self
            .selected_index
            .saturating_sub(MAX_VISIBLE_AGENTS / 2)
            .min(row_count.saturating_sub(MAX_VISIBLE_AGENTS));
        let end_index = (start_index + MAX_VISIBLE_AGENTS).min(row_count);
        let mut lines = Vec::new();
        for index in start_index..end_index {
            if index == 0 {
                lines.push(self.render_main_thread_line(width, index));
                continue;
            }
            if let Some(row) = rows.get(index - 1) {
                lines.push(self.render_agent_line(row, index, width));
            }
        }
        lines
    }
}

/// Agent tree switcher with borders and key hints.
pub struct AgentSwitcherComponent {
    container: Container,
    list: ComponentRef,
}

impl AgentSwitcherComponent {
    pub fn new(
        agents: Vec<AgentSnapshot>,
        selected_agent_id: Option<JsString>,
        on_select: Rc<dyn Fn(JsString)>,
        on_cancel: Rc<dyn Fn()>,
        on_inactive_select: Rc<dyn Fn(AgentSnapshot)>,
    ) -> Self {
        let list = AgentSwitcherList::new(agents, selected_agent_id, on_select, on_cancel, on_inactive_select);
        let list = component(list);

        let mut container = Container::new();
        container.add_child(component(dynamic_border_with_accent()));
        container.add_child(component(Spacer::new(1)));
        container.add_child(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Accent, theme.bold("  Agents"))),
            1,
            0,
        )));
        container.add_child(component(Text::new(
            raw_key_hint("↑↓", "navigate")
                + "  "
                + &key_hint("tui.select.confirm", "select")
                + "  "
                + &key_hint("app.agent.toggleClosed", "closed")
                + "  "
                + &key_hint("tui.select.cancel", "cancel"),
            1,
            0,
        )));
        container.add_child(component(Spacer::new(1)));
        container.add_child(Rc::clone(&list));
        container.add_child(component(Spacer::new(1)));
        container.add_child(component(dynamic_border_with_accent()));

        Self { container, list }
    }

    pub fn get_list(&self) -> ComponentRef {
        Rc::clone(&self.list)
    }
}

fn dynamic_border_with_accent() -> pi_tui::components::dynamic_border::DynamicBorder {
    pi_tui::components::dynamic_border::DynamicBorder::new(Rc::new(|line: &JsString| {
        with_theme(|theme| theme.fg(ThemeColor::Accent, line))
    }))
}

impl Component for AgentSwitcherComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn handle_input(&mut self, key_data: &JsString) {
        self.list.borrow_mut().handle_input(key_data);
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
    }
}
