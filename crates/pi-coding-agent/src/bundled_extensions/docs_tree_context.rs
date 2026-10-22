//! Bundled `docs-tree-context` extension (`extensions/docs-tree-context/src/index.ts`).

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::core::extensions::types::{
    BeforeAgentStartEvent, BeforeAgentStartEventResult, BoxFuture, ExtensionApi, ExtensionContext, HandlerError,
    SessionStartEvent,
};
use crate::core::tools::node_fs::node_fs_error;

const CUSTOM_SECTION_START: &str = "<repo_docs_files>";
const CUSTOM_SECTION_END: &str = "</repo_docs_files>";
const EXCLUDED_DOC_DIRS: [&str; 3] = ["claw-specs", "openclaw", "wiki"];
const EXPLICIT_DOC_FILES: [&str; 2] = ["docs/openclaw/README.md", "docs/wiki/README.md"];

pub fn docs_tree_context_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let docs_context: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let start_context = Arc::clone(&docs_context);
    pi.on_session_start(
        move |_event: SessionStartEvent,
              ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let start_context = Arc::clone(&start_context);
            Box::pin(async move {
                let context = build_docs_context(&ctx.cwd()?)?;
                *start_context.lock().unwrap() = context;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;

    let agent_context = Arc::clone(&docs_context);
    pi.on_before_agent_start(
        move |event: BeforeAgentStartEvent,
              _ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>> {
            let context = agent_context.lock().unwrap().clone();
            Box::pin(async move {
                let Some(context) = context else {
                    return Ok(None);
                };
                Ok(Some(BeforeAgentStartEventResult {
                    message: None,
                    system_prompt: Some(insert_after_user_rules(&event.system_prompt, &context).into()),
                }))
            })
        },
    )
    .map_err(HandlerError::new)
}

fn path_exists(path: &Path) -> Result<bool, HandlerError> {
    match std::fs::metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(HandlerError::new(node_fs_error(
            &error,
            "stat",
            &path.to_string_lossy(),
        ))),
    }
}

fn to_portable_path(path: &str) -> String {
    path.replace(std::path::MAIN_SEPARATOR, "/")
}

fn collect_docs_files(cwd: &Path, current_dir: &Path, files: &mut Vec<String>) -> Result<(), HandlerError> {
    let entries = std::fs::read_dir(current_dir)
        .map_err(|error| HandlerError::new(node_fs_error(&error, "scandir", &current_dir.to_string_lossy())))?;
    for entry in entries {
        let entry = entry.map_err(|error| HandlerError::new(error.to_string()))?;
        let absolute_path = current_dir.join(entry.file_name());
        let relative_path = to_portable_path(&crate::utils::paths::relative_path(
            &cwd.to_string_lossy(),
            &absolute_path.to_string_lossy(),
        ));
        let file_type = entry
            .file_type()
            .map_err(|error| HandlerError::new(error.to_string()))?;

        if file_type.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if EXCLUDED_DOC_DIRS.contains(&name.as_str()) {
                continue;
            }
            collect_docs_files(cwd, &absolute_path, files)?;
            continue;
        }

        if file_type.is_file() {
            files.push(relative_path);
        }
    }
    Ok(())
}

/// `buildDocsContext`.
pub fn build_docs_context(cwd: &str) -> Result<Option<String>, HandlerError> {
    let cwd = Path::new(cwd);
    let docs_dir = cwd.join("docs");
    if !path_exists(&docs_dir)? {
        return Ok(None);
    }

    let mut files = Vec::new();
    collect_docs_files(cwd, &docs_dir, &mut files)?;

    for file in EXPLICIT_DOC_FILES {
        if path_exists(&cwd.join(file))? {
            files.push(file.to_owned());
        }
    }

    let mut seen = HashSet::new();
    files.retain(|file| seen.insert(file.clone()));
    files.sort_by(|left, right| left.encode_utf16().cmp(right.encode_utf16()));
    if files.is_empty() {
        return Ok(None);
    }

    Ok(Some(format!(
        "{CUSTOM_SECTION_START}\ndocs/ file tree:\n{}\n{CUSTOM_SECTION_END}",
        files.join(", ")
    )))
}

fn insert_after_user_rules(system_prompt: &str, docs_context: &str) -> String {
    if system_prompt.contains(CUSTOM_SECTION_START) {
        return system_prompt.to_owned();
    }

    let user_rules_end = "</user_rules>";
    let Some(index) = system_prompt.find(user_rules_end) else {
        return format!("{system_prompt}\n\n{docs_context}");
    };
    let insertion_index = index + user_rules_end.len();
    format!(
        "{}\n\n{docs_context}{}",
        &system_prompt[..insertion_index],
        &system_prompt[insertion_index..]
    )
}
