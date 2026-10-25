//! Ported from `coding-agent/src/modes/interactive/components/footer.ts`.

use std::sync::Arc;

use pi_ai::request::CallbackError;
use pi_ai::types::Model;
use pi_js_compat::number::to_fixed;
use pi_js_compat::{
    js_format,
    js_json::{JsString, JsValue, is_truthy},
};
use pi_tui::tui::Component;
use pi_tui::utils::{truncate_to_width_with, visible_width};

use crate::core::agent_session::AgentSession;
use crate::core::experimental::are_experimental_features_enabled;
use crate::core::footer_data_provider::{FooterDataProvider, FooterSessionOverride};
use crate::core::session_manager::KnownSessionEntry;
use crate::core::usage_totals::{add_usage_to_totals, create_usage_totals};
use crate::modes::interactive::theme::{ThemeColor, with_theme};
use crate::utils::footer_status::format_extension_status_text_lines;

pub use super::footer_format::{format_cwd_for_footer, format_tokens};

fn truncate(text: &JsString, width: usize, ellipsis: impl Into<JsString>) -> JsString {
    truncate_to_width_with(text, width, ellipsis, false)
}

#[derive(Clone, Copy, Debug)]
struct FooterContextUsage {
    context_window: f64,
    percent: Option<f64>,
}

#[derive(Clone)]
struct SessionFooterData {
    input: f64,
    output: f64,
    cache_read: f64,
    cache_write: f64,
    cost: f64,
    latest_cache_hit_rate: Option<f64>,
    context_usage: Option<FooterContextUsage>,
    session_name: Option<JsString>,
}

/// Footer showing pwd, token stats, context usage, and extension statuses.
pub struct FooterComponent {
    auto_compact_enabled: bool,
    session: Arc<AgentSession>,
    footer_data: Arc<FooterDataProvider>,
    cached_session_data: Option<SessionFooterData>,
    session_override: Option<FooterSessionOverride>,
}

impl FooterComponent {
    pub fn new(session: Arc<AgentSession>, footer_data: Arc<FooterDataProvider>) -> Self {
        Self {
            auto_compact_enabled: true,
            session,
            footer_data,
            cached_session_data: None,
            session_override: None,
        }
    }

    pub fn set_session(&mut self, session: Arc<AgentSession>) {
        self.session = session;
        self.cached_session_data = None;
        self.clear_session_override();
    }

    pub fn set_auto_compact_enabled(&mut self, enabled: bool) {
        self.auto_compact_enabled = enabled;
    }

    pub fn set_session_override(&mut self, session_override: FooterSessionOverride) {
        self.session_override = Some(session_override);
        self.cached_session_data = None;
    }

    pub fn clear_session_override(&mut self) {
        self.session_override = None;
        self.cached_session_data = None;
    }

    fn get_session_data(&mut self) -> SessionFooterData {
        if let Some(cached) = &self.cached_session_data {
            return cached.clone();
        }

        let context_usage = self.session_override.as_ref().map_or_else(
            || {
                self.session.get_context_usage().map(|usage| FooterContextUsage {
                    context_window: usage.context_window,
                    percent: Some(usage.percent),
                })
            },
            |override_| {
                override_.context_usage.map(|usage| FooterContextUsage {
                    context_window: usage.context_window,
                    percent: usage.percent,
                })
            },
        );
        let override_manager = self
            .session_override
            .as_ref()
            .and_then(|override_| override_.session_manager.clone());
        let session_manager = override_manager.or_else(|| {
            self.session_override.is_none().then(|| {
                crate::core::readonly_session_manager::create_readonly_session_manager(
                    self.session.session_manager_handle(),
                )
            })
        });
        let mut data = SessionFooterData {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            cost: 0.0,
            latest_cache_hit_rate: None,
            context_usage,
            session_name: session_manager.as_ref().and_then(|manager| manager.get_session_name()),
        };
        let mut usage_totals = create_usage_totals();
        let entries = session_manager
            .as_ref()
            .map(|manager| manager.get_entries())
            .unwrap_or_default();
        for entry in entries {
            if let Some(KnownSessionEntry::Message(message)) = entry.known().as_deref()
                && let pi_agent_core::agent::AgentMessage::Assistant(assistant) = message.message.as_ref()
            {
                add_usage_to_totals(&mut usage_totals, &assistant.usage);
                let latest_prompt_tokens =
                    assistant.usage.input + assistant.usage.cache_read + assistant.usage.cache_write;
                data.latest_cache_hit_rate =
                    (latest_prompt_tokens > 0.0).then(|| assistant.usage.cache_read / latest_prompt_tokens * 100.0);
            }
        }
        data.input = usage_totals.input;
        data.output = usage_totals.output;
        data.cache_read = usage_totals.cache_read;
        data.cache_write = usage_totals.cache_write;
        data.cost = usage_totals.cost;
        self.cached_session_data = Some(data.clone());
        data
    }
}

