//! Coding-agent skill loading (`core/skills.ts`).

use std::collections::HashSet;
use std::fs;
use std::path::{MAIN_SEPARATOR, Path};

use pi_agent_core::harness::skills::IgnoreMatcher;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::package_config;
use crate::core::diagnostics::{ResourceCollision, ResourceDiagnostic, ResourceDiagnosticType, ResourceType};
use crate::core::source_info::{SourceInfo, SourceScope, SyntheticSourceInfoOptions, create_synthetic_source_info};
use crate::utils::frontmatter::parse_frontmatter;
use crate::utils::paths::{PathInputOptions, canonicalize_path, resolve_path};

const MAX_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_LENGTH: usize = 1024;
const IGNORE_FILE_NAMES: [&str; 3] = [".gitignore", ".ignore", ".fdignore"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub file_path: String,
    pub base_dir: String,
    pub source_info: SourceInfo,
    pub disable_model_invocation: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LoadSkillsResult {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<ResourceDiagnostic>,
}

#[derive(Debug, Clone)]
pub struct LoadSkillsFromDirOptions {
    pub dir: String,
    pub source: String,
}

/// Loads skills from one directory tree.
pub fn load_skills_from_dir(options: &LoadSkillsFromDirOptions) -> LoadSkillsResult {
    load_skills_from_dir_internal(&options.dir, &options.source, true, None, None)
}

#[derive(Debug, Clone)]
pub struct LoadSkillsOptions {
    /// Working directory for project-local skills.
    pub cwd: String,
    /// Agent config directory for global skills.
    pub agent_dir: String,
    /// Explicit skill paths (files or directories).
    pub skill_paths: Vec<String>,
    /// Include default skills directories.
    pub include_defaults: bool,
}

/// Loads skills from default locations and explicit paths, keeping the first name winner.
pub fn load_skills(options: &LoadSkillsOptions) -> Result<LoadSkillsResult, crate::utils::paths::PathError> {
    let process_cwd =
        std::env::current_dir().map_or_else(|_| "/".to_owned(), |path| path.to_string_lossy().into_owned());
    let resolved_cwd = resolve_path(&options.cwd, &process_cwd, &PathInputOptions::default())?;
    let resolved_agent_dir = resolve_path(&options.agent_dir, &process_cwd, &PathInputOptions::default())?;

    let mut skill_map: Vec<(String, Skill)> = Vec::new();
    let mut real_path_set: HashSet<String> = HashSet::new();
    let mut all_diagnostics = Vec::new();
    let mut collision_diagnostics = Vec::new();

    if options.include_defaults {
        let user = load_skills_from_dir_internal(
            &Path::new(&resolved_agent_dir).join("skills").to_string_lossy(),
            "user",
            true,
            None,
            None,
        );
        add_loaded_skills(
            user,
            &mut skill_map,
            &mut real_path_set,
            &mut all_diagnostics,
            &mut collision_diagnostics,
        );
        let project = load_skills_from_dir_internal(
            &Path::new(&resolved_cwd)
                .join(&package_config().config_dir_name)
                .join("skills")
                .to_string_lossy(),
            "project",
            true,
            None,
            None,
        );
        add_loaded_skills(
            project,
            &mut skill_map,
            &mut real_path_set,
            &mut all_diagnostics,
            &mut collision_diagnostics,
        );
    }

    let user_skills_dir = Path::new(&resolved_agent_dir).join("skills");
    let project_skills_dir = Path::new(&resolved_cwd)
        .join(&package_config().config_dir_name)
        .join("skills");

    for raw_path in &options.skill_paths {
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
            all_diagnostics.push(warning("skill path does not exist", &resolved_path));
            continue;
        }

        let source = if !options.include_defaults {
            if is_under_path(path, &user_skills_dir) {
                "user"
            } else if is_under_path(path, &project_skills_dir) {
                "project"
            } else {
                "path"
            }
        } else {
            "path"
        };

        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                all_diagnostics.push(warning(&error.to_string(), &resolved_path));
                continue;
            }
        };
        if metadata.is_dir() {
            let result = load_skills_from_dir_internal(&resolved_path, source, true, None, None);
            add_loaded_skills(
                result,
                &mut skill_map,
                &mut real_path_set,
                &mut all_diagnostics,
                &mut collision_diagnostics,
            );
        } else if metadata.is_file() && resolved_path.ends_with(".md") {
            let result = load_skill_from_file(&resolved_path, source);
            if result.skill.is_some() {
                add_loaded_skills(
                    LoadSkillsResult {
                        skills: result.skill.into_iter().collect(),
                        diagnostics: result.diagnostics,
                    },
                    &mut skill_map,
                    &mut real_path_set,
                    &mut all_diagnostics,
                    &mut collision_diagnostics,
                );
            } else {
                all_diagnostics.extend(result.diagnostics);
            }
        } else {
            all_diagnostics.push(warning("skill path is not a markdown file", &resolved_path));
        }
    }

    let mut diagnostics = all_diagnostics;
    diagnostics.extend(collision_diagnostics);
    Ok(LoadSkillsResult {
        skills: skill_map.into_iter().map(|(_, skill)| skill).collect(),
        diagnostics,
    })
}

