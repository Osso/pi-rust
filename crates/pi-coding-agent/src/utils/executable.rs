//! POSIX executable availability (`utils/executable.ts`).

use indexmap::IndexMap;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::node_error::NativeNodeError;
use pi_js_compat::node_fs::{decode_node_path, node_fs_error_record};
use serde::Deserialize;

use super::paths::{node_join, node_resolve};

const DEFAULT_UNIX_PATH: &str = "/usr/bin:/bin";

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExecutableAvailabilityOptions {
    pub cwd: Option<JsString>,
    /// Preserve property order: the first case-insensitive PATH key wins, even if undefined.
    pub env: Option<IndexMap<JsString, Option<JsString>>>,
}

pub fn is_executable_available(
    command: &JsString,
    options: &ExecutableAvailabilityOptions,
) -> Result<bool, NativeNodeError> {
    if command.is_empty() {
        return Ok(false);
    }
    let cwd = match &options.cwd {
        Some(cwd) => cwd.clone(),
        None => read_process_cwd()?,
    };
    if command.contains('/') {
        let path = resolve_executable_path(&cwd, command)?;
        return Ok(is_executable_file(&path));
    }
    let environment = options.env.clone().unwrap_or_else(read_process_environment);
    let search_path = read_environment_value(&environment, "PATH").unwrap_or_else(|| DEFAULT_UNIX_PATH.into());
    Ok(search_path.split(':').any(|directory| {
        let directory = if directory.is_empty() { cwd.clone() } else { directory };
        is_executable_file(&node_join(directory, command))
    }))
}

fn read_environment_value(environment: &IndexMap<JsString, Option<JsString>>, name: &str) -> Option<JsString> {
    environment
        .iter()
        .find(|(key, _)| key.to_lowercase() == JsString::from(name.to_lowercase()))
        .and_then(|(_, value)| value.clone())
}

fn read_process_environment() -> IndexMap<JsString, Option<JsString>> {
    pi_js_compat::process_env::vars()
        .into_iter()
        .map(|(key, value)| (key.into(), Some(value.into())))
        .collect()
}

fn read_process_cwd() -> Result<JsString, NativeNodeError> {
    std::env::current_dir()
        .map(|path| decode_node_path(path.as_os_str()))
        .map_err(|error| node_fs_error_record(&error, "uv_cwd", &JsString::default()).into())
}

fn resolve_executable_path(cwd: &JsString, command: &JsString) -> Result<JsString, NativeNodeError> {
    let base = if command.starts_with('/') || cwd.starts_with('/') {
        cwd.clone()
    } else {
        node_join(read_process_cwd()?, cwd)
    };
    node_resolve(base, command).map_err(|error| NativeNodeError::new("Error", error.message().clone()))
}

fn is_executable_file(path: &JsString) -> bool {
    let path = encode_node_utf8(path);
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => rustix::fs::access(&path, rustix::fs::Access::EXEC_OK).is_ok(),
        _ => false,
    }
}
