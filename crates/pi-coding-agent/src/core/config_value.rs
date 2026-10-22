use std::collections::HashMap;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::utils::shell::{CommandTransport, get_shell_config};
use pi_js_compat::js_json::encode_node_utf8;

enum TemplatePart<'a> {
    Literal(String),
    Environment(&'a str),
}

enum ConfigValueReference<'a> {
    Command,
    Template(Vec<TemplatePart<'a>>),
}

/// Returns the environment name when the complete value is exactly one reference.
pub fn get_config_value_env_var_name(config: &str) -> Option<&str> {
    let ConfigValueReference::Template(parts) = parse_config_value_reference(config) else {
        return None;
    };

    match parts.as_slice() {
        [TemplatePart::Environment(name)] => Some(*name),
        _ => None,
    }
}

/// Returns unique environment references in first-seen order.
pub fn get_config_value_env_var_names(config: &str) -> Vec<String> {
    let ConfigValueReference::Template(parts) = parse_config_value_reference(config) else {
        return Vec::new();
    };

    get_template_env_var_names(&parts)
}

/// Reports environment references missing from both the explicit and process environments.
pub fn get_missing_config_value_env_var_names(config: &str, env: Option<&HashMap<String, String>>) -> Vec<String> {
    get_config_value_env_var_names(config)
        .into_iter()
        .filter(|name| !has_config_environment_value(name, env))
        .collect()
}

/// Reports whether every environment reference is currently configured.
pub fn is_config_value_configured(config: &str, env: Option<&HashMap<String, String>>) -> bool {
    get_missing_config_value_env_var_names(config, env).is_empty()
}

/// Reports whether the value is a command-backed configuration reference.
pub fn is_command_config_value(config: &str) -> bool {
    matches!(parse_config_value_reference(config), ConfigValueReference::Command)
}

fn has_config_environment_value(name: &str, env: Option<&HashMap<String, String>>) -> bool {
    let explicit_value_exists = env
        .and_then(|values| values.get(name))
        .is_some_and(|value| !value.is_empty());
    explicit_value_exists || process_environment_value_exists(name)
}

fn process_environment_value_exists(name: &str) -> bool {
    pi_js_compat::process_env::get(name).is_some_and(|value| !value.is_empty())
}

fn parse_config_value_reference(config: &str) -> ConfigValueReference<'_> {
    if config.starts_with('!') {
        return ConfigValueReference::Command;
    }

    ConfigValueReference::Template(parse_config_value_template(config))
}

fn parse_config_value_template(config: &str) -> Vec<TemplatePart<'_>> {
    let mut parts = Vec::new();
    let mut index = 0;

    while index < config.len() {
        let Some(relative_dollar_index) = config[index..].find('$') else {
            append_literal(&mut parts, &config[index..]);
            break;
        };
        let dollar_index = index + relative_dollar_index;
        append_literal(&mut parts, &config[index..dollar_index]);
        index = parse_dollar(config, dollar_index, &mut parts);
    }

    parts
}

fn parse_dollar<'a>(config: &'a str, dollar_index: usize, parts: &mut Vec<TemplatePart<'a>>) -> usize {
    let next_index = dollar_index + 1;
    match config[next_index..].chars().next() {
        Some(next @ ('$' | '!')) => {
            append_literal(parts, &config[next_index..next_index + next.len_utf8()]);
            next_index + next.len_utf8()
        }
        Some('{') => parse_braced_reference(config, dollar_index, parts),
        _ => parse_unbraced_reference(config, dollar_index, parts),
    }
}

fn parse_braced_reference<'a>(config: &'a str, dollar_index: usize, parts: &mut Vec<TemplatePart<'a>>) -> usize {
    let name_start = dollar_index + 2;
    let Some(relative_end_index) = config[name_start..].find('}') else {
        append_literal(parts, "$");
        return dollar_index + 1;
    };
    let end_index = name_start + relative_end_index;
    let name = &config[name_start..end_index];

    if is_environment_name(name) {
        parts.push(TemplatePart::Environment(name));
    } else {
        append_literal(parts, &config[dollar_index..=end_index]);
    }

    end_index + 1
}

fn parse_unbraced_reference<'a>(config: &'a str, dollar_index: usize, parts: &mut Vec<TemplatePart<'a>>) -> usize {
    let name_start = dollar_index + 1;
    let name_length = environment_name_prefix_length(&config[name_start..]);
    if name_length == 0 {
        append_literal(parts, "$");
        return name_start;
    }

    let name_end = name_start + name_length;
    parts.push(TemplatePart::Environment(&config[name_start..name_end]));
    name_end
}

fn append_literal(parts: &mut Vec<TemplatePart<'_>>, value: &str) {
    if value.is_empty() {
        return;
    }

    if let Some(TemplatePart::Literal(previous)) = parts.last_mut() {
        previous.push_str(value);
        return;
    }

    parts.push(TemplatePart::Literal(value.to_owned()));
}

fn get_template_env_var_names(parts: &[TemplatePart<'_>]) -> Vec<String> {
    let mut names = Vec::new();
    for part in parts {
        let TemplatePart::Environment(name) = part else {
            continue;
        };
        if names.iter().any(|existing| existing == name) {
            continue;
        }
        names.push((*name).to_owned());
    }
    names
}

fn is_environment_name(value: &str) -> bool {
    let prefix_length = environment_name_prefix_length(value);
    prefix_length > 0 && prefix_length == value.len()
}

fn environment_name_prefix_length(value: &str) -> usize {
    let bytes = value.as_bytes();
    let Some(first) = bytes.first() else {
        return 0;
    };
    if !is_environment_name_start(*first) {
        return 0;
    }

    1 + bytes[1..]
        .iter()
        .take_while(|byte| is_environment_name_continue(**byte))
        .count()
}

const fn is_environment_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

const fn is_environment_name_continue(byte: u8) -> bool {
    is_environment_name_start(byte) || byte.is_ascii_digit()
}

// =============================================================================
// Value Resolution
// =============================================================================

const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

/// Failure from [`resolve_config_value_or_throw`] and [`resolve_headers_or_throw`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigValueError {
    message: String,
}