fn add_loaded_skills(
    result: LoadSkillsResult,
    skill_map: &mut Vec<(String, Skill)>,
    real_path_set: &mut HashSet<String>,
    all_diagnostics: &mut Vec<ResourceDiagnostic>,
    collision_diagnostics: &mut Vec<ResourceDiagnostic>,
) {
    all_diagnostics.extend(result.diagnostics);
    for skill in result.skills {
        let real_path = canonicalize_path(&skill.file_path);
        if real_path_set.contains(&real_path) {
            continue;
        }

        if let Some((_, existing)) = skill_map.iter().find(|(name, _)| *name == skill.name) {
            collision_diagnostics.push(ResourceDiagnostic {
                diagnostic_type: ResourceDiagnosticType::Collision,
                message: format!("name \"{}\" collision", skill.name),
                path: Some(skill.file_path.clone()),
                collision: Some(ResourceCollision {
                    resource_type: ResourceType::Skill,
                    name: skill.name.clone(),
                    winner_path: existing.file_path.clone(),
                    loser_path: skill.file_path.clone(),
                    winner_source: None,
                    loser_source: None,
                }),
            });
        } else {
            real_path_set.insert(real_path);
            skill_map.push((skill.name.clone(), skill));
        }
    }
}

fn warning(message: &str, path: &str) -> ResourceDiagnostic {
    ResourceDiagnostic {
        diagnostic_type: ResourceDiagnosticType::Warning,
        message: message.to_owned(),
        path: Some(path.to_owned()),
        collision: None,
    }
}

fn is_under_path(target: &Path, root: &Path) -> bool {
    target == root || target.starts_with(root)
}

fn to_posix_path(path: &Path) -> String {
    path.to_string_lossy().replace(MAIN_SEPARATOR, "/")
}

fn prefix_ignore_pattern(line: &str, prefix: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('#') && !trimmed.starts_with("\\#") {
        return None;
    }

    let mut pattern = line;
    let mut negated = false;
    if let Some(stripped) = pattern.strip_prefix('!') {
        negated = true;
        pattern = stripped;
    } else if let Some(stripped) = pattern.strip_prefix("\\!") {
        pattern = stripped;
    }
    if let Some(stripped) = pattern.strip_prefix('/') {
        pattern = stripped;
    }

    let prefixed = format!("{prefix}{pattern}");
    Some(if negated { format!("!{prefixed}") } else { prefixed })
}

