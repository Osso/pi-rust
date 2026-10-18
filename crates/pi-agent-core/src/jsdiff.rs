//! Port of the parts of `diff@8.0.4` used by the agent and coding-agent tools:
//! `diffLines`, `diffWords`, `structuredPatch`, and `createTwoFilesPatch` with
//! `FILE_HEADERS_ONLY`.

use std::collections::HashMap;
use std::rc::Rc;

/// One diff part with source-compatible flags and joined token value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub added: bool,
    pub removed: bool,
    pub value: String,
}

#[derive(Debug, Clone)]
struct Component {
    count: usize,
    added: bool,
    removed: bool,
    previous: Option<Rc<Component>>,
}

#[derive(Debug, Clone)]
struct Path {
    old_pos: i64,
    last: Option<Rc<Component>>,
}

/// `diffLines(old_str, new_str)` with default options.
pub fn diff_lines(old_text: &str, new_text: &str) -> Vec<Part> {
    let old_tokens = remove_empty(tokenize(old_text));
    let new_tokens = remove_empty(tokenize(new_text));
    let components = Search::new(&old_tokens, &new_tokens).run();
    build_values(&components, &old_tokens, &new_tokens, Mode::Lines)
}

/// `diffWords(old_str, new_str)` with default options.
pub fn diff_words(old_text: &str, new_text: &str) -> Vec<Part> {
    let old_tokens = remove_empty(tokenize_words(old_text));
    let new_tokens = remove_empty(tokenize_words(new_text));
    let components = Search::new_words(&old_tokens, &new_tokens).run();
    let mut parts = build_values(&components, &old_tokens, &new_tokens, Mode::Words);
    dedupe_change_object_whitespace(&mut parts);
    parts
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Lines,
    Words,
}

