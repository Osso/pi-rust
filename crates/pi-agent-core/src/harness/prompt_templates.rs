use super::frontmatter::parse_frontmatter;
use super::types::{ExecutionEnv, FileError, FileErrorCode, FileInfo, FileKind, PromptTemplate};
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
pub async fn load_prompt_templates(env: &dyn ExecutionEnv, paths: &[&JsString]) -> PromptTemplateLoadResult {
    let mut result = PromptTemplateLoadResult::default();
    for path in paths {
        let info = match env.file_info(path).await {
            Ok(info) => info,
            Err(error) => {
                push_file_info_diagnostic(&mut result.diagnostics, &error, path);
                continue;
            }
        };
        match resolve_kind(env, &info, &mut result.diagnostics).await {
            Some(FileKind::Directory) => {
                let directory = load_templates_from_dir(env, &info.path).await;
                result.prompt_templates.extend(directory.prompt_templates);
                result.diagnostics.extend(directory.diagnostics);
            }
            Some(FileKind::File) if info.name.ends_with(".md") => {
                let (prompt_template, diagnostics) = load_template_from_file(env, &info.path, &info.name).await;
                if let Some(prompt_template) = prompt_template {
                    result.prompt_templates.push(prompt_template);
                }
                result.diagnostics.extend(diagnostics);
            }
            _ => {}
        }
    }
    result
}

/// Load prompt templates from source-tagged paths.
///
/// `map_prompt_template` maps each loaded template into an application subtype while preserving
/// its source.
pub async fn load_sourced_prompt_templates<TSource: Clone>(
    env: &dyn ExecutionEnv,
    inputs: &[(JsString, TSource)],
    map_prompt_template: Option<&PromptTemplateMapper<'_, TSource>>,
) -> SourcedPromptTemplateLoadResult<TSource> {
    let mut prompt_templates = Vec::new();
    let mut diagnostics = Vec::new();
    for (path, source) in inputs {
        let result = load_prompt_templates(env, &[path]).await;
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
}

async fn load_templates_from_dir(env: &dyn ExecutionEnv, dir: &JsString) -> PromptTemplateLoadResult {
    let mut result = PromptTemplateLoadResult::default();
    let entries = match env.list_dir(dir, None).await {
        Ok(entries) => entries,
        Err(error) => {
            result.diagnostics.push(PromptTemplateDiagnostic {
                r#type: PromptTemplateDiagnosticType::Warning,
                code: PromptTemplateDiagnosticCode::ListFailed,
                message: error.message,
                path: dir.to_owned(),
            });
            return result;
        }
    };

    let mut sorted = entries;
    sorted.sort_by(|left, right| locale_compare_utf16(left.name.units(), right.name.units()));
    for entry in sorted {
        if resolve_kind(env, &entry, &mut result.diagnostics).await != Some(FileKind::File)
            || !entry.name.ends_with(".md")
        {
            continue;
        }
        let (prompt_template, diagnostics) = load_template_from_file(env, &entry.path, &entry.name).await;
        if let Some(prompt_template) = prompt_template {
            result.prompt_templates.push(prompt_template);
        }
        result.diagnostics.extend(diagnostics);
    }
    result
}

async fn load_template_from_file(
    env: &dyn ExecutionEnv,
    file_path: &JsString,
    file_name: &JsString,
) -> (Option<PromptTemplate>, Vec<PromptTemplateDiagnostic>) {
    let mut diagnostics = Vec::new();
    let content = match env.read_text_file(file_path, None).await {
        Ok(content) => content,
        Err(error) => {
            diagnostics.push(PromptTemplateDiagnostic {
                r#type: PromptTemplateDiagnosticType::Warning,
                code: PromptTemplateDiagnosticCode::ReadFailed,
                message: error.message,
                path: file_path.to_owned(),
            });
            return (None, diagnostics);
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
            return (None, diagnostics);
        }
    };

    let frontmatter = &parsed.frontmatter;
    let first_line = parsed.body.split('\n').find(|line| !line.trim().is_empty());
    let mut description = frontmatter
        .get("description")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    if description.is_empty()
        && let Some(first_line) = first_line
    {
        description = utf16_prefix(first_line, 60);
        if first_line.encode_utf16().count() > 60 {
            description.push_str("...");
        }
    }
    (
        Some(PromptTemplate {
            name: strip_markdown_extension(file_name),
            description: Some(description),
            content: parsed.body,
        }),
        diagnostics,
    )
}

fn strip_markdown_extension(file_name: &JsString) -> JsString {
    let suffix_start = file_name.len().saturating_sub(3);
    if file_name.len() >= 3 && file_name.slice(suffix_start, file_name.len()).to_lowercase() == ".md" {
        return file_name.slice(0, suffix_start);
    }
    file_name.clone()
}

/// `String.prototype.slice(0, length)` over UTF-16 code units.
fn utf16_prefix(value: &str, length: usize) -> String {
    String::from_utf16_lossy(&value.encode_utf16().take(length).collect::<Vec<u16>>())
}

async fn resolve_kind(
    env: &dyn ExecutionEnv,
    info: &FileInfo,
    diagnostics: &mut Vec<PromptTemplateDiagnostic>,
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

/// Parses ASCII-space/tab-delimited arguments with source-compatible quote handling.
///
/// Quotes are removed; backslashes and other whitespace remain literal.
/// Unmatched quotes are accepted, and empty arguments are omitted.
pub fn parse_command_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for character in input.chars() {
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            } else {
                current.push(character);
            }
        } else if character == '\'' || character == '"' {
            quote = Some(character);
        } else if character == ' ' || character == '\t' {
            push_nonempty_token(&mut args, &mut current);
        } else {
            current.push(character);
        }
    }

    push_nonempty_token(&mut args, &mut current);
    args
}

