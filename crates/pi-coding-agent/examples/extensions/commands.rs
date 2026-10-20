//! Opt-in native port of `coding-agent/examples/extensions/commands.ts`.
//! Embed this file and register its factory explicitly; it is not autoloaded.

use std::sync::Arc;

use pi_coding_agent::core::extensions::{ExtensionApi, ExtensionCommandContext, HandlerError, NotifyType};
use pi_coding_agent::core::slash_commands::SlashCommandInfo;
use pi_js_compat::string::js_trim;
use serde_json::json;

pub fn commands_extension(api: &ExtensionApi) -> Result<(), HandlerError> {
    let commands_api = api.clone();
    api.register_command(
        "commands",
        Some("List available slash commands".to_owned()),
        Some(Arc::new(|prefix| {
            let matches: Vec<_> = ["extension", "prompt", "skill"]
                .into_iter()
                .filter(|source| source.starts_with(prefix))
                .map(|source| json!({"value":source,"label":source}))
                .collect();
            (!matches.is_empty()).then_some(matches)
        })),
        Arc::new(move |args, ctx| {
            let api = commands_api.clone();
            Box::pin(async move { list_commands(&api, &args, ctx).await })
        }),
    )?;
    Ok(())
}

async fn list_commands(api: &ExtensionApi, args: &str, ctx: ExtensionCommandContext) -> Result<(), HandlerError> {
    let commands = api.get_commands()?;
    let source_filter = js_trim(args);
    let filtered: Vec<_> = commands
        .iter()
        .filter(|command| source_filter.is_empty() || command.source.as_str() == source_filter)
        .collect();
    let ui = ctx.ui()?;
    if filtered.is_empty() {
        let message = if source_filter.is_empty() {
            "No commands found".to_owned()
        } else {
            format!("No {source_filter} commands found")
        };
        ui.notify(&message, Some(NotifyType::Info));
        return Ok(());
    }

    let mut items = Vec::new();
    for (source, label) in [("extension", "Extensions"), ("prompt", "Prompts"), ("skill", "Skills")] {
        let group: Vec<_> = filtered
            .iter()
            .filter(|command| command.source.as_str() == source)
            .collect();
        if !group.is_empty() {
            items.push(format!("--- {label} ---"));
            items.extend(group.into_iter().map(|command| format_command(command)));
        }
    }
    let Some(selected) = ui.select("Available Commands", &items, None).await? else {
        return Ok(());
    };
    if selected.is_empty() || selected.starts_with("---") {
        return Ok(());
    }
    // JS removes one UTF-16 code unit, not one scalar. An unmatched surrogate
    // cannot match any name in the native String input domain.
    let name_units: Vec<_> = selected
        .split(" - ")
        .next()
        .unwrap_or_default()
        .encode_utf16()
        .skip(1)
        .collect();
    let Ok(name) = String::from_utf16(&name_units) else {
        return Ok(());
    };
    let Some(command) = commands.iter().find(|command| command.name == name) else {
        return Ok(());
    };
    if !command.source_info.path.is_empty()
        && ui
            .confirm(
                &command.name,
                &format!("View source path?\n{}", command.source_info.path),
                None,
            )
            .await?
    {
        ui.notify(&command.source_info.path, Some(NotifyType::Info));
    }
    Ok(())
}

fn format_command(command: &SlashCommandInfo) -> String {
    let mut formatted = format!("/{}", command.name);
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
