//! Ported from `coding-agent/src/modes/interactive/components/agent-switcher.ts`.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use pi_js_compat::string::utf16_len;
use pi_tui::tui::component;
use pi_tui::utils::truncate_to_width_with;
use pi_tui::{Component, Container, Spacer, Text, visible_width};

use crate::core::multi_agent_store::{AgentSnapshot, is_active_lifecycle, lifecycle_name};
use crate::modes::interactive::components::dynamic_border::dynamic_border_with;
use crate::modes::interactive::components::keybinding_hints::{key_hint, raw_key_hint};
use crate::modes::interactive::theme::{Theme, ThemeBg, ThemeColor};

const MAIN_THREAD_AGENT_ID: &str = "main";
const MAX_VISIBLE_AGENTS: usize = 10;

struct AgentSwitcherRow {
    agent: AgentSnapshot,
    tree_prefix: String,
}

fn format_lifecycle(lifecycle: &str) -> String {
    lifecycle.replace('_', " ")
}

fn format_agent_id(agent_id: &str) -> String {
    if utf16_len(agent_id) <= 12 {
        return agent_id.to_string();
    }
    format!("{}…", agent_id.chars().take(11).collect::<String>())
}

fn build_agent_rows(agents: &[AgentSnapshot]) -> Vec<AgentSwitcherRow> {
    let visible_agent_ids: HashSet<&str> = agents.iter().map(|agent| agent.id.as_str()).collect();
    let mut children_by_parent_id: HashMap<&str, Vec<&AgentSnapshot>> = HashMap::new();
    let mut roots: Vec<&AgentSnapshot> = Vec::new();

    for agent in agents {
        let Some(parent_id) = agent
            .parent_id
            .as_deref()
            .filter(|parent| visible_agent_ids.contains(parent))
        else {
            roots.push(agent);
            continue;
        };
        children_by_parent_id.entry(parent_id).or_default().push(agent);
    }

    let mut rows = Vec::new();
    for (root_index, root) in roots.iter().enumerate() {
        append_agent(
            &mut rows,
            &children_by_parent_id,
            root,
            "",
            root_index == roots.len() - 1,
            true,
        );
    }
    rows
}