/// Substitutes positional, slice, `$ARGUMENTS`, and `$@` placeholders in source order.
///
/// Aggregate replacements preserve JavaScript replacement-string semantics.
pub fn substitute_args(content: &str, args: &[String]) -> String {
    let positional = replace_positional(content, args);
    let slices = replace_slices(&positional, args);
    let all_args = args.join(" ");
    let arguments = replace_with_js_replacement_semantics(&slices, "$ARGUMENTS", &all_args);
    replace_with_js_replacement_semantics(&arguments, "$@", &all_args)
}

/// Formats `template.content` with positional arguments.
///
/// Pass `&[]` to represent the TypeScript default empty argument list.
pub fn format_prompt_template_invocation(template: &PromptTemplate, args: &[String]) -> String {
    substitute_args(&template.content, args)
}

fn push_nonempty_token(args: &mut Vec<String>, current: &mut String) {
    if !current.is_empty() {
        args.push(std::mem::take(current));
    }
}

fn replace_positional(content: &str, args: &[String]) -> String {
    let bytes = content.as_bytes();
    let mut result = String::with_capacity(content.len());
    let mut copy_start = 0;
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'$' {
            let digit_start = index + 1;
            if digit_start < bytes.len() && is_ascii_digit(bytes[digit_start]) {
                let match_end = scan_ascii_digits(bytes, digit_start);
                result.push_str(&content[copy_start..index]);
                append_positional_value(&mut result, &bytes[digit_start..match_end], args);
                copy_start = match_end;
                index = match_end;
                continue;
            }
        }
        index += 1;
    }

    result.push_str(&content[copy_start..]);
    result
}

fn append_positional_value(result: &mut String, digits: &[u8], args: &[String]) {
    let Some(position) = parse_decimal_at_most(digits, args.len()) else {
        return;
    };
    if position > 0 {
        result.push_str(&args[position - 1]);
    }
}

fn replace_slices(content: &str, args: &[String]) -> String {
    let bytes = content.as_bytes();
    let mut result = String::with_capacity(content.len());
    let mut copy_start = 0;
    let mut index = 0;

    while index < bytes.len() {
        if let Some(slice_match) = parse_slice_match(bytes, index) {
            result.push_str(&content[copy_start..index]);
            append_slice_value(&mut result, &slice_match, args);
            copy_start = slice_match.end;
            index = slice_match.end;
        } else {
            index += 1;
        }
    }

    result.push_str(&content[copy_start..]);
    result
}

