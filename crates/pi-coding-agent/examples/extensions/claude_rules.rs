//! Opt-in native `claude-rules.ts`; lists rules without loading their contents.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
use pi_js_compat::node_fs::node_fs_error_record;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub fn claude_rules_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let files = Arc::new(Mutex::new(Vec::<String>::new()));
    let scanned = files.clone();
    pi.on_session_start(
        move |_: SessionStartEvent, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let scanned = scanned.clone();
            Box::pin(async move {
                let path = Path::new(&encode_node_utf8(&ctx.cwd()?)).join(".claude/rules");
                let found = find_markdown_files(&path, "")?;
                let count = found.len();
                *scanned.lock().map_err(|e| HandlerError::new(e.to_string()))? = found;
                if count > 0 {
                    ctx.ui()?.notify(
                        &JsString::from(format!("Found {count} rule(s) in .claude/rules/")),
                        Some(NotifyType::Info),
                    );
                }
                Ok(None)
            })
        },
    )?;
    pi.on_before_agent_start(move |event:BeforeAgentStartEvent,_:ExtensionContext|->BoxFuture<'static,Result<Option<BeforeAgentStartEventResult>,HandlerError>> {
        let files=files.clone();
        Box::pin(async move {
            let files=files.lock().map_err(|e|HandlerError::new(e.to_string()))?;
            if files.is_empty(){return Ok(None);}
            let list=files.iter().map(|file|format!("- .claude/rules/{file}")).collect::<Vec<_>>().join("\n");
            Ok(Some(BeforeAgentStartEventResult{message:None,system_prompt:Some(js_format!("{}\n\n## Project Rules\n\nThe following project rules are available in .claude/rules/:\n\n{}\n\nWhen working on tasks related to these rules, use the read tool to load the relevant rule files for guidance.\n", event.system_prompt, list))}))
        })
    }).map_err(HandlerError::new)
}

fn find_markdown_files(dir: &Path, base: &str) -> Result<Vec<String>, HandlerError> {
    // Node existsSync returns false on a failed existence probe, but readdir
    // errors for an existing non-directory still propagate.
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let fs_error = |e: std::io::Error| {
        HandlerError::new(
            node_fs_error_record(
                &e,
                "scandir",
                &JsString::from(dir.to_str().expect("filesystem path must be UTF-8")),
            )
            .message,
        )
    };
    let mut entries = std::fs::read_dir(dir)
        .map_err(fs_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(fs_error)?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut results = Vec::new();
    for entry in entries {
        let name = pi_js_compat::node_fs::decode_node_path(&entry.file_name())
            .to_utf8()
            .expect("Node OS-byte decoder returns Unicode scalars");
        let relative = if base.is_empty() {
            name.clone()
        } else {
            format!("{base}/{name}")
        };
        let kind = entry.file_type().map_err(fs_error)?;
        if kind.is_dir() {
            results.extend(find_markdown_files(&entry.path(), &relative)?);
        } else if kind.is_file() && name.ends_with(".md") {
            results.push(relative);
        }
    }
    Ok(results)
}