/// `tokenize(value, {})`: split into lines with their terminators.
pub fn tokenize(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        current.push(character);
        if character == '\n' {
            tokens.push(std::mem::take(&mut current));
        } else if character == '\r' && characters.peek() == Some(&'\n') {
            current.push(characters.next().expect("peeked character"));
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn remove_empty(tokens: Vec<String>) -> Vec<String> {
    tokens.into_iter().filter(|token| !token.is_empty()).collect()
}

struct Search<'a> {
    old_tokens: &'a [String],
    new_tokens: &'a [String],
    mode: Mode,
    best_path: HashMap<i64, Path>,
    min_diagonal: i64,
    max_diagonal: i64,
}

impl<'a> Search<'a> {
    fn new(old_tokens: &'a [String], new_tokens: &'a [String]) -> Self {
        Self::with_mode(old_tokens, new_tokens, Mode::Lines)
    }

    fn new_words(old_tokens: &'a [String], new_tokens: &'a [String]) -> Self {
        Self::with_mode(old_tokens, new_tokens, Mode::Words)
    }

    fn with_mode(old_tokens: &'a [String], new_tokens: &'a [String], mode: Mode) -> Self {
        Self {
            old_tokens,
            new_tokens,
            mode,
            best_path: HashMap::new(),
            min_diagonal: i64::MIN,
            max_diagonal: i64::MAX,
        }
    }

    fn run(mut self) -> Vec<Component> {
        let old_len = self.old_tokens.len() as i64;
        let new_len = self.new_tokens.len() as i64;
        let mut start = Path {
            old_pos: -1,
            last: None,
        };
        let new_pos = extract_common(&mut start, self.new_tokens, self.old_tokens, 0, self.mode);
        if start.old_pos + 1 >= old_len && new_pos + 1 >= new_len {
            return flatten(start.last);
        }
        self.best_path.insert(0, start);

        let max_edit_length = new_len + old_len;
        let mut edit_length = 1;
        while edit_length <= max_edit_length {
            let mut diagonal = (-edit_length).max(self.min_diagonal);
            let upper = edit_length.min(self.max_diagonal);
            while diagonal <= upper {
                if let Some(component) = self.extend_diagonal(diagonal) {
                    return component;
                }
                diagonal += 2;
            }
            edit_length += 1;
        }
        Vec::new()
    }

    fn extend_diagonal(&mut self, diagonal: i64) -> Option<Vec<Component>> {
        let old_len = self.old_tokens.len() as i64;
        let new_len = self.new_tokens.len() as i64;
        let remove_path = self.best_path.remove(&(diagonal - 1));
        let add_path = self.best_path.get(&(diagonal + 1)).cloned();

        let can_add = add_path.as_ref().is_some_and(|path| {
            let new_pos = path.old_pos - diagonal;
            (0..new_len).contains(&new_pos)
        });
        let can_remove = remove_path.as_ref().is_some_and(|path| path.old_pos + 1 < old_len);
        if !can_add && !can_remove {
            self.best_path.remove(&diagonal);
            return None;
        }

        let use_add = match (&remove_path, &add_path) {
            (None, _) => true,
            (Some(remove), Some(add)) => !can_remove || (can_add && remove.old_pos < add.old_pos),
            (Some(_), None) => !can_remove,
        };
        let mut base_path = if use_add {
            add_to_path(add_path.as_ref().expect("add path"), true, false, 0)
        } else {
            add_to_path(remove_path.as_ref().expect("remove path"), false, true, 1)
        };
        let new_pos = extract_common(&mut base_path, self.new_tokens, self.old_tokens, diagonal, self.mode);
        if base_path.old_pos + 1 >= old_len && new_pos + 1 >= new_len {
            return Some(flatten(base_path.last));
        }

        let old_done = base_path.old_pos + 1 >= old_len;
        let new_done = new_pos + 1 >= new_len;
        self.best_path.insert(diagonal, base_path);
        if old_done {
            self.max_diagonal = self.max_diagonal.min(diagonal - 1);
        }
        if new_done {
            self.min_diagonal = self.min_diagonal.max(diagonal + 1);
        }
        None
    }
}

fn add_to_path(path: &Path, added: bool, removed: bool, old_pos_increment: i64) -> Path {
    let last = match &path.last {
        Some(last) if last.added == added && last.removed == removed => Component {
            count: last.count + 1,
            added,
            removed,
            previous: last.previous.clone(),
        },
        last => Component {
            count: 1,
            added,
            removed,
            previous: last.clone(),
        },
    };
    Path {
        old_pos: path.old_pos + old_pos_increment,
        last: Some(Rc::new(last)),
    }
}

fn extract_common(path: &mut Path, new_tokens: &[String], old_tokens: &[String], diagonal: i64, mode: Mode) -> i64 {
    let new_len = new_tokens.len() as i64;
    let old_len = old_tokens.len() as i64;
    let mut old_pos = path.old_pos;
    let mut new_pos = old_pos - diagonal;
    let mut common = 0;
    while new_pos + 1 < new_len
        && old_pos + 1 < old_len
        && tokens_equal(
            &old_tokens[(old_pos + 1) as usize],
            &new_tokens[(new_pos + 1) as usize],
            mode,
        )
    {
        new_pos += 1;
        old_pos += 1;
        common += 1;
    }
    if common > 0 {
        path.last = Some(Rc::new(Component {
            count: common as usize,
            added: false,
            removed: false,
            previous: path.last.clone(),
        }));
    }
    path.old_pos = old_pos;
    new_pos
}

fn flatten(last: Option<Rc<Component>>) -> Vec<Component> {
    let mut components = Vec::new();
    let mut current = last;
    while let Some(component) = current {
        components.push((*component).clone());
        current = component.previous.clone();
    }
    components.reverse();
    components
}

fn tokens_equal(left: &str, right: &str, mode: Mode) -> bool {
    match mode {
        Mode::Lines => left == right,
        Mode::Words => left.trim() == right.trim(),
    }
}

fn build_values(components: &[Component], old_tokens: &[String], new_tokens: &[String], mode: Mode) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut old_pos = 0;
    let mut new_pos = 0;
    for component in components {
        if component.removed {
            parts.push(Part {
                added: false,
                removed: true,
                value: join_tokens(&old_tokens[old_pos..old_pos + component.count], mode),
            });
            old_pos += component.count;
        } else {
            parts.push(Part {
                added: component.added,
                removed: false,
                value: join_tokens(&new_tokens[new_pos..new_pos + component.count], mode),
            });
            new_pos += component.count;
            if !component.added {
                old_pos += component.count;
            }
        }
    }
    parts
}

fn join_tokens(tokens: &[String], mode: Mode) -> String {
    match mode {
        Mode::Lines => tokens.concat(),
        Mode::Words => tokens
            .iter()
            .enumerate()
            .map(|(index, token)| {
                if index == 0 {
                    token.clone()
                } else {
                    token.trim_start_matches(js_whitespace).to_string()
                }
            })
            .collect(),
    }
}