struct SliceMatch<'a> {
    end: usize,
    start_digits: &'a [u8],
    length_digits: Option<&'a [u8]>,
}

fn parse_slice_match(bytes: &[u8], start: usize) -> Option<SliceMatch<'_>> {
    let prefix_end = start.checked_add(4)?;
    if bytes.get(start..prefix_end) != Some(b"${@:") {
        return None;
    }

    let start_digits_begin = prefix_end;
    let start_digits_end = scan_ascii_digits(bytes, start_digits_begin);
    if start_digits_begin == start_digits_end {
        return None;
    }

    let mut cursor = start_digits_end;
    let length_digits = if bytes.get(cursor) == Some(&b':') {
        let length_digits_begin = cursor + 1;
        let length_digits_end = scan_ascii_digits(bytes, length_digits_begin);
        if length_digits_begin == length_digits_end {
            return None;
        }
        cursor = length_digits_end;
        Some(&bytes[length_digits_begin..length_digits_end])
    } else {
        None
    };

    if bytes.get(cursor) != Some(&b'}') {
        return None;
    }

    Some(SliceMatch {
        end: cursor + 1,
        start_digits: &bytes[start_digits_begin..start_digits_end],
        length_digits,
    })
}

fn append_slice_value(result: &mut String, slice_match: &SliceMatch<'_>, args: &[String]) {
    let start_number = parse_decimal_at_most(slice_match.start_digits, args.len());
    let start = start_number.map_or(args.len(), |number| number.saturating_sub(1));
    let end = match slice_match.length_digits {
        None => args.len(),
        Some(length_digits) => {
            let length = parse_decimal_at_most(length_digits, args.len()).unwrap_or(args.len());
            start.saturating_add(length).min(args.len())
        }
    };

    append_joined_args(result, &args[start..end]);
}

fn append_joined_args(result: &mut String, args: &[String]) {
    for (index, argument) in args.iter().enumerate() {
        if index > 0 {
            result.push(' ');
        }
        result.push_str(argument);
    }
}

fn replace_with_js_replacement_semantics(content: &str, needle: &str, replacement: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut search_start = 0;

    while let Some(relative_match_start) = content[search_start..].find(needle) {
        let match_start = search_start + relative_match_start;
        let match_end = match_start + needle.len();
        result.push_str(&content[search_start..match_start]);
        append_replacement(&mut result, content, match_start, match_end, replacement);
        search_start = match_end;
    }

    result.push_str(&content[search_start..]);
    result
}

fn append_replacement(output: &mut String, content: &str, match_start: usize, match_end: usize, replacement: &str) {
    let bytes = replacement.as_bytes();
    let mut literal_start = 0;
    let mut index = 0;

    while index < bytes.len().saturating_sub(1) {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }

        let token = bytes[index + 1];
        let token_end = index + 2;
        let value = match token {
            b'$' => Some("$"),
            b'&' => Some(&content[match_start..match_end]),
            b'`' => Some(&content[..match_start]),
            b'\'' => Some(&content[match_end..]),
            _ => None,
        };
        let Some(value) = value else {
            index += 1;
            continue;
        };

        output.push_str(&replacement[literal_start..index]);
        output.push_str(value);
        literal_start = token_end;
        index = token_end;
    }

    output.push_str(&replacement[literal_start..]);
}

fn scan_ascii_digits(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len() && is_ascii_digit(bytes[end]) {
        end += 1;
    }
    end
}

fn is_ascii_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

fn parse_decimal_at_most(digits: &[u8], maximum: usize) -> Option<usize> {
    let mut value = 0;
    for &digit in digits {
        let digit = usize::from(digit - b'0');
        if value > maximum / 10 || (value == maximum / 10 && digit > maximum % 10) {
            return None;
        }
        value = value * 10 + digit;
    }
    Some(value)
}
