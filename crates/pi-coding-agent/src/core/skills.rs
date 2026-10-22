//! Coding-agent skill discovery (`core/skills.ts`), retaining native path and text units.
use std::collections::HashSet;
use std::fs;

use indexmap::IndexMap;
use pi_agent_core::harness::skills::IgnoreMatcher;
use pi_js_compat::js_json::{JsObject, JsString, JsValue, encode_node_utf8};
use serde::{Deserialize, Serialize};

use crate::config::package_config;
use crate::core::diagnostics::{ResourceCollision, ResourceDiagnostic, ResourceDiagnosticType, ResourceType};
use crate::core::source_info::{SourceInfo, SourceScope, SyntheticSourceInfoOptions, create_synthetic_source_info};
use crate::utils::frontmatter::parse_frontmatter;
use crate::utils::paths::{
    PathInputOptions, canonicalize_path, node_basename, node_dirname, node_join, process_cwd, relative_path,
    resolve_path,
};

const MAX_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_LENGTH: usize = 1024;
const IGNORE_FILE_NAMES: [&str; 3] = [".gitignore", ".ignore", ".fdignore"];
pub type SkillFrontmatter = JsObject;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: JsString,
    pub description: JsString,
    pub file_path: JsString,
    pub base_dir: JsString,
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
    pub dir: JsString,
    pub source: JsString,
}

pub fn load_skills_from_dir(options: &LoadSkillsFromDirOptions) -> Result<LoadSkillsResult, JsString> {
    load_skills_from_dir_internal(&options.dir, &options.source, true, None, None)
}

#[derive(Debug, Clone)]
pub struct LoadSkillsOptions {
    pub cwd: JsString,
    pub agent_dir: JsString,
    pub skill_paths: Vec<JsString>,
    pub include_defaults: bool,
}

pub fn load_skills(options: &LoadSkillsOptions) -> Result<LoadSkillsResult, JsString> {
    let process_cwd = process_cwd().map_err(|error| error.message().clone())?;
    let cwd = resolve_path(&options.cwd, &process_cwd, &Default::default()).map_err(|error| error.message().clone())?;
    let agent_dir =
        resolve_path(&options.agent_dir, &process_cwd, &Default::default()).map_err(|error| error.message().clone())?;
    let user = node_join(&agent_dir, "skills");
    let project = node_join(node_join(&cwd, &package_config().config_dir_name), "skills");
    let mut loaded = SkillAccumulator::default();
    if options.include_defaults {
        loaded.add(load_skills_from_dir_internal(&user, &"user".into(), true, None, None)?);
        loaded.add(load_skills_from_dir_internal(
            &project,
            &"project".into(),
            true,
            None,
            None,
        )?);
    }
    for raw in &options.skill_paths {
        let path = resolve_path(
            raw,
            &cwd,
            &PathInputOptions {
                trim: true,
                ..Default::default()
            },
        )
        .map_err(|error| error.message().clone())?;
        if !std::path::Path::new(&encode_node_utf8(&path)).exists() {
            loaded.diagnostics.push(warning("skill path does not exist", &path));
            continue;
        }
        let source = if !options.include_defaults && is_under_path(&path, &user) {
            "user"
        } else if !options.include_defaults && is_under_path(&path, &project) {
            "project"
        } else {
            "path"
        };
        match fs::metadata(encode_node_utf8(&path)) {
            Ok(metadata) if metadata.is_dir() => {
                loaded.add(load_skills_from_dir_internal(&path, &source.into(), true, None, None)?)
            }
            Ok(metadata) if metadata.is_file() && path.ends_with(".md") => {
                loaded.add(load_skill_from_file(&path, &source.into()))
            }
            Ok(_) => loaded
                .diagnostics
                .push(warning("skill path is not a markdown file", &path)),
            Err(error) => loaded.diagnostics.push(warning(error.to_string(), &path)),
        }
    }
    loaded.diagnostics.extend(loaded.collisions);
    Ok(LoadSkillsResult {
        skills: loaded.skills.into_values().collect(),
        diagnostics: loaded.diagnostics,
    })
}

#[derive(Default)]
struct SkillAccumulator {
    skills: IndexMap<JsString, Skill>,
    real_paths: HashSet<JsString>,
    diagnostics: Vec<ResourceDiagnostic>,
    collisions: Vec<ResourceDiagnostic>,
}