/// JavaScript `\s`.
fn js_whitespace(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

/// `WordDiff.tokenize` with the default (non-segmenter) tokenizer.
fn tokenize_words(value: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let characters: Vec<char> = value.chars().collect();
    let mut index = 0;
    while index < characters.len() {
        if is_word_character(characters[index]) {
            let start = index;
            while index < characters.len() && is_word_character(characters[index]) {
                index += 1;
            }
            parts.push(characters[start..index].iter().collect());
        } else if js_whitespace(characters[index]) {
            let start = index;
            while index < characters.len() && js_whitespace(characters[index]) {
                index += 1;
            }
            parts.push(characters[start..index].iter().collect());
        } else {
            parts.push(characters[index].to_string());
            index += 1;
        }
    }

    let mut tokens: Vec<String> = Vec::new();
    let mut previous: Option<String> = None;
    for part in parts {
        let part_is_whitespace = part.chars().any(js_whitespace);
        if part_is_whitespace {
            if previous.is_none() {
                tokens.push(part.clone());
            } else {
                let last = tokens.pop().unwrap_or_default();
                tokens.push(format!("{last}{part}"));
            }
        } else if let Some(previous_part) = &previous {
            if previous_part.chars().any(js_whitespace) {
                if tokens.last() == Some(previous_part) {
                    let last = tokens.pop().unwrap_or_default();
                    tokens.push(format!("{last}{part}"));
                } else {
                    tokens.push(format!("{previous_part}{part}"));
                }
            } else {
                tokens.push(part.clone());
            }
        } else {
            tokens.push(part.clone());
        }
        previous = Some(part);
    }
    tokens
}

fn is_word_character(character: char) -> bool {
    matches!(
        character,
        'a'..='z'
            | 'A'..='Z'
            | '0'..='9'
            | '_'
            | '\u{00AD}'
            | '\u{00C0}'..='\u{00D6}'
            | '\u{00D8}'..='\u{00F6}'
            | '\u{00F8}'..='\u{02C6}'
            | '\u{02C8}'..='\u{02D7}'
            | '\u{02DE}'..='\u{02FF}'
            | '\u{1E00}'..='\u{1EFF}'
    )
}

fn dedupe_change_object_whitespace(parts: &mut [Part]) {
    let mut last_keep: Option<usize> = None;
    let mut insertion: Option<usize> = None;
    let mut deletion: Option<usize> = None;
    for index in 0..parts.len() {
        if parts[index].added {
            insertion = Some(index);
        } else if parts[index].removed {
            deletion = Some(index);
        } else {
            if insertion.is_some() || deletion.is_some() {
                dedupe_whitespace_in_change_objects(parts, last_keep, deletion, insertion, Some(index));
            }
            last_keep = Some(index);
            insertion = None;
            deletion = None;
        }
    }
    if insertion.is_some() || deletion.is_some() {
        dedupe_whitespace_in_change_objects(parts, last_keep, deletion, insertion, None);
    }
}

fn dedupe_whitespace_in_change_objects(
    parts: &mut [Part],
    start_keep: Option<usize>,
    deletion: Option<usize>,
    insertion: Option<usize>,
    end_keep: Option<usize>,
) {
    if let (Some(deletion_index), Some(insertion_index)) = (deletion, insertion) {
        let deletion_value = parts[deletion_index].value.clone();
        let insertion_value = parts[insertion_index].value.clone();
        let (old_prefix, old_suffix) = leading_and_trailing_ws(&deletion_value);
        let (new_prefix, new_suffix) = leading_and_trailing_ws(&insertion_value);
        if let Some(keep) = start_keep {
            let common = longest_common_prefix(&old_prefix, &new_prefix);
            parts[keep].value = replace_suffix(&parts[keep].value, &new_prefix, &common);
            parts[deletion_index].value = remove_prefix(&parts[deletion_index].value, &common);
            parts[insertion_index].value = remove_prefix(&parts[insertion_index].value, &common);
        }
        if let Some(keep) = end_keep {
            let common = longest_common_suffix(&old_suffix, &new_suffix);
            parts[keep].value = replace_prefix(&parts[keep].value, &new_suffix, &common);
            parts[deletion_index].value = remove_suffix(&parts[deletion_index].value, &common);
            parts[insertion_index].value = remove_suffix(&parts[insertion_index].value, &common);
        }
    } else if let Some(insertion_index) = insertion {
        if start_keep.is_some() {
            let whitespace = leading_ws(&parts[insertion_index].value);
            parts[insertion_index].value = parts[insertion_index].value[whitespace.len()..].to_string();
        }
        if let Some(keep) = end_keep {
            let whitespace = leading_ws(&parts[keep].value);
            parts[keep].value = parts[keep].value[whitespace.len()..].to_string();
        }
    } else if let (Some(start), Some(end)) = (start_keep, end_keep) {
        let Some(deletion_index) = deletion else {
            return;
        };
        let new_ws_full = leading_ws(&parts[end].value);
        let (deletion_ws_start, deletion_ws_end) = leading_and_trailing_ws(&parts[deletion_index].value);
        let new_ws_start = longest_common_prefix(&new_ws_full, &deletion_ws_start);
        let deletion_value = remove_prefix(&parts[deletion_index].value, &new_ws_start);
        let remaining_new_ws = remove_prefix(&new_ws_full, &new_ws_start);
        let new_ws_end = longest_common_suffix(&remaining_new_ws, &deletion_ws_end);
        parts[deletion_index].value = remove_suffix(&deletion_value, &new_ws_end);
        parts[end].value = replace_prefix(&parts[end].value, &new_ws_full, &new_ws_end);
        let start_suffix = &new_ws_full[..new_ws_full.len() - new_ws_end.len()];
        parts[start].value = replace_suffix(&parts[start].value, &new_ws_full, start_suffix);
    } else if let Some(end) = end_keep {
        let Some(deletion_index) = deletion else {
            return;
        };
        let end_keep_ws_prefix = leading_ws(&parts[end].value);
        let deletion_ws_suffix = trailing_ws(&parts[deletion_index].value);
        let overlap = maximum_overlap(&deletion_ws_suffix, &end_keep_ws_prefix);
        parts[deletion_index].value = remove_suffix(&parts[deletion_index].value, &overlap);
    } else if let Some(start) = start_keep {
        let Some(deletion_index) = deletion else {
            return;
        };
        let start_keep_ws_suffix = trailing_ws(&parts[start].value);
        let deletion_ws_prefix = leading_ws(&parts[deletion_index].value);
        let overlap = maximum_overlap(&start_keep_ws_suffix, &deletion_ws_prefix);
        parts[deletion_index].value = remove_prefix(&parts[deletion_index].value, &overlap);
    }
}

fn leading_ws(value: &str) -> String {
    value
        .chars()
        .take_while(|character| js_whitespace(*character))
        .collect()
}

fn trailing_ws(value: &str) -> String {
    let mut characters: Vec<char> = value
        .chars()
        .rev()
        .take_while(|character| js_whitespace(*character))
        .collect();
    characters.reverse();
    characters.into_iter().collect()
}

fn leading_and_trailing_ws(value: &str) -> (String, String) {
    (leading_ws(value), trailing_ws(value))
}

fn longest_common_prefix(left: &str, right: &str) -> String {
    left.chars()
        .zip(right.chars())
        .take_while(|(left, right)| left == right)
        .map(|(left, _)| left)
        .collect()
}

fn longest_common_suffix(left: &str, right: &str) -> String {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.is_empty() || right.is_empty() || left.last() != right.last() {
        return String::new();
    }
    let mut count = 0;
    while count < left.len() && count < right.len() && left[left.len() - 1 - count] == right[right.len() - 1 - count] {
        count += 1;
    }
    left[left.len() - count..].iter().collect()
}

fn replace_prefix(string: &str, old_prefix: &str, new_prefix: &str) -> String {
    assert!(
        string.starts_with(old_prefix),
        "string {string:?} doesn't start with prefix {old_prefix:?}; this is a bug"
    );
    format!("{new_prefix}{}", &string[old_prefix.len()..])
}

fn replace_suffix(string: &str, old_suffix: &str, new_suffix: &str) -> String {
    if old_suffix.is_empty() {
        return format!("{string}{new_suffix}");
    }
    assert!(
        string.ends_with(old_suffix),
        "string {string:?} doesn't end with suffix {old_suffix:?}; this is a bug"
    );
    format!("{}{new_suffix}", &string[..string.len() - old_suffix.len()])
}

fn remove_prefix(string: &str, old_prefix: &str) -> String {
    replace_prefix(string, old_prefix, "")
}

fn remove_suffix(string: &str, old_suffix: &str) -> String {
    replace_suffix(string, old_suffix, "")
}

fn maximum_overlap(left: &str, right: &str) -> String {
    right.chars().take(overlap_count(left, right)).collect()
}

fn overlap_count(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let start_left = left.len().saturating_sub(right.len());
    let end_right = right.len().min(left.len());
    if end_right == 0 {
        return 0;
    }
    let mut map = vec![0usize; end_right];
    let mut k = 0;
    for index in 1..end_right {
        if right[index] == right[k] {
            map[index] = map[k];
        } else {
            map[index] = k;
        }
        while k > 0 && right[index] != right[k] {
            k = map[k];
        }
        if right[index] == right[k] {
            k += 1;
        }
    }
    k = 0;
    for character in left.iter().skip(start_left) {
        while k > 0 && Some(character) != right.get(k) {
            k = map[k];
        }
        if Some(character) == right.get(k) {
            k += 1;
        }
    }
    k
}

/// One `structuredPatch` hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: i64,
    pub old_lines: i64,
    pub new_start: i64,
    pub new_lines: i64,
    pub lines: Vec<String>,
}