impl FooterComponent {
    pub fn render_checked(&mut self, width: usize) -> Result<Vec<JsString>, CallbackError> {
        let state = self.session.state();
        let displayed_model: Option<Model> = self
            .session_override
            .as_ref()
            .map_or_else(|| Some(state.model.clone()), |override_| override_.model.clone());
        let displayed_thinking_level = self.session_override.as_ref().map_or_else(
            || state.thinking_level.as_str().into(),
            |override_| override_.thinking_level.clone(),
        );
        let session_data = self.get_session_data();

        let context_usage = session_data.context_usage;
        let context_window = context_usage
            .as_ref()
            .map(|usage| usage.context_window)
            .or_else(|| displayed_model.as_ref().map(|model| model.context_window))
            .unwrap_or_else(|| {
                if self.session_override.is_some() {
                    0.0
                } else {
                    state.model.context_window
                }
            });
        let context_percent_value = context_usage.as_ref().and_then(|usage| usage.percent).unwrap_or(0.0);
        let context_percent = context_usage
            .as_ref()
            .and_then(|usage| usage.percent)
            .map_or_else(|| "?".to_owned(), |percent| to_fixed(percent, 1));

        let cwd = self.session_override.as_ref().map_or_else(
            || self.session.session_manager().get_cwd().to_owned(),
            |override_| override_.cwd.clone(),
        );
        let home = pi_js_compat::process_env::get("HOME")
            .filter(|home| !home.is_empty())
            .or_else(|| pi_js_compat::process_env::get("USERPROFILE"));
        let home = home.map(JsString::from);
        let mut pwd = format_cwd_for_footer(&cwd, home.as_ref());

        let branch = if self.session_override.is_some() {
            None
        } else {
            self.footer_data.get_git_branch()
        };
        if let Some(branch) = branch.filter(|branch| !branch.is_empty()) {
            pwd = js_format!("{} ({})", pwd, branch);
        }
        if let Some(session_name) = session_data.session_name.as_ref().filter(|name| !name.is_empty()) {
            pwd = js_format!("{} • {}", pwd, session_name);
        }

        let mut stats_parts: Vec<JsString> = Vec::new();
        if let Some(executable_name) = self.footer_data.get_executable_name().filter(|name| !name.is_empty()) {
            stats_parts.push(js_format!("[{}]", executable_name));
        }
        if is_truthy(&JsValue::Number(session_data.input)) {
            stats_parts.push(js_format!("↑{}", format_tokens(session_data.input)));
        }
        if is_truthy(&JsValue::Number(session_data.output)) {
            stats_parts.push(js_format!("↓{}", format_tokens(session_data.output)));
        }
        if is_truthy(&JsValue::Number(session_data.cache_read)) {
            stats_parts.push(js_format!("R{}", format_tokens(session_data.cache_read)));
        }
        if is_truthy(&JsValue::Number(session_data.cache_write)) {
            stats_parts.push(js_format!("W{}", format_tokens(session_data.cache_write)));
        }
        if (session_data.cache_read > 0.0 || session_data.cache_write > 0.0)
            && let Some(rate) = session_data.latest_cache_hit_rate
        {
            stats_parts.push(js_format!("CH{}%", to_fixed(rate, 1)));
        }

        let using_subscription = displayed_model
            .as_ref()
            .is_some_and(|model| self.session.model_registry().is_using_oauth(model));
        if is_truthy(&JsValue::Number(session_data.cost)) || using_subscription {
            stats_parts.push(js_format!(
                "${}{}",
                to_fixed(session_data.cost, 3),
                if using_subscription { " (sub)" } else { "" }
            ));
        }

        let auto_indicator = if self.auto_compact_enabled { " (auto)" } else { "" };
        let context_percent_display = if context_percent == "?" {
            format!("?/{}{auto_indicator}", format_tokens(context_window))
        } else {
            format!("{context_percent}%/{}{auto_indicator}", format_tokens(context_window))
        };
        let context_percent_str = if context_percent_value > 90.0 {
            with_theme(|theme| theme.fg(ThemeColor::Error, &context_percent_display))
        } else if context_percent_value > 70.0 {
            with_theme(|theme| theme.fg(ThemeColor::Warning, &context_percent_display))
        } else {
            context_percent_display.into()
        };
        stats_parts.push(context_percent_str);
        if are_experimental_features_enabled() {
            stats_parts.push(js_format!(
                "{} {}",
                with_theme(|theme| theme.fg(ThemeColor::Dim, "•")),
                with_theme(|theme| theme.bold(theme.fg(ThemeColor::Warning, "xp")))
            ));
        }

        let mut stats_left = JsString::join(stats_parts, " ");
        let mut stats_left_width = visible_width(&stats_left);
        if stats_left_width > width {
            stats_left = truncate(&stats_left, width, "...");
            stats_left_width = visible_width(&stats_left);
        }

        let min_padding = 2;
        let model_name = displayed_model
            .as_ref()
            .filter(|model| !model.id.is_empty())
            .map_or_else(|| "no-model".to_owned(), |model| model.id.clone());
        let mut right_side_without_provider = JsString::from(model_name);
        if displayed_model.as_ref().is_some_and(|model| model.reasoning) {
            right_side_without_provider =
                js_format!("{} • effort {}", right_side_without_provider, displayed_thinking_level);
        }

        let mut right_side = right_side_without_provider.clone();
        if self.footer_data.get_available_provider_count() > 1.0
            && let Some(model) = &displayed_model
        {
            right_side = js_format!("({}) {}", model.provider, right_side_without_provider);
            if stats_left_width + min_padding + visible_width(&right_side) > width {
                right_side = right_side_without_provider.clone();
            }
        }

        let right_side_width = visible_width(&right_side);
        let total_needed = stats_left_width + min_padding + right_side_width;

        let remainder = if total_needed <= width {
            let padding = " ".repeat(width - stats_left_width - right_side_width);
            js_format!("{}{}", padding, right_side)
        } else {
            let available_for_right = width.saturating_sub(stats_left_width + min_padding);
            if available_for_right > 0 {
                let truncated_right = truncate(&right_side, available_for_right, "");
                let truncated_right_width = visible_width(&truncated_right);
                let padding = " ".repeat(width.saturating_sub(stats_left_width + truncated_right_width));
                js_format!("{}{}", padding, truncated_right)
            } else {
                JsString::default()
            }
        };

        let dim_stats_left = with_theme(|theme| theme.fg(ThemeColor::Dim, &stats_left));
        let dim_remainder = with_theme(|theme| theme.fg(ThemeColor::Dim, &remainder));
        let dim_ellipsis = with_theme(|theme| theme.fg(ThemeColor::Dim, "..."));
        let pwd_line = truncate(
            &with_theme(|theme| theme.fg(ThemeColor::Dim, &pwd)),
            width,
            &dim_ellipsis,
        );
        let mut lines = vec![pwd_line, js_format!("{}{}", dim_stats_left, dim_remainder)];

        let extension_lines = format_extension_status_text_lines(&self.footer_data.get_extension_statuses()?)
            .into_iter()
            .map(|line| truncate(&line, width, &dim_ellipsis));
        lines.extend(extension_lines);
        Ok(lines)
    }
}

impl Component for FooterComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        // Frozen tui.ts:962-1013,1897-1902 leaves scheduled render throws uncaught.
        self.render_checked(width)
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error))
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.cached_session_data = None;
    }
}
