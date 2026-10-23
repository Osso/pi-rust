//! Port of `packages/coding-agent/src/core/permissions/rule-store.ts`.
//!
//! `writePermissionRules` edits `settings.json` in place: it replaces an existing top-level
//! `permissionRules` value and otherwise appends one before the final brace, preserving the rest of
//! the file byte-for-byte (compact files stay compact, formatted files get a two-space block).

use crate::utils::paths::node_join;
use pi_js_compat::js_json::encode_node_utf8;
use std::path::Path;

use pi_js_compat::js_json::{JsObject as Map, JsString, JsValue as Value};

use crate::config::package_config;
use pi_js_compat::js_json;

/// Where an allow rule is persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionRuleDestination {
    Session,
    UserSettings,
    ProjectSettings,
    LocalSettings,
}

/// Only `allow` rules exist today; kept as an enum to mirror the source union.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionRuleBehavior {
    Allow,
}

/// One permission update requested by a tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRuleUpdate {
    pub destination: PermissionRuleDestination,
    pub behavior: PermissionRuleBehavior,
    pub rules: Vec<JsString>,
}

impl PermissionRuleUpdate {
    pub fn new(destination: PermissionRuleDestination, rules: Vec<JsString>) -> Self {
        Self {
            destination,
            behavior: PermissionRuleBehavior::Allow,
            rules,
        }
    }
}

/// Inputs for a persisted allow-rule write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritePermissionRulesOptions {
    pub cwd: JsString,
    pub agent_dir: JsString,
    pub destination: PermissionRuleDestination,
    pub behavior: PermissionRuleBehavior,
    pub tool_name: String,
    pub rules: Vec<JsString>,
}

/// Failure writing permission rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleStoreError {
    message: JsString,
}

impl RuleStoreError {
    pub fn message(&self) -> &JsString {
        &self.message
    }
}

impl std::fmt::Display for RuleStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}

impl std::error::Error for RuleStoreError {}

pub type PermissionRuleWriter = Box<dyn Fn(&WritePermissionRulesOptions) + Send + Sync>;

type AllowRules = Vec<(JsString, Vec<JsString>)>;

/// In-memory allow rules plus the optional persistence hook.
pub struct PermissionRuleStore {
    allow_rules: AllowRules,
    cwd: Option<JsString>,
    agent_dir: Option<JsString>,
    writer: PermissionRuleWriter,
}

impl Default for PermissionRuleStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionRuleStore {
    pub fn new() -> Self {
        Self::with_writer(Box::new(|options| {
            let _ = write_permission_rules(options);
        }))
    }

    pub fn with_writer(writer: PermissionRuleWriter) -> Self {
        Self {
            allow_rules: Vec::new(),
            cwd: None,
            agent_dir: None,
            writer,
        }
    }

    pub fn from_settings(settings: Option<&Value>) -> Self {
        let mut store = Self::new();
        store.add_settings_rules(settings);
        store
    }

    pub fn set_locations(&mut self, cwd: Option<JsString>, agent_dir: Option<JsString>) {
        self.cwd = cwd;
        self.agent_dir = agent_dir;
    }

    pub fn has_allow_rule(&self, tool_name: &str, rule_content: &JsString) -> bool {
        self.allow_rules
            .iter()
            .find(|(tool, _)| tool == tool_name)
            .is_some_and(|(_, rules)| rules.iter().any(|rule| rule == rule_content))
    }

    pub fn add_allow_rules(&mut self, tool_name: &str, rules: &[JsString]) {
        self.add_rules(tool_name, rules);
    }

    pub fn add_settings_rules(&mut self, settings: Option<&Value>) {
        let permission_rules = read_permission_rules(settings.and_then(|settings| settings.get("permissionRules")));
        for (tool_name, rules) in permission_rules {
            self.add_rules(&tool_name, &rules);
        }
    }

    pub fn apply_updated_permissions(&mut self, tool_name: &str, updates: Option<&[PermissionRuleUpdate]>) {
        for update in updates.unwrap_or_default() {
            if update.destination == PermissionRuleDestination::Session {
                self.add_rules(tool_name, &update.rules);
                continue;
            }
            let (Some(cwd), Some(agent_dir)) = (self.cwd.clone(), self.agent_dir.clone()) else {
                continue;
            };
            (self.writer)(&WritePermissionRulesOptions {
                cwd,
                agent_dir,
                destination: update.destination,
                behavior: update.behavior,
                tool_name: tool_name.to_owned(),
                rules: update.rules.clone(),
            });
        }
    }

