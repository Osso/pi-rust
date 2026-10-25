//! Project trust UI context (`cli/project-trust.ts`).

use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};
use std::sync::Arc;

use pi_ai::request::{BoxFuture, CallbackError};

use crate::core::extensions::types::{ExtensionUiContext, NotifyType, ProjectTrustContext};
use crate::core::project_trust::AppMode;
use crate::utils::chalk::Chalk;

/// Startup selectors used by the trust prompt. The TUI-backed implementation
/// arrives with `cli/startup-ui.ts`; the trait keeps this module testable.
pub trait ProjectTrustStartupUi: Send + Sync {
    fn select<'a>(
        &'a self,
        title: &'a JsString,
        options: &'a [JsString],
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>>;
    fn input<'a>(
        &'a self,
        title: &'a JsString,
        placeholder: Option<&'a JsString>,
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>>;
}

/// Process-terminal adapter for prompts before the main TUI exists.
pub struct TerminalProjectTrustStartupUi {
    pub settings: crate::core::settings_manager::Settings,
}

fn run_startup_dialog<F, Fut>(
    settings: crate::core::settings_manager::Settings,
    show: F,
) -> BoxFuture<'static, Result<Option<JsString>, CallbackError>>
where
    F: FnOnce(crate::core::settings_manager::SettingsManager) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<Option<JsString>, CallbackError>> + 'static,
{
    Box::pin(async move {
        tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| CallbackError::new(error.to_string()))?;
            let settings = crate::core::settings_manager::SettingsManager::in_memory(settings, Default::default());
            runtime.block_on(show(settings))
        })
        .await
        .map_err(|error| CallbackError::new(format!("Startup UI worker failed: {error}")))?
    })
}

impl ProjectTrustStartupUi for TerminalProjectTrustStartupUi {
    fn select<'a>(
        &'a self,
        title: &'a JsString,
        options: &'a [JsString],
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        let title = title.clone();
        let choices = options.iter().map(|label| (label.clone(), label.clone())).collect();
        run_startup_dialog(self.settings.clone(), move |settings| async move {
            let terminal = std::rc::Rc::new(std::cell::RefCell::new(pi_tui::terminal::ProcessTerminal::new()));
            let env = pi_js_compat::process_env::vars().into_iter().collect();
            crate::cli::startup_ui::show_startup_selector(&settings, terminal, &env, &title, choices).await
        })
    }

    fn input<'a>(
        &'a self,
        title: &'a JsString,
        placeholder: Option<&'a JsString>,
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        let title = title.clone();
        let placeholder = placeholder.cloned();
        run_startup_dialog(self.settings.clone(), move |settings| async move {
            let terminal = std::rc::Rc::new(std::cell::RefCell::new(pi_tui::terminal::ProcessTerminal::new()));
            let env = pi_js_compat::process_env::vars().into_iter().collect();
            crate::cli::startup_ui::show_startup_input(&settings, terminal, &env, &title, placeholder.as_ref()).await
        })
    }
}

pub struct CreateProjectTrustContextOptions {
    pub cwd: JsString,
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
        title: &JsString,
        options: &[JsString],
        _opts: Option<&crate::core::extensions::types::ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        let title = title.to_owned();
        let options = options.to_vec();
        Box::pin(async move {
            if !self.interactive() {
                return Ok(None);
            }
            let Some(startup_ui) = self.startup_ui.as_ref() else {
                return Ok(None);
            };
            startup_ui.select(&title, &options).await
        })
    }

    fn confirm<'a>(
        &'a self,
        title: &JsString,
        message: &JsString,
        _opts: Option<&crate::core::extensions::types::ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, CallbackError>> {
        let title = js_format!("{}\n{}", title, message);
        Box::pin(async move {
            if !self.interactive() {
                return Ok(false);
            }
            let Some(startup_ui) = self.startup_ui.as_ref() else {
                return Ok(false);
            };
            let options = [JsString::from("Yes"), JsString::from("No")];
            let selected = startup_ui.select(&title, &options).await?;
            Ok(selected.as_ref().is_some_and(|selected| selected == "Yes"))
        })
    }

    fn input<'a>(
        &'a self,
        title: &JsString,
        placeholder: Option<&JsString>,
        _opts: Option<&crate::core::extensions::types::ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        let title = title.to_owned();
        let placeholder = placeholder.cloned();
        Box::pin(async move {
            if !self.interactive() {
                return Ok(None);
            }
            let Some(startup_ui) = self.startup_ui.as_ref() else {
                return Ok(None);
            };
            startup_ui.input(&title, placeholder.as_ref()).await
        })
    }

    fn notify(
        &self,
        message: &pi_js_compat::js_json::JsString,
        notify_type: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        if self.mode == AppMode::Interactive {
            return Ok(());
        }
        let chalk = Chalk::stderr();
        let colored = match notify_type.unwrap_or(NotifyType::Info) {
            NotifyType::Error => chalk.red(message),
            NotifyType::Warning => chalk.yellow(message),
            NotifyType::Info => chalk.cyan(message),
        };
        eprintln!("{}", encode_node_utf8(&colored));
        Ok(())
    }
}
