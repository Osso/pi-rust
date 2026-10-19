//! Combined slash-command and file-path autocomplete.
//!
//! Ported from `packages/tui/src/autocomplete.ts`. Cursor positions are UTF-8
//! byte offsets (the typed Rust mapping of JavaScript UTF-16 code units).
//! Path handling reproduces Node's POSIX `path.join`/`dirname`/`basename`
//! normalization, and fd is invoked with the same argument order as the source.

use std::fs;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::Stdio;
use std::rc::Rc;

use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::fuzzy::fuzzy_filter;
use pi_js_compat::locale_compare;
use pi_js_compat::string::js_trim;

const PATH_DELIMITERS: [char; 5] = [' ', '\t', '"', '\'', '='];

/// One completion candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutocompleteItem {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
}

impl AutocompleteItem {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
        }
    }
}

/// Callback supplying argument completions for a slash command.
pub type ArgumentCompletions = Box<dyn Fn(&str) -> Option<Vec<AutocompleteItem>> + Send + Sync>;

/// A slash command, optionally supplying argument completions.
#[derive(Default)]
pub struct SlashCommand {
    pub name: String,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub get_argument_completions: Option<ArgumentCompletions>,
}

impl SlashCommand {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

/// A command table entry: either a slash command or a plain completion item.
pub enum CommandEntry {
    Command(SlashCommand),
    Item(AutocompleteItem),
}

impl CommandEntry {
    fn name(&self) -> &str {
        match self {
            CommandEntry::Command(command) => &command.name,
            CommandEntry::Item(item) => &item.value,
        }
    }
}

/// Suggestions for the current cursor position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutocompleteSuggestions {
    pub items: Vec<AutocompleteItem>,
    pub prefix: String,
}

/// Options for [`CombinedAutocompleteProvider::get_suggestions`].
#[derive(Default)]
pub struct SuggestionsOptions<'a> {
    /// Cancellation signal, mapping the TypeScript `AbortSignal`.
    pub signal: Option<&'a CancellationToken>,
    /// Force path extraction even when the text does not look path-like.
    pub force: bool,
}

/// Result of applying a completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionResult {
    pub lines: Vec<String>,
    pub cursor_line: usize,
    pub cursor_col: usize,
}

/// Future returned by [`AutocompleteProvider::get_suggestions`].
pub type AutocompleteFuture = Pin<Box<dyn Future<Output = Option<AutocompleteSuggestions>>>>;

/// Options passed to a provider request.
#[derive(Clone, Debug)]
pub struct AutocompleteOptions {
    pub signal: CancellationToken,
    pub force: bool,
}

/// Pluggable completion provider, mapping the source's `AutocompleteProvider`.
pub trait AutocompleteProvider {
    fn trigger_characters(&self) -> Vec<String> {
        Vec::new()
    }

    fn should_trigger_file_completion(
        &self,
        _lines: &[String],
        _cursor_line: usize,
        _cursor_col: usize,
    ) -> Option<bool> {
        None
    }

    fn get_suggestions(
        self: Rc<Self>,
        lines: Vec<String>,
        cursor_line: usize,
        cursor_col: usize,
        options: AutocompleteOptions,
    ) -> AutocompleteFuture;

    fn apply_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &str,
    ) -> CompletionResult;
}

/// Combined provider handling both slash commands and file paths.
pub struct CombinedAutocompleteProvider {
    commands: Vec<CommandEntry>,
    base_path: String,
    fd_path: Option<String>,
}

impl CombinedAutocompleteProvider {
    pub fn new(commands: Vec<CommandEntry>, base_path: impl Into<String>, fd_path: Option<String>) -> Self {
        Self {
            commands,
            base_path: base_path.into(),
            fd_path,
        }
    }