    fn add_rules(&mut self, tool_name: impl Into<JsString>, rules: &[JsString]) {
        let tool_name = tool_name.into();
        match self.allow_rules.iter_mut().find(|(tool, _)| tool == &tool_name) {
            Some((_, existing)) => push_unique(existing, rules),
            None => self.allow_rules.push((tool_name, rules.to_vec())),
        }
    }

    /// Snapshot of the in-memory rules, for tests and diagnostics.
    pub fn allow_rules(&self) -> std::collections::BTreeMap<JsString, Vec<JsString>> {
        self.allow_rules.iter().cloned().collect()
    }
}

fn push_unique(existing: &mut Vec<JsString>, rules: &[JsString]) {
    for rule in rules {
        if !existing.contains(rule) {
            existing.push(rule.clone());
        }
    }
}

/// `buildPermissionRuleContent`: bash uses the raw command, every other tool its compact JSON input.
pub fn build_permission_rule_content(tool_name: &str, input: &Map) -> JsString {
    if tool_name == "bash"
        && let Some(Value::String(command)) = input.get("command")
    {
        return command.clone();
    }
    js_json::stringify_lossless(&Value::Object(input.clone())).into()
}

/// Persists an allow rule, preserving the rest of the settings file.
pub fn write_permission_rules(options: &WritePermissionRulesOptions) -> Result<(), RuleStoreError> {
    let settings_path = resolve_settings_path(options);
    let current_content = match std::fs::read(encode_node_utf8(&settings_path))
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    {
        Ok(content) => Some(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(RuleStoreError {
                message: error.to_string().into(),
            });
        }
    };
    let mut current_settings = read_settings(&settings_path)?;
    let permission_rules = read_permission_rules(current_settings.get("permissionRules"));
    let existing_rules = permission_rules
        .iter()
        .find(|(tool, _)| tool == &options.tool_name)
        .map(|(_, rules)| rules.clone())
        .unwrap_or_default();
    let mut next_rules = existing_rules;
    push_unique(&mut next_rules, &options.rules);

    let mut behavior_rules: Vec<(JsString, Vec<JsString>)> = permission_rules;
    match behavior_rules.iter_mut().find(|(tool, _)| tool == &options.tool_name) {
        Some((_, rules)) => *rules = next_rules,
        None => behavior_rules.push((options.tool_name.clone().into(), next_rules)),
    }
    let permission_rules_value = permission_rules_to_value(&behavior_rules);
    current_settings.insert("permissionRules".into(), permission_rules_value.clone());

    if let Some(parent) = Path::new(&encode_node_utf8(&settings_path)).parent() {
        std::fs::create_dir_all(parent).map_err(|error| RuleStoreError {
            message: error.to_string().into(),
        })?;
    }
    let content = update_permission_rules_content(current_content.as_deref(), &permission_rules_value)?;
    std::fs::write(encode_node_utf8(&settings_path), content).map_err(|error| RuleStoreError {
        message: error.to_string().into(),
    })
}

fn resolve_settings_path(options: &WritePermissionRulesOptions) -> JsString {
    match options.destination {
        PermissionRuleDestination::UserSettings => join(&options.agent_dir, "settings.json"),
        PermissionRuleDestination::ProjectSettings => {
            join(&join(&options.cwd, &package_config().config_dir_name), "settings.json")
        }
        PermissionRuleDestination::LocalSettings => join(
            &join(&options.cwd, &package_config().config_dir_name),
            "settings.local.json",
        ),
        PermissionRuleDestination::Session => join(&options.agent_dir, "settings.json"),
    }
}

fn join(base: &JsString, segment: impl Into<JsString>) -> JsString {
    node_join(base, segment)
}

fn read_settings(path: &JsString) -> Result<Map, RuleStoreError> {
    if !Path::new(&encode_node_utf8(path)).exists() {
        return Ok(Map::new());
    }
    let content = std::fs::read(encode_node_utf8(path))
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|error| RuleStoreError {
            message: error.to_string().into(),
        })?;
    let parsed = js_json::parse_lossless(&content).map_err(|error| RuleStoreError {
        message: error.message().clone(),
    })?;
    Ok(match parsed {
        Value::Object(object) => object,
        _ => Map::new(),
    })
}

fn read_permission_rules(value: Option<&Value>) -> AllowRules {
    let Some(Value::Object(rules)) = value else {
        return Vec::new();
    };
    let Some(Value::Object(allow)) = rules.get("allow") else {
        return Vec::new();
    };
    allow
        .iter()
        .filter_map(|(tool_name, rules)| {
            let Value::Array(rules) = rules else {
                return None;
            };
            let rules: Vec<JsString> = rules.iter().filter_map(|rule| rule.as_string().cloned()).collect();
            Some((tool_name.clone(), rules))
        })
        .collect()
}