fn add_ignore_rules(matcher: &IgnoreMatcher, dir: &Path, root: &Path) {
    let relative_dir = dir.strip_prefix(root).unwrap_or(Path::new(""));
    let prefix = if relative_dir.as_os_str().is_empty() {
        String::new()
    } else {
        format!("{}/", to_posix_path(relative_dir))
    };

    for filename in IGNORE_FILE_NAMES {
        let ignore_path = dir.join(filename);
        if !ignore_path.exists() {
            continue;
        }
        let Ok(content) = fs::read_to_string(&ignore_path) else {
            continue;
        };
        let patterns: Vec<String> = content
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .filter_map(|line| prefix_ignore_pattern(line, &prefix))
            .collect();
        if !patterns.is_empty() {
            matcher.add(&patterns);
        }
    }
}

fn load_skills_from_dir_internal(
    dir: &str,
    source: &str,
    include_root_files: bool,
    ignore_matcher: Option<&IgnoreMatcher>,
    root_dir: Option<&Path>,
) -> LoadSkillsResult {
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();
    let path = Path::new(dir);
    if !path.exists() {
        return LoadSkillsResult { skills, diagnostics };
    }

    let root = root_dir.map_or_else(|| path.to_path_buf(), Path::to_path_buf);
    let owned_matcher;
    let matcher = match ignore_matcher {
        Some(matcher) => matcher,
        None => {
            owned_matcher = IgnoreMatcher::new(&root.to_string_lossy());
            &owned_matcher
        }
    };
    add_ignore_rules(matcher, path, &root);

    let Ok(entries) = fs::read_dir(path) else {
        return LoadSkillsResult { skills, diagnostics };
    };
    let entries: Vec<fs::DirEntry> = entries.filter_map(Result::ok).collect();

    for entry in &entries {
        if entry.file_name() != "SKILL.md" {
            continue;
        }
        let full_path = path.join(entry.file_name());
        let is_file = match resolved_kind(&entry.path()) {
            Some((is_directory, is_file)) => {
                if is_directory {
                    false
                } else {
                    is_file
                }
            }
            None => continue,
        };
        let relative_path = to_posix_path(full_path.strip_prefix(&root).unwrap_or(&full_path));
        if !is_file || matcher.ignores(&relative_path, false) {
            continue;
        }
        let result = load_skill_from_file(&full_path.to_string_lossy(), source);
        if let Some(skill) = result.skill {
            skills.push(skill);
        }
        diagnostics.extend(result.diagnostics);
        return LoadSkillsResult { skills, diagnostics };
    }

    for entry in &entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }

        let full_path = path.join(entry.file_name());
        let Some((is_directory, is_file)) = resolved_kind(&full_path) else {
            continue;
        };
        let relative_path = to_posix_path(full_path.strip_prefix(&root).unwrap_or(&full_path));
        if matcher.ignores(&relative_path, is_directory) {
            continue;
        }

        if is_directory {
            let sub_result =
                load_skills_from_dir_internal(&full_path.to_string_lossy(), source, false, Some(matcher), Some(&root));
            skills.extend(sub_result.skills);
            diagnostics.extend(sub_result.diagnostics);
            continue;
        }

        if !is_file || !include_root_files || !name.ends_with(".md") {
            continue;
        }

        let result = load_skill_from_file(&full_path.to_string_lossy(), source);
        if let Some(skill) = result.skill {
            skills.push(skill);
        }
        diagnostics.extend(result.diagnostics);
    }

    LoadSkillsResult { skills, diagnostics }
}

/// Returns `(is_directory, is_file)` following symlinks; `None` for a broken link.
fn resolved_kind(path: &Path) -> Option<(bool, bool)> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.is_dir(), metadata.is_file()))
}

struct SkillFileResult {
    skill: Option<Skill>,
    diagnostics: Vec<ResourceDiagnostic>,
}

