use super::frontmatter::parse_frontmatter;
use super::types::{ExecutionEnv, FileError, FileErrorCode, FileInfo, FileKind, PromptTemplate};
use crate::harness::types::{NodeRejection, separate_rejection};
use pi_js_compat::collator::locale_compare_utf16;
use pi_js_compat::js_json::JsString;

/// Stable diagnostic codes emitted while loading prompt templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTemplateDiagnosticCode {
    FileInfoFailed,
    ListFailed,
    ReadFailed,
    ParseFailed,
}

impl PromptTemplateDiagnosticCode {
    /// Returns the exact source diagnostic code literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FileInfoFailed => "file_info_failed",
            Self::ListFailed => "list_failed",
            Self::ReadFailed => "read_failed",
            Self::ParseFailed => "parse_failed",
        }
    }
}

/// Diagnostic severity emitted while loading prompt templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptTemplateDiagnosticType {
    Warning,
}

impl PromptTemplateDiagnosticType {
    /// Returns the exact source diagnostic severity literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
        }
    }
}

/// Value-only diagnostic emitted while loading prompt templates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTemplateDiagnostic {
    pub r#type: PromptTemplateDiagnosticType,
    pub code: PromptTemplateDiagnosticCode,
    pub message: JsString,
    pub path: JsString,
}

/// Prompt templates and diagnostics produced by [`load_prompt_templates`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PromptTemplateLoadResult {
    pub prompt_templates: Vec<PromptTemplate>,
    pub diagnostics: Vec<PromptTemplateDiagnostic>,
}

/// A prompt template tagged with its application-defined source value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedPromptTemplate<TSource> {
    pub prompt_template: PromptTemplate,
    pub source: TSource,
}

/// A diagnostic tagged with its application-defined source value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedPromptTemplateDiagnostic<TSource> {
    pub diagnostic: PromptTemplateDiagnostic,
    pub source: TSource,
}

/// Sourced prompt templates and diagnostics produced by [`load_sourced_prompt_templates`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcedPromptTemplateLoadResult<TSource> {
    pub prompt_templates: Vec<SourcedPromptTemplate<TSource>>,
    pub diagnostics: Vec<SourcedPromptTemplateDiagnostic<TSource>>,
}

/// Application subtype mapping applied to each loaded prompt template.
pub type PromptTemplateMapper<'a, TSource> = dyn Fn(&PromptTemplate, &TSource) -> PromptTemplate + Sync + 'a;

/// Load prompt templates from one or more paths.
///
/// Directory inputs load direct `.md` children non-recursively; file inputs load explicit `.md`
/// files. Missing paths and non-markdown files are skipped; read and parse failures become
/// diagnostics.
pub async fn load_prompt_templates(
    env: &dyn ExecutionEnv,
    paths: &[&JsString],
) -> Result<PromptTemplateLoadResult, NodeRejection> {
    Ok({
        let mut result = PromptTemplateLoadResult::default();
        for path in paths {
            let info = match separate_rejection(env.file_info(path).await)? {
                Ok(info) => info,
                Err(error) => {
                    push_file_info_diagnostic(&mut result.diagnostics, &error, path);
                    continue;
                }
            };
            match resolve_kind(env, &info, &mut result.diagnostics).await? {
                Some(FileKind::Directory) => {
                    let directory = load_templates_from_dir(env, &info.path).await?;
                    result.prompt_templates.extend(directory.prompt_templates);
                    result.diagnostics.extend(directory.diagnostics);
                }
                Some(FileKind::File) if info.name.ends_with(".md") => {
                    let (prompt_template, diagnostics) = load_template_from_file(env, &info.path, &info.name).await?;
                    if let Some(prompt_template) = prompt_template {
                        result.prompt_templates.push(prompt_template);
                    }
                    result.diagnostics.extend(diagnostics);
                }
                _ => {}
            }
        }
        result
    })
}