impl std::fmt::Display for ConfigValueError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ConfigValueError {}

fn command_result_cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
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
pub fn resolve_config_value(config: &str, env: Option<&HashMap<String, String>>) -> Option<String> {
    match parse_config_value_reference(config) {
        ConfigValueReference::Command => execute_command(config),
        ConfigValueReference::Template(parts) => resolve_template(&parts, env),
    }
}

/// Like [`resolve_config_value`] but always executes commands.
pub fn resolve_config_value_uncached(config: &str, env: Option<&HashMap<String, String>>) -> Option<String> {
    match parse_config_value_reference(config) {
        ConfigValueReference::Command => execute_command_uncached(config),
        ConfigValueReference::Template(parts) => resolve_template(&parts, env),
    }
}

/// Resolves a value or reports which source failed.
pub fn resolve_config_value_or_throw(
    config: &str,
    description: &str,
    env: Option<&HashMap<String, String>>,
) -> Result<String, ConfigValueError> {
    if let Some(resolved) = resolve_config_value_uncached(config, env) {
        return Ok(resolved);
    }

    match parse_config_value_reference(config) {
        ConfigValueReference::Command => {
            let command = &config[1..];
            return Err(ConfigValueError {
                message: format!("Failed to resolve {description} from shell command: {command}"),
            });
        }
        ConfigValueReference::Template(_) => {
            let missing = get_missing_config_value_env_var_names(config, env);
            if missing.len() == 1 {
                return Err(ConfigValueError {
                    message: format!(
                        "Failed to resolve {description} from environment variable: {}",
                        missing[0]
                    ),
                });
            }
            if missing.len() > 1 {
                return Err(ConfigValueError {
                    message: format!(
                        "Failed to resolve {description} from environment variables: {}",
                        missing.join(", ")
                    ),
                });
            }
        }
    }

    Err(ConfigValueError {
        message: format!("Failed to resolve {description}"),
    })
}

/// Resolves every header value; empty results are dropped.
pub fn resolve_headers(
    headers: Option<&HashMap<String, String>>,
    env: Option<&HashMap<String, String>>,
) -> Option<HashMap<String, String>> {
    let headers = headers?;
    let resolved: HashMap<String, String> = headers
        .iter()
        .filter_map(|(key, value)| resolve_config_value(value, env).map(|value| (key.clone(), value)))
        .collect();
    (!resolved.is_empty()).then_some(resolved)
}

/// Resolves every header value, reporting the first failure with its header name.
pub fn resolve_headers_or_throw(
    headers: Option<&HashMap<String, String>>,
    description: &str,
    env: Option<&HashMap<String, String>>,
) -> Result<Option<HashMap<String, String>>, ConfigValueError> {
    let Some(headers) = headers else {
        return Ok(None);
    };
    let mut resolved = HashMap::new();
    for (key, value) in headers {
        let value = resolve_config_value_or_throw(value, &format!("{description} header \"{key}\""), env)?;
        resolved.insert(key.clone(), value);
    }
    Ok((!resolved.is_empty()).then_some(resolved))
}

fn resolve_template(parts: &[TemplatePart<'_>], env: Option<&HashMap<String, String>>) -> Option<String> {
    let mut resolved = String::new();
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

fn resolve_environment_value(name: &str, env: Option<&HashMap<String, String>>) -> Option<String> {
    env.and_then(|values| values.get(name))
        .filter(|value| !value.is_empty())
        .cloned()
        .or_else(|| pi_js_compat::process_env::get(name).filter(|value| !value.is_empty()))
}

fn execute_command(command_config: &str) -> Option<String> {
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

fn execute_command_uncached(command_config: &str) -> Option<String> {
    let command = &command_config[1..];
    #[cfg(windows)]
    {
        let (executed, value) = execute_with_configured_shell(command);
        if executed {
            return value;
        }
        execute_with_default_shell(command)
    }
    #[cfg(not(windows))]
    {
        execute_with_default_shell(command)
    }
}

/// `execSync(command)`: `/bin/sh -c`, ten-second timeout, stdout only.
fn execute_with_default_shell(command: &str) -> Option<String> {
    #[cfg(windows)]
    let mut process = {
        let mut process = std::process::Command::new("cmd");
        process.arg("/C").arg(command);
        process
    };
    #[cfg(not(windows))]
    let mut process = {
        let mut process = std::process::Command::new("sh");
        process.arg("-c").arg(command);
        process
    };
    process
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let output = run_with_timeout(process)?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

/// The configured shell (stdin transport writes the command to stdin); `false` when it cannot start.
#[cfg_attr(not(windows), allow(dead_code))]
fn execute_with_configured_shell(command: &str) -> (bool, Option<String>) {
    let Ok(config) = get_shell_config(None) else {
        return (false, None);
    };
    // Node encodes the shell path and argv to UTF-8 at the spawn boundary.
    let mut process = std::process::Command::new(encode_node_utf8(&config.shell));
    process.args(config.args.iter().map(encode_node_utf8));
    let command_from_stdin = config.command_transport == CommandTransport::Stdin;
    if !command_from_stdin {
        process.arg(command);
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
        let _ = stdin.write_all(command.as_bytes());
    }
    let Some(output) = wait_with_timeout(child) else {
        return (true, None);
    };
    if !output.status.success() {
        return (true, None);
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
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
