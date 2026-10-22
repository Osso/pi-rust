//! Ported from `coding-agent/src/modes/interactive/external-editor.ts`.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};

use crate::core::tools::node_fs::{node_fs_error, node_read_error};

/// `ExternalEditorOptions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalEditorOptions {
    pub command: JsString,
    pub content: JsString,
}

/// `ExternalEditorResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalEditorResult {
    Complete { content: JsString },
    Failed,
}

/// Edits `content` in the user's external editor and returns the saved text.
///
/// `mkdtempSync`/`writeFileSync`/`readFileSync` throws escape the TypeScript
/// function as a rejected promise; callers that do not catch them reach the
/// process-level uncaught handler.
pub fn edit_in_external_editor(options: &ExternalEditorOptions) -> Result<ExternalEditorResult, JsString> {
    let directory = tempfile::Builder::new()
        .prefix("pi-editor-")
        .tempdir()
        .map_err(|error| {
            JsString::from(node_fs_error(
                &error,
                "mkdtemp",
                &std::env::temp_dir().display().to_string(),
            ))
        })?;
    set_private_directory_permissions(directory.path())?;
    let file_path = directory.path().join("prompt.md");
    std::fs::write(&file_path, encode_node_utf8(&options.content))
        .map_err(|error| JsString::from(node_fs_error(&error, "open", &file_path.display().to_string())))?;
    let _ = std::io::stdout().write_all(
        encode_node_utf8(&js_format!(
            "Launching external editor: {}\nPi will resume when the editor exits.\n",
            options.command
        ))
        .as_bytes(),
    );

    let status = spawn_editor(&options.command, &file_path);
    let Some(status) = status else {
        return Ok(ExternalEditorResult::Failed);
    };
    if !status.success() {
        return Ok(ExternalEditorResult::Failed);
    }

    let content = std::fs::read(&file_path)
        .map_err(|error| JsString::from(node_read_error(&error, &file_path.display().to_string())))?;
    let content = String::from_utf8_lossy(&content);
    Ok(ExternalEditorResult::Complete {
        content: content.strip_suffix('\n').unwrap_or(&content).into(),
    })
}

/// `mkdtempSync` creates the directory with mode 0700.
fn set_private_directory_permissions(path: &std::path::Path) -> Result<(), JsString> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| JsString::from(node_fs_error(&error, "chmod", &path.display().to_string())))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn spawn_editor(command: &JsString, file_path: &PathBuf) -> Option<std::process::ExitStatus> {
    let command = encode_node_utf8(command);
    let mut parts = command.split(' ');
    let editor = parts.next().unwrap_or("");
    let editor_args: Vec<&str> = parts.collect();

    let mut process = if cfg!(target_os = "windows") {
        let mut process = Command::new("cmd");
        process.arg("/d").arg("/s").arg("/c").arg(&command).arg(file_path);
        process
    } else {
        let mut process = Command::new(editor);
        process.args(&editor_args).arg(file_path);
        process
    };
    process
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .ok()
}
