//! Opt-in native port of `coding-agent/examples/extensions/commands.ts`.
//! Embed this file and register its factory explicitly; it is not autoloaded.

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use pi_coding_agent::core::extensions::{ExtensionApi, ExtensionCommandContext, HandlerError, NotifyType};
use pi_coding_agent::core::slash_commands::SlashCommandInfo;

pub fn commands_extension(api: &ExtensionApi) -> Result<(), HandlerError> {
    let commands_api = api.clone();
    api.register_command(
        "commands",
        Some("List available slash commands".into()),
        Some(Arc::new(|prefix| {
            let matches: Vec<_> = ["extension", "prompt", "skill"]
                .into_iter()
                .filter(|source| pi_js_compat::js_json::JsString::from(*source).starts_with(prefix))
                .map(|source| pi_js_compat::js_json!({"value":source,"label":source}))
                .collect();
            Ok((!matches.is_empty()).then_some(matches))
        })),
        Arc::new(move |args, ctx| {
            let api = commands_api.clone();
            Box::pin(async move { list_commands(&api, &args, ctx).await })
        }),
    )?;
    Ok(())
}

async fn list_commands(api: &ExtensionApi, args: &JsString, ctx: ExtensionCommandContext) -> Result<(), HandlerError> {
    let commands = api.get_commands()?;
    let source_filter = args.trim();
    let filtered: Vec<_> = commands
        .iter()
        .filter(|command| source_filter.is_empty() || source_filter == command.source.as_str())
        .collect();
    let ui = ctx.ui()?;
    if filtered.is_empty() {
        let message = if source_filter.is_empty() {
            JsString::from("No commands found")
        } else {
            js_format!("No {} commands found", source_filter)
        };
        ui.notify(&JsString::from(&message), Some(NotifyType::Info))?;
        return Ok(());
    }

    let mut items = Vec::new();
    for (source, label) in [("extension", "Extensions"), ("prompt", "Prompts"), ("skill", "Skills")] {
        let group: Vec<_> = filtered
            .iter()
            .filter(|command| command.source.as_str() == source)
            .collect();
        if !group.is_empty() {
            items.push(js_format!("--- {} ---", label));
            items.extend(group.into_iter().map(|command| format_command(command)));
        }
    }
    let Some(selected) = ui
        .select(
            &JsString::from("Available Commands"),
            &items.iter().map(JsString::from).collect::<Vec<_>>(),
            None,
        )
        .await?
    else {
        return Ok(());
    };
    if selected.is_empty() || selected.starts_with("---") {
        return Ok(());
    }
    // JS removes one UTF-16 code unit, not one scalar. An unmatched surrogate
    // cannot match any name in the native String input domain.
    let end = selected.find(" - ").unwrap_or(selected.len());
    let name = selected.slice(1, end);
    let Some(command) = commands.iter().find(|command| name == command.name) else {
        return Ok(());
    };
    if !command.source_info.path.is_empty()
        && ui
            .confirm(
                &command.name,
                &js_format!("View source path?\n{}", command.source_info.path),
                None,
            )
            .await?
    {
        ui.notify(&command.source_info.path, Some(NotifyType::Info))?;
    }
    Ok(())
}

fn format_command(command: &SlashCommandInfo) -> JsString {
    let mut formatted = js_format!("/{}", command.name);
    if let Some(description) = command
        .description
        .as_ref()
        .filter(|description| !description.is_empty())
    {
        formatted.push_str(" - ");
        formatted.push_str(description);
    }
    formatted
}
