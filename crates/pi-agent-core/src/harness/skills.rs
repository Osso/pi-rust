use std::sync::Mutex;

use crate::harness::types::{NodeRejection, OperationFailure, separate_rejection};
use pi_js_compat::collator::locale_compare_utf16;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::node_ignore::{Ignore, IgnoreError};

use super::frontmatter::parse_frontmatter;
use super::types::{ExecutionEnv, FileError, FileErrorCode, FileInfo, FileKind, Skill};

const MAX_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_LENGTH: usize = 1024;
const IGNORE_FILE_NAMES: [&str; 3] = [".gitignore", ".ignore", ".fdignore"];

/// Formats one skill invocation with optional additional instructions.
///
/// Skill fields are interpolated without XML escaping.
pub fn format_skill_invocation(skill: &Skill, additional_instructions: Option<&JsString>) -> JsString {
    let dirname = dirname_env_path(&skill.file_path);
    let skill_block = js_format!(
        "<skill name=\"{}\" location=\"{}\">\nReferences are relative to {}.\n\n{}\n</skill>",
        &skill.name,
        &skill.file_path,
        dirname,
        &skill.content
    );

    match additional_instructions {
        Some(instructions) if !instructions.is_empty() => {
            js_format!("{}\n\n{}", skill_block, instructions)
        }
        _ => skill_block,
    }
}

fn dirname_env_path(path: &JsString) -> JsString {
    let end = path
        .units()
        .iter()
        .rposition(|unit| !is_path_separator(*unit))
        .map_or(0, |index| index + 1);
    let normalized = path.slice(0, end);
    let Some(index) = normalized.units().iter().rposition(|unit| is_path_separator(*unit)) else {
        return "/".into();
    };
    if index == 0 {
        return "/".into();
    }
    normalized.slice(0, index)
}

fn is_path_separator(unit: u16) -> bool {
    unit == 0x2f
}

/// Stable diagnostic codes emitted while loading skills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillDiagnosticCode {
    FileInfoFailed,
    ListFailed,
    ReadFailed,
    ParseFailed,
    InvalidMetadata,
}

impl SkillDiagnosticCode {
    /// Returns the exact source diagnostic code literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FileInfoFailed => "file_info_failed",
            Self::ListFailed => "list_failed",
            Self::ReadFailed => "read_failed",
            Self::ParseFailed => "parse_failed",
            Self::InvalidMetadata => "invalid_metadata",
        }
    }
}

/// Diagnostic severity emitted while loading skills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillDiagnosticType {
    Warning,
}

impl SkillDiagnosticType {
    /// Returns the exact source diagnostic severity literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
        }
    }
}

/// Value-only diagnostic emitted while loading skills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDiagnostic {
    pub r#type: SkillDiagnosticType,
    pub code: SkillDiagnosticCode,
    pub message: JsString,
    pub path: JsString,
}

/// Skills and diagnostics produced by [`load_skills`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkillLoadResult {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<SkillDiagnostic>,
}

/// A skill tagged with its application-defined source value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedSkill<TSource> {
    pub skill: Skill,
    pub source: TSource,
}

/// A diagnostic tagged with its application-defined source value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedSkillDiagnostic<TSource> {
    pub diagnostic: SkillDiagnostic,
    pub source: TSource,
}

/// Sourced skills and diagnostics produced by [`load_sourced_skills`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedSkillLoadResult<TSource> {
    pub skills: Vec<SourcedSkill<TSource>>,
    pub diagnostics: Vec<SourcedSkillDiagnostic<TSource>>,
}

/// Application subtype mapping applied to each loaded skill.
pub type SkillMapper<'a, TSource> = dyn Fn(&Skill, &TSource) -> Skill + Sync + 'a;

