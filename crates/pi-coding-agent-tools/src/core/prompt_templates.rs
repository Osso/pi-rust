//! Coding-agent prompt templates (`core/prompt-templates.ts`).

use std::fs;

use pi_js_compat::js_json::{JsValue, encode_node_utf8};
use pi_js_compat::js_regex::{JsCaptures, JsRegex};
use serde::{Deserialize, Serialize};

use crate::config::package_config;
use crate::core::source_info::{SourceInfo, SourceScope, SyntheticSourceInfoOptions, create_synthetic_source_info};
use crate::utils::frontmatter::parse_frontmatter;
use crate::utils::paths::{PathInputOptions, node_basename, node_dirname, node_join, process_cwd, resolve_path};
use pi_js_compat::js_json::JsString;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    pub name: JsString,
    pub description: JsString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<JsString>,
    pub content: JsString,
    pub source_info: SourceInfo,
    pub file_path: JsString,
}

/// Parses command arguments, honoring single and double quotes.
pub fn parse_command_args(args_string: impl Into<JsString>) -> Vec<JsString> {
    let mut args = Vec::new();
    let mut current = JsString::default();
    let mut quote: Option<u16> = None;

    for character in args_string.into().units().iter().copied() {
        match quote {
            Some(active) if character == active => quote = None,
            Some(_) => current.push_unit(character),
            None if character == 0x22 || character == 0x27 => quote = Some(character),
            None if char::from_u32(u32::from(character)).is_some_and(is_js_whitespace) => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            None => current.push_unit(character),
        }
    }

    if !current.is_empty() {
        args.push(current);
    }
    args
}

fn is_js_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{0009}'..='\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

/// Substitutes `$1`, `$@`, `$ARGUMENTS`, defaults, and bash-style slices.
pub fn substitute_args(content: &JsString, args: &[JsString]) -> JsString {
    let all_args = JsString::join(args, " ");
    let pattern = JsRegex::new(r"\$\{([0-9]+):-([^}]*)\}|\$\{@:([0-9]+)(?::([0-9]+))?\}|\$(ARGUMENTS|@|[0-9]+)")
        .expect("prompt template substitution regex is valid");
    pattern.replace_all_with(content, |captures| substitution_value(captures, args, &all_args))
}

fn substitution_value(captures: &JsCaptures, args: &[JsString], all_args: &JsString) -> JsString {
    if let Some(target) = captures.get(1) {
        let value = positional_arg(args, &target.text()).unwrap_or_default();
        return if value.is_empty() {
            captures.get(2).map(|value| value.text()).unwrap_or_default()
        } else {
            value
        };
    }
    if let Some(slice_start) = captures.get(3) {
        let start = parse_position(&slice_start.text()).saturating_sub(1);
        let end = captures.get(4).map_or(args.len(), |length| {
            start.saturating_add(parse_position(&length.text())).min(args.len())
        });
        return args
            .get(start..end)
            .map_or_else(JsString::default, |slice| JsString::join(slice, " "));
    }
    let simple = captures.get(5).map(|value| value.text()).unwrap_or_default();
    if simple == "ARGUMENTS" || simple == "@" {
        return all_args.clone();
    }
    positional_arg(args, &simple).unwrap_or_default()
}

fn parse_position(text: &JsString) -> usize {
    // Captures contain ASCII digits only. Overflow behaves like an out-of-range JS slice index.
    text.units().iter().fold(0usize, |value, unit| {
        value
            .saturating_mul(10)
            .saturating_add(usize::from(*unit - u16::from(b'0')))
    })
}

fn positional_arg(args: &[JsString], text: &JsString) -> Option<JsString> {
    let index = parse_position(text).checked_sub(1)?;
    args.get(index).cloned()
}

#[derive(Debug, Clone)]
pub struct LoadPromptTemplatesOptions {
    pub cwd: JsString,
    pub agent_dir: JsString,
    pub prompt_paths: Vec<JsString>,
    pub include_defaults: bool,
}

/// Loads global, project, and explicit prompt templates in source order.
pub fn load_prompt_templates(options: &LoadPromptTemplatesOptions) -> Result<Vec<PromptTemplate>, JsString> {
    let process_cwd = process_cwd().map_err(|error| error.message().clone())?;
    let cwd = resolve_path(&options.cwd, &process_cwd, &PathInputOptions::default())
        .map_err(|error| error.message().clone())?;
    let agent_dir = resolve_path(&options.agent_dir, &process_cwd, &PathInputOptions::default())
        .map_err(|error| error.message().clone())?;
    let global = node_join(&agent_dir, "prompts");
    let project = node_join(node_join(&cwd, &package_config().config_dir_name), "prompts");
    let source_info = |path: &JsString| template_source_info(path, &global, &project);
    let mut templates = Vec::new();
    if options.include_defaults {
        templates.extend(load_templates_from_dir(&global, &source_info));
        templates.extend(load_templates_from_dir(&project, &source_info));
    }
    for raw_path in &options.prompt_paths {
        let path = resolve_path(
            raw_path,
            &cwd,
            &PathInputOptions {
                trim: true,
                ..Default::default()
            },
        )
        .map_err(|error| error.message().clone())?;
        let encoded = encode_node_utf8(&path);
        if !std::path::Path::new(&encoded).exists() {
            continue;
        }
        let metadata = fs::metadata(&encoded).map_err(|error| JsString::from(error.to_string()))?;
        if metadata.is_dir() {
            templates.extend(load_templates_from_dir(&path, &source_info));
        } else if metadata.is_file() && path.ends_with(".md") {
            let source = source_info(&path)?;
            if let Some(template) = load_template_from_file(&path, &source) {
                templates.push(template);
            }
        }
    }
    Ok(templates)
}

