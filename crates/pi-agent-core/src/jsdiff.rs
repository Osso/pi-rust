//! Port of the parts of `diff@8.0.4` used by the agent and coding-agent tools:
//! `diffLines`, `diffWords`, `structuredPatch`, and `createTwoFilesPatch` with
//! `FILE_HEADERS_ONLY`.

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::rc::Rc;

/// One diff part with source-compatible flags and joined token value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub added: bool,
    pub removed: bool,
    pub value: JsString,
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
pub fn diff_lines(old_text: impl Into<JsString>, new_text: impl Into<JsString>) -> Vec<Part> {
    let old_text = old_text.into();
    let old_text = &old_text;
    let new_text = new_text.into();
    let new_text = &new_text;

    let old_tokens = remove_empty(tokenize(old_text));
    let new_tokens = remove_empty(tokenize(new_text));
    let components = Search::new(&old_tokens, &new_tokens).run();
    build_values(&components, &old_tokens, &new_tokens, Mode::Lines)
}

/// `diffWords(old_str, new_str)` with default options.
pub fn diff_words(old_text: impl Into<JsString>, new_text: impl Into<JsString>) -> Vec<Part> {
    let old_text = old_text.into();
    let old_text = &old_text;
    let new_text = new_text.into();
    let new_text = &new_text;

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
pub fn tokenize(value: impl Into<JsString>) -> Vec<JsString> {
    let value = value.into();
    let units = value.units();
    let mut tokens = Vec::new();
    let mut start = 0;
    for (index, unit) in units.iter().enumerate() {
        if *unit == 0x0a {
            tokens.push(value.slice(start, index + 1));
            start = index + 1;
        }
    }
    if start < value.len() {
        tokens.push(value.slice(start, value.len()));
    }
    tokens
}

fn remove_empty(tokens: Vec<JsString>) -> Vec<JsString> {
    tokens.into_iter().filter(|token| !token.is_empty()).collect()
}

struct Search<'a> {
    old_tokens: &'a [JsString],
    new_tokens: &'a [JsString],
    mode: Mode,
    best_path: HashMap<i64, Path>,
    min_diagonal: i64,
    max_diagonal: i64,
}

impl<'a> Search<'a> {
    fn new(old_tokens: &'a [JsString], new_tokens: &'a [JsString]) -> Self {
        Self::with_mode(old_tokens, new_tokens, Mode::Lines)
    }

    fn new_words(old_tokens: &'a [JsString], new_tokens: &'a [JsString]) -> Self {
        Self::with_mode(old_tokens, new_tokens, Mode::Words)
    }

