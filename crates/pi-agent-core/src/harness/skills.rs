use std::sync::Mutex;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use super::frontmatter::parse_frontmatter;
use super::types::{ExecutionEnv, FileError, FileErrorCode, FileInfo, FileKind, Skill};

const MAX_NAME_LENGTH: usize = 64;
const MAX_DESCRIPTION_LENGTH: usize = 1024;
const IGNORE_FILE_NAMES: [&str; 3] = [".gitignore", ".ignore", ".fdignore"];

/// Formats one skill invocation with optional additional instructions.
///
/// Skill fields are interpolated without XML escaping.
pub fn format_skill_invocation(skill: &Skill, additional_instructions: Option<&str>) -> String {
    let dirname = dirname_env_path(&skill.file_path);
    let skill_block = format!(
        "<skill name=\"{}\" location=\"{}\">\nReferences are relative to {}.\n\n{}\n</skill>",
        skill.name, skill.file_path, dirname, skill.content
    );

    match additional_instructions {
        Some(instructions) if !instructions.is_empty() => {
            format!("{skill_block}\n\n{instructions}")
        }
        _ => skill_block,
    }
}

fn dirname_env_path(path: &str) -> String {
    let normalized = path.trim_end_matches(is_path_separator);
    let Some((separator_byte_index, separator_utf16_index)) = last_separator(normalized) else {
        return "/".to_owned();
    };

    if separator_utf16_index == 2 && second_utf16_unit_is_colon(normalized) {
        return normalized[..separator_byte_index + 1].to_owned();
    }
    if separator_utf16_index == 0 {
        return "/".to_owned();
    }
    normalized[..separator_byte_index].to_owned()
}

fn last_separator(path: &str) -> Option<(usize, usize)> {
    let mut last = None;
    let mut utf16_index = 0;
    for (byte_index, character) in path.char_indices() {
        if is_path_separator(character) {
            last = Some((byte_index, utf16_index));
        }
        utf16_index += character.len_utf16();
    }
    last
}

fn is_path_separator(character: char) -> bool {
    character == '/' || character == '\\'
}