fn is_under_path(target: &JsString, root: &JsString) -> bool {
    target == root || target.starts_with(root.clone() + "/")
}

fn template_source_info(path: &JsString, global: &JsString, project: &JsString) -> Result<SourceInfo, JsString> {
    let (scope, base_dir) = if is_under_path(path, global) {
        (Some(SourceScope::User), global.clone())
    } else if is_under_path(path, project) {
        (Some(SourceScope::Project), project.clone())
    } else {
        let metadata = fs::metadata(encode_node_utf8(path)).map_err(|error| JsString::from(error.to_string()))?;
        (
            None,
            if metadata.is_dir() {
                path.clone()
            } else {
                node_dirname(path)
            },
        )
    };
    Ok(create_synthetic_source_info(
        path,
        &SyntheticSourceInfoOptions {
            source: "local".into(),
            scope,
            origin: None,
            base_dir: Some(base_dir),
        },
    ))
}

fn load_template_from_file(file_path: &JsString, source_info: &SourceInfo) -> Option<PromptTemplate> {
    // Frozen loadTemplateFromFile catches the whole read/parse operation.
    let bytes = match fs::read(encode_node_utf8(file_path)) {
        Ok(bytes) => bytes,
        Err(_) => return None,
    };
    let raw_content = JsString::from(String::from_utf8_lossy(&bytes).as_ref());
    let parsed = match parse_frontmatter(&raw_content) {
        Ok(parsed) => parsed,
        Err(_) => return None,
    };
    let name = node_basename(file_path);
    let name = name.strip_suffix(".md").unwrap_or(name);
    let mut description = parsed
        .frontmatter
        .get("description")
        .and_then(JsValue::as_string)
        .cloned()
        .unwrap_or_default();
    if description.is_empty()
        && let Some(first_line) = parsed.body.split('\n').find(|line| !line.trim().is_empty())
    {
        description = first_line.slice(0, 60);
        if first_line.len() > 60 {
            description.push_str("...");
        }
    }
    let argument_hint = parsed
        .frontmatter
        .get("argument-hint")
        .and_then(JsValue::as_string)
        .filter(|hint| !hint.is_empty())
        .cloned();
    Some(PromptTemplate {
        name,
        description,
        argument_hint,
        content: parsed.body,
        source_info: source_info.clone(),
        file_path: file_path.clone(),
    })
}

fn load_templates_from_dir(
    dir: &JsString,
    source_info: &dyn Fn(&JsString) -> Result<SourceInfo, JsString>,
) -> Vec<PromptTemplate> {
    let mut templates = Vec::new();
    // readdirSync throws as one operation; never discard individual read_dir errors.
    let entries = match fs::read_dir(encode_node_utf8(dir)) {
        Ok(entries) => entries,
        Err(_) => return templates,
    };
    let mut entries = match entries.collect::<Result<Vec<_>, _>>() {
        Ok(entries) => entries,
        Err(_) => return templates,
    };
    entries.sort_by(|left, right| {
        left.file_name()
            .as_encoded_bytes()
            .cmp(right.file_name().as_encoded_bytes())
    });
    for entry in entries {
        let name = JsString::from(entry.file_name().to_string_lossy().as_ref());
        let path = node_join(dir, &name);
        let is_file = fs::metadata(encode_node_utf8(&path)).is_ok_and(|metadata| metadata.is_file());
        if !is_file || !name.ends_with(".md") {
            continue;
        }
        let source = match source_info(&path) {
            Ok(source) => source,
            Err(_) => return templates,
        };
        if let Some(template) = load_template_from_file(&path, &source) {
            templates.push(template);
        }
    }
    templates
}

/// Expands a `/template args` message when the template name matches.
pub fn expand_prompt_template(text: impl Into<JsString>, templates: &[PromptTemplate]) -> JsString {
    let text = text.into();
    if !text.starts_with('/') {
        return text;
    }
    let units = text.units();
    let end = units
        .iter()
        .skip(1)
        .position(|unit| char::from_u32(u32::from(*unit)).is_some_and(is_js_whitespace))
        .map_or(units.len(), |index| index + 1);
    let name = JsString::from_utf16(units[1..end].to_vec());
    let Some(template) = templates.iter().find(|template| name == template.name) else {
        return text;
    };
    let args = JsString::from_utf16(units[end..].to_vec()).trim_start();
    substitute_args(&template.content, &parse_command_args(args))
}
