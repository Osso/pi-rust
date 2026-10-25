//! Opt-in native port of `coding-agent/examples/extensions/custom-header.ts`.
//! Embed this source and register `custom_header_extension` in an explicit factory.

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use pi_coding_agent::core::extensions::types::{
    BoxFuture, ExtensionApi, ExtensionContext, ExtensionMode, ExtensionUiComponent, ExtensionUiHeader, HandlerError,
    NotifyType,
};
use pi_coding_agent::modes::interactive::theme::{Theme, ThemeColor};

struct CustomHeader {
    theme: Arc<Theme>,
}

impl ExtensionUiComponent for CustomHeader {
    fn render(&self, _width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        let mut lines = pi_mascot(&self.theme);
        lines.push(js_format!(
            "{}{}",
            self.theme.fg(ThemeColor::Muted, "   shitty coding agent"),
            self.theme
                .fg(ThemeColor::Dim, &js_format!(" v{}", pi_coding_agent::config::version()))
        ));
        lines
    }

    fn invalidate(&self) {}
}

fn pi_mascot(theme: &Theme) -> Vec<JsString> {
    let eye = js_format!("█{}", theme.fg(ThemeColor::Dim, "▌"));
    let eyes = js_format!("     {}  {}", eye, eye);
    let bar = js_format!("  {}", theme.fg(ThemeColor::Accent, "█".repeat(14)));
    let leg = js_format!(
        "     {}    {}",
        theme.fg(ThemeColor::Accent, "██"),
        theme.fg(ThemeColor::Accent, "██")
    );
    vec![
        JsString::default(),
        eyes,
        bar,
        leg.clone(),
        leg.clone(),
        leg.clone(),
        leg,
        JsString::default(),
    ]
}

pub fn custom_header_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_session_start(
        |_event, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            Box::pin(async move {
                if ctx.mode()? == ExtensionMode::Tui {
                    ctx.ui()?.set_header(Some(Arc::new(|context| {
                        Ok(ExtensionUiHeader {
                            component: Box::new(CustomHeader { theme: context.theme }),
                            set_expanded: None,
                        })
                    })))?;
                }
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;

    pi.register_command(
        "builtin-header",
        Some("Restore built-in header with keybinding hints".into()),
        None,
        Arc::new(|_args, ctx| {
            Box::pin(async move {
                let ui = ctx.ui()?;
                ui.set_header(None)?;
                ui.notify(&JsString::from("Built-in header restored"), Some(NotifyType::Info))?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