fn second_utf16_unit_is_colon(value: &str) -> bool {
    value.encode_utf16().nth(1) == Some(':' as u16)
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
    pub message: String,
    pub path: String,
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
pub async fn load_skills(env: &dyn ExecutionEnv, dirs: &[&str]) -> SkillLoadResult {
    let mut result = SkillLoadResult::default();
    for dir in dirs {
        let root_info = match env.file_info(dir).await {
            Ok(info) => info,
            Err(error) => {
                push_file_info_diagnostic(&mut result.diagnostics, &error, dir);
                continue;
            }
        };
        if resolve_kind(env, &root_info, &mut result.diagnostics).await != Some(FileKind::Directory) {
            continue;
        }
        let root_dir = root_info.path.clone();
        let mut loader = Loader::new(env, &root_dir);
        loader.load(&root_dir).await;
        result.skills.extend(loader.skills);
        result.diagnostics.extend(loader.diagnostics);
    }
    result
}

/// Load skills from source-tagged directories.
///
/// `map_skill` maps each loaded skill into an application subtype while preserving its source.
pub async fn load_sourced_skills<TSource: Clone>(
    env: &dyn ExecutionEnv,
    inputs: &[(String, TSource)],
    map_skill: Option<&SkillMapper<'_, TSource>>,
) -> SourcedSkillLoadResult<TSource> {
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();
    for (path, source) in inputs {
        let result = load_skills(env, &[path.as_str()]).await;
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
    SourcedSkillLoadResult { skills, diagnostics }
}

/// Gitignore-style matcher accumulating rules from every visited directory.
struct IgnoreMatcher {
    state: Mutex<IgnoreMatcherState>,
}

struct IgnoreMatcherState {
    builder: GitignoreBuilder,
    compiled: Option<Gitignore>,
}

impl IgnoreMatcher {
    fn new(root_dir: &str) -> Self {
        Self {
            state: Mutex::new(IgnoreMatcherState {
                builder: GitignoreBuilder::new(root_dir),
                compiled: None,
            }),
        }
    }

    fn add(&self, patterns: &[String]) {
        let mut state = lock_matcher(&self.state);
        for pattern in patterns {
            let _ = state.builder.add_line(None, pattern);
        }
        state.compiled = None;
    }

    /// Matches one path, mirroring the source `ignore().ignores(path)` single-path check.
    fn ignores(&self, path: &str, is_dir: bool) -> bool {
        let mut state = lock_matcher(&self.state);
        if state.compiled.is_none() {
            state.compiled = state.builder.build().ok();
        }
        state
            .compiled
            .as_ref()
            .is_some_and(|matcher| matcher.matched(path, is_dir).is_ignore())
    }
}

fn lock_matcher(state: &Mutex<IgnoreMatcherState>) -> std::sync::MutexGuard<'_, IgnoreMatcherState> {
    state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct Loader<'a> {
    env: &'a dyn ExecutionEnv,
    root_dir: &'a str,
    matcher: IgnoreMatcher,
    skills: Vec<Skill>,
    diagnostics: Vec<SkillDiagnostic>,
}

enum Action {
    LoadDir { dir: String, include_root_files: bool },
    LoadFile { file_path: String, parent_dir_name: String },
}

impl<'a> Loader<'a> {
    fn new(env: &'a dyn ExecutionEnv, root_dir: &'a str) -> Self {
        Self {
            env,
            root_dir,
            matcher: IgnoreMatcher::new(root_dir),
            skills: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    async fn load(&mut self, root: &str) {
        let mut actions = vec![Action::LoadDir {
            dir: root.to_owned(),
            include_root_files: true,
        }];
        while let Some(action) = actions.pop() {
            match action {
                Action::LoadDir {
                    dir,
                    include_root_files,
                } => actions.extend(self.prepare_dir(&dir, include_root_files).await),
                Action::LoadFile {
                    file_path,
                    parent_dir_name,
                } => {
                    let (skill, diagnostics) = self.load_skill_file(&file_path, &parent_dir_name).await;
                    if let Some(skill) = skill {
                        self.skills.push(skill);
                    }
                    self.diagnostics.extend(diagnostics);
                }
            }
        }
    }

    /// Lists one directory and returns its follow-up actions in source order.
    async fn prepare_dir(&mut self, dir: &str, include_root_files: bool) -> Vec<Action> {
        let dir_info = match self.env.file_info(dir).await {
            Ok(info) => info,
            Err(error) => {
                push_file_info_diagnostic(&mut self.diagnostics, &error, dir);
                return Vec::new();
            }
        };
        if resolve_kind(self.env, &dir_info, &mut self.diagnostics).await != Some(FileKind::Directory) {
            return Vec::new();
        }
        self.add_ignore_rules(dir).await;

        let entries = match self.env.list_dir(dir, None).await {
            Ok(entries) => entries,
            Err(error) => {
                self.diagnostics.push(SkillDiagnostic {
                    r#type: SkillDiagnosticType::Warning,
                    code: SkillDiagnosticCode::ListFailed,
                    message: error.message,
                    path: dir.to_owned(),
                });
                return Vec::new();
            }
        };

        if let Some(action) = self.skill_file_action(&entries, &dir_info.name).await {
            return vec![action];
        }
        self.child_actions(entries, &dir_info.name, include_root_files).await
    }

    async fn skill_file_action(&mut self, entries: &[FileInfo], parent_dir_name: &str) -> Option<Action> {
        for entry in entries {
            if entry.name != "SKILL.md" {
                continue;
            }
            if resolve_kind(self.env, entry, &mut self.diagnostics).await != Some(FileKind::File) {
                continue;
            }
            if self.matcher.ignores(&entry.path, false) {
                continue;
            }
            return Some(Action::LoadFile {
                file_path: entry.path.clone(),
                parent_dir_name: parent_dir_name.to_owned(),
            });
        }
        None
    }

    async fn child_actions(
        &mut self,
        entries: Vec<FileInfo>,
        parent_dir_name: &str,
        include_root_files: bool,
    ) -> Vec<Action> {
        let mut sorted = entries;
        sorted.sort_by(|left, right| locale_compare(&left.name, &right.name));

        let mut actions = Vec::new();
        for entry in sorted {
            if entry.name.starts_with('.') || entry.name == "node_modules" {
                continue;
            }
            let Some(kind) = resolve_kind(self.env, &entry, &mut self.diagnostics).await else {
                continue;
            };
            if self.matcher.ignores(&entry.path, kind == FileKind::Directory) {
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
        actions
    }

    async fn add_ignore_rules(&mut self, dir: &str) {
        let relative_dir = relative_env_path(self.root_dir, dir);
        let prefix = if relative_dir.is_empty() {
            String::new()
        } else {
            format!("{relative_dir}/")
        };

        for filename in IGNORE_FILE_NAMES {
            let parts = [dir.to_owned(), filename.to_owned()];
            let ignore_path = match self.env.join_path(&parts, None).await {
                Ok(path) => path,
                Err(error) => {
                    push_file_info_diagnostic(&mut self.diagnostics, &error, dir);
                    continue;
                }
            };
            let info = match self.env.file_info(&ignore_path).await {
                Ok(info) => info,
                Err(error) => {
                    push_file_info_diagnostic(&mut self.diagnostics, &error, &ignore_path);
                    continue;
                }
            };
            if info.kind != FileKind::File {
                continue;
            }
            let content = match self.env.read_text_file(&ignore_path, None).await {
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
            let patterns: Vec<String> = content
                .split('\n')
                .filter_map(|line| prefix_ignore_pattern(line, &prefix))
                .collect();
            if !patterns.is_empty() {
                self.matcher.add(&patterns);
            }
        }
    }

    async fn load_skill_file(&self, file_path: &str, parent_dir_name: &str) -> (Option<Skill>, Vec<SkillDiagnostic>) {
        let mut diagnostics = Vec::new();
        let content = match self.env.read_text_file(file_path, None).await {
            Ok(content) => content,
            Err(error) => {
                diagnostics.push(SkillDiagnostic {
                    r#type: SkillDiagnosticType::Warning,
                    code: SkillDiagnosticCode::ReadFailed,
                    message: error.message,
                    path: file_path.to_owned(),
                });
                return (None, diagnostics);
            }
        };
        let parsed = match parse_frontmatter(&content) {
            Ok(parsed) => parsed,
            Err(error) => {
                diagnostics.push(SkillDiagnostic {
                    r#type: SkillDiagnosticType::Warning,
                    code: SkillDiagnosticCode::ParseFailed,
                    message: error.message().to_owned(),
                    path: file_path.to_owned(),
                });
                return (None, diagnostics);
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
            .unwrap_or(parent_dir_name);
        for error in validate_name(name, parent_dir_name) {
            diagnostics.push(invalid_metadata(&error, file_path));
        }
        let Some(description) = description.filter(|description| !description.trim().is_empty()) else {
            return (None, diagnostics);
        };

        (
            Some(Skill {
                name: name.to_owned(),
                description: description.to_owned(),
                content: parsed.body,
                file_path: file_path.to_owned(),
                disable_model_invocation: Some(
                    frontmatter.get("disable-model-invocation") == Some(&serde_json::Value::Bool(true)),
                ),
            }),
            diagnostics,
        )
    }
}

fn push_file_info_diagnostic(diagnostics: &mut Vec<SkillDiagnostic>, error: &FileError, path: &str) {
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

fn invalid_metadata(message: &str, path: &str) -> SkillDiagnostic {
    SkillDiagnostic {
        r#type: SkillDiagnosticType::Warning,
        code: SkillDiagnosticCode::InvalidMetadata,
        message: message.to_owned(),
        path: path.to_owned(),
    }
}

async fn resolve_kind(
    env: &dyn ExecutionEnv,
    info: &FileInfo,
    diagnostics: &mut Vec<SkillDiagnostic>,
) -> Option<FileKind> {
    if info.kind == FileKind::File || info.kind == FileKind::Directory {
        return Some(info.kind);
    }
    let canonical_path = match env.canonical_path(&info.path).await {
        Ok(path) => path,
        Err(error) => {
            push_file_info_diagnostic(diagnostics, &error, &info.path);
            return None;
        }
    };
    let target = match env.file_info(&canonical_path).await {
        Ok(target) => target,
        Err(error) => {
            push_file_info_diagnostic(diagnostics, &error, &info.path);
            return None;
        }
    };
    match target.kind {
        FileKind::File | FileKind::Directory => Some(target.kind),
        FileKind::Symlink => None,
    }
}

fn prefix_ignore_pattern(line: &str, prefix: &str) -> Option<String> {
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
    let prefixed = if prefix.is_empty() {
        pattern.to_owned()
    } else {
        format!("{prefix}{pattern}")
    };
    Some(if negated { format!("!{prefixed}") } else { prefixed })
}

fn validate_name(name: &str, parent_dir_name: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if name != parent_dir_name {
        errors.push(format!(
            "name \"{name}\" does not match parent directory \"{parent_dir_name}\""
        ));
    }
    let length = name.encode_utf16().count();
    if length > MAX_NAME_LENGTH {
        errors.push(format!("name exceeds {MAX_NAME_LENGTH} characters ({length})"));
    }
    if !name.chars().all(is_name_character) {
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

fn is_name_character(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
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

fn relative_env_path(root: &str, path: &str) -> String {
    let normalized_root = root.replace('\\', "/");
    let normalized_root = normalized_root.trim_end_matches('/');
    let normalized_path = path.replace('\\', "/");
    let normalized_path = normalized_path.trim_end_matches('/');
    if normalized_path == normalized_root {
        return String::new();
    }
    match normalized_path.strip_prefix(normalized_root) {
        Some(rest) if rest.starts_with('/') => rest[1..].to_owned(),
        _ => normalized_path.trim_start_matches('/').to_owned(),
    }
}

/// Approximates the default `Intl.Collator` ordering for ASCII names:
/// case-insensitive primary order, then lowercase before uppercase.
fn locale_compare(left: &str, right: &str) -> std::cmp::Ordering {
    let primary = left.to_lowercase().cmp(&right.to_lowercase());
    if primary != std::cmp::Ordering::Equal {
        return primary;
    }
    for (left_character, right_character) in left.chars().zip(right.chars()) {
        if left_character == right_character {
            continue;
        }
        let left_rank = u8::from(left_character.is_uppercase());
        let right_rank = u8::from(right_character.is_uppercase());
        return left_rank
            .cmp(&right_rank)
            .then_with(|| left_character.cmp(&right_character));
    }
    left.len().cmp(&right.len())
}