    pub async fn get_suggestions(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
        options: SuggestionsOptions<'_>,
    ) -> Option<AutocompleteSuggestions> {
        let current_line = lines.get(cursor_line).map_or("", String::as_str);
        let cursor = clamp_boundary(current_line, cursor_col);
        let text_before_cursor = &current_line[..cursor];

        if let Some(at_prefix) = extract_at_prefix(text_before_cursor) {
            let parsed = parse_path_prefix(&at_prefix);
            let suggestions = self
                .get_fuzzy_file_suggestions(&parsed.raw_prefix, parsed.is_quoted_prefix, options.signal)
                .await;
            if suggestions.is_empty() {
                return None;
            }
            return Some(AutocompleteSuggestions {
                items: suggestions,
                prefix: at_prefix,
            });
        }

        if text_before_cursor.starts_with('/') {
            if let Some(space_index) = text_before_cursor.find(' ') {
                let command_name = &text_before_cursor[1..space_index];
                let argument_text = &text_before_cursor[space_index + 1..];
                let command = self.commands.iter().find(|command| command.name() == command_name);
                if let Some(CommandEntry::Command(command)) = command
                    && let Some(completions) = &command.get_argument_completions
                {
                    let argument_suggestions = completions(argument_text);
                    if argument_suggestions
                        .as_ref()
                        .is_none_or(|suggestions| suggestions.is_empty())
                    {
                        return None;
                    }
                    return Some(AutocompleteSuggestions {
                        items: argument_suggestions.unwrap_or_default(),
                        prefix: argument_text.to_string(),
                    });
                }
                if !options.force {
                    return None;
                }
            } else {
                return self.command_name_suggestions(text_before_cursor);
            }
        }

        let path_match = extract_path_prefix(text_before_cursor, options.force)?;
        let suggestions = self.get_file_suggestions(&path_match);
        if suggestions.is_empty() {
            return None;
        }
        Some(AutocompleteSuggestions {
            items: suggestions,
            prefix: path_match,
        })
    }

    fn command_name_suggestions(&self, text_before_cursor: &str) -> Option<AutocompleteSuggestions> {
        let prefix = &text_before_cursor[1..];
        let command_items: Vec<CommandItem> = self
            .commands
            .iter()
            .map(|command| {
                let (description, hint) = match command {
                    CommandEntry::Command(command) => {
                        (command.description.as_deref(), command.argument_hint.as_deref())
                    }
                    CommandEntry::Item(item) => (item.description.as_deref(), None),
                };
                let description = description.unwrap_or("");
                let full_description = match hint {
                    Some(hint) if description.is_empty() => hint.to_string(),
                    Some(hint) => format!("{hint} — {description}"),
                    None => description.to_string(),
                };
                CommandItem {
                    name: command.name().to_string(),
                    description: if full_description.is_empty() {
                        None
                    } else {
                        Some(full_description)
                    },
                }
            })
            .collect();

        let filtered = fuzzy_filter(&command_items, prefix, |item| vec![item.name.clone()]);
        if filtered.is_empty() {
            return None;
        }
        let items = filtered
            .into_iter()
            .map(|item| AutocompleteItem {
                value: item.name.clone(),
                label: item.name,
                description: item.description,
            })
            .collect();
        Some(AutocompleteSuggestions {
            items,
            prefix: text_before_cursor.to_string(),
        })
    }