fn append_agent(
    rows: &mut Vec<AgentSwitcherRow>,
    children_by_parent_id: &HashMap<&str, Vec<&AgentSnapshot>>,
    agent: &AgentSnapshot,
    prefix: &str,
    is_last_sibling: bool,
    is_root: bool,
) {
    let tree_prefix = if is_root {
        String::new()
    } else {
        format!("{prefix}{}", if is_last_sibling { "└─ " } else { "├─ " })
    };
    rows.push(AgentSwitcherRow {
        agent: agent.clone(),
        tree_prefix,
    });
    let children = children_by_parent_id
        .get(agent.id.as_str())
        .cloned()
        .unwrap_or_default();
    let child_prefix = if is_root {
        String::new()
    } else {
        format!("{prefix}{}", if is_last_sibling { "   " } else { "│  " })
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

/// Selectable agent list with the main-thread row.
pub struct AgentSwitcherList {
    all_agents: Vec<AgentSnapshot>,
    on_cancel: Rc<dyn Fn()>,
    on_inactive_select: Rc<dyn Fn(&AgentSnapshot)>,
    on_select: Rc<dyn Fn(&str)>,
    selected_agent_id: Option<String>,
    selected_index: usize,
    show_closed_agents: bool,
    theme: Arc<Theme>,
}

impl AgentSwitcherList {
    pub fn new(
        agents: Vec<AgentSnapshot>,
        selected_agent_id: Option<String>,
        on_select: Rc<dyn Fn(&str)>,
        on_cancel: Rc<dyn Fn()>,
        on_inactive_select: Rc<dyn Fn(&AgentSnapshot)>,
        theme: Arc<Theme>,
    ) -> Self {
        let mut list = Self {
            all_agents: agents,
            on_cancel,
            on_inactive_select,
            on_select,
            selected_agent_id,
            selected_index: 0,
            show_closed_agents: false,
            theme,
        };
        let selected_agent_index = list
            .agent_rows()
            .iter()
            .position(|row| {
                list.selected_agent_id.as_deref() == Some(row.agent.id.as_str())
                    && is_active_lifecycle(row.agent.lifecycle)
            })
            .map_or(0, |index| index + 1);
        list.selected_index = selected_agent_index;
        list
    }

    fn agents(&self) -> Vec<&AgentSnapshot> {
        if self.show_closed_agents {
            self.all_agents.iter().collect()
        } else {
            self.all_agents
                .iter()
                .filter(|agent| is_active_lifecycle(agent.lifecycle))
                .collect()
        }
    }

    fn agent_rows(&self) -> Vec<AgentSwitcherRow> {
        let agents: Vec<AgentSnapshot> = self.agents().into_iter().cloned().collect();
        build_agent_rows(&agents)
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn show_closed_agents(&self) -> bool {
        self.show_closed_agents
    }

    fn render_main_thread_line(&self, width: usize, index: usize) -> String {
        let is_selected = index == self.selected_index;
        let cursor = if is_selected {
            self.theme.fg(ThemeColor::Accent, "› ")
        } else {
            "  ".to_string()
        };
        let left_text = format!("{cursor}{}", self.theme.bold("Main thread"));
        let right_text = self.theme.fg(ThemeColor::Muted, "current");
        let spacing = width
            .saturating_sub(visible_width(&left_text) + visible_width(&right_text))
            .max(1);
        let line = format!("{left_text}{}{right_text}", " ".repeat(spacing));
        let line = if is_selected {
            self.theme.bg(ThemeBg::SelectedBg, &line)
        } else {
            line
        };
        truncate_to_width_with(&line, width, "", false)
    }

    fn render_agent_line(&self, row: &AgentSwitcherRow, index: usize, width: usize) -> String {
        let agent = &row.agent;
        let is_selected = index == self.selected_index;
        let is_current =
            self.selected_agent_id.as_deref() == Some(agent.id.as_str()) && is_active_lifecycle(agent.lifecycle);
        let cursor = if is_selected {
            self.theme.fg(ThemeColor::Accent, "› ")
        } else {
            "  ".to_string()
        };
        let slot_label = agent
            .slot
            .as_ref()
            .map_or_else(|| "--".to_string(), |slot| format!("#{}", slot.index));
        let row_label = format!("{}.", index + 1);
        let selected_marker = if is_current { " selected" } else { "" };
        let inactive_marker = if is_active_lifecycle(agent.lifecycle) {
            ""
        } else {
            " inactive"
        };
        let lifecycle = format_lifecycle(lifecycle_name(agent.lifecycle));
        let left_text = format!(
            "{cursor}{}{}",
            self.theme.fg(
                ThemeColor::Muted,
                &format!("{}{slot_label} {row_label} ", row.tree_prefix)
            ),
            self.theme.bold(&agent.display_name)
        );
        let id_text = self
            .theme
            .fg(ThemeColor::Dim, &format!(" {}", format_agent_id(&agent.id)));
        let right_text = self.theme.fg(
            ThemeColor::Muted,
            &format!("{lifecycle}{inactive_marker} {}{selected_marker}", agent.agent_type),
        );
        let left_width = visible_width(&left_text) + visible_width(&id_text);
        let spacing = width.saturating_sub(left_width + visible_width(&right_text)).max(1);
        let line = format!("{left_text}{id_text}{}{right_text}", " ".repeat(spacing));
        let line = if is_selected {
            self.theme.bg(ThemeBg::SelectedBg, &line)
        } else {
            line
        };
        truncate_to_width_with(&line, width, "", false)
    }

    fn render_visible_rows(&self, width: usize) -> Vec<String> {
        let rows = self.agent_rows();
        let row_count = rows.len() + 1;
        let start_index = (self.selected_index as i64 - (MAX_VISIBLE_AGENTS / 2) as i64)
            .min(row_count as i64 - MAX_VISIBLE_AGENTS as i64)
            .max(0) as usize;
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

    fn confirm_selection(&self) {
        if self.selected_index == 0 {
            (self.on_select)(MAIN_THREAD_AGENT_ID);
            return;
        }
        let rows = self.agent_rows();
        let Some(selected) = rows.get(self.selected_index - 1).map(|row| &row.agent) else {
            return;
        };
        if !is_active_lifecycle(selected.lifecycle) {
            (self.on_inactive_select)(selected);
            return;
        }
        (self.on_select)(&selected.id);
    }
}

impl Component for AgentSwitcherList {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.render_visible_rows(width);
        let row_count = self.agent_rows().len() + 1;
        if row_count > MAX_VISIBLE_AGENTS {
            let scroll_text = format!("  ({}/{row_count})", self.selected_index + 1);
            lines.push(self.theme.fg(
                ThemeColor::Muted,
                &truncate_to_width_with(&scroll_text, width, "", false),
            ));
        }
        let hidden_count = self
            .all_agents
            .iter()
            .filter(|agent| !is_active_lifecycle(agent.lifecycle))
            .count();
        if hidden_count > 0 {
            let status = if self.show_closed_agents {
                format!(
                    "  Showing {hidden_count} closed agent{}",
                    if hidden_count == 1 { "" } else { "s" }
                )
            } else {
                format!(
                    "  {hidden_count} closed agent{} hidden",
                    if hidden_count == 1 { "" } else { "s" }
                )
            };
            lines.push(
                self.theme
                    .fg(ThemeColor::Dim, &truncate_to_width_with(&status, width, "", false)),
            );
        }
        lines
    }

    fn handle_input(&mut self, data: &str) {
        let keybindings = pi_tui::get_keybindings();
        if keybindings.matches(data, "tui.select.up") {
            self.selected_index = self.selected_index.saturating_sub(1);
            return;
        }
        if keybindings.matches(data, "tui.select.down") {
            self.selected_index = (self.selected_index + 1).min(self.agent_rows().len());
            return;
        }
        if keybindings.matches(data, "tui.select.pageUp") {
            self.selected_index = self.selected_index.saturating_sub(MAX_VISIBLE_AGENTS);
            return;
        }
        if keybindings.matches(data, "tui.select.pageDown") {
            self.selected_index = (self.selected_index + MAX_VISIBLE_AGENTS).min(self.agent_rows().len());
            return;
        }
        if keybindings.matches(data, "app.agent.toggleClosed") {
            self.show_closed_agents = !self.show_closed_agents;
            self.selected_index = self.selected_index.min(self.agent_rows().len());
            return;
        }
        if keybindings.matches(data, "tui.select.confirm") {
            self.confirm_selection();
            return;
        }
        if keybindings.matches(data, "tui.select.cancel") {
            (self.on_cancel)();
        }
    }
}

/// Agent switcher dialog with the main-thread row and the agent tree.
pub struct AgentSwitcherComponent {
    list: AgentSwitcherList,
    top: Container,
    bottom: Container,
}

impl AgentSwitcherComponent {
    pub fn new(
        agents: Vec<AgentSnapshot>,
        selected_agent_id: Option<String>,
        on_select: Rc<dyn Fn(&str)>,
        on_cancel: Rc<dyn Fn()>,
        on_inactive_select: Rc<dyn Fn(&AgentSnapshot)>,
        theme: Arc<Theme>,
    ) -> Self {
        let list = AgentSwitcherList::new(
            agents,
            selected_agent_id,
            on_select,
            on_cancel,
            on_inactive_select,
            theme.clone(),
        );

        let accent_border = {
            let theme = theme.clone();
            Rc::new(move |text: &str| theme.fg(ThemeColor::Accent, text))
        };
        let mut top = Container::new();
        top.add_child(component(dynamic_border_with(accent_border.clone())));
        top.add_child(component(Spacer::new(1)));
        top.add_child(component(Text::new(
            theme.fg(ThemeColor::Accent, &theme.bold("  Agents")),
            1,
            0,
        )));
        let controls = [
            raw_key_hint(&theme, "↑↓", "navigate"),
            key_hint(&theme, "tui.select.confirm", "select"),
            key_hint(&theme, "app.agent.toggleClosed", "closed"),
            key_hint(&theme, "tui.select.cancel", "cancel"),
        ]
        .join("  ");
        top.add_child(component(Text::new(controls, 1, 0)));
        top.add_child(component(Spacer::new(1)));

        let mut bottom = Container::new();
        bottom.add_child(component(Spacer::new(1)));
        bottom.add_child(component(dynamic_border_with(accent_border)));

        Self { list, top, bottom }
    }

    pub fn list(&self) -> &AgentSwitcherList {
        &self.list
    }

    pub fn list_mut(&mut self) -> &mut AgentSwitcherList {
        &mut self.list
    }
}

impl Component for AgentSwitcherComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.top.render_children(width);
        lines.extend(self.list.render(width));
        lines.extend(self.bottom.render_children(width));
        lines
    }

    fn handle_input(&mut self, data: &str) {
        self.list.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.top.invalidate_children();
        self.list.invalidate();
        self.bottom.invalidate_children();
    }
}