/// Load prompt templates from source-tagged paths.
///
/// `map_prompt_template` maps each loaded template into an application subtype while preserving
/// its source.
pub async fn load_sourced_prompt_templates<TSource: Clone>(
    env: &dyn ExecutionEnv,
    inputs: &[(JsString, TSource)],
    map_prompt_template: Option<&PromptTemplateMapper<'_, TSource>>,
) -> Result<SourcedPromptTemplateLoadResult<TSource>, NodeRejection> {
    Ok({
        let mut prompt_templates = Vec::new();
        let mut diagnostics = Vec::new();
        for (path, source) in inputs {
            let result = load_prompt_templates(env, &[path]).await?;
            for prompt_template in result.prompt_templates {
                let mapped = match map_prompt_template {
                    Some(map) => map(&prompt_template, source),
                    None => prompt_template,
                };
                prompt_templates.push(SourcedPromptTemplate {
                    prompt_template: mapped,
                    source: source.clone(),
                });
            }
            for diagnostic in result.diagnostics {
                diagnostics.push(SourcedPromptTemplateDiagnostic {
                    diagnostic,
                    source: source.clone(),
                });
            }
        }
        SourcedPromptTemplateLoadResult {
            prompt_templates,
            diagnostics,
        }
    })
}

async fn load_templates_from_dir(
    env: &dyn ExecutionEnv,
    dir: &JsString,
) -> Result<PromptTemplateLoadResult, NodeRejection> {
    Ok({
        let mut result = PromptTemplateLoadResult::default();
        let entries = match separate_rejection(env.list_dir(dir, None).await)? {
            Ok(entries) => entries,
            Err(error) => {
                result.diagnostics.push(PromptTemplateDiagnostic {
                    r#type: PromptTemplateDiagnosticType::Warning,
                    code: PromptTemplateDiagnosticCode::ListFailed,
                    message: error.message,
                    path: dir.to_owned(),
                });
                return Ok(result);
            }
        };

        let mut sorted = entries;
        sorted.sort_by(|left, right| locale_compare_utf16(left.name.units(), right.name.units()));
        for entry in sorted {
            if resolve_kind(env, &entry, &mut result.diagnostics).await? != Some(FileKind::File)
                || !entry.name.ends_with(".md")
            {
                continue;
            }
            let (prompt_template, diagnostics) = load_template_from_file(env, &entry.path, &entry.name).await?;
            if let Some(prompt_template) = prompt_template {
                result.prompt_templates.push(prompt_template);
            }
            result.diagnostics.extend(diagnostics);
        }
        result
    })
}

async fn load_template_from_file(
    env: &dyn ExecutionEnv,
    file_path: &JsString,
    file_name: &JsString,
) -> Result<(Option<PromptTemplate>, Vec<PromptTemplateDiagnostic>), NodeRejection> {
    Ok({
        let mut diagnostics = Vec::new();
        let content = match separate_rejection(env.read_text_file(file_path, None).await)? {
            Ok(content) => content,
            Err(error) => {
                diagnostics.push(PromptTemplateDiagnostic {
                    r#type: PromptTemplateDiagnosticType::Warning,
                    code: PromptTemplateDiagnosticCode::ReadFailed,
                    message: error.message,
                    path: file_path.to_owned(),
                });
                return Ok((None, diagnostics));
            }
        };
        let parsed = match parse_frontmatter(&content) {
            Ok(parsed) => parsed,
            Err(error) => {
                diagnostics.push(PromptTemplateDiagnostic {
                    r#type: PromptTemplateDiagnosticType::Warning,
                    code: PromptTemplateDiagnosticCode::ParseFailed,
                    message: error.message().into(),
                    path: file_path.to_owned(),
                });
                return Ok((None, diagnostics));
            }
        };

        let frontmatter = &parsed.frontmatter;
        let first_line = parsed.body.split('\n').find(|line| !line.trim().is_empty());
        let mut description = frontmatter
            .get("description")
            .and_then(|value| value.as_str())
            .map(JsString::from)
            .unwrap_or_default();
        if description.is_empty()
            && let Some(first_line) = first_line
        {
            description = JsString::from(first_line).slice(0, 60);
            if first_line.encode_utf16().count() > 60 {
                description.push_str("...");
            }
        }
        (
            Some(PromptTemplate {
                name: strip_markdown_extension(file_name),
                description: Some(description),
                content: parsed.body.into(),
            }),
            diagnostics,
        )
    })
}