    pub fn apply_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &str,
    ) -> CompletionResult {
        let current_line = lines.get(cursor_line).map_or("", String::as_str);
        let before_end = clamp_boundary(current_line, cursor_col.saturating_sub(prefix.len()));
        let before_prefix = &current_line[..before_end];
        let after_cursor = &current_line[clamp_boundary(current_line, cursor_col)..];

        let is_quoted_prefix = prefix.starts_with('"') || prefix.starts_with("@\"");
        let has_leading_quote_after_cursor = after_cursor.starts_with('"');
        let has_trailing_quote_in_item = item.value.ends_with('"');
        let adjusted_after_cursor = if is_quoted_prefix && has_trailing_quote_in_item && has_leading_quote_after_cursor
        {
            &after_cursor[1..]
        } else {
            after_cursor
        };

        let is_slash_command =
            prefix.starts_with('/') && js_trim(before_prefix).is_empty() && !prefix[1..].contains('/');
        if is_slash_command {
            let new_line = format!("{before_prefix}/{} {adjusted_after_cursor}", item.value);
            let mut new_lines = lines.to_vec();
            if let Some(line) = new_lines.get_mut(cursor_line) {
                *line = new_line;
            }
            return CompletionResult {
                lines: new_lines,
                cursor_line,
                cursor_col: before_prefix.len() + item.value.len() + 2,
            };
        }

        let is_directory = item.label.ends_with('/');
        let has_trailing_quote = item.value.ends_with('"');
        let cursor_offset = if is_directory && has_trailing_quote {
            item.value.len() - 1
        } else {
            item.value.len()
        };

        if prefix.starts_with('@') {
            let suffix = if is_directory { "" } else { " " };
            let new_line = format!("{before_prefix}{}{suffix}{adjusted_after_cursor}", item.value);
            let mut new_lines = lines.to_vec();
            if let Some(line) = new_lines.get_mut(cursor_line) {
                *line = new_line;
            }
            return CompletionResult {
                lines: new_lines,
                cursor_line,
                cursor_col: before_prefix.len() + cursor_offset + suffix.len(),
            };
        }

        let text_before_cursor = &current_line[..clamp_boundary(current_line, cursor_col)];
        let new_line = format!("{before_prefix}{}{adjusted_after_cursor}", item.value);
        let mut new_lines = lines.to_vec();
        if let Some(line) = new_lines.get_mut(cursor_line) {
            *line = new_line;
        }

        if text_before_cursor.contains('/') && text_before_cursor.contains(' ') {
            return CompletionResult {
                lines: new_lines,
                cursor_line,
                cursor_col: before_prefix.len() + cursor_offset,
            };
        }

        CompletionResult {
            lines: new_lines,
            cursor_line,
            cursor_col: before_prefix.len() + cursor_offset,
        }
    }

    pub fn should_trigger_file_completion(&self, lines: &[String], cursor_line: usize, cursor_col: usize) -> bool {
        let current_line = lines.get(cursor_line).map_or("", String::as_str);
        let text_before_cursor = &current_line[..clamp_boundary(current_line, cursor_col)];
        let trimmed = js_trim(text_before_cursor);
        if trimmed.starts_with('/') && !trimmed.contains(' ') {
            return false;
        }
        true
    }

    fn get_file_suggestions(&self, prefix: &str) -> Vec<AutocompleteItem> {
        let parsed = parse_path_prefix(prefix);
        let mut expanded_prefix = parsed.raw_prefix.clone();
        if expanded_prefix.starts_with('~') {
            expanded_prefix = self.expand_home_path(&expanded_prefix);
        }

        let is_root_prefix = matches!(parsed.raw_prefix.as_str(), "" | "./" | "../" | "~" | "~/" | "/")
            || (parsed.is_at_prefix && parsed.raw_prefix.is_empty());

        let (search_dir, search_prefix) = if is_root_prefix || parsed.raw_prefix.ends_with('/') {
            let dir = if parsed.raw_prefix.starts_with('~') || expanded_prefix.starts_with('/') {
                expanded_prefix.clone()
            } else {
                posix_join(&[self.base_path.as_str(), expanded_prefix.as_str()])
            };
            (dir, String::new())
        } else {
            let dir = posix_dirname(&expanded_prefix);
            let file = posix_basename(&expanded_prefix);
            let search_dir = if parsed.raw_prefix.starts_with('~') || expanded_prefix.starts_with('/') {
                dir
            } else {
                posix_join(&[self.base_path.as_str(), dir.as_str()])
            };
            (search_dir, file)
        };

        let Ok(entries) = fs::read_dir(&search_dir) else {
            return Vec::new();
        };
        let lower_search_prefix = search_prefix.to_lowercase();
        let mut suggestions = Vec::new();

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.to_lowercase().starts_with(&lower_search_prefix) {
                continue;
            }

            let file_type = entry.file_type().ok();
            let mut is_directory = file_type.is_some_and(|file_type| file_type.is_dir());
            if !is_directory && file_type.is_some_and(|file_type| file_type.is_symlink()) {
                is_directory = fs::metadata(entry.path()).is_ok_and(|metadata| metadata.is_dir());
            }

            let relative_path = relative_completion_path(&parsed.raw_prefix, &name);
            let relative_path = to_display_path(&relative_path);
            let path_value = if is_directory {
                format!("{relative_path}/")
            } else {
                relative_path
            };
            let value = build_completion_value(&path_value, parsed.is_at_prefix, parsed.is_quoted_prefix);
            suggestions.push(AutocompleteItem {
                value,
                label: if is_directory { format!("{name}/") } else { name },
                description: None,
            });
        }

        suggestions.sort_by(compare_suggestions);
        suggestions
    }

    fn score_entry(file_path: &str, query: &str, is_directory: bool) -> i64 {
        let file_name = posix_basename(file_path).to_lowercase();
        let lower_query = query.to_lowercase();
        let mut score = if file_name == lower_query {
            100
        } else if file_name.starts_with(&lower_query) {
            80
        } else if file_name.contains(&lower_query) {
            50
        } else if file_path.to_lowercase().contains(&lower_query) {
            30
        } else {
            0
        };
        if is_directory && score > 0 {
            score += 10;
        }
        score
    }

    async fn get_fuzzy_file_suggestions(
        &self,
        query: &str,
        is_quoted_prefix: bool,
        signal: Option<&CancellationToken>,
    ) -> Vec<AutocompleteItem> {
        let Some(fd_path) = &self.fd_path else {
            return Vec::new();
        };
        if signal.is_some_and(CancellationToken::is_cancelled) {
            return Vec::new();
        }

        let scoped_query = self.resolve_scoped_fuzzy_query(query);
        let fd_base_dir = scoped_query
            .as_ref()
            .map_or(self.base_path.clone(), |scoped| scoped.base_dir.clone());
        let fd_query = scoped_query
            .as_ref()
            .map_or_else(|| query.to_string(), |scoped| scoped.query.clone());
        let entries = walk_directory_with_fd(&fd_base_dir, fd_path, &fd_query, 100, signal).await;
        if signal.is_some_and(CancellationToken::is_cancelled) {
            return Vec::new();
        }

        let mut scored_entries: Vec<(FdEntry, i64)> = entries
            .into_iter()
            .map(|entry| {
                let score = if fd_query.is_empty() {
                    1
                } else {
                    Self::score_entry(&entry.path, &fd_query, entry.is_directory)
                };
                (entry, score)
            })
            .filter(|(_, score)| *score > 0)
            .collect();
        scored_entries.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        scored_entries.truncate(20);

        scored_entries
            .into_iter()
            .map(|(entry, _)| {
                let path_without_slash = if entry.is_directory {
                    entry.path.strip_suffix('/').unwrap_or(&entry.path).to_string()
                } else {
                    entry.path.clone()
                };
                let display_path = match &scoped_query {
                    Some(scoped) => self.scoped_path_for_display(&scoped.display_base, &path_without_slash),
                    None => path_without_slash.clone(),
                };
                let entry_name = posix_basename(&path_without_slash);
                let completion_path = if entry.is_directory {
                    format!("{display_path}/")
                } else {
                    display_path.clone()
                };
                let value = build_completion_value(&completion_path, true, is_quoted_prefix);
                AutocompleteItem {
                    value,
                    label: if entry.is_directory {
                        format!("{entry_name}/")
                    } else {
                        entry_name
                    },
                    description: Some(display_path),
                }
            })
            .collect()
    }

    fn expand_home_path(&self, path: &str) -> String {
        if let Some(rest) = path.strip_prefix("~/") {
            let expanded_path = posix_join(&[&home_dir(), rest]);
            return if path.ends_with('/') && !expanded_path.ends_with('/') {
                format!("{expanded_path}/")
            } else {
                expanded_path
            };
        }
        if path == "~" {
            return home_dir();
        }
        path.to_string()
    }

    fn resolve_scoped_fuzzy_query(&self, raw_query: &str) -> Option<ScopedQuery> {
        let normalized_query = to_display_path(raw_query);
        let slash_index = normalized_query.rfind('/')?;
        let display_base = normalized_query[..=slash_index].to_string();
        let query = normalized_query[slash_index + 1..].to_string();

        let base_dir = if display_base.starts_with("~/") {
            self.expand_home_path(&display_base)
        } else if display_base.starts_with('/') {
            display_base.clone()
        } else {
            posix_join(&[self.base_path.as_str(), display_base.as_str()])
        };

        if !Path::new(&base_dir).is_dir() {
            return None;
        }
        Some(ScopedQuery {
            base_dir,
            query,
            display_base,
        })
    }

    fn scoped_path_for_display(&self, display_base: &str, relative_path: &str) -> String {
        let normalized_relative_path = to_display_path(relative_path);
        if display_base == "/" {
            return format!("/{normalized_relative_path}");
        }
        format!("{}{normalized_relative_path}", to_display_path(display_base))
    }
}

