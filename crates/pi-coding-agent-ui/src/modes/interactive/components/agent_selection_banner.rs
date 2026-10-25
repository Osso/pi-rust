//! Ported from `coding-agent/src/modes/interactive/components/agent-selection-banner.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::sync::Arc;

use pi_tui::{Component, truncate_to_width};

use crate::core::multi_agent_store::{MultiAgentStore, lifecycle_name};

use crate::modes::interactive::theme::{Theme, ThemeColor};

fn format_lifecycle(lifecycle: &str) -> String {
    lifecycle.replace('_', " ")
}

fn selected_agent_text(store: &MultiAgentStore) -> Option<JsString> {
    let selected_agent_id = store.get_selected_agent_id()?;
    let selected_agent = store.get_agent(&selected_agent_id)?;
    Some(js_format!(
        "Agent {}: {} ({})",
        &selected_agent.id,
        &selected_agent.display_name,
        format_lifecycle(lifecycle_name(selected_agent.lifecycle))
    ))
}

/// Banner showing the currently selected child agent.
pub struct AgentSelectionBannerComponent {
    store: Option<Arc<MultiAgentStore>>,
    theme: Arc<Theme>,
}

impl AgentSelectionBannerComponent {
    pub fn new(store: Option<Arc<MultiAgentStore>>, theme: Arc<Theme>) -> Self {
        Self { store, theme }
    }
}

impl Component for AgentSelectionBannerComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let Some(store) = &self.store else {
            return Vec::new();
        };
        let Some(text) = selected_agent_text(store) else {
            return Vec::new();
        };
        let label = self.theme.fg(ThemeColor::Accent, self.theme.bold(&text));
        vec![truncate_to_width(&label, width)]
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}
}
