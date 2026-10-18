//! Project trust resolution (`core/project-trust.ts`).

use crate::config::package_config;
use crate::core::extensions::runner::emit_project_trust_event;
use crate::core::extensions::types::{
    ExtensionMode, LoadExtensionsResult, ProjectTrustContext, ProjectTrustEvent, ProjectTrustEventDecision,
};
use crate::core::settings_manager::DefaultProjectTrust;
use crate::core::trust_manager::{
    ProjectTrustDecision, ProjectTrustOption, ProjectTrustStore, get_project_trust_options,
    has_trust_requiring_project_resources,
};

/// `AppMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Interactive,
    Print,
    Json,
    Rpc,
}

impl AppMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::Print => "print",
            Self::Json => "json",
            Self::Rpc => "rpc",
        }
    }

    /// `mode === "interactive" ? "tui" : mode`.
    pub const fn extension_mode(self) -> ExtensionMode {
        match self {
            Self::Interactive => ExtensionMode::Tui,
            Self::Print => ExtensionMode::Print,
            Self::Json => ExtensionMode::Json,
            Self::Rpc => ExtensionMode::Rpc,
        }
    }
}

/// Options for [`resolve_project_trusted`].
pub struct ResolveProjectTrustedOptions<'a> {
    pub cwd: &'a str,
    pub trust_store: &'a ProjectTrustStore,
    pub trust_override: Option<bool>,
    pub default_project_trust: Option<DefaultProjectTrust>,
    pub extensions_result: Option<&'a LoadExtensionsResult>,
    pub project_trust_context: &'a ProjectTrustContext,
    pub on_extension_error: Option<&'a dyn Fn(String)>,
}

/// `formatProjectTrustPrompt`.
pub fn format_project_trust_prompt(cwd: &str) -> String {
    format!(
        "Trust project folder?\n{cwd}\n\nThis allows pi to load {} settings and resources, install missing project packages, and execute project extensions.",
        package_config().config_dir_name
    )
}

/// `resolveProjectTrusted`.
pub async fn resolve_project_trusted(options: ResolveProjectTrustedOptions<'_>) -> bool {
    if let Some(trust_override) = options.trust_override {
        return trust_override;
    }
    if !has_trust_requiring_project_resources(options.cwd) {
        return true;
    }

    if let Some(extensions_result) = options.extensions_result {
        let (result, errors) = emit_project_trust_event(
            extensions_result,
            ProjectTrustEvent {
                cwd: options.cwd.to_owned(),
            },
            options.project_trust_context,
        )
        .await;
        for error in errors {
            if let Some(on_extension_error) = options.on_extension_error {
                on_extension_error(format!(
                    "Extension \"{}\" project_trust error: {}",
                    error.extension_path, error.error
                ));
            }
        }
        if let Some(result) = result {
            let trusted = result.trusted == ProjectTrustEventDecision::Yes;
            if result.remember == Some(true) {
                let _ = options.trust_store.set(
                    options.cwd,
                    if trusted {
                        ProjectTrustDecision::Trusted
                    } else {
                        ProjectTrustDecision::Untrusted
                    },
                );
            }
            return trusted;
        }
    }

    match options.trust_store.get(options.cwd) {
        Ok(ProjectTrustDecision::Trusted) => return true,
        Ok(ProjectTrustDecision::Untrusted) => return false,
        Ok(ProjectTrustDecision::Unknown) | Err(_) => {}
    }

    match options.default_project_trust.unwrap_or(DefaultProjectTrust::Ask) {
        DefaultProjectTrust::Always => return true,
        DefaultProjectTrust::Never => return false,
        DefaultProjectTrust::Ask => {}
    }

    if !options.project_trust_context.has_ui {
        return false;
    }

    let selected = select_project_trust_option(options.cwd, options.project_trust_context).await;
    match selected {
        Some(selected) => {
            save_project_trust_prompt_result(options.trust_store, &selected);
            selected.trusted
        }
        None => false,
    }
}

async fn select_project_trust_option(cwd: &str, context: &ProjectTrustContext) -> Option<ProjectTrustOption> {
    let options = get_project_trust_options(cwd, true);
    let labels: Vec<String> = options.iter().map(|option| option.label.clone()).collect();
    let selected = context
        .ui
        .select(&format_project_trust_prompt(cwd), &labels, None)
        .await
        .ok()
        .flatten()?;
    options.into_iter().find(|option| option.label == selected)
}

fn save_project_trust_prompt_result(trust_store: &ProjectTrustStore, result: &ProjectTrustOption) {
    if result.updates.is_empty() {
        return;
    }
    let _ = trust_store.set_many(&result.updates);
}