#[derive(Clone)]
struct CommandItem {
    name: String,
    description: Option<String>,
}

struct ScopedQuery {
    base_dir: String,
    query: String,
    display_base: String,
}

struct FdEntry {
    path: String,
    is_directory: bool,
}

#[derive(Debug)]
struct PathPrefix {
    raw_prefix: String,
    is_at_prefix: bool,
    is_quoted_prefix: bool,
}

fn parse_path_prefix(prefix: &str) -> PathPrefix {
    if let Some(raw_prefix) = prefix.strip_prefix("@\"") {
        return PathPrefix {
            raw_prefix: raw_prefix.to_string(),
            is_at_prefix: true,
            is_quoted_prefix: true,
        };
    }
    if let Some(raw_prefix) = prefix.strip_prefix('"') {
        return PathPrefix {
            raw_prefix: raw_prefix.to_string(),
            is_at_prefix: false,
            is_quoted_prefix: true,
        };
    }
    if let Some(raw_prefix) = prefix.strip_prefix('@') {
        return PathPrefix {
            raw_prefix: raw_prefix.to_string(),
            is_at_prefix: true,
            is_quoted_prefix: false,
        };
    }
    PathPrefix {
        raw_prefix: prefix.to_string(),
        is_at_prefix: false,
        is_quoted_prefix: false,
    }
}

