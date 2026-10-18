//! Coding-agent prompt templates (`core/prompt-templates.ts`).

use std::fs;
use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::package_config;
use crate::core::source_info::{SourceInfo, SourceScope, SyntheticSourceInfoOptions, create_synthetic_source_info};
use crate::utils::frontmatter::parse_frontmatter;
use crate::utils::paths::{PathInputOptions, resolve_path};
use pi_js_compat::locale_compare;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
    pub content: String,
    pub source_info: SourceInfo,
    pub file_path: String,
}

/// Parses command arguments, honoring single and double quotes.
pub fn parse_command_args(args_string: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for character in args_string.chars() {
        match quote {
            Some(active) if character == active => quote = None,
            Some(_) => current.push(character),
            None if character == '"' || character == '\'' => quote = Some(character),
            None if is_js_whitespace(character) => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            None => current.push(character),
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
pub fn substitute_args(content: &str, args: &[String]) -> String {
    let all_args = args.join(" ");
    let pattern =
        Regex::new(r"\$\{([0-9]+|ARGUMENTS|@):-([^}]*)\}|\$\{@:([0-9]+)(?::([0-9]+))?\}|\$(ARGUMENTS|@|[0-9]+)")
            .expect("prompt template substitution regex is valid");

    pattern
        .replace_all(content, |captures: &regex::Captures<'_>| {
            if let Some(target) = captures.get(1) {
                let default_value = captures.get(2).map_or("", |value| value.as_str());
                let value = if matches!(target.as_str(), "@" | "ARGUMENTS") {
                    all_args.clone()
                } else {
                    positional_arg(args, target.as_str()).unwrap_or_default()
                };
                return if value.is_empty() {
                    default_value.to_owned()
                } else {
                    value
                };
            }

            if let Some(slice_start) = captures.get(3) {
                let start = positional_index(slice_start.as_str());
                if let Some(slice_length) = captures.get(4) {
                    let length = slice_length.as_str().parse::<usize>().unwrap_or(0);
                    let end = start.saturating_add(length).min(args.len());
                    return args.get(start..end).map_or_else(String::new, |slice| slice.join(" "));
                }
                return args.get(start..).map_or_else(String::new, |slice| slice.join(" "));
            }

            let simple = captures.get(5).map_or("", |value| value.as_str());
            if matches!(simple, "ARGUMENTS" | "@") {
                return all_args.clone();
            }
            positional_arg(args, simple).unwrap_or_default()
        })
        .into_owned()
}

fn positional_index(text: &str) -> usize {
    text.parse::<usize>().map_or(0, |index| index.saturating_sub(1))
}

fn positional_arg(args: &[String], text: &str) -> Option<String> {
    let index = text.parse::<usize>().ok()?.checked_sub(1)?;
    args.get(index).cloned()
}

#[derive(Debug, Clone)]
pub struct LoadPromptTemplatesOptions {
    /// Working directory for project-local templates.
    pub cwd: String,
    /// Agent config directory for global templates.
    pub agent_dir: String,
    /// Explicit prompt template paths (files or directories).
    pub prompt_paths: Vec<String>,
    /// Include default prompt directories.
    pub include_defaults: bool,
}

/// Loads global, project, and explicit prompt templates in source order.
pub fn load_prompt_templates(
    options: &LoadPromptTemplatesOptions,
) -> Result<Vec<PromptTemplate>, crate::utils::paths::PathError> {
    let process_cwd =
        std::env::current_dir().map_or_else(|_| "/".to_owned(), |path| path.to_string_lossy().into_owned());
    let resolved_cwd = resolve_path(&options.cwd, &process_cwd, &PathInputOptions::default())?;
    let resolved_agent_dir = resolve_path(&options.agent_dir, &process_cwd, &PathInputOptions::default())?;

    let global_prompts_dir = Path::new(&resolved_agent_dir).join("prompts");
    let project_prompts_dir = Path::new(&resolved_cwd)
        .join(&package_config().config_dir_name)
        .join("prompts");

    let get_source_info = |resolved_path: &Path| -> SourceInfo {
        if is_under_path(resolved_path, &global_prompts_dir) {
            return create_synthetic_source_info(
                &resolved_path.to_string_lossy(),
                &SyntheticSourceInfoOptions {
                    source: "local".to_owned(),
                    scope: Some(SourceScope::User),
                    origin: None,
                    base_dir: Some(global_prompts_dir.to_string_lossy().into_owned()),
                },
            );
        }
        if is_under_path(resolved_path, &project_prompts_dir) {
            return create_synthetic_source_info(
                &resolved_path.to_string_lossy(),
                &SyntheticSourceInfoOptions {
                    source: "local".to_owned(),
                    scope: Some(SourceScope::Project),
                    origin: None,
                    base_dir: Some(project_prompts_dir.to_string_lossy().into_owned()),
                },
            );
        }
        let base_dir = if fs::metadata(resolved_path).is_ok_and(|metadata| metadata.is_dir()) {
            resolved_path.to_path_buf()
        } else {
            resolved_path
                .parent()
                .map_or_else(|| resolved_path.to_path_buf(), Path::to_path_buf)
        };
        create_synthetic_source_info(
            &resolved_path.to_string_lossy(),
            &SyntheticSourceInfoOptions {
                source: "local".to_owned(),
                scope: None,
                origin: None,
                base_dir: Some(base_dir.to_string_lossy().into_owned()),
            },
        )
    };

    let mut templates = Vec::new();
    if options.include_defaults {
        templates.extend(load_templates_from_dir(&global_prompts_dir, &get_source_info));
        templates.extend(load_templates_from_dir(&project_prompts_dir, &get_source_info));
    }

    for raw_path in &options.prompt_paths {
        let resolved_path = resolve_path(
            raw_path,
            &resolved_cwd,
            &PathInputOptions {
                trim: true,
                ..PathInputOptions::default()
            },
        )?;
        let path = Path::new(&resolved_path);
        if !path.exists() {
            continue;
        }

        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };
        if metadata.is_dir() {
            templates.extend(load_templates_from_dir(path, &get_source_info));
        } else if metadata.is_file()
            && resolved_path.ends_with(".md")
            && let Some(template) = load_template_from_file(path, &get_source_info(path))
        {
            templates.push(template);
        }
    }

    Ok(templates)
}

fn is_under_path(target: &Path, root: &Path) -> bool {
    target == root || target.starts_with(root)
}

fn load_template_from_file(file_path: &Path, source_info: &SourceInfo) -> Option<PromptTemplate> {
    let raw_content = fs::read_to_string(file_path).ok()?;
    let parsed = parse_frontmatter(&raw_content).ok()?;
    let frontmatter = parsed.frontmatter;
    let body = parsed.body;

    let name = file_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = name.strip_suffix(".md").unwrap_or(&name).to_owned();

    let mut description = frontmatter
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if description.is_empty()
        && let Some(first_line) = body.split('\n').find(|line| !line.trim().is_empty())
    {
        description = slice_utf16(first_line, 60);
        if first_line.encode_utf16().count() > 60 {
            description.push_str("...");
        }
    }

    let argument_hint = frontmatter
        .get("argument-hint")
        .and_then(Value::as_str)
        .filter(|hint| !hint.is_empty())
        .map(str::to_owned);

    Some(PromptTemplate {
        name,
        description,
        argument_hint,
        content: body,
        source_info: source_info.clone(),
        file_path: file_path.to_string_lossy().into_owned(),
    })
}

fn slice_utf16(value: &str, length: usize) -> String {
    String::from_utf16_lossy(&value.encode_utf16().take(length).collect::<Vec<u16>>())
}

fn load_templates_from_dir(dir: &Path, get_source_info: &dyn Fn(&Path) -> SourceInfo) -> Vec<PromptTemplate> {
    let mut templates = Vec::new();
    if !dir.exists() {
        return templates;
    }

    let Ok(entries) = fs::read_dir(dir) else {
        return templates;
    };
    let mut entries: Vec<fs::DirEntry> = entries.filter_map(Result::ok).collect();
    // Node's `readdirSync` returns libuv's `scandir` order (locale-collated names).
    entries.sort_by(|left, right| {
        locale_compare(
            &left.file_name().to_string_lossy(),
            &right.file_name().to_string_lossy(),
        )
    });
    for entry in entries {
        let full_path = dir.join(entry.file_name());
        let is_file = fs::metadata(&full_path).is_ok_and(|metadata| metadata.is_file());
        if is_file
            && entry.file_name().to_string_lossy().ends_with(".md")
            && let Some(template) = load_template_from_file(&full_path, &get_source_info(&full_path))
        {
            templates.push(template);
        }
    }
    templates
}

/// Expands a `/template args` message when the template name matches.
pub fn expand_prompt_template(text: &str, templates: &[PromptTemplate]) -> String {
    if !text.starts_with('/') {
        return text.to_owned();
    }

    let pattern = Regex::new(r"^/(\S+)(?:\s+([\s\S]*))?$").expect("prompt template expansion regex is valid");
    let Some(captures) = pattern.captures(text) else {
        return text.to_owned();
    };
    let template_name = captures.get(1).map_or("", |value| value.as_str());
    let args_string = captures.get(2).map_or("", |value| value.as_str());

    let Some(template) = templates.iter().find(|template| template.name == template_name) else {
        return text.to_owned();
    };
    let args = parse_command_args(args_string);
    substitute_args(&template.content, &args)
}
