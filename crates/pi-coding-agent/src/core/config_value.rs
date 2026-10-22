use std::collections::HashMap;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::utils::shell::{CommandTransport, get_shell_config};
use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};

enum TemplatePart {
    Literal(JsString),
    Environment(String),
}

enum ConfigValueReference {
    Command,
    Template(Vec<TemplatePart>),
}

/// Returns the ASCII environment name when the complete value is exactly one reference.
pub fn get_config_value_env_var_name(config: &JsString) -> Option<String> {
    let ConfigValueReference::Template(parts) = parse_config_value_reference(config) else {
        return None;
    };
    match parts.as_slice() {
        [TemplatePart::Environment(name)] => Some(name.clone()),
        _ => None,
    }
}

pub fn get_config_value_env_var_names(config: &JsString) -> Vec<String> {
    let ConfigValueReference::Template(parts) = parse_config_value_reference(config) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for part in parts {
        if let TemplatePart::Environment(name) = part
            && !names.contains(&name)
        {
            names.push(name);
        }
    }
    names
}

pub fn get_missing_config_value_env_var_names(
    config: &JsString,
    env: Option<&HashMap<String, JsString>>,
) -> Vec<String> {
    get_config_value_env_var_names(config)
        .into_iter()
        .filter(|name| !has_config_environment_value(name, env))
        .collect()
}

pub fn is_config_value_configured(config: &JsString, env: Option<&HashMap<String, JsString>>) -> bool {
    get_missing_config_value_env_var_names(config, env).is_empty()
}

pub fn is_command_config_value(config: &JsString) -> bool {
    matches!(parse_config_value_reference(config), ConfigValueReference::Command)
}

fn has_config_environment_value(name: &str, env: Option<&HashMap<String, JsString>>) -> bool {
    env.and_then(|values| values.get(name))
        .is_some_and(|value| !value.is_empty())
        || pi_js_compat::process_env::get(name).is_some_and(|value| !value.is_empty())
}

fn parse_config_value_reference(config: &JsString) -> ConfigValueReference {
    if config.starts_with("!") {
        return ConfigValueReference::Command;
    }
    ConfigValueReference::Template(parse_config_value_template(config))
}

fn parse_config_value_template(config: &JsString) -> Vec<TemplatePart> {
    let mut parts = Vec::new();
    let mut index = 0;
    while index < config.len() {
        let Some(dollar_index) = config.find_from("$", index) else {
            append_literal(&mut parts, config.slice(index, config.len()));
            break;
        };
        append_literal(&mut parts, config.slice(index, dollar_index));
        index = parse_dollar(config, dollar_index, &mut parts);
    }
    parts
}

fn parse_dollar(config: &JsString, index: usize, parts: &mut Vec<TemplatePart>) -> usize {
    let next = index + 1;
    match config.units().get(next) {
        Some(36 | 33) => {
            append_literal(parts, config.slice(next, next + 1));
            next + 1
        }
        Some(123) => parse_braced_reference(config, index, parts),
        _ => parse_unbraced_reference(config, index, parts),
    }
}

fn parse_braced_reference(config: &JsString, index: usize, parts: &mut Vec<TemplatePart>) -> usize {
    let start = index + 2;
    let Some(end) = config.find_from("}", start) else {
        append_literal(parts, "$".into());
        return index + 1;
    };
    let units = &config.units()[start..end];
    if environment_name_prefix_length(units) == units.len() && !units.is_empty() {
        parts.push(TemplatePart::Environment(ascii_environment_name(units)));
    } else {
        append_literal(parts, config.slice(index, end + 1));
    }
    end + 1
}

fn parse_unbraced_reference(config: &JsString, index: usize, parts: &mut Vec<TemplatePart>) -> usize {
    let start = index + 1;
    let length = environment_name_prefix_length(&config.units()[start..]);
    if length == 0 {
        append_literal(parts, "$".into());
        return start;
    }
    let end = start + length;
    parts.push(TemplatePart::Environment(ascii_environment_name(
        &config.units()[start..end],
    )));
    end
}