impl SkillAccumulator {
    fn add(&mut self, result: LoadSkillsResult) {
        self.diagnostics.extend(result.diagnostics);
        for skill in result.skills {
            let real_path = canonicalize_path(&skill.file_path);
            if self.real_paths.contains(&real_path) {
                continue;
            }
            if let Some(existing) = self.skills.get(&skill.name) {
                self.collisions.push(ResourceDiagnostic {
                    diagnostic_type: ResourceDiagnosticType::Collision,
                    message: JsString::from("name \"") + &skill.name + "\" collision",
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
                self.real_paths.insert(real_path);
                self.skills.insert(skill.name.clone(), skill);
            }
        }
    }
}

fn is_under_path(target: &JsString, root: &JsString) -> bool {
    target == root || target.starts_with(root.clone() + "/")
}

fn prefix_ignore_pattern(line: JsString, prefix: &JsString) -> Option<JsString> {
    let trimmed = line.trim();
    if trimmed.is_empty() || (trimmed.starts_with('#') && !trimmed.starts_with("\\#")) {
        return None;
    }
    let negated = line.starts_with('!');
    let pattern = if negated || line.starts_with("\\!") {
        line.slice(1, line.len())
    } else {
        line
    };
    let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
    let prefixed = prefix.clone() + pattern;
    Some(if negated {
        JsString::from("!") + prefixed
    } else {
        prefixed
    })
}

fn add_ignore_rules(matcher: &IgnoreMatcher, dir: &JsString, root: &JsString) -> Result<(), JsString> {
    let relative = relative_path(root, dir).map_err(|error| error.message().clone())?;
    let prefix = if relative.is_empty() { relative } else { relative + "/" };
    for filename in IGNORE_FILE_NAMES {
        // Frozen catches each ignore-file read and ignores the unavailable file.
        let bytes = match fs::read(encode_node_utf8(&node_join(dir, filename))) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let content = JsString::from(String::from_utf8_lossy(&bytes).as_ref());
        let patterns: Vec<_> = content
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .filter_map(|line| prefix_ignore_pattern(line, &prefix))
            .collect();
        matcher.add(&patterns);
    }
    Ok(())
}

fn load_skills_from_dir_internal(
    dir: &JsString,
    source: &JsString,
    include_root: bool,
    matcher: Option<&IgnoreMatcher>,
    root: Option<&JsString>,
) -> Result<LoadSkillsResult, JsString> {
    let mut result = LoadSkillsResult::default();
    if !std::path::Path::new(&encode_node_utf8(dir)).exists() {
        return Ok(result);
    }
    let root = root.unwrap_or(dir);
    let owned_matcher;
    let matcher = match matcher {
        Some(matcher) => matcher,
        None => {
            owned_matcher = IgnoreMatcher::new(root);
            &owned_matcher
        }
    };
    add_ignore_rules(matcher, dir, root)?;
    // Frozen catches the whole directory traversal, preserving results already accumulated.
    match scan_skill_directory(dir, source, include_root, matcher, root, &mut result) {
        Ok(()) | Err(_) => Ok(result),
    }
}

fn scan_skill_directory(
    dir: &JsString,
    source: &JsString,
    include_root: bool,
    matcher: &IgnoreMatcher,
    root: &JsString,
    result: &mut LoadSkillsResult,
) -> Result<(), JsString> {
    let mut entries = fs::read_dir(encode_node_utf8(dir))
        .map_err(|error| JsString::from(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| JsString::from(error.to_string()))?;
    entries.sort_by(|left, right| {
        left.file_name()
            .as_encoded_bytes()
            .cmp(right.file_name().as_encoded_bytes())
    });
    for entry in &entries {
        if entry.file_name() != "SKILL.md" {
            continue;
        }
        let path = node_join(dir, "SKILL.md");
        let metadata = match fs::metadata(encode_node_utf8(&path)) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if !metadata.is_file() || is_ignored(matcher, &path, false)? {
            continue;
        }
        merge_skill_result(result, load_skill_from_file(&path, source));
        return Ok(());
    }
    for entry in entries {
        let name = JsString::from(entry.file_name().to_string_lossy().as_ref());
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let path = node_join(dir, &name);
        let metadata = match fs::metadata(encode_node_utf8(&path)) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if is_ignored(matcher, &path, metadata.is_dir())? {
            continue;
        }
        if metadata.is_dir() {
            merge_skill_result(
                result,
                load_skills_from_dir_internal(&path, source, false, Some(matcher), Some(root))?,
            );
        } else if metadata.is_file() && include_root && name.ends_with(".md") {
            merge_skill_result(result, load_skill_from_file(&path, source));
        }
    }
    Ok(())
}

fn is_ignored(matcher: &IgnoreMatcher, path: &JsString, directory: bool) -> Result<bool, JsString> {
    // IgnoreMatcher computes the relative path from its native root itself.
    matcher.ignores(path, directory).map_err(|error| error.message)
}

fn merge_skill_result(result: &mut LoadSkillsResult, next: LoadSkillsResult) {
    result.skills.extend(next.skills);
    result.diagnostics.extend(next.diagnostics);
}

fn load_skill_from_file(path: &JsString, source: &JsString) -> LoadSkillsResult {
    let parsed = fs::read(encode_node_utf8(path))
        .map_err(|error| JsString::from(error.to_string()))
        .and_then(|bytes| {
            parse_frontmatter(JsString::from(String::from_utf8_lossy(&bytes).as_ref()))
                .map_err(|error| error.message().clone())
        });
    let parsed = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            return LoadSkillsResult {
                skills: vec![],
                diagnostics: vec![warning(error, path)],
            };
        }
    };
    let directory = node_dirname(path);
    let description = parsed.frontmatter.get("description").and_then(JsValue::as_string);
    let mut diagnostics: Vec<_> = validate_description(description)
        .into_iter()
        .map(|message| warning(message, path))
        .collect();
    let name = parsed
        .frontmatter
        .get("name")
        .and_then(JsValue::as_string)
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| node_basename(&directory));
    diagnostics.extend(validate_name(&name).into_iter().map(|message| warning(message, path)));
    let Some(description) = description.filter(|description| !description.trim().is_empty()) else {
        return LoadSkillsResult {
            skills: vec![],
            diagnostics,
        };
    };
    let skill = Skill {
        name,
        description: description.clone(),
        file_path: path.clone(),
        base_dir: directory.clone(),
        source_info: create_skill_source_info(path, &directory, source),
        disable_model_invocation: parsed.frontmatter.get("disable-model-invocation") == Some(&JsValue::Bool(true)),
    };
    LoadSkillsResult {
        skills: vec![skill],
        diagnostics,
    }
}

