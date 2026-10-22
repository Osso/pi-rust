//! Bundled `approval-controls` extension (`extensions/approval-controls/src/index.ts`).

use std::sync::Arc;

use crate::core::extensions::types::{ExtensionApi, ExtensionCommandContext, HandlerError, NotifyType};
use crate::core::permissions::presets::{SandboxProfileName, SandboxProfileScope, is_sandbox_profile_name};
use pi_js_compat::js_json::JsString;

const SANDBOX_USAGE: &str = "Usage: /sandbox [read-only|workspace-write|full-access|inherit] [session|project|global]";

struct SandboxCommandSelection {
    profile: Option<SandboxProfileName>,
    scope: SandboxProfileScope,
}

pub fn approval_controls_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_command(
        "approvals",
        Some("Select approval preset".to_owned()),
        None,
        Arc::new(|_args: pi_js_compat::js_json::JsString, ctx: ExtensionCommandContext| {
            Box::pin(async move {
                ctx.show_approval_selector()?;
                clear_editor(&ctx)?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)?;

    pi.register_command(
        "sandbox",
        Some("Select or set sandbox profile".to_owned()),
        None,
        Arc::new(|args: pi_js_compat::js_json::JsString, ctx: ExtensionCommandContext| {
            Box::pin(async move { handle_sandbox_command(&args, &ctx).await })
        }),
    )
    .map_err(HandlerError::new)
}

async fn handle_sandbox_command(
    args: &pi_js_compat::js_json::JsString,
    ctx: &ExtensionCommandContext,
) -> Result<(), HandlerError> {
    if args.trim().is_empty() {
        ctx.show_sandbox_selector()?;
        clear_editor(ctx)?;
        return Ok(());
    }
    let Some(selection) = parse_sandbox_selection(args) else {
        ctx.ui()?.notify(&SANDBOX_USAGE.into(), Some(NotifyType::Warning));
        clear_editor(ctx)?;
        return Ok(());
    };
    ctx.set_sandbox_profile(selection.profile, selection.scope)?;
    ctx.ui()?.notify(
        &format!(
            "Sandbox profile: {} ({})",
            selection.profile.map(SandboxProfileName::as_str).unwrap_or("inherit"),
            scope_str(selection.scope)
        )
        .into(),
        Some(NotifyType::Info),
    );
    clear_editor(ctx)?;
    Ok(())
}

fn clear_editor(ctx: &ExtensionCommandContext) -> Result<(), HandlerError> {
    ctx.ui()?.set_editor_text(&JsString::default());
    Ok(())
}

fn parse_sandbox_selection(args: &pi_js_compat::js_json::JsString) -> Option<SandboxCommandSelection> {
    let normalized = args.trim().to_lowercase().to_utf8().ok()?;
    let mut parts = normalized.split_whitespace();
    let profile_argument = parts.next()?;
    let scope_argument = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let scope = match scope_argument {
        "session" => SandboxProfileScope::Session,
        "project" => SandboxProfileScope::Project,
        "global" => SandboxProfileScope::Global,
        _ => return None,
    };
    if profile_argument == "inherit" {
        return (scope == SandboxProfileScope::Session).then_some(SandboxCommandSelection { profile: None, scope });
    }
    if !is_sandbox_profile_name(profile_argument) {
        return None;
    }
    Some(SandboxCommandSelection {
        profile: SandboxProfileName::parse(profile_argument),
        scope,
    })
}

fn scope_str(scope: SandboxProfileScope) -> &'static str {
    match scope {
        SandboxProfileScope::Session => "session",
        SandboxProfileScope::Project => "project",
        SandboxProfileScope::Global => "global",
    }
}