fn load_skill_from_file(file_path: &str, source: &str) -> SkillFileResult {
    let mut diagnostics = Vec::new();

    let content = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(error) => {
            diagnostics.push(warning(&error.to_string(), file_path));
            return SkillFileResult {
                skill: None,
                diagnostics,
            };
        }
    };
    let frontmatter = match parse_frontmatter(&content) {
        Ok(parsed) => parsed.frontmatter,
        Err(error) => {
            diagnostics.push(warning(&error.to_string(), file_path));
            return SkillFileResult {
                skill: None,
                diagnostics,
            };
        }
    };

    let path = Path::new(file_path);
    let skill_dir = path
        .parent()
        .map_or_else(String::new, |parent| parent.to_string_lossy().into_owned());
    let parent_dir_name = path
        .parent()
        .and_then(Path::file_name)
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());

    let description = frontmatter.get("description").and_then(Value::as_str);
    for error in validate_description(description) {
        diagnostics.push(warning(&error, file_path));
    }

    let name = frontmatter
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .map_or_else(|| parent_dir_name.clone(), str::to_owned);
    for error in validate_name(&name) {
        diagnostics.push(warning(&error, file_path));
    }

    let Some(description) = description.filter(|description| !description.trim().is_empty()) else {
        return SkillFileResult {
            skill: None,
            diagnostics,
        };
    };

    SkillFileResult {
        skill: Some(Skill {
            name,
            description: description.to_owned(),
            file_path: file_path.to_owned(),
            base_dir: skill_dir.clone(),
            source_info: create_skill_source_info(file_path, &skill_dir, source),
            disable_model_invocation: frontmatter.get("disable-model-invocation") == Some(&Value::Bool(true)),
        }),
        diagnostics,
    }
}

fn create_skill_source_info(file_path: &str, base_dir: &str, source: &str) -> SourceInfo {
    let (source_name, scope) = match source {
        "user" => ("local", Some(SourceScope::User)),
        "project" => ("local", Some(SourceScope::Project)),
        "path" => ("local", None),
        other => (other, None),
    };
    create_synthetic_source_info(
        file_path,
        &SyntheticSourceInfoOptions {
            source: source_name.to_owned(),
            scope,
            origin: None,
            base_dir: Some(base_dir.to_owned()),
        },
    )
}

fn validate_name(name: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if name.encode_utf16().count() > MAX_NAME_LENGTH {
        errors.push(format!(
            "name exceeds {MAX_NAME_LENGTH} characters ({})",
            name.encode_utf16().count()
        ));
    }
    if !is_valid_name(name) {
        errors.push("name contains invalid characters (must be lowercase a-z, 0-9, hyphens only)".to_owned());
    }
    if name.starts_with('-') || name.ends_with('-') {
        errors.push("name must not start or end with a hyphen".to_owned());
    }
    if name.contains("--") {
        errors.push("name must not contain consecutive hyphens".to_owned());
    }
    errors
}

fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
}

fn validate_description(description: Option<&str>) -> Vec<String> {
    match description {
        None => vec!["description is required".to_owned()],
        Some(description) if description.trim().is_empty() => vec!["description is required".to_owned()],
        Some(description) if description.encode_utf16().count() > MAX_DESCRIPTION_LENGTH => vec![format!(
            "description exceeds {MAX_DESCRIPTION_LENGTH} characters ({})",
            description.encode_utf16().count()
        )],
        Some(_) => Vec::new(),
    }
}

/// Formats the visible skills for a system prompt.
pub fn format_skills_for_prompt(skills: &[Skill]) -> String {
    let visible: Vec<&Skill> = skills.iter().filter(|skill| !skill.disable_model_invocation).collect();
    if visible.is_empty() {
        return String::new();
    }

    let mut lines = vec![
        "\n\nThe following skills provide specialized instructions for specific tasks.".to_owned(),
        "Use the read tool to load a skill's file when the task matches its description.".to_owned(),
        "When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.".to_owned(),
        String::new(),
        "<available_skills>".to_owned(),
    ];
    for skill in visible {
        lines.push("  <skill>".to_owned());
        lines.push(format!("    <name>{}</name>", escape_xml(&skill.name)));
        lines.push(format!(
            "    <description>{}</description>",
            escape_xml(&skill.description)
        ));
        lines.push(format!("    <location>{}</location>", escape_xml(&skill.file_path)));
        lines.push("  </skill>".to_owned());
    }
    lines.push("</available_skills>".to_owned());
    lines.join("\n")
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
