//! Opt-in native footer with live branch token totals and branch-change redraws.
use pi_agent_core::AgentMessage;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::footer_data_provider::{BranchChangeSubscription, FooterDataProvider};
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry};
use pi_coding_agent::modes::interactive::theme::{Theme, ThemeColor};
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::number::{js_number_to_string, to_fixed};
use pi_tui::utils::{truncate_to_width, visible_width};
use std::sync::{Arc, Mutex};

struct CustomFooter {
    ctx: ExtensionCommandContext,
    theme: Arc<Theme>,
    data: FooterDataProvider,
    subscription: Mutex<Option<BranchChangeSubscription>>,
}

fn format_tokens(value: f64) -> String {
    if value < 1000.0 {
        js_number_to_string(value)
    } else {
        format!("{}k", to_fixed(value / 1000.0, 1))
    }
}

impl ExtensionUiComponent for CustomFooter {
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        let manager = self.ctx.session_manager().expect("active footer context");
        let branch = manager.get_branch(None);
        let (mut input, mut output, mut cost) = (0.0, 0.0, 0.0);
        for entry in branch {
            if let SessionEntry::Known(KnownSessionEntry::Message(entry)) = entry
                && let AgentMessage::Assistant(message) = *entry.message
            {
                input += message.usage.input;
                output += message.usage.output;
                cost += message.usage.cost.total;
            }
        }
        let left = self.theme.fg(
            ThemeColor::Dim,
            format!(
                "↑{} ↓{} ${}",
                format_tokens(input),
                format_tokens(output),
                to_fixed(cost, 3)
            ),
        );
        let branch = self
            .data
            .get_git_branch()
            .filter(|branch| !branch.is_empty())
            .map(|branch| js_format!(" ({})", branch))
            .unwrap_or_default();
        let model = self
            .ctx
            .model()
            .expect("active footer model")
            .map(|model| model.id)
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| "no-model".into());
        let right = self.theme.fg(ThemeColor::Dim, &js_format!("{}{}", model, branch));
        let pad = " ".repeat(
            width
                .saturating_sub(visible_width(&left) + visible_width(&right))
                .max(1),
        );
        vec![truncate_to_width(&js_format!("{}{}{}", left, pad, right), width)]
    }
    fn invalidate(&self) {}
    fn dispose(&self) {
        self.subscription.lock().expect("footer subscription lock").take();
    }
}

pub fn custom_footer_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let enabled = Arc::new(Mutex::new(false));
    pi.register_command(
        "footer",
        Some("Toggle custom footer".into()),
        None,
        Arc::new(move |_, ctx| {
            let enabled = enabled.clone();
            Box::pin(async move {
                let enabled = {
                    let mut value = enabled.lock().map_err(|error| HandlerError::new(error.to_string()))?;
                    *value = !*value;
                    *value
                };
                let ui = ctx.ui()?;
                if enabled {
                    ui.set_footer(Some(Arc::new(move |context| {
                        let tui = context.tui;
                        let subscription = context.footer_data.on_branch_change(move || {
                            tui.request_render();
                            Ok(())
                        });
                        Box::new(CustomFooter {
                            ctx: ctx.clone(),
                            theme: context.theme,
                            data: context.footer_data,
                            subscription: Mutex::new(Some(subscription)),
                        })
                    })));
                    ui.notify(&JsString::from("Custom footer enabled"), Some(NotifyType::Info))?;
                } else {
                    ui.set_footer(None);
                    ui.notify(&JsString::from("Default footer restored"), Some(NotifyType::Info))?;
                }
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
