//! Native port of `regex-replace-pi-tool`.

mod cli_runner;
mod operation;
mod presentation;

use serde_json::json;
use std::sync::Arc;

use crate::core::extensions::{ExtensionApi, HandlerError, ToolDefinition, require_extension_context};
use crate::utils::paths::{node_dirname, node_join, node_normalize};
use cli_runner::JsonCliRunner;

pub fn regex_replace_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let extension_directory = node_dirname(pi.resolved_path());
    let binary_path = node_normalize(node_join(&extension_directory, "../target/release/regex-replace-json"));
    let runner = JsonCliRunner {
        pi: pi.clone(),
        binary_path,
    };
    pi.register_tool(ToolDefinition {
        name: "regex_replace".to_owned(),
        label: "Regex Replace".into(),
        description: "Safely replace a Rust regex across gitignore-aware files with an exact expected match count, dry-run diffs, stale-plan protection, and bounded scope.".into(),
        prompt_snippet: Some("Regex replace across files with exact expected match counts and unified diffs".into()),
        prompt_guidelines: Some(vec![
            "Use regex_replace for multi-file regex changes; use edit for exact single-file text replacements.".into(),
            "Call regex_replace with dryRun=true first when the expected match count is uncertain.".into(),
            "Do not pass maxFiles, maxTotalBytes, or maxMatches unless the user explicitly requests a non-default limit.".into(),
        ]),
        approval_required: Some(true),
        approval_kind: None,
        parameters: parameters(),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(move |_call_id, params, signal, _on_update, ctx| {
            let runner = runner.clone();
            Box::pin(async move {
                if signal.as_ref().is_some_and(|signal| signal.aborted()) {
                    return Err(HandlerError::new("Regex replacement cancelled before planning"));
                }
                let ctx = require_extension_context(ctx)?;
                let result = operation::run_regex_replace(&params, ctx.cwd()?, &runner, signal).await?;
                presentation::tool_result(&result)
            })
        }),
        render_call: Some(Arc::new(presentation::render_call)),
        render_result: Some(Arc::new(presentation::render_result)),
    })
}

fn parameters() -> serde_json::Value {
    json!({
        "type": "object", "additionalProperties": false,
        "properties": {
            "files": {"type":"string","description":"Gitignore-aware glob relative to the current working directory"},
            "pattern": {"type":"string","description":"Pattern using Rust regex syntax"},
            "replacement": {"type":"string","description":"Replacement using $1, $2, and $0 capture references"},
            "expectedMatches": {"type":"integer","minimum":1,"description":"Exact number of matches required"},
            "dryRun": {"type":"boolean","description":"Preview without writing (default: false)"},
            "maxFiles": {"type":"integer","minimum":1,"description":"Maximum matched files (default: 100)"},
            "maxTotalBytes": {"type":"integer","minimum":1,"description":"Maximum total input bytes (default: 10 MiB)"},
            "maxMatches": {"type":"integer","minimum":1,"description":"Maximum replacements (default: 10000)"}
        },
        "required": ["files", "pattern", "replacement", "expectedMatches"]
    })
}