fn build_completion_value(path: &str, is_at_prefix: bool, is_quoted_prefix: bool) -> String {
    let needs_quotes = is_quoted_prefix || path.contains(' ');
    let prefix = if is_at_prefix { "@" } else { "" };
    if !needs_quotes {
        return format!("{prefix}{path}");
    }
    format!("{prefix}\"{path}\"")
}

fn to_display_path(value: &str) -> String {
    value.replace('\\', "/")
}

fn escape_regex(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        if ".*+?^${}()|[]\\".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn build_fd_path_query(query: &str) -> String {
    let normalized = to_display_path(query);
    if !normalized.contains('/') {
        return normalized;
    }

    let has_trailing_separator = normalized.ends_with('/');
    let trimmed = normalized.trim_matches('/');
    if trimmed.is_empty() {
        return normalized;
    }

    let separator_pattern = r"[\\/]";
    let segments: Vec<String> = trimmed
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(escape_regex)
        .collect();
    if segments.is_empty() {
        return normalized;
    }

    let mut pattern = segments.join(separator_pattern);
    if has_trailing_separator {
        pattern.push_str(separator_pattern);
    }
    pattern
}

fn find_last_delimiter(text: &str) -> Option<usize> {
    text.char_indices()
        .rev()
        .find(|(_, character)| PATH_DELIMITERS.contains(character))
        .map(|(index, _)| index)
}

fn find_unclosed_quote_start(text: &str) -> Option<usize> {
    let mut in_quotes = false;
    let mut quote_start = None;
    for (index, character) in text.char_indices() {
        if character == '"' {
            in_quotes = !in_quotes;
            if in_quotes {
                quote_start = Some(index);
            }
        }
    }
    if in_quotes { quote_start } else { None }
}

fn is_token_start(text: &str, index: usize) -> bool {
    index == 0
        || text[..index]
            .chars()
            .next_back()
            .is_some_and(|character| PATH_DELIMITERS.contains(&character))
}

fn extract_quoted_prefix(text: &str) -> Option<String> {
    let quote_start = find_unclosed_quote_start(text)?;
    if quote_start > 0 && text[..quote_start].ends_with('@') {
        if !is_token_start(text, quote_start - 1) {
            return None;
        }
        return Some(text[quote_start - 1..].to_string());
    }
    if !is_token_start(text, quote_start) {
        return None;
    }
    Some(text[quote_start..].to_string())
}

fn extract_at_prefix(text: &str) -> Option<String> {
    if let Some(quoted_prefix) = extract_quoted_prefix(text)
        && quoted_prefix.starts_with("@\"")
    {
        return Some(quoted_prefix);
    }

    let token_start = find_last_delimiter(text).map_or(0, |index| index + 1);
    if text[token_start..].starts_with('@') {
        return Some(text[token_start..].to_string());
    }
    None
}

fn extract_path_prefix(text: &str, force_extract: bool) -> Option<String> {
    if let Some(quoted_prefix) = extract_quoted_prefix(text) {
        return Some(quoted_prefix);
    }

    let path_prefix = match find_last_delimiter(text) {
        Some(index) => &text[index + 1..],
        None => text,
    };

    if force_extract {
        return Some(path_prefix.to_string());
    }
    if path_prefix.contains('/') || path_prefix.starts_with('.') || path_prefix.starts_with("~/") {
        return Some(path_prefix.to_string());
    }
    if path_prefix.is_empty() && text.ends_with(' ') {
        return Some(path_prefix.to_string());
    }
    None
}

fn relative_completion_path(display_prefix: &str, name: &str) -> String {
    if display_prefix.ends_with('/') {
        return format!("{display_prefix}{name}");
    }
    if display_prefix.contains('/') || display_prefix.contains('\\') {
        if let Some(home_relative_dir) = display_prefix.strip_prefix("~/") {
            let dir = posix_dirname(home_relative_dir);
            return if dir == "." {
                format!("~/{name}")
            } else {
                format!("~/{}", posix_join(&[dir.as_str(), name]))
            };
        }
        if display_prefix.starts_with('/') {
            let dir = posix_dirname(display_prefix);
            return if dir == "/" {
                format!("/{name}")
            } else {
                format!("{dir}/{name}")
            };
        }
        let relative_path = posix_join(&[posix_dirname(display_prefix).as_str(), name]);
        return if display_prefix.starts_with("./") && !relative_path.starts_with("./") {
            format!("./{relative_path}")
        } else {
            relative_path
        };
    }
    if display_prefix.starts_with('~') {
        return format!("~/{name}");
    }
    name.to_string()
}

fn compare_suggestions(left: &AutocompleteItem, right: &AutocompleteItem) -> std::cmp::Ordering {
    let left_is_dir = left.value.ends_with('/');
    let right_is_dir = right.value.ends_with('/');
    if left_is_dir != right_is_dir {
        return right_is_dir.cmp(&left_is_dir);
    }
    locale_compare(&left.label, &right.label)
}

fn clamp_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn home_dir() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn normalize_path(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let is_absolute = path.starts_with('/');
    let trailing_separator = path.ends_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => match segments.last() {
                Some(last) if *last != ".." => {
                    segments.pop();
                }
                Some(_) => {}
                None if !is_absolute => segments.push(".."),
                None => {}
            },
            _ => segments.push(segment),
        }
    }

    let mut normalized = segments.join("/");
    if normalized.is_empty() {
        if is_absolute {
            return "/".to_string();
        }
        return if trailing_separator { "./" } else { "." }.to_string();
    }
    if trailing_separator {
        normalized.push('/');
    }
    if is_absolute {
        format!("/{normalized}")
    } else {
        normalized
    }
}