/// Load skills from one or more directories.
///
/// Traverses directories recursively, loads `SKILL.md` files and direct root `.md` files, honors
/// `.gitignore`/`.ignore`/`.fdignore` rules, and reports diagnostics for invalid skill files.
pub async fn load_skills(
    env: &dyn ExecutionEnv,
    dirs: &[&JsString],
) -> Result<SkillLoadResult, OperationFailure<IgnoreError>> {
    let mut result = SkillLoadResult::default();
    for dir in dirs {
        let root_info = match separate_rejection(env.file_info(dir).await)? {
            Ok(info) => info,
            Err(error) => {
                push_file_info_diagnostic(&mut result.diagnostics, &error, dir);
                continue;
            }
        };
        if resolve_kind(env, &root_info, &mut result.diagnostics).await? != Some(FileKind::Directory) {
            continue;
        }
        let root_dir = root_info.path.clone();
        let mut loader = Loader::new(env, &root_dir);
        loader.load(&root_dir).await?;
        result.skills.extend(loader.skills);
        result.diagnostics.extend(loader.diagnostics);
    }
    Ok(result)
}

/// Load skills from source-tagged directories.
///
/// `map_skill` maps each loaded skill into an application subtype while preserving its source.
pub async fn load_sourced_skills<TSource: Clone>(
    env: &dyn ExecutionEnv,
    inputs: &[(JsString, TSource)],
    map_skill: Option<&SkillMapper<'_, TSource>>,
) -> Result<SourcedSkillLoadResult<TSource>, OperationFailure<IgnoreError>> {
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();
    for (path, source) in inputs {
        let result = load_skills(env, &[path]).await?;
        for skill in result.skills {
            let mapped = match map_skill {
                Some(map) => map(&skill, source),
                None => skill,
            };
            skills.push(SourcedSkill {
                skill: mapped,
                source: source.clone(),
            });
        }
        for diagnostic in result.diagnostics {
            diagnostics.push(SourcedSkillDiagnostic {
                diagnostic,
                source: source.clone(),
            });
        }
    }
    Ok(SourcedSkillLoadResult { skills, diagnostics })
}

/// Gitignore-style matcher accumulating rules from every visited directory.
pub struct IgnoreMatcher {
    root_dir: JsString,
    state: Mutex<Ignore>,
}

impl IgnoreMatcher {
    pub fn new(root_dir: impl Into<JsString>) -> Self {
        Self {
            root_dir: root_dir.into(),
            state: Mutex::new(Ignore::new()),
        }
    }

    pub fn add(&self, patterns: &[JsString]) {
        let mut matcher = self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for pattern in patterns {
            matcher.add(pattern.clone());
        }
    }

    pub fn ignores(&self, path: impl Into<JsString>, is_dir: bool) -> Result<bool, IgnoreError> {
        let mut relative = relative_env_path(&self.root_dir, &path.into());
        if is_dir && !relative.ends_with('/') {
            relative.push_str("/");
        }
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .ignores(relative)
    }
}

struct Loader<'a> {
    env: &'a dyn ExecutionEnv,
    root_dir: &'a JsString,
    matcher: IgnoreMatcher,
    skills: Vec<Skill>,
    diagnostics: Vec<SkillDiagnostic>,
}

enum Action {
    LoadDir {
        dir: JsString,
        include_root_files: bool,
    },
    LoadFile {
        file_path: JsString,
        parent_dir_name: JsString,
    },
}

