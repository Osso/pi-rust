//! minimatch's level-two POSIX path preprocessing, used by Node path.matchesGlob.
use crate::js_json::JsString;

pub(super) fn file_parts(mut parts: Vec<JsString>, preserve_slashes: bool) -> Vec<JsString> {
    loop {
        let squeezed = squeeze_empty_parts(&mut parts, preserve_slashes);
        let resolved = resolve_parents(&mut parts, false);
        if !squeezed && !resolved {
            return ensure_nonempty(parts);
        }
    }
}

pub(super) fn pattern_parts(mut patterns: Vec<Vec<JsString>>, preserve_slashes: bool) -> Vec<Vec<JsString>> {
    loop {
        let mut changed = false;
        let mut index = 0;
        while index < patterns.len() {
            let alternatives = split_globstar_parents(&mut patterns[index]);
            changed |= !alternatives.is_empty();
            patterns.extend(alternatives);
            changed |= squeeze_empty_parts(&mut patterns[index], preserve_slashes);
            changed |= resolve_parents(&mut patterns[index], true);
            index += 1;
        }
        if !changed {
            return patterns.into_iter().map(ensure_nonempty).collect();
        }
    }
}

fn ensure_nonempty(parts: Vec<JsString>) -> Vec<JsString> {
    if parts.is_empty() {
        vec![JsString::default()]
    } else {
        parts
    }
}

fn squeeze_empty_parts(parts: &mut Vec<JsString>, preserve_slashes: bool) -> bool {
    if preserve_slashes {
        return false;
    }
    let mut changed = false;
    let mut index = 1;
    while index + 1 < parts.len() {
        let preserve_unc = index == 1 && parts[index].is_empty() && parts[0].is_empty();
        if !preserve_unc && (parts[index] == "." || parts[index].is_empty()) {
            parts.remove(index);
            changed = true;
        } else {
            index += 1;
        }
    }
    if parts.len() == 2 && parts[0] == "." && (parts[1] == "." || parts[1].is_empty()) {
        parts.pop();
        changed = true;
    }
    changed
}

fn resolve_parents(parts: &mut Vec<JsString>, preserve_globstar_dot: bool) -> bool {
    let mut changed = false;
    let mut index = 1;
    while index < parts.len() {
        if parts[index] == ".." && is_parent_resolvable(&parts[index - 1]) {
            let need_dot = preserve_globstar_dot && index == 1 && parts.get(index + 1).is_some_and(|part| part == "**");
            let replacement = if need_dot {
                vec![JsString::from(".")]
            } else {
                Vec::new()
            };
            parts.splice(index - 1..=index, replacement);
            if preserve_globstar_dot && parts.is_empty() {
                parts.push(JsString::default());
            }
            index = index.saturating_sub(2).max(1);
            changed = true;
        } else {
            index += 1;
        }
    }
    changed
}

fn is_parent_resolvable(part: &JsString) -> bool {
    !part.is_empty() && part != "." && part != ".." && part != "**"
}

fn split_globstar_parents(parts: &mut Vec<JsString>) -> Vec<Vec<JsString>> {
    let mut alternatives = Vec::new();
    let mut index = 0;
    while index < parts.len() {
        if parts[index] != "**" {
            index += 1;
            continue;
        }
        while parts.get(index + 1).is_some_and(|part| part == "**") {
            parts.remove(index + 1);
        }
        let parent = parts.get(index + 1).is_some_and(|part| part == "..");
        let following = parts.get(index + 2).is_some_and(is_traversal_successor);
        let next = parts.get(index + 3).is_some_and(is_traversal_successor);
        if parent && following && next {
            parts.remove(index);
            let mut alternative = parts.clone();
            alternative[index] = JsString::from("**");
            alternatives.push(alternative);
        } else {
            index += 1;
        }
    }
    alternatives
}

fn is_traversal_successor(part: &JsString) -> bool {
    !part.is_empty() && part != "." && part != ".."
}