fn create_skill_source_info(path: &JsString, base: &JsString, source: &JsString) -> SourceInfo {
    let (source, scope) = if source == "user" {
        (JsString::from("local"), Some(SourceScope::User))
    } else if source == "project" {
        (JsString::from("local"), Some(SourceScope::Project))
    } else if source == "path" {
        (JsString::from("local"), None)
    } else {
        (source.clone(), None)
    };
    create_synthetic_source_info(
        path,
        &SyntheticSourceInfoOptions {
            source,
            scope,
            origin: None,
            base_dir: Some(base.clone()),
        },
    )
}

fn validate_name(name: &JsString) -> Vec<JsString> {
    let mut errors = Vec::new();
    if name.len() > MAX_NAME_LENGTH {
        errors.push(format!("name exceeds {MAX_NAME_LENGTH} characters ({})", name.len()).into());
    }
    if name.is_empty()
        || !name
            .units()
            .iter()
            .all(|unit| matches!(unit, 0x61..=0x7a | 0x30..=0x39 | 0x2d))
    {
        errors.push("name contains invalid characters (must be lowercase a-z, 0-9, hyphens only)".into());
    }
    if name.starts_with('-') || name.ends_with('-') {
        errors.push("name must not start or end with a hyphen".into());
    }
    if name.contains("--") {
        errors.push("name must not contain consecutive hyphens".into());
    }
    errors
}

fn validate_description(description: Option<&JsString>) -> Vec<JsString> {
    match description {
        None => vec!["description is required".into()],
        Some(description) if description.trim().is_empty() => vec!["description is required".into()],
        Some(description) if description.len() > MAX_DESCRIPTION_LENGTH => vec![
            format!(
                "description exceeds {MAX_DESCRIPTION_LENGTH} characters ({})",
                description.len()
            )
            .into(),
        ],
        Some(_) => vec![],
    }
}

pub fn format_skills_for_prompt(skills: &[Skill]) -> JsString {
    let visible: Vec<_> = skills.iter().filter(|skill| !skill.disable_model_invocation).collect();
    if visible.is_empty() {
        return JsString::default();
    }
    let mut lines: Vec<JsString> = vec![
        "\n\nThe following skills provide specialized instructions for specific tasks.".into(),
        "Use the read tool to load a skill's file when the task matches its description.".into(),
        "When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.".into(),
        "".into(), "<available_skills>".into(),
    ];
    for skill in visible {
        lines.extend([
            "  <skill>".into(),
            JsString::from("    <name>") + escape_xml(&skill.name) + "</name>",
            JsString::from("    <description>") + escape_xml(&skill.description) + "</description>",
            JsString::from("    <location>") + escape_xml(&skill.file_path) + "</location>",
            "  </skill>".into(),
        ]);
    }
    lines.push("</available_skills>".into());
    JsString::join(lines, "\n")
}

fn escape_xml(text: &JsString) -> JsString {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn warning(message: impl Into<JsString>, path: &JsString) -> ResourceDiagnostic {
    ResourceDiagnostic {
        diagnostic_type: ResourceDiagnosticType::Warning,
        message: message.into(),
        path: Some(path.clone()),
        collision: None,
    }
}