fn posix_join(parts: &[&str]) -> String {
    normalize_path(&parts.join("/"))
}

pub fn posix_dirname(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let has_root = path.starts_with('/');
    let bytes = path.as_bytes();
    let mut end: Option<usize> = None;
    let mut matched_slash = true;
    for index in (1..bytes.len()).rev() {
        if bytes[index] == b'/' {
            if !matched_slash {
                end = Some(index);
                break;
            }
        } else {
            matched_slash = false;
        }
    }
    match end {
        None if has_root => "/".to_string(),
        None => ".".to_string(),
        Some(1) if has_root => "//".to_string(),
        Some(index) => path[..index].to_string(),
    }
}

pub fn posix_basename(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let bytes = path.as_bytes();
    let mut start = 0;
    let mut end: Option<usize> = None;
    let mut matched_slash = true;
    for index in (0..bytes.len()).rev() {
        if bytes[index] == b'/' {
            if !matched_slash {
                start = index + 1;
                break;
            }
        } else if end.is_none() {
            matched_slash = false;
            end = Some(index + 1);
        }
    }
    match end {
        None => String::new(),
        Some(end) => path[start..end].to_string(),
    }
}

async fn walk_directory_with_fd(
    base_dir: &str,
    fd_path: &str,
    query: &str,
    max_results: usize,
    signal: Option<&CancellationToken>,
) -> Vec<FdEntry> {
    let mut args: Vec<String> = vec![
        "--base-directory".to_string(),
        base_dir.to_string(),
        "--max-results".to_string(),
        max_results.to_string(),
        "--type".to_string(),
        "f".to_string(),
        "--type".to_string(),
        "d".to_string(),
        "--follow".to_string(),
        "--hidden".to_string(),
        "--no-ignore-vcs".to_string(),
        "--exclude".to_string(),
        ".git".to_string(),
        "--exclude".to_string(),
        ".git/*".to_string(),
        "--exclude".to_string(),
        ".git/**".to_string(),
    ];

    let project_ignore_file = posix_join(&[base_dir, ".gitignore"]);
    if Path::new(&project_ignore_file).exists() {
        args.push("--ignore-file".to_string());
        args.push(project_ignore_file);
    }
    if to_display_path(query).contains('/') {
        args.push("--full-path".to_string());
    }
    if !query.is_empty() {
        args.push(build_fd_path_query(query));
    }

    if signal.is_some_and(CancellationToken::is_cancelled) {
        return Vec::new();
    }

    let mut command = Command::new(fd_path);
    command
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let Ok(mut child) = command.spawn() else {
        return Vec::new();
    };
    let Some(mut stdout) = child.stdout.take() else {
        return Vec::new();
    };
    let reader = tokio::spawn(async move {
        let mut output = String::new();
        let _ = stdout.read_to_string(&mut output).await;
        output
    });

    let status = match signal {
        Some(token) => tokio::select! {
            status = child.wait() => status,
            _ = token.cancelled() => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                let _ = reader.await;
                return Vec::new();
            }
        },
        None => child.wait().await,
    };
    let Ok(status) = status else {
        return Vec::new();
    };
    let stdout = reader.await.unwrap_or_default();
    if !status.success() || stdout.is_empty() {
        return Vec::new();
    }

    let trimmed = js_trim(&stdout);
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();
    for line in trimmed.split('\n').filter(|line| !line.is_empty()) {
        let display_line = to_display_path(line);
        let has_trailing_separator = display_line.ends_with('/');
        let normalized_path = if has_trailing_separator {
            &display_line[..display_line.len() - 1]
        } else {
            &display_line
        };
        if normalized_path == ".git" || normalized_path.starts_with(".git/") || normalized_path.contains("/.git/") {
            continue;
        }
        results.push(FdEntry {
            path: display_line,
            is_directory: has_trailing_separator,
        });
    }
    results
}

impl AutocompleteProvider for CombinedAutocompleteProvider {
    fn get_suggestions(
        self: Rc<Self>,
        lines: Vec<String>,
        cursor_line: usize,
        cursor_col: usize,
        options: AutocompleteOptions,
    ) -> AutocompleteFuture {
        Box::pin(async move {
            CombinedAutocompleteProvider::get_suggestions(
                &self,
                &lines,
                cursor_line,
                cursor_col,
                SuggestionsOptions {
                    signal: Some(&options.signal),
                    force: options.force,
                },
            )
            .await
        })
    }

    fn should_trigger_file_completion(&self, lines: &[String], cursor_line: usize, cursor_col: usize) -> Option<bool> {
        Some(CombinedAutocompleteProvider::should_trigger_file_completion(
            self,
            lines,
            cursor_line,
            cursor_col,
        ))
    }

    fn apply_completion(
        &self,
        lines: &[String],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &str,
    ) -> CompletionResult {
        CombinedAutocompleteProvider::apply_completion(self, lines, cursor_line, cursor_col, item, prefix)
    }
}
