use std::process::{Command, Stdio};

use pi_js_compat::js_json::{JsString, encode_node_utf8};

use crate::utils::paths::{PathInputOptions, resolve_path};

/// Read the absolute git common directory for `cwd`, if `cwd` is in a repository.
pub fn read_git_common_directory(cwd: &JsString) -> Option<JsString> {
    let output = Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .current_dir(encode_node_utf8(cwd))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let common_directory = String::from_utf8_lossy(&output.stdout);
    let trimmed = common_directory.trim();
    if trimmed.is_empty() {
        return None;
    }

    resolve_path(trimmed, cwd, &PathInputOptions::default()).ok()
}