/// `createTwoFilesPatch(name, name, old, new, undefined, undefined, { context, headerOptions: FILE_HEADERS_ONLY })`.
pub fn create_two_files_patch(name: &str, old_text: &str, new_text: &str, context: usize) -> String {
    let patch = structured_patch(old_text, new_text, context);
    let mut lines = vec![format!("--- {name}"), format!("+++ {name}")];
    for hunk in patch {
        let old_start = if hunk.old_lines == 0 {
            hunk.old_start - 1
        } else {
            hunk.old_start
        };
        let new_start = if hunk.new_lines == 0 {
            hunk.new_start - 1
        } else {
            hunk.new_start
        };
        lines.push(format!(
            "@@ -{},{} +{},{} @@",
            old_start, hunk.old_lines, new_start, hunk.new_lines
        ));
        lines.extend(hunk.lines);
    }
    let mut output = lines.join("\n");
    output.push('\n');
    output
}

struct Segment {
    added: bool,
    removed: bool,
    lines: Vec<String>,
}

/// `structuredPatch(old, new, undefined, undefined, undefined, { context })`.
pub fn structured_patch(old_text: &str, new_text: &str, context: usize) -> Vec<Hunk> {
    let mut segments: Vec<Segment> = diff_lines(old_text, new_text)
        .into_iter()
        .map(|part| Segment {
            added: part.added,
            removed: part.removed,
            lines: split_lines(&part.value),
        })
        .collect();
    segments.push(Segment {
        added: false,
        removed: false,
        lines: Vec::new(),
    });

    let mut hunks = Vec::new();
    let mut old_range_start = 0;
    let mut new_range_start = 0;
    let mut current: Vec<String> = Vec::new();
    let mut old_line = 1;
    let mut new_line = 1;

    for index in 0..segments.len() {
        let segment = &segments[index];
        if segment.added || segment.removed {
            if old_range_start == 0 {
                let previous = index.checked_sub(1).map(|previous| &segments[previous]);
                old_range_start = old_line;
                new_range_start = new_line;
                if let Some(previous) = previous {
                    current = if context > 0 {
                        let start = previous.lines.len().saturating_sub(context);
                        previous.lines[start..].iter().map(|line| format!(" {line}")).collect()
                    } else {
                        Vec::new()
                    };
                    let count = current.len() as i64;
                    old_range_start -= count;
                    new_range_start -= count;
                }
            }
            for line in &segment.lines {
                current.push(format!("{}{}", if segment.added { '+' } else { '-' }, line));
            }
            if segment.added {
                new_line += segment.lines.len() as i64;
            } else {
                old_line += segment.lines.len() as i64;
            }
        } else {
            if old_range_start != 0 {
                if segment.lines.len() <= context * 2 && index < segments.len() - 2 {
                    current.extend(segment.lines.iter().map(|line| format!(" {line}")));
                } else {
                    let context_size = segment.lines.len().min(context);
                    current.extend(segment.lines[..context_size].iter().map(|line| format!(" {line}")));
                    hunks.push(Hunk {
                        old_start: old_range_start,
                        old_lines: old_line - old_range_start + context_size as i64,
                        new_start: new_range_start,
                        new_lines: new_line - new_range_start + context_size as i64,
                        lines: std::mem::take(&mut current),
                    });
                    old_range_start = 0;
                    new_range_start = 0;
                }
            }
            old_line += segment.lines.len() as i64;
            new_line += segment.lines.len() as i64;
        }
    }

    for hunk in &mut hunks {
        let mut index = 0;
        while index < hunk.lines.len() {
            if hunk.lines[index].ends_with('\n') {
                hunk.lines[index].pop();
                index += 1;
            } else {
                hunk.lines.insert(index + 1, "\\ No newline at end of file".to_owned());
                index += 2;
            }
        }
    }
    hunks
}

fn split_lines(text: &str) -> Vec<String> {
    let has_trailing_newline = text.ends_with('\n');
    let mut lines: Vec<String> = text.split('\n').map(|line| format!("{line}\n")).collect();
    if has_trailing_newline {
        lines.pop();
    } else if let Some(last) = lines.last_mut() {
        last.pop();
    }
    lines
}
