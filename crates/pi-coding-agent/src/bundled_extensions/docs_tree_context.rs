//! Bundled `docs-tree-context` extension (`extensions/docs-tree-context/src/index.ts`).

use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::core::extensions::types::{
    BeforeAgentStartEvent, BeforeAgentStartEventResult, BoxFuture, ExtensionApi, ExtensionContext, HandlerError,
    SessionStartEvent,
};
use crate::core::tools::node_fs::node_fs_error;
use crate::utils::paths::{node_join, relative_path};

const CUSTOM_SECTION_START: &str = "<repo_docs_files>";
const CUSTOM_SECTION_END: &str = "</repo_docs_files>";
const EXCLUDED_DOC_DIRS: [&str; 3] = ["claw-specs", "openclaw", "wiki"];
const EXPLICIT_DOC_FILES: [&str; 2] = ["docs/openclaw/README.md", "docs/wiki/README.md"];

pub fn docs_tree_context_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let docs_context: Arc<Mutex<Option<JsString>>> = Arc::new(Mutex::new(None));

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
                    system_prompt: Some(insert_after_user_rules(&event.system_prompt, &context)),
                }))
            })
        },
    )
    .map_err(HandlerError::new)
}

/// Node's fs error formatting repairs unpaired units in the path, as the fs boundary does.
fn fs_error(error: &std::io::Error, syscall: &str, path: &JsString) -> HandlerError {
    HandlerError::new(node_fs_error(error, syscall, &encode_node_utf8(path)))
}

fn path_exists(path: &JsString) -> Result<bool, HandlerError> {
    match std::fs::metadata(encode_node_utf8(path)) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(fs_error(&error, "stat", path)),
    }
}

/// `path.split(sep).join("/")`.
fn to_portable_path(path: &JsString) -> JsString {
    JsString::join(path.split(std::path::MAIN_SEPARATOR), "/")
}

fn collect_docs_files(cwd: &JsString, current_dir: &JsString, files: &mut Vec<JsString>) -> Result<(), HandlerError> {
    let entries =
        std::fs::read_dir(encode_node_utf8(current_dir)).map_err(|error| fs_error(&error, "scandir", current_dir))?;
    for entry in entries {
        let entry = entry.map_err(|error| HandlerError::new(error.to_string()))?;
        // `readdir` decodes entry names as UTF-8.
        let name = JsString::from(entry.file_name().to_string_lossy().as_ref());
        let absolute_path = node_join(current_dir.clone(), name.clone());
        let relative = relative_path(cwd, &absolute_path).map_err(|error| HandlerError::new(error.message().clone()))?;
        let relative_path = to_portable_path(&relative);
        let file_type = entry
            .file_type()
            .map_err(|error| HandlerError::new(error.to_string()))?;

        if file_type.is_dir() {
            if EXCLUDED_DOC_DIRS.iter().any(|excluded| name == **excluded) {
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
pub fn build_docs_context(cwd: &JsString) -> Result<Option<JsString>, HandlerError> {
    let docs_dir = node_join(cwd.clone(), "docs");
    if !path_exists(&docs_dir)? {
        return Ok(None);
    }

    let mut files = Vec::new();
    collect_docs_files(cwd, &docs_dir, &mut files)?;

    for file in EXPLICIT_DOC_FILES {
        if path_exists(&node_join(cwd.clone(), file))? {
            files.push(file.into());
        }
    }

    // `[...new Set(files)].sort()`: first occurrence wins, then UTF-16 code unit order.
    let mut seen = HashSet::new();
    files.retain(|file| seen.insert(file.clone()));
    files.sort_by(|left, right| left.units().cmp(right.units()));
    if files.is_empty() {
        return Ok(None);
    }

    Ok(Some(js_format!(
        "{}\ndocs/ file tree:\n{}\n{}",
        CUSTOM_SECTION_START,
        JsString::join(files, ", "),
        CUSTOM_SECTION_END
    )))
}

fn insert_after_user_rules(system_prompt: &JsString, docs_context: &JsString) -> JsString {
    if system_prompt.contains(CUSTOM_SECTION_START) {
        return system_prompt.to_owned();
    }

    let user_rules_end = "</user_rules>";
    let Some(index) = system_prompt.find(user_rules_end) else {
        return system_prompt.clone() + "\n\n" + docs_context.clone();
    };
    let insertion_index = index + user_rules_end.len();
    system_prompt.slice(0, insertion_index)
        + "\n\n"
        + docs_context.clone()
        + system_prompt.slice(insertion_index, system_prompt.len())
}