fn permission_rules_to_value(rules: &AllowRules) -> Value {
    let mut allow = Map::new();
    for (tool_name, tool_rules) in rules {
        allow.insert(
            tool_name.clone(),
            Value::Array(tool_rules.iter().cloned().map(Value::String).collect()),
        );
    }
    let mut permission_rules = Map::new();
    permission_rules.insert("allow".into(), Value::Object(allow));
    Value::Object(permission_rules)
}

fn update_permission_rules_content(
    current_content: Option<&str>,
    permission_rules: &Value,
) -> Result<String, RuleStoreError> {
    let formatted_rules = js_json::stringify_lossless_pretty(permission_rules);
    let Some(current_content) = current_content else {
        return Ok(format!(
            "{}\n",
            js_json::stringify_lossless_pretty(&wrap_permission_rules(permission_rules))
        ));
    };

    if let Some((start, end)) = find_top_level_property_value_range(current_content, "permissionRules")? {
        return Ok(format!(
            "{}{}{}",
            &current_content[..start],
            formatted_rules,
            &current_content[end..]
        ));
    }

    let Some(closing_brace_index) = current_content.rfind('}') else {
        return Ok(format!(
            "{}\n",
            js_json::stringify_lossless_pretty(&wrap_permission_rules(permission_rules))
        ));
    };

    let before_brace = &current_content[..closing_brace_index];
    let after_brace = &current_content[closing_brace_index..];
    let has_existing_properties = before_brace.trim() != "{";
    let separator = if has_existing_properties { "," } else { "" };
    let has_newline = before_brace.contains('\n');
    let line_prefix = if has_newline { "\n  " } else { "" };
    let line_suffix = if has_newline { "\n" } else { "" };
    let compact_rules = if has_newline {
        formatted_rules.replace('\n', "\n  ")
    } else {
        js_json::stringify_lossless(permission_rules)
    };
    Ok(format!(
        "{}{separator}{line_prefix}\"permissionRules\":{}{compact_rules}{line_suffix}{after_brace}",
        before_brace.trim_end(),
        if has_newline { " " } else { "" }
    ))
}

fn wrap_permission_rules(permission_rules: &Value) -> Value {
    let mut object = Map::new();
    object.insert("permissionRules".into(), permission_rules.clone());
    Value::Object(object)
}

/// Finds the value range of a top-level property in a JSON object text.
fn find_top_level_property_value_range(
    content: &str,
    property_name: &str,
) -> Result<Option<(usize, usize)>, RuleStoreError> {
    let bytes = content.as_bytes();
    let mut depth = 0i32;
    let mut index = 0usize;

    while index < bytes.len() {
        let character = bytes[index];
        if character == b'"' {
            let end_quote = find_string_end(content, index);
            let literal = &content[index..=end_quote];
            let name = js_json::parse_lossless(literal).map_err(|error| RuleStoreError {
                message: error.message().clone(),
            })?;
            if depth == 1 && name.as_string().is_some_and(|name| name == property_name) {
                let Some(colon_offset) = content[end_quote + 1..].find(':') else {
                    return Ok(None);
                };
                let colon_index = end_quote + 1 + colon_offset;
                let Some(start) = find_next_non_whitespace(content, colon_index + 1) else {
                    return Ok(None);
                };
                return Ok(Some((start, find_json_value_end(content, start))));
            }
            index = end_quote + 1;
            continue;
        }
        if character == b'{' {
            depth += 1;
        }
        if character == b'}' {
            depth -= 1;
        }
        index += 1;
    }

    Ok(None)
}

/// JavaScript `\s`: ASCII whitespace plus the Unicode space separators and the BOM.
fn is_javascript_whitespace(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

fn find_next_non_whitespace(content: &str, start: usize) -> Option<usize> {
    content[start..]
        .char_indices()
        .find(|(_, character)| !is_javascript_whitespace(*character))
        .map(|(offset, _)| start + offset)
}

fn find_string_end(content: &str, start_quote: usize) -> usize {
    let mut escaped = false;
    for (offset, character) in content[start_quote + 1..].char_indices() {
        let index = start_quote + 1 + offset;
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return index;
        }
    }
    start_quote
}

fn find_json_value_end(content: &str, start: usize) -> usize {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, character) in content[start..].char_indices() {
        let index = start + offset;
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => {
                if depth == 0 {
                    return index;
                }
                depth -= 1;
            }
            ',' if depth == 0 => return index,
            _ => {}
        }
    }

    content.len()
}
