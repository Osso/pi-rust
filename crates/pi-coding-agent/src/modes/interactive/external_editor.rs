//! Ported from `coding-agent/src/modes/interactive/external-editor.ts`.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// `ExternalEditorOptions`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalEditorOptions {
    pub command: String,
    pub content: String,
}

/// `ExternalEditorResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalEditorResult {
    Complete { content: String },
    Failed,
}

/// Edits `content` in the user's external editor and returns the saved text.
pub fn edit_in_external_editor(options: &ExternalEditorOptions) -> ExternalEditorResult {
    let directory = tempfile::Builder::new()
        .prefix("pi-editor-")
        .tempdir()
        .expect("temporary editor directory must be creatable");
    set_private_directory_permissions(directory.path());
    let file_path = directory.path().join("prompt.md");
    std::fs::write(&file_path, &options.content).expect("prompt file must be writable");
    let _ = std::io::stdout().write_all(
        format!(
            "Launching external editor: {}\nPi will resume when the editor exits.\n",
            options.command
        )
        .as_bytes(),
    );

    let status = spawn_editor(&options.command, &file_path);
    let Some(status) = status else {
        return ExternalEditorResult::Failed;
    };
    if !status.success() {
        return ExternalEditorResult::Failed;
    }

    let content = std::fs::read(&file_path).expect("prompt file must be readable");
    let content = String::from_utf8_lossy(&content);
    ExternalEditorResult::Complete {
        content: content.strip_suffix('\n').unwrap_or(&content).to_string(),
    }
}

/// `mkdtempSync` creates the directory with mode 0700.
fn set_private_directory_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .expect("temporary editor directory must be private");
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn spawn_editor(command: &str, file_path: &PathBuf) -> Option<std::process::ExitStatus> {
    let mut parts = command.split(' ');
    let editor = parts.next().unwrap_or("");
    let editor_args: Vec<&str> = parts.collect();

    let mut process = if cfg!(target_os = "windows") {
        let mut process = Command::new("cmd");
        process.arg("/d").arg("/s").arg("/c").arg(command).arg(file_path);
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