fn append_literal(parts: &mut Vec<TemplatePart>, value: JsString) {
    if value.is_empty() {
        return;
    }
    if let Some(TemplatePart::Literal(previous)) = parts.last_mut() {
        previous.push_str(value);
    } else {
        parts.push(TemplatePart::Literal(value));
    }
}

fn ascii_environment_name(units: &[u16]) -> String {
    // Called only after ASCII grammar validation; unmatched units are a no-match, never encoded.
    units.iter().map(|unit| char::from(*unit as u8)).collect()
}

fn environment_name_prefix_length(units: &[u16]) -> usize {
    let is_start = |unit: u16| matches!(unit, 65..=90 | 97..=122 | 95);
    let Some(first) = units.first() else {
        return 0;
    };
    if !is_start(*first) {
        return 0;
    }
    1 + units[1..]
        .iter()
        .take_while(|unit| is_start(**unit) || matches!(**unit, 48..=57))
        .count()
}

// =============================================================================
// Value Resolution
// =============================================================================

const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// Failure from [`resolve_config_value_or_throw`] and [`resolve_headers_or_throw`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValueError {
    message: JsString,
}

impl std::fmt::Display for ConfigValueError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}

impl std::error::Error for ConfigValueError {}

fn command_result_cache() -> &'static Mutex<HashMap<JsString, Option<JsString>>> {
    static CACHE: OnceLock<Mutex<HashMap<JsString, Option<JsString>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Clears the command cache; exported for tests, like `clearConfigValueCache`.
pub fn clear_config_value_cache() {
    command_result_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}

/// Resolves a config value: `!command`, `$ENV`/`${ENV}` templates, `$$`/`$!` escapes, or a literal.
pub fn resolve_config_value(config: &JsString, env: Option<&HashMap<String, JsString>>) -> Option<JsString> {
    match parse_config_value_reference(config) {
        ConfigValueReference::Command => execute_command(config),
        ConfigValueReference::Template(parts) => resolve_template(&parts, env),
    }
}

/// Like [`resolve_config_value`] but always executes commands.
pub fn resolve_config_value_uncached(config: &JsString, env: Option<&HashMap<String, JsString>>) -> Option<JsString> {
    match parse_config_value_reference(config) {
        ConfigValueReference::Command => execute_command_uncached(config),
        ConfigValueReference::Template(parts) => resolve_template(&parts, env),
    }
}

/// Resolves a value or reports which source failed.
pub fn resolve_config_value_or_throw(
    config: &JsString,
    description: &JsString,
    env: Option<&HashMap<String, JsString>>,
) -> Result<JsString, ConfigValueError> {
    if let Some(resolved) = resolve_config_value_uncached(config, env) {
        return Ok(resolved);
    }

    match parse_config_value_reference(config) {
        ConfigValueReference::Command => {
            let command = config.slice(1, config.len());
            return Err(ConfigValueError {
                message: js_format!("Failed to resolve {} from shell command: {}", description, command),
            });
        }
        ConfigValueReference::Template(_) => {
            let missing = get_missing_config_value_env_var_names(config, env);
            if missing.len() == 1 {
                return Err(ConfigValueError {
                    message: js_format!(
                        "Failed to resolve {} from environment variable: {}",
                        description,
                        &missing[0]
                    ),
                });
            }
            if missing.len() > 1 {
                return Err(ConfigValueError {
                    message: js_format!(
                        "Failed to resolve {} from environment variables: {}",
                        description,
                        missing.join(", ")
                    ),
                });
            }
        }
    }

    Err(ConfigValueError {
        message: js_format!("Failed to resolve {}", description),
    })
}

/// Resolves every header value; empty results are dropped.
pub fn resolve_headers(
    headers: Option<&HashMap<String, JsString>>,
    env: Option<&HashMap<String, JsString>>,
) -> Option<HashMap<String, JsString>> {
    let headers = headers?;
    let resolved: HashMap<String, JsString> = headers
        .iter()
        .filter_map(|(key, value)| resolve_config_value(value, env).map(|value| (key.clone(), value)))
        .collect();
    (!resolved.is_empty()).then_some(resolved)
}

