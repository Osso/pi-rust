use std::collections::HashMap;

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
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
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
