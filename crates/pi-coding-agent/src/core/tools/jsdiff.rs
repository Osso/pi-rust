//! Private port of the parts of `diff@8.0.4` that the coding agent uses:
//! `diffLines` with default options and `createTwoFilesPatch` with
//! `FILE_HEADERS_ONLY`.

use std::collections::HashMap;
use std::rc::Rc;

/// One diff part with source-compatible flags and joined token value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Part {
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
pub(crate) fn diff_lines(old_text: &str, new_text: &str) -> Vec<Part> {
    let old_tokens = remove_empty(tokenize(old_text));
    let new_tokens = remove_empty(tokenize(new_text));
    let components = compute(&old_tokens, &new_tokens);
    build_values(&components, &old_tokens, &new_tokens)
}

/// `tokenize(value, {})`: split into lines with their terminators.
fn tokenize(value: &str) -> Vec<String> {
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

fn compute(old_tokens: &[String], new_tokens: &[String]) -> Vec<Component> {
    let old_len = old_tokens.len() as i64;
    let new_len = new_tokens.len() as i64;
    let mut best_path: HashMap<i64, Path> = HashMap::new();
    let mut start = Path {
        old_pos: -1,
        last: None,
    };
    let new_pos = extract_common(&mut start, new_tokens, old_tokens, 0);
    if start.old_pos + 1 >= old_len && new_pos + 1 >= new_len {
        return flatten(start.last);
    }
    best_path.insert(0, start);

    let mut min_diagonal = i64::MIN;
    let mut max_diagonal = i64::MAX;
    let mut edit_length = 1;
    let max_edit_length = new_len + old_len;
    while edit_length <= max_edit_length {
        let mut diagonal = (-edit_length).max(min_diagonal);
        let upper = edit_length.min(max_diagonal);
        while diagonal <= upper {
            if let Some(component) = extend_diagonal(
                &mut best_path,
                diagonal,
                old_tokens,
                new_tokens,
                &mut min_diagonal,
                &mut max_diagonal,
            ) {
                return component;
            }
            diagonal += 2;
        }
        edit_length += 1;
    }
    Vec::new()
}

#[allow(clippy::too_many_arguments)]
fn extend_diagonal(
    best_path: &mut HashMap<i64, Path>,
    diagonal: i64,
    old_tokens: &[String],
    new_tokens: &[String],
    min_diagonal: &mut i64,
    max_diagonal: &mut i64,
) -> Option<Vec<Component>> {
    let old_len = old_tokens.len() as i64;
    let new_len = new_tokens.len() as i64;
    let remove_path = best_path.remove(&(diagonal - 1));
    let add_path = best_path.get(&(diagonal + 1)).cloned();

    let can_add = add_path.as_ref().is_some_and(|path| {
        let new_pos = path.old_pos - diagonal;
        (0..new_len).contains(&new_pos)
    });
    let can_remove = remove_path.as_ref().is_some_and(|path| path.old_pos + 1 < old_len);
    if !can_add && !can_remove {
        best_path.remove(&diagonal);
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
    let new_pos = extract_common(&mut base_path, new_tokens, old_tokens, diagonal);
    if base_path.old_pos + 1 >= old_len && new_pos + 1 >= new_len {
        return Some(flatten(base_path.last));
    }

    let old_done = base_path.old_pos + 1 >= old_len;
    let new_done = new_pos + 1 >= new_len;
    best_path.insert(diagonal, base_path);
    if old_done {
        *max_diagonal = (*max_diagonal).min(diagonal - 1);
    }
    if new_done {
        *min_diagonal = (*min_diagonal).max(diagonal + 1);
    }
    None
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

fn extract_common(path: &mut Path, new_tokens: &[String], old_tokens: &[String], diagonal: i64) -> i64 {
    let new_len = new_tokens.len() as i64;
    let old_len = old_tokens.len() as i64;
    let mut old_pos = path.old_pos;
    let mut new_pos = old_pos - diagonal;
    let mut common = 0;
    while new_pos + 1 < new_len
        && old_pos + 1 < old_len
        && old_tokens[(old_pos + 1) as usize] == new_tokens[(new_pos + 1) as usize]
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

fn build_values(components: &[Component], old_tokens: &[String], new_tokens: &[String]) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut old_pos = 0;
    let mut new_pos = 0;
    for component in components {
        if component.removed {
            parts.push(Part {
                added: false,
                removed: true,
                value: old_tokens[old_pos..old_pos + component.count].concat(),
            });
            old_pos += component.count;
        } else {
            parts.push(Part {
                added: component.added,
                removed: false,
                value: new_tokens[new_pos..new_pos + component.count].concat(),
            });
            new_pos += component.count;
            if !component.added {
                old_pos += component.count;
            }
        }
    }
    parts
}

/// `createTwoFilesPatch(name, name, old, new, undefined, undefined, { context, headerOptions: FILE_HEADERS_ONLY })`.
pub(crate) fn create_two_files_patch(name: &str, old_text: &str, new_text: &str, context: usize) -> String {
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

struct Hunk {
    old_start: i64,
    old_lines: i64,
    new_start: i64,
    new_lines: i64,
    lines: Vec<String>,
}

struct Segment {
    added: bool,
    removed: bool,
    lines: Vec<String>,
}

fn structured_patch(old_text: &str, new_text: &str, context: usize) -> Vec<Hunk> {
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