impl<'a> Loader<'a> {
    fn new(env: &'a dyn ExecutionEnv, root_dir: &'a JsString) -> Self {
        Self {
            env,
            root_dir,
            matcher: IgnoreMatcher::new(root_dir),
            skills: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    async fn load(&mut self, root: &JsString) -> Result<(), OperationFailure<IgnoreError>> {
        let mut actions = vec![Action::LoadDir {
            dir: root.to_owned(),
            include_root_files: true,
        }];
        while let Some(action) = actions.pop() {
            match action {
                Action::LoadDir {
                    dir,
                    include_root_files,
                } => actions.extend(self.prepare_dir(&dir, include_root_files).await?),
                Action::LoadFile {
                    file_path,
                    parent_dir_name,
                } => {
                    let (skill, diagnostics) = self.load_skill_file(&file_path, &parent_dir_name).await?;
                    if let Some(skill) = skill {
                        self.skills.push(skill);
                    }
                    self.diagnostics.extend(diagnostics);
                }
            }
        }
        Ok(())
    }

    /// Lists one directory and returns its follow-up actions in source order.
    async fn prepare_dir(
        &mut self,
        dir: &JsString,
        include_root_files: bool,
    ) -> Result<Vec<Action>, OperationFailure<IgnoreError>> {
        let dir_info = match separate_rejection(self.env.file_info(dir).await)? {
            Ok(info) => info,
            Err(error) => {
                push_file_info_diagnostic(&mut self.diagnostics, &error, dir);
                return Ok(Vec::new());
            }
        };
        if resolve_kind(self.env, &dir_info, &mut self.diagnostics).await? != Some(FileKind::Directory) {
            return Ok(Vec::new());
        }
        self.add_ignore_rules(dir).await?;

        let entries = match separate_rejection(self.env.list_dir(dir, None).await)? {
            Ok(entries) => entries,
            Err(error) => {
                self.diagnostics.push(SkillDiagnostic {
                    r#type: SkillDiagnosticType::Warning,
                    code: SkillDiagnosticCode::ListFailed,
                    message: error.message,
                    path: dir.to_owned(),
                });
                return Ok(Vec::new());
            }
        };

        if let Some(action) = self.skill_file_action(&entries, &dir_info.name).await? {
            return Ok(vec![action]);
        }
        self.child_actions(entries, &dir_info.name, include_root_files).await
    }

    async fn skill_file_action(
        &mut self,
        entries: &[FileInfo],
        parent_dir_name: &JsString,
    ) -> Result<Option<Action>, OperationFailure<IgnoreError>> {
        for entry in entries {
            if entry.name != "SKILL.md" {
                continue;
            }
            if resolve_kind(self.env, entry, &mut self.diagnostics).await? != Some(FileKind::File) {
                continue;
            }
            if self.matcher.ignores(&entry.path, false)? {
                continue;
            }
            return Ok(Some(Action::LoadFile {
                file_path: entry.path.clone(),
                parent_dir_name: parent_dir_name.to_owned(),
            }));
        }
        Ok(None)
    }

    async fn child_actions(
        &mut self,
        entries: Vec<FileInfo>,
        parent_dir_name: &JsString,
        include_root_files: bool,
    ) -> Result<Vec<Action>, OperationFailure<IgnoreError>> {
        let mut sorted = entries;
        sorted.sort_by(|left, right| locale_compare_utf16(left.name.units(), right.name.units()));

        let mut actions = Vec::new();
        for entry in sorted {
            if entry.name.starts_with('.') || entry.name == "node_modules" {
                continue;
            }
            let Some(kind) = resolve_kind(self.env, &entry, &mut self.diagnostics).await? else {
                continue;
            };
            if self.matcher.ignores(&entry.path, kind == FileKind::Directory)? {
                continue;
            }
            if kind == FileKind::Directory {
                actions.push(Action::LoadDir {
                    dir: entry.path,
                    include_root_files: false,
                });
                continue;
            }
            if !include_root_files || !entry.name.ends_with(".md") {
                continue;
            }
            actions.push(Action::LoadFile {
                file_path: entry.path,
                parent_dir_name: parent_dir_name.to_owned(),
            });
        }
        actions.reverse();
        Ok(actions)
    }

    async fn add_ignore_rules(&mut self, dir: &JsString) -> Result<(), NodeRejection> {
        Ok({
            let relative_dir = relative_env_path(self.root_dir, dir);
            let prefix = if relative_dir.is_empty() {
                JsString::default()
            } else {
                relative_dir + "/"
            };

            for filename in IGNORE_FILE_NAMES {
                let parts = [dir.to_owned(), JsString::from(filename)];
                let ignore_path = match self.env.join_path(&parts, None).await {
                    Ok(path) => path,
                    Err(error) => {
                        push_file_info_diagnostic(&mut self.diagnostics, &error, dir);
                        continue;
                    }
                };
                let info = match separate_rejection(self.env.file_info(&ignore_path).await)? {
                    Ok(info) => info,
                    Err(error) => {
                        push_file_info_diagnostic(&mut self.diagnostics, &error, &ignore_path);
                        continue;
                    }
                };
                if info.kind != FileKind::File {
                    continue;
                }
                let content = match separate_rejection(self.env.read_text_file(&ignore_path, None).await)? {
                    Ok(content) => content,
                    Err(error) => {
                        self.diagnostics.push(SkillDiagnostic {
                            r#type: SkillDiagnosticType::Warning,
                            code: SkillDiagnosticCode::ReadFailed,
                            message: error.message,
                            path: ignore_path,
                        });
                        continue;
                    }
                };
                let patterns: Vec<JsString> = content
                    .split('\n')
                    .filter_map(|line| prefix_ignore_pattern(line, &prefix))
                    .collect();
                if !patterns.is_empty() {
                    self.matcher.add(&patterns);
                }
            }
        })
    }

    async fn load_skill_file(
        &self,
        file_path: &JsString,
        parent_dir_name: &JsString,
    ) -> Result<(Option<Skill>, Vec<SkillDiagnostic>), NodeRejection> {
        Ok({
            let mut diagnostics = Vec::new();
            let content = match separate_rejection(self.env.read_text_file(file_path, None).await)? {
                Ok(content) => content,
                Err(error) => {
                    diagnostics.push(SkillDiagnostic {
                        r#type: SkillDiagnosticType::Warning,
                        code: SkillDiagnosticCode::ReadFailed,
                        message: error.message,
                        path: file_path.to_owned(),
                    });
                    return Ok((None, diagnostics));
                }
            };
            let parsed = match parse_frontmatter(&content) {
                Ok(parsed) => parsed,
                Err(error) => {
                    diagnostics.push(SkillDiagnostic {
                        r#type: SkillDiagnosticType::Warning,
                        code: SkillDiagnosticCode::ParseFailed,
                        message: error.message().into(),
                        path: file_path.to_owned(),
                    });
                    return Ok((None, diagnostics));
                }
            };
            let frontmatter = &parsed.frontmatter;
            let description = frontmatter.get("description").and_then(|value| value.as_str());
            for error in validate_description(description) {
                diagnostics.push(invalid_metadata(&error, file_path));
            }
            let name = frontmatter
                .get("name")
                .and_then(|value| value.as_str())
                .filter(|name| !name.is_empty())
                .map(JsString::from)
                .unwrap_or_else(|| parent_dir_name.clone());
            for error in validate_name(&name, parent_dir_name) {
                diagnostics.push(invalid_metadata(&error, file_path));
            }
            let Some(description) = description.filter(|description| !description.trim().is_empty()) else {
                return Ok((None, diagnostics));
            };

            (
                Some(Skill {
                    name,
                    description: description.into(),
                    content: parsed.body.into(),
                    file_path: file_path.to_owned(),
                    disable_model_invocation: Some(
                        frontmatter.get("disable-model-invocation") == Some(&serde_json::Value::Bool(true)),
                    ),
                }),
                diagnostics,
            )
        })
    }
}

fn push_file_info_diagnostic(diagnostics: &mut Vec<SkillDiagnostic>, error: &FileError, path: &JsString) {
    if error.code == FileErrorCode::NotFound {
        return;
    }
    diagnostics.push(SkillDiagnostic {
        r#type: SkillDiagnosticType::Warning,
        code: SkillDiagnosticCode::FileInfoFailed,
        message: error.message.clone(),
        path: path.to_owned(),
    });
}

fn invalid_metadata(message: impl Into<JsString>, path: &JsString) -> SkillDiagnostic {
    SkillDiagnostic {
        r#type: SkillDiagnosticType::Warning,
        code: SkillDiagnosticCode::InvalidMetadata,
        message: message.into(),
        path: path.to_owned(),
    }
}

async fn resolve_kind(
    env: &dyn ExecutionEnv,
    info: &FileInfo,
    diagnostics: &mut Vec<SkillDiagnostic>,
) -> Result<Option<FileKind>, NodeRejection> {
    Ok({
        if info.kind == FileKind::File || info.kind == FileKind::Directory {
            return Ok(Some(info.kind));
        }
        let canonical_path = match separate_rejection(env.canonical_path(&info.path).await)? {
            Ok(path) => path,
            Err(error) => {
                push_file_info_diagnostic(diagnostics, &error, &info.path);
                return Ok(None);
            }
        };
        let target = match separate_rejection(env.file_info(&canonical_path).await)? {
            Ok(target) => target,
            Err(error) => {
                push_file_info_diagnostic(diagnostics, &error, &info.path);
                return Ok(None);
            }
        };
        match target.kind {
            FileKind::File | FileKind::Directory => Some(target.kind),
            FileKind::Symlink => None,
        }
    })
}

fn prefix_ignore_pattern(line: &str, prefix: &JsString) -> Option<JsString> {
    let trimmed = line.trim();
    if trimmed.is_empty() || (trimmed.starts_with('#') && !trimmed.starts_with("\\#")) {
        return None;
    }
    let mut pattern = line;
    let mut negated = false;
    if let Some(rest) = pattern.strip_prefix('!') {
        negated = true;
        pattern = rest;
    } else if let Some(rest) = pattern.strip_prefix("\\!") {
        pattern = rest;
    }
    if let Some(rest) = pattern.strip_prefix('/') {
        pattern = rest;
    }
    let prefixed = prefix.clone() + pattern;
    Some(if negated {
        JsString::from("!") + prefixed
    } else {
        prefixed
    })
}

fn validate_name(name: &JsString, parent_dir_name: &JsString) -> Vec<JsString> {
    let mut errors = Vec::new();
    if name != parent_dir_name {
        errors.push(js_format!(
            "name \"{}\" does not match parent directory \"{}\"",
            name,
            parent_dir_name
        ));
    }
    let length = name.len();
    if length > MAX_NAME_LENGTH {
        errors.push(format!("name exceeds {MAX_NAME_LENGTH} characters ({length})").into());
    }
    if !name
        .units()
        .iter()
        .all(|unit| matches!(*unit, 0x61..=0x7a | 0x30..=0x39 | 0x2d))
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

fn validate_description(description: Option<&str>) -> Vec<String> {
    let mut errors = Vec::new();
    match description {
        None => errors.push("description is required".to_owned()),
        Some(description) if description.trim().is_empty() => errors.push("description is required".to_owned()),
        Some(description) => {
            let length = description.encode_utf16().count();
            if length > MAX_DESCRIPTION_LENGTH {
                errors.push(format!(
                    "description exceeds {MAX_DESCRIPTION_LENGTH} characters ({length})"
                ));
            }
        }
    }
    errors
}

fn relative_env_path(root: &JsString, path: &JsString) -> JsString {
    let root = root.replace('\\', "/");
    let path = path.replace('\\', "/");
    let root_end = root
        .units()
        .iter()
        .rposition(|unit| *unit != 0x2f)
        .map_or(0, |index| index + 1);
    let path_end = path
        .units()
        .iter()
        .rposition(|unit| *unit != 0x2f)
        .map_or(0, |index| index + 1);
    let root = root.slice(0, root_end);
    let path = path.slice(0, path_end);
    if path == root {
        return JsString::default();
    }
    if let Some(rest) = path.strip_prefix(&root)
        && rest.starts_with('/')
    {
        return rest.slice(1, rest.len());
    }
    let start = path.units().iter().position(|unit| *unit != 0x2f).unwrap_or(path.len());
    path.slice(start, path.len())
}

impl From<NodeRejection> for OperationFailure<IgnoreError> {
    fn from(error: NodeRejection) -> Self {
        Self::Rejected(error)
    }
}