fn strip_markdown_extension(file_name: &JsString) -> JsString {
    let suffix_start = file_name.len().saturating_sub(3);
    if file_name.len() >= 3 && file_name.slice(suffix_start, file_name.len()).to_lowercase() == ".md" {
        return file_name.slice(0, suffix_start);
    }
    file_name.clone()
}

async fn resolve_kind(
    env: &dyn ExecutionEnv,
    info: &FileInfo,
    diagnostics: &mut Vec<PromptTemplateDiagnostic>,
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

fn push_file_info_diagnostic(diagnostics: &mut Vec<PromptTemplateDiagnostic>, error: &FileError, path: &JsString) {
    if error.code == FileErrorCode::NotFound {
        return;
    }
    diagnostics.push(PromptTemplateDiagnostic {
        r#type: PromptTemplateDiagnosticType::Warning,
        code: PromptTemplateDiagnosticCode::FileInfoFailed,
        message: error.message.clone(),
        path: path.to_owned(),
    });
}

/// Parses ASCII-space/tab-delimited arguments, removing quotes and omitting empty tokens.
pub fn parse_command_args(input: impl Into<JsString>) -> Vec<JsString> {
    let input = input.into();
    let mut args = Vec::new();
    let mut current = JsString::default();
    let mut quote = None;
    for &unit in input.units() {
        match quote {
            Some(active) if unit == active => quote = None,
            Some(_) => current.push_unit(unit),
            None if unit == 0x22 || unit == 0x27 => quote = Some(unit),
            None if unit == 0x20 || unit == 0x09 => push_nonempty_token(&mut args, &mut current),
            None => current.push_unit(unit),
        }
    }
    push_nonempty_token(&mut args, &mut current);
    args
}

fn push_nonempty_token(args: &mut Vec<JsString>, current: &mut JsString) {
    if !current.is_empty() {
        args.push(std::mem::take(current));
    }
}

/// Substitutes in frozen order; aggregate string replacements interpret JavaScript dollar tokens.
pub fn substitute_args(content: impl Into<JsString>, args: &[JsString]) -> JsString {
    use pi_js_compat::js_regex::JsRegex;
    let positional = JsRegex::new(r"\$(\d+)").expect("valid positional regex");
    let result = positional.replace_all_with(content, |captures| {
        let index = parse_position(&captures.get(1).expect("position capture").text());
        index
            .checked_sub(1)
            .and_then(|index| args.get(index))
            .cloned()
            .unwrap_or_default()
    });
    let slices = JsRegex::new(r"\$\{@:(\d+)(?::(\d+))?\}").expect("valid slice regex");
    let result = slices.replace_all_with(&result, |captures| {
        let start = parse_position(&captures.get(1).expect("start capture").text())
            .saturating_sub(1)
            .min(args.len());
        let end = captures.get(2).map_or(args.len(), |length| {
            start.saturating_add(parse_position(&length.text())).min(args.len())
        });
        JsString::join(&args[start..end], " ")
    });
    let all_args = JsString::join(args, " ");
    let arguments = JsRegex::new(r"\$ARGUMENTS")
        .expect("valid arguments regex")
        .replace_all(result, &all_args);
    JsRegex::new(r"\$@")
        .expect("valid aggregate regex")
        .replace_all(arguments, all_args)
}

fn parse_position(text: &JsString) -> usize {
    text.units().iter().fold(0usize, |value, digit| {
        value
            .saturating_mul(10)
            .saturating_add(usize::from(*digit - u16::from(b'0')))
    })
}

/// Formats only the template content with positional arguments.
pub fn format_prompt_template_invocation(template: &PromptTemplate, args: &[JsString]) -> JsString {
    substitute_args(&template.content, args)
}
