//! Project trust UI context (`cli/project-trust.ts`).

use std::sync::Arc;

use pi_ai::request::BoxFuture;

use crate::core::extensions::types::{ExtensionUiContext, NotifyType, ProjectTrustContext};
use crate::core::project_trust::AppMode;
use crate::utils::chalk::Chalk;

/// Startup selectors used by the trust prompt. The TUI-backed implementation
/// arrives with `cli/startup-ui.ts`; the trait keeps this module testable.
pub trait ProjectTrustStartupUi: Send + Sync {
    fn select<'a>(&'a self, title: &'a str, options: &'a [String]) -> BoxFuture<'a, Option<String>>;
    fn input<'a>(&'a self, title: &'a str, placeholder: Option<&'a str>) -> BoxFuture<'a, Option<String>>;
}

pub struct CreateProjectTrustContextOptions {
    pub cwd: String,
    pub mode: AppMode,
    pub has_ui: bool,
    pub startup_ui: Option<Arc<dyn ProjectTrustStartupUi>>,
}

/// `createProjectTrustContext`.
pub fn create_project_trust_context(options: CreateProjectTrustContextOptions) -> ProjectTrustContext {
    ProjectTrustContext {
        cwd: options.cwd,
        mode: options.mode.extension_mode(),
        has_ui: options.has_ui,
        ui: Arc::new(ProjectTrustUi {
            mode: options.mode,
            has_ui: options.has_ui,
            startup_ui: options.startup_ui,
        }),
    }
}

struct ProjectTrustUi {
    mode: AppMode,
    has_ui: bool,
    startup_ui: Option<Arc<dyn ProjectTrustStartupUi>>,
}

impl ProjectTrustUi {
    fn interactive(&self) -> bool {
        self.has_ui && self.mode == AppMode::Interactive
    }
}

impl ExtensionUiContext for ProjectTrustUi {
    fn select<'a>(
        &'a self,
        title: &str,
        options: &[String],
        _opts: Option<&crate::core::extensions::types::ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<String>, String>> {
        let title = title.to_owned();
        let options = options.to_vec();
        Box::pin(async move {
            if !self.interactive() {
                return Ok(None);
            }
            let Some(startup_ui) = self.startup_ui.as_ref() else {
                return Ok(None);
            };
            Ok(startup_ui.select(&title, &options).await)
        })
    }

    fn confirm<'a>(
        &'a self,
        title: &str,
        message: &str,
        _opts: Option<&crate::core::extensions::types::ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, String>> {
        let title = format!("{title}\n{message}");
        Box::pin(async move {
            if !self.interactive() {
                return Ok(false);
            }
            let Some(startup_ui) = self.startup_ui.as_ref() else {
                return Ok(false);
            };
            let options = ["Yes".to_owned(), "No".to_owned()];
            let selected = startup_ui.select(&title, &options).await;
            Ok(selected.as_deref() == Some("Yes"))
        })
    }

    fn input<'a>(
        &'a self,
        title: &str,
        placeholder: Option<&str>,
        _opts: Option<&crate::core::extensions::types::ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<String>, String>> {
        let title = title.to_owned();
        let placeholder = placeholder.map(str::to_owned);
        Box::pin(async move {
            if !self.interactive() {
                return Ok(None);
            }
            let Some(startup_ui) = self.startup_ui.as_ref() else {
                return Ok(None);
            };
            Ok(startup_ui.input(&title, placeholder.as_deref()).await)
        })
    }

    fn notify(&self, message: &pi_js_compat::js_json::JsString, notify_type: Option<NotifyType>) {
        if self.mode == AppMode::Interactive {
            return;
        }
        let message = pi_js_compat::js_json::encode_node_utf8(message);
        let message = message.as_str();
        let chalk = Chalk::stderr();
        let colored = match notify_type.unwrap_or(NotifyType::Info) {
            NotifyType::Error => chalk.red(message),
            NotifyType::Warning => chalk.yellow(message),
            NotifyType::Info => chalk.cyan(message),
        };
        eprintln!("{colored}");
    }
}
