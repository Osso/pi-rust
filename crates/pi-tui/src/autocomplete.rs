//! Combined slash-command and file-path autocomplete.
//!
//! Ported from `packages/tui/src/autocomplete.ts`. Cursor positions are UTF-8
//! code-unit offsets, including positions inside surrogate pairs.
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
use pi_js_compat::collator::locale_compare_utf16;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

const PATH_DELIMITERS: [u16; 5] = [32, 9, 34, 39, 61];

/// One completion candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutocompleteItem {
    pub value: JsString,
    pub label: JsString,
    pub description: Option<JsString>,
}

impl AutocompleteItem {
    pub fn new(value: impl Into<JsString>, label: impl Into<JsString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
        }
    }
}

/// Callback supplying argument completions for a slash command.
pub type ArgumentCompletions = Box<dyn Fn(&JsString) -> Option<Vec<AutocompleteItem>> + Send + Sync>;

/// A slash command, optionally supplying argument completions.
#[derive(Default)]
pub struct SlashCommand {
    pub name: JsString,
    pub description: Option<JsString>,
    pub argument_hint: Option<JsString>,
    pub get_argument_completions: Option<ArgumentCompletions>,
}

impl SlashCommand {
    pub fn new(name: impl Into<JsString>) -> Self {
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
    fn name(&self) -> &JsString {
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
    pub prefix: JsString,
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
    pub lines: Vec<JsString>,
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
    fn trigger_characters(&self) -> Vec<JsString> {
        Vec::new()
    }

    fn should_trigger_file_completion(
        &self,
        _lines: &[JsString],
        _cursor_line: usize,
        _cursor_col: usize,
    ) -> Option<bool> {
        None
    }

    fn get_suggestions(
        self: Rc<Self>,
        lines: Vec<JsString>,
        cursor_line: usize,
        cursor_col: usize,
        options: AutocompleteOptions,
    ) -> AutocompleteFuture;

    fn apply_completion(
        &self,
        lines: &[JsString],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &JsString,
    ) -> CompletionResult;
}

/// Combined provider handling both slash commands and file paths.
pub struct CombinedAutocompleteProvider {
    commands: Vec<CommandEntry>,
    base_path: JsString,
    fd_path: Option<JsString>,
}

impl CombinedAutocompleteProvider {
    pub fn new(commands: Vec<CommandEntry>, base_path: impl Into<JsString>, fd_path: Option<JsString>) -> Self {
        Self {
            commands,
            base_path: base_path.into(),
            fd_path,
        }
    }

    pub async fn get_suggestions(
        &self,
        lines: &[JsString],
        cursor_line: usize,
        cursor_col: usize,
        options: SuggestionsOptions<'_>,
    ) -> Option<AutocompleteSuggestions> {
        let current_line = lines.get(cursor_line).cloned().unwrap_or_default();
        let cursor = clamp_boundary(&current_line, cursor_col);
        let text_before_cursor = current_line.slice(0, cursor);

        if let Some(at_prefix) = extract_at_prefix(&text_before_cursor) {
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

        if text_before_cursor.starts_with("/") {
            if let Some(space_index) = text_before_cursor.find(" ") {
                let command_name = text_before_cursor.slice(1, space_index);
                let argument_text = text_before_cursor.slice(space_index + 1, text_before_cursor.len());
                let command = self.commands.iter().find(|command| *command.name() == command_name);
                if let Some(CommandEntry::Command(command)) = command
                    && let Some(completions) = &command.get_argument_completions
                {
                    let argument_suggestions = completions(&argument_text);
                    if argument_suggestions
                        .as_ref()
                        .is_none_or(|suggestions| suggestions.is_empty())
                    {
                        return None;
                    }
                    return Some(AutocompleteSuggestions {
                        items: argument_suggestions.unwrap_or_default(),
                        prefix: argument_text.clone(),
                    });
                }
                if !options.force {
                    return None;
                }
            } else {
                return self.command_name_suggestions(&text_before_cursor);
            }
        }

        let path_match = extract_path_prefix(&text_before_cursor, options.force)?;
        let suggestions = self.get_file_suggestions(&path_match);
        if suggestions.is_empty() {
            return None;
        }
        Some(AutocompleteSuggestions {
            items: suggestions,
            prefix: path_match,
        })
    }

    fn command_name_suggestions(&self, text_before_cursor: &JsString) -> Option<AutocompleteSuggestions> {
        let prefix = text_before_cursor.slice(1, text_before_cursor.len());
        let command_items: Vec<CommandItem> = self
            .commands
            .iter()
            .map(|command| {
                let (description, hint) = match command {
                    CommandEntry::Command(command) => (command.description.as_ref(), command.argument_hint.as_ref()),
                    CommandEntry::Item(item) => (item.description.as_ref(), None),
                };
                let description = description.cloned().unwrap_or_default();
                let full_description = match hint {
                    Some(hint) if description.is_empty() => hint.clone(),
                    Some(hint) => hint.clone() + " — " + &description,
                    None => description.clone(),
                };
                CommandItem {
                    name: command.name().clone(),
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
            prefix: text_before_cursor.clone(),
        })
    }

    pub fn apply_completion(
        &self,
        lines: &[JsString],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &JsString,
    ) -> CompletionResult {
        let current_line = lines.get(cursor_line).cloned().unwrap_or_default();
        let prefix_end = cursor_col as i128 - prefix.len() as i128;
        let before_end = normalize_slice_index(current_line.len(), prefix_end);
        let before_prefix = current_line.slice(0, before_end);
        let after_cursor = current_line.slice(clamp_boundary(&current_line, cursor_col), current_line.len());

        let is_quoted_prefix = prefix.starts_with("\"") || prefix.starts_with("@\"");
        let has_leading_quote_after_cursor = after_cursor.starts_with("\"");
        let has_trailing_quote_in_item = item.value.ends_with("\"");
        let adjusted_after_cursor = if is_quoted_prefix && has_trailing_quote_in_item && has_leading_quote_after_cursor
        {
            after_cursor.slice(1, after_cursor.len())
        } else {
            after_cursor
        };

        let is_slash_command =
            prefix.starts_with("/") && before_prefix.trim().is_empty() && !prefix.slice(1, prefix.len()).contains("/");
        if is_slash_command {
            let new_line = before_prefix.clone() + "/" + &item.value + " " + &adjusted_after_cursor;
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

        let is_directory = item.label.ends_with("/");
        let has_trailing_quote = item.value.ends_with("\"");
        let cursor_offset = if is_directory && has_trailing_quote {
            item.value.len() - 1
        } else {
            item.value.len()
        };

        if prefix.starts_with("@") {
            let suffix = if is_directory { "" } else { " " };
            let new_line = before_prefix.clone() + &item.value + suffix + &adjusted_after_cursor;
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

        let text_before_cursor = current_line.slice(0, clamp_boundary(&current_line, cursor_col));
        let new_line = before_prefix.clone() + &item.value + &adjusted_after_cursor;
        let mut new_lines = lines.to_vec();
        if let Some(line) = new_lines.get_mut(cursor_line) {
            *line = new_line;
        }

        if text_before_cursor.contains("/") && text_before_cursor.contains(" ") {
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

    pub fn should_trigger_file_completion(&self, lines: &[JsString], cursor_line: usize, cursor_col: usize) -> bool {
        let current_line = lines.get(cursor_line).cloned().unwrap_or_default();
        let text_before_cursor = current_line.slice(0, clamp_boundary(&current_line, cursor_col));
        let trimmed = text_before_cursor.trim();
        if trimmed.starts_with("/") && !trimmed.contains(" ") {
            return false;
        }
        true
    }

    fn get_file_suggestions(&self, prefix: &JsString) -> Vec<AutocompleteItem> {
        let parsed = parse_path_prefix(prefix);
        let mut expanded_prefix = parsed.raw_prefix.clone();
        if expanded_prefix.starts_with("~") {
            expanded_prefix = self.expand_home_path(&expanded_prefix);
        }

        let is_root_prefix = ["", "./", "../", "~", "~/", "/"]
            .iter()
            .any(|prefix| parsed.raw_prefix == *prefix)
            || (parsed.is_at_prefix && parsed.raw_prefix.is_empty());

        let (search_dir, search_prefix) = if is_root_prefix || parsed.raw_prefix.ends_with("/") {
            let dir = if parsed.raw_prefix.starts_with("~") || expanded_prefix.starts_with("/") {
                expanded_prefix.clone()
            } else {
                posix_join(&[self.base_path.clone(), expanded_prefix.clone()])
            };
            (dir, JsString::default())
        } else {
            let dir = posix_dirname(&expanded_prefix);
            let file = posix_basename(&expanded_prefix);
            let search_dir = if parsed.raw_prefix.starts_with("~") || expanded_prefix.starts_with("/") {
                dir
            } else {
                posix_join(&[self.base_path.clone(), dir.clone()])
            };
            (search_dir, file)
        };

        let Ok(entries) = fs::read_dir(encode_node_utf8(&search_dir)) else {
            return Vec::new();
        };
        let lower_search_prefix = search_prefix.to_lowercase();
        let mut suggestions = Vec::new();

        for entry in entries.flatten() {
            let name = JsString::from(entry.file_name().to_string_lossy().as_ref());
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
                relative_path.clone() + "/"
            } else {
                relative_path
            };
            let value = build_completion_value(&path_value, parsed.is_at_prefix, parsed.is_quoted_prefix);
            suggestions.push(AutocompleteItem {
                value,
                label: if is_directory { name.clone() + "/" } else { name },
                description: None,
            });
        }

        suggestions.sort_by(compare_suggestions);
        suggestions
    }

    fn score_entry(file_path: &JsString, query: &JsString, is_directory: bool) -> i64 {
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
        query: &JsString,
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
            .map_or_else(|| query.clone(), |scoped| scoped.query.clone());
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
                    entry.path.strip_suffix("/").unwrap_or_else(|| entry.path.clone())
                } else {
                    entry.path.clone()
                };
                let display_path = match &scoped_query {
                    Some(scoped) => self.scoped_path_for_display(&scoped.display_base, &path_without_slash),
                    None => path_without_slash.clone(),
                };
                let entry_name = posix_basename(&path_without_slash);
                let completion_path = if entry.is_directory {
                    display_path.clone() + "/"
                } else {
                    display_path.clone()
                };
                let value = build_completion_value(&completion_path, true, is_quoted_prefix);
                AutocompleteItem {
                    value,
                    label: if entry.is_directory {
                        entry_name.clone() + "/"
                    } else {
                        entry_name
                    },
                    description: Some(display_path),
                }
            })
            .collect()
    }

    fn expand_home_path(&self, path: &JsString) -> JsString {
        if let Some(rest) = path.strip_prefix("~/") {
            let expanded_path = posix_join(&[home_dir(), rest]);
            return if path.ends_with("/") && !expanded_path.ends_with("/") {
                expanded_path + "/"
            } else {
                expanded_path
            };
        }
        if path == "~" {
            return home_dir();
        }
        path.clone()
    }

    fn resolve_scoped_fuzzy_query(&self, raw_query: &JsString) -> Option<ScopedQuery> {
        let normalized_query = to_display_path(raw_query);
        let slash_index = normalized_query.units().iter().rposition(|unit| *unit == 47)?;
        let display_base = normalized_query.slice(0, slash_index + 1);
        let query = normalized_query.slice(slash_index + 1, normalized_query.len());

        let base_dir = if display_base.starts_with("~/") {
            self.expand_home_path(&display_base)
        } else if display_base.starts_with("/") {
            display_base.clone()
        } else {
            posix_join(&[self.base_path.clone(), display_base.clone()])
        };

        if !Path::new(&encode_node_utf8(&base_dir)).is_dir() {
            return None;
        }
        Some(ScopedQuery {
            base_dir,
            query,
            display_base,
        })
    }

    fn scoped_path_for_display(&self, display_base: &JsString, relative_path: &JsString) -> JsString {
        let normalized_relative_path = to_display_path(relative_path);
        if display_base == "/" {
            return JsString::from("/") + normalized_relative_path;
        }
        to_display_path(display_base) + normalized_relative_path
    }
}

#[derive(Clone)]
struct CommandItem {
    name: JsString,
    description: Option<JsString>,
}

struct ScopedQuery {
    base_dir: JsString,
    query: JsString,
    display_base: JsString,
}

struct FdEntry {
    path: JsString,
    is_directory: bool,
}

#[derive(Debug)]
struct PathPrefix {
    raw_prefix: JsString,
    is_at_prefix: bool,
    is_quoted_prefix: bool,
}

fn parse_path_prefix(prefix: &JsString) -> PathPrefix {
    if let Some(raw_prefix) = prefix.strip_prefix("@\"") {
        return PathPrefix {
            raw_prefix: raw_prefix.clone(),
            is_at_prefix: true,
            is_quoted_prefix: true,
        };
    }
    if let Some(raw_prefix) = prefix.strip_prefix("\"") {
        return PathPrefix {
            raw_prefix: raw_prefix.clone(),
            is_at_prefix: false,
            is_quoted_prefix: true,
        };
    }
    if let Some(raw_prefix) = prefix.strip_prefix("@") {
        return PathPrefix {
            raw_prefix: raw_prefix.clone(),
            is_at_prefix: true,
            is_quoted_prefix: false,
        };
    }
    PathPrefix {
        raw_prefix: prefix.clone(),
        is_at_prefix: false,
        is_quoted_prefix: false,
    }
}

fn build_completion_value(path: &JsString, is_at_prefix: bool, is_quoted_prefix: bool) -> JsString {
    let needs_quotes = is_quoted_prefix || path.contains(" ");
    let prefix = if is_at_prefix { "@" } else { "" };
    if !needs_quotes {
        return JsString::from(prefix) + path;
    }
    JsString::from(prefix) + "\"" + path + "\""
}

fn to_display_path(value: &JsString) -> JsString {
    value.replace("\\", "/")
}

fn escape_regex(value: &JsString) -> JsString {
    let special = JsString::from(".*+?^${}()|[]\\");
    let mut escaped = JsString::default();
    for unit in value.units() {
        if special.units().contains(unit) {
            escaped.push_str("\\");
        }
        escaped.push_unit(*unit);
    }
    escaped
}

fn build_fd_path_query(query: &JsString) -> JsString {
    let normalized = to_display_path(query);
    if !normalized.contains("/") {
        return normalized;
    }

    let has_trailing_separator = normalized.ends_with("/");
    let start = normalized
        .units()
        .iter()
        .position(|unit| *unit != 47)
        .unwrap_or(normalized.len());
    let end = normalized
        .units()
        .iter()
        .rposition(|unit| *unit != 47)
        .map_or(0, |index| index + 1);
    let trimmed = normalized.slice(start, end);
    if trimmed.is_empty() {
        return normalized;
    }

    let separator_pattern = r"[\\/]";
    let segments: Vec<JsString> = trimmed
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|value| escape_regex(&value))
        .collect();
    if segments.is_empty() {
        return normalized;
    }

    let mut pattern = JsString::join(segments.iter(), separator_pattern);
    if has_trailing_separator {
        pattern.push_str(separator_pattern);
    }
    pattern
}

fn find_last_delimiter(text: &JsString) -> Option<usize> {
    text.units().iter().rposition(|unit| PATH_DELIMITERS.contains(unit))
}

fn find_unclosed_quote_start(text: &JsString) -> Option<usize> {
    let mut in_quotes = false;
    let mut quote_start = None;
    for (index, unit) in text.units().iter().enumerate() {
        if *unit == u16::from(b'"') {
            in_quotes = !in_quotes;
            if in_quotes {
                quote_start = Some(index);
            }
        }
    }
    if in_quotes { quote_start } else { None }
}

fn is_token_start(text: &JsString, index: usize) -> bool {
    index == 0
        || text
            .units()
            .get(index - 1)
            .is_some_and(|unit| PATH_DELIMITERS.contains(unit))
}

fn extract_quoted_prefix(text: &JsString) -> Option<JsString> {
    let quote_start = find_unclosed_quote_start(text)?;
    if quote_start > 0 && text.slice(0, quote_start).ends_with("@") {
        if !is_token_start(text, quote_start - 1) {
            return None;
        }
        return Some(text.slice(quote_start - 1, text.len()));
    }
    if !is_token_start(text, quote_start) {
        return None;
    }
    Some(text.slice(quote_start, text.len()))
}

fn extract_at_prefix(text: &JsString) -> Option<JsString> {
    if let Some(quoted_prefix) = extract_quoted_prefix(text)
        && quoted_prefix.starts_with("@\"")
    {
        return Some(quoted_prefix);
    }

    let token_start = find_last_delimiter(text).map_or(0, |index| index + 1);
    if text.slice(token_start, text.len()).starts_with("@") {
        return Some(text.slice(token_start, text.len()));
    }
    None
}

fn extract_path_prefix(text: &JsString, force_extract: bool) -> Option<JsString> {
    if let Some(quoted_prefix) = extract_quoted_prefix(text) {
        return Some(quoted_prefix);
    }

    let path_prefix = match find_last_delimiter(text) {
        Some(index) => text.slice(index + 1, text.len()),
        None => text.clone(),
    };

    if force_extract {
        return Some(path_prefix.clone());
    }
    if path_prefix.contains("/") || path_prefix.starts_with(".") || path_prefix.starts_with("~/") {
        return Some(path_prefix.clone());
    }
    if path_prefix.is_empty() && text.ends_with(" ") {
        return Some(path_prefix.clone());
    }
    None
}

fn relative_completion_path(display_prefix: &JsString, name: &JsString) -> JsString {
    if display_prefix.ends_with("/") {
        return display_prefix.clone() + name;
    }
    if display_prefix.contains("/") || display_prefix.contains('\\') {
        if let Some(home_relative_dir) = display_prefix.strip_prefix("~/") {
            let dir = posix_dirname(home_relative_dir);
            return if dir == "." {
                JsString::from("~/") + name
            } else {
                JsString::from("~/") + posix_join(&[dir.clone(), name.clone()])
            };
        }
        if display_prefix.starts_with("/") {
            let dir = posix_dirname(display_prefix);
            return if dir == "/" {
                JsString::from("/") + name
            } else {
                dir + "/" + name
            };
        }
        let relative_path = posix_join(&[posix_dirname(display_prefix), name.clone()]);
        return if display_prefix.starts_with("./") && !relative_path.starts_with("./") {
            JsString::from("./") + relative_path
        } else {
            relative_path
        };
    }
    if display_prefix.starts_with("~") {
        return JsString::from("~/") + name;
    }
    name.clone()
}

fn compare_suggestions(left: &AutocompleteItem, right: &AutocompleteItem) -> std::cmp::Ordering {
    let left_is_dir = left.value.ends_with("/");
    let right_is_dir = right.value.ends_with("/");
    if left_is_dir != right_is_dir {
        return right_is_dir.cmp(&left_is_dir);
    }
    locale_compare_utf16(left.label.units(), right.label.units())
}

fn clamp_boundary(text: &JsString, index: usize) -> usize {
    index.min(text.len())
}

fn normalize_slice_index(length: usize, index: i128) -> usize {
    let offset = if index < 0 { length as i128 + index } else { index };
    offset.clamp(0, length as i128) as usize
}

fn home_dir() -> JsString {
    pi_js_compat::process_env::get("HOME").unwrap_or_default().into()
}

fn normalize_path(path: &JsString) -> JsString {
    if path.is_empty() {
        return JsString::from(".");
    }
    let is_absolute = path.starts_with("/");
    let trailing_separator = path.ends_with("/");
    let mut segments: Vec<JsString> = Vec::new();
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            match segments.last() {
                Some(last) if last != ".." => {
                    segments.pop();
                }
                Some(_) => {}
                None if !is_absolute => segments.push(JsString::from("..")),
                None => {}
            }
        } else {
            segments.push(segment);
        }
    }
    let mut normalized = JsString::join(segments.iter(), "/");
    if normalized.is_empty() {
        if is_absolute {
            return JsString::from("/");
        }
        return JsString::from(if trailing_separator { "./" } else { "." });
    }
    if trailing_separator {
        normalized.push_str("/");
    }
    if is_absolute {
        JsString::from("/") + normalized
    } else {
        normalized
    }
}

fn posix_join(parts: &[JsString]) -> JsString {
    normalize_path(&JsString::join(parts.iter(), "/"))
}

pub fn posix_dirname(path: impl Into<JsString>) -> JsString {
    let path = path.into();
    if path.is_empty() {
        return JsString::from(".");
    }
    let has_root = path.starts_with("/");
    let bytes = path.units();
    let mut end: Option<usize> = None;
    let mut matched_slash = true;
    for index in (1..bytes.len()).rev() {
        if bytes[index] == u16::from(b'/') {
            if !matched_slash {
                end = Some(index);
                break;
            }
        } else {
            matched_slash = false;
        }
    }
    match end {
        None if has_root => JsString::from("/"),
        None => JsString::from("."),
        Some(1) if has_root => JsString::from("//"),
        Some(index) => path.slice(0, index),
    }
}

pub fn posix_basename(path: impl Into<JsString>) -> JsString {
    let path = path.into();
    if path.is_empty() {
        return JsString::default();
    }
    let bytes = path.units();
    let mut start = 0;
    let mut end: Option<usize> = None;
    let mut matched_slash = true;
    for index in (0..bytes.len()).rev() {
        if bytes[index] == u16::from(b'/') {
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
        None => JsString::default(),
        Some(end) => path.slice(start, end),
    }
}

async fn walk_directory_with_fd(
    base_dir: &JsString,
    fd_path: &JsString,
    query: &JsString,
    max_results: usize,
    signal: Option<&CancellationToken>,
) -> Vec<FdEntry> {
    let mut args: Vec<JsString> = vec![
        JsString::from("--base-directory"),
        base_dir.clone(),
        JsString::from("--max-results"),
        JsString::from(max_results.to_string()),
        JsString::from("--type"),
        JsString::from("f"),
        JsString::from("--type"),
        JsString::from("d"),
        JsString::from("--follow"),
        JsString::from("--hidden"),
        JsString::from("--no-ignore-vcs"),
        JsString::from("--exclude"),
        JsString::from(".git"),
        JsString::from("--exclude"),
        JsString::from(".git/*"),
        JsString::from("--exclude"),
        JsString::from(".git/**"),
    ];

    let project_ignore_file = posix_join(&[base_dir.clone(), JsString::from(".gitignore")]);
    if Path::new(&encode_node_utf8(&project_ignore_file)).exists() {
        args.push(JsString::from("--ignore-file"));
        args.push(project_ignore_file);
    }
    if to_display_path(query).contains("/") {
        args.push(JsString::from("--full-path"));
    }
    if !query.is_empty() {
        args.push(build_fd_path_query(query));
    }

    if signal.is_some_and(CancellationToken::is_cancelled) {
        return Vec::new();
    }

    let mut command = Command::new(encode_node_utf8(fd_path));
    command
        .args(args.iter().map(encode_node_utf8))
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
        let mut output = Vec::new();
        let _ = stdout.read_to_end(&mut output).await;
        JsString::from(String::from_utf8_lossy(&output).as_ref())
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

    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();
    for line in trimmed.split('\n').filter(|line| !line.is_empty()) {
        let display_line = to_display_path(&line);
        let has_trailing_separator = display_line.ends_with("/");
        let normalized_path = if has_trailing_separator {
            display_line.slice(0, display_line.len() - 1)
        } else {
            display_line.clone()
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
        lines: Vec<JsString>,
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

    fn should_trigger_file_completion(
        &self,
        lines: &[JsString],
        cursor_line: usize,
        cursor_col: usize,
    ) -> Option<bool> {
        Some(CombinedAutocompleteProvider::should_trigger_file_completion(
            self,
            lines,
            cursor_line,
            cursor_col,
        ))
    }

    fn apply_completion(
        &self,
        lines: &[JsString],
        cursor_line: usize,
        cursor_col: usize,
        item: &AutocompleteItem,
        prefix: &JsString,
    ) -> CompletionResult {
        CombinedAutocompleteProvider::apply_completion(self, lines, cursor_line, cursor_col, item, prefix)
    }
}