/// Resolves every header value, reporting the first failure with its header name.
pub fn resolve_headers_or_throw(
    headers: Option<&HashMap<String, JsString>>,
    description: &JsString,
    env: Option<&HashMap<String, JsString>>,
) -> Result<Option<HashMap<String, JsString>>, ConfigValueError> {
    let Some(headers) = headers else {
        return Ok(None);
    };
    let mut resolved = HashMap::new();
    for (key, value) in headers {
        let value = resolve_config_value_or_throw(value, &js_format!("{} header \"{}\"", description, key), env)?;
        resolved.insert(key.clone(), value);
    }
    Ok((!resolved.is_empty()).then_some(resolved))
}

fn resolve_template(parts: &[TemplatePart], env: Option<&HashMap<String, JsString>>) -> Option<JsString> {
    let mut resolved = JsString::default();
    for part in parts {
        match part {
            TemplatePart::Literal(value) => resolved.push_str(value),
            TemplatePart::Environment(name) => {
                resolved.push_str(&resolve_environment_value(name, env)?);
            }
        }
    }
    Some(resolved)
}

fn resolve_environment_value(name: &str, env: Option<&HashMap<String, JsString>>) -> Option<JsString> {
    env.and_then(|values| values.get(name))
        .filter(|value| !value.is_empty())
        .cloned()
        .or_else(|| {
            pi_js_compat::process_env::get(name)
                .filter(|value| !value.is_empty())
                .map(JsString::from)
        })
}

fn execute_command(command_config: &JsString) -> Option<JsString> {
    if let Some(cached) = command_result_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(command_config)
    {
        return cached.clone();
    }
    let result = execute_command_uncached(command_config);
    command_result_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(command_config.to_owned(), result.clone());
    result
}

fn execute_command_uncached(command_config: &JsString) -> Option<JsString> {
    let command = command_config.slice(1, command_config.len());
    #[cfg(windows)]
    {
        let (executed, value) = execute_with_configured_shell(&command);
        if executed {
            return value;
        }
        execute_with_default_shell(&command)
    }
    #[cfg(not(windows))]
    {
        execute_with_default_shell(&command)
    }
}

/// `execSync(command)`: `/bin/sh -c`, ten-second timeout, stdout only.
fn execute_with_default_shell(command: &JsString) -> Option<JsString> {
    #[cfg(windows)]
    let mut process = {
        let mut process = std::process::Command::new("cmd");
        process.arg("/C").arg(encode_node_utf8(command));
        process
    };
    #[cfg(not(windows))]
    let mut process = {
        let mut process = std::process::Command::new("sh");
        process.arg("-c").arg(encode_node_utf8(command));
        process
    };
    process
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let output = run_with_timeout(process)?;
    let value = JsString::from(String::from_utf8_lossy(&output.stdout).into_owned()).trim();
    (!value.is_empty()).then_some(value)
}

/// The configured shell (stdin transport writes the command to stdin); `false` when it cannot start.
#[cfg_attr(not(windows), allow(dead_code))]
fn execute_with_configured_shell(command: &JsString) -> (bool, Option<JsString>) {
    let Ok(config) = get_shell_config(None) else {
        return (false, None);
    };
    // Node encodes the shell path and argv to UTF-8 at the spawn boundary.
    let mut process = std::process::Command::new(encode_node_utf8(&config.shell));
    process.args(config.args.iter().map(encode_node_utf8));
    let command_from_stdin = config.command_transport == CommandTransport::Stdin;
    if !command_from_stdin {
        process.arg(encode_node_utf8(command));
    }
    process
        .stdin(if command_from_stdin {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return (false, None),
        Err(_) => return (true, None),
    };
    if command_from_stdin && let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(encode_node_utf8(command).as_bytes());
    }
    let Some(output) = wait_with_timeout(child) else {
        return (true, None);
    };
    if !output.status.success() {
        return (true, None);
    }
    let value = JsString::from(String::from_utf8_lossy(&output.stdout).into_owned()).trim();
    (true, (!value.is_empty()).then_some(value))
}

fn run_with_timeout(mut process: std::process::Command) -> Option<std::process::Output> {
    let child = process.spawn().ok()?;
    wait_with_timeout(child)
}

fn wait_with_timeout(mut child: std::process::Child) -> Option<std::process::Output> {
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => return None,
        }
    }
}