    fn with_mode(old_tokens: &'a [JsString], new_tokens: &'a [JsString], mode: Mode) -> Self {
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

fn extract_common(path: &mut Path, new_tokens: &[JsString], old_tokens: &[JsString], diagonal: i64, mode: Mode) -> i64 {
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

fn tokens_equal(left: &JsString, right: &JsString, mode: Mode) -> bool {
    match mode {
        Mode::Lines => left == right,
        Mode::Words => left.trim() == right.trim(),
    }
}

fn build_values(components: &[Component], old_tokens: &[JsString], new_tokens: &[JsString], mode: Mode) -> Vec<Part> {
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

fn join_tokens(tokens: &[JsString], mode: Mode) -> JsString {
    match mode {
        Mode::Lines => JsString::join(tokens, ""),
        Mode::Words => JsString::join(
            tokens.iter().enumerate().map(
                |(index, token)| {
                    if index == 0 { token.clone() } else { token.trim_start() }
                },
            ),
            "",
        ),
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum WordTokenClass {
    Word,
    Whitespace,
    Punctuation,
}

struct WordCharacter {
    start: usize,
    end: usize,
    class: WordTokenClass,
}

fn decode_word_characters(value: &JsString) -> Vec<WordCharacter> {
    let mut offset = 0;
    char::decode_utf16(value.units().iter().copied())
        .map(|decoded| {
            // Regex `u` treats a valid pair as one character and a lone unit as punctuation.
            let character = decoded.ok();
            let class = if character.is_some_and(is_word_character) {
                WordTokenClass::Word
            } else if character.is_some_and(js_whitespace) {
                WordTokenClass::Whitespace
            } else {
                WordTokenClass::Punctuation
            };
            let start = offset;
            offset += character.map_or(1, char::len_utf16);
            WordCharacter {
                start,
                end: offset,
                class,
            }
        })
        .collect()
}

/// `WordDiff.tokenize` with the default (non-segmenter) tokenizer.
fn tokenize_words(value: &JsString) -> Vec<JsString> {
    let characters = decode_word_characters(value);
    let mut parts = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let start = index;
        let class = characters[index].class;
        index += 1;
        if class != WordTokenClass::Punctuation {
            while index < characters.len() && characters[index].class == class {
                index += 1;
            }
        }
        parts.push(value.slice(characters[start].start, characters[index - 1].end));
    }
    stitch_word_whitespace(parts)
}

fn has_word_whitespace(text: &JsString) -> bool {
    text.units()
        .iter()
        .any(|unit| pi_js_compat::string::is_js_whitespace_unit(*unit))
}

fn stitch_word_whitespace(parts: Vec<JsString>) -> Vec<JsString> {
    let mut tokens = Vec::new();
    let mut previous: Option<JsString> = None;
    for part in parts {
        let part_is_whitespace = has_word_whitespace(&part);
        let previous_is_whitespace = previous.as_ref().is_some_and(has_word_whitespace);
        let append_to_tail = if part_is_whitespace {
            previous.is_some()
        } else {
            previous_is_whitespace && tokens.last() == previous.as_ref()
        };
        let prefix = if append_to_tail {
            tokens.pop().unwrap_or_default()
        } else if previous_is_whitespace {
            previous.clone().unwrap_or_default()
        } else {
            JsString::default()
        };
        tokens.push(prefix + &part);
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
            parts[insertion_index].value = parts[insertion_index]
                .value
                .slice(whitespace.len(), parts[insertion_index].value.len());
        }
        if let Some(keep) = end_keep {
            let whitespace = leading_ws(&parts[keep].value);
            parts[keep].value = parts[keep].value.slice(whitespace.len(), parts[keep].value.len());
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
        let start_suffix = &new_ws_full.slice(0, new_ws_full.len() - new_ws_end.len());
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

fn leading_ws(value: &JsString) -> JsString {
    value.slice(0, value.len() - value.trim_start().len())
}

fn trailing_ws(value: &JsString) -> JsString {
    value.slice(value.trim_end().len(), value.len())
}

fn leading_and_trailing_ws(value: &JsString) -> (JsString, JsString) {
    (leading_ws(value), trailing_ws(value))
}

fn longest_common_prefix(left: &JsString, right: &JsString) -> JsString {
    let count = left
        .units()
        .iter()
        .zip(right.units())
        .take_while(|(left, right)| left == right)
        .count();
    left.slice(0, count)
}

fn longest_common_suffix(left: &JsString, right: &JsString) -> JsString {
    let count = left
        .units()
        .iter()
        .rev()
        .zip(right.units().iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    left.slice(left.len() - count, left.len())
}

fn replace_prefix(string: &JsString, old_prefix: &JsString, new_prefix: &JsString) -> JsString {
    assert!(string.starts_with(old_prefix), "invalid diff prefix");
    new_prefix.clone() + string.slice(old_prefix.len(), string.len())
}

fn replace_suffix(string: &JsString, old_suffix: &JsString, new_suffix: &JsString) -> JsString {
    assert!(string.ends_with(old_suffix), "invalid diff suffix");
    string.slice(0, string.len() - old_suffix.len()) + new_suffix
}

fn remove_prefix(string: &JsString, old_prefix: &JsString) -> JsString {
    replace_prefix(string, old_prefix, &JsString::default())
}

fn remove_suffix(string: &JsString, old_suffix: &JsString) -> JsString {
    replace_suffix(string, old_suffix, &JsString::default())
}

fn maximum_overlap(left: &JsString, right: &JsString) -> JsString {
    right.slice(0, overlap_count(left, right))
}

fn overlap_count(left: &JsString, right: &JsString) -> usize {
    let left = left.units();
    let right = right.units();
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
    pub lines: Vec<JsString>,
}

/// `createTwoFilesPatch(name, name, old, new, undefined, undefined, { context, headerOptions: FILE_HEADERS_ONLY })`.
pub fn create_two_files_patch(
    name: impl Into<JsString>,
    old_text: impl Into<JsString>,
    new_text: impl Into<JsString>,
    context: usize,
) -> JsString {
    let name = name.into();
    let name = &name;
    let old_text = old_text.into();
    let old_text = &old_text;
    let new_text = new_text.into();
    let new_text = &new_text;

    let patch = structured_patch(old_text, new_text, context);
    let mut lines = vec![JsString::from("--- ") + name, JsString::from("+++ ") + name];
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
        lines.push(
            format!(
                "@@ -{},{} +{},{} @@",
                old_start, hunk.old_lines, new_start, hunk.new_lines
            )
            .into(),
        );
        lines.extend(hunk.lines);
    }
    let mut output = JsString::join(lines, "\n");
    output.push_str("\n");
    output
}

struct Segment {
    added: bool,
    removed: bool,
    lines: Vec<JsString>,
}

/// `structuredPatch(old, new, undefined, undefined, undefined, { context })`.
pub fn structured_patch(old_text: impl Into<JsString>, new_text: impl Into<JsString>, context: usize) -> Vec<Hunk> {
    let old_text = old_text.into();
    let old_text = &old_text;
    let new_text = new_text.into();
    let new_text = &new_text;

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
    let mut current: Vec<JsString> = Vec::new();
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
                        previous.lines[start..]
                            .iter()
                            .map(|line| JsString::from(" ") + line)
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let count = current.len() as i64;
                    old_range_start -= count;
                    new_range_start -= count;
                }
            }
            for line in &segment.lines {
                current.push(JsString::from(if segment.added { '+' } else { '-' }) + line);
            }
            if segment.added {
                new_line += segment.lines.len() as i64;
            } else {
                old_line += segment.lines.len() as i64;
            }
        } else {
            if old_range_start != 0 {
                if segment.lines.len() <= context * 2 && index < segments.len() - 2 {
                    current.extend(segment.lines.iter().map(|line| JsString::from(" ") + line));
                } else {
                    let context_size = segment.lines.len().min(context);
                    current.extend(
                        segment.lines[..context_size]
                            .iter()
                            .map(|line| JsString::from(" ") + line),
                    );
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
                hunk.lines[index] = hunk.lines[index].slice(0, hunk.lines[index].len() - 1);
                index += 1;
            } else {
                hunk.lines.insert(index + 1, "\\ No newline at end of file".into());
                index += 2;
            }
        }
    }
    hunks
}

fn split_lines(text: &JsString) -> Vec<JsString> {
    let has_trailing_newline = text.ends_with('\n');
    let mut lines: Vec<JsString> = text.split('\n').map(|line| line + "\n").collect();
    if has_trailing_newline {
        lines.pop();
    } else if let Some(last) = lines.last_mut() {
        *last = last.slice(0, last.len() - 1);
    }
    lines
}
