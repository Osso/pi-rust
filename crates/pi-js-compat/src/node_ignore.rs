//! `ignore@7.0.5` default matcher, using JavaScript UTF-16 and legacy RegExp semantics.
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use crate::js_json::JsString;
use crate::js_regex::{JsCaptures, JsRegex};

/// Frozen JS error (`TypeError`, `RangeError`, or lazy RegExp `SyntaxError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IgnoreError {
    pub name: JsString,
    pub message: JsString,
}

#[derive(Clone, Debug)]
struct Rule {
    negative: bool,
    source: JsString,
    regex: OnceLock<Result<JsRegex, IgnoreError>>,
}

impl Rule {
    fn matches(&self, path: &JsString) -> Result<bool, IgnoreError> {
        let compiled = self.regex.get_or_init(|| {
            JsRegex::with_flags(&self.source, "i").map_err(|error| IgnoreError {
                name: "SyntaxError".into(),
                message: error.message().clone(),
            })
        });
        compiled
            .as_ref()
            .map(|regex| regex.is_match(path))
            .map_err(Clone::clone)
    }
}

/// Default `ignore()` instance (`ignorecase: true`, `allowRelativePaths: false`).
#[derive(Debug, Default)]
pub struct Ignore {
    rules: Vec<Rule>,
    cache: Mutex<BTreeMap<JsString, bool>>,
}

impl Clone for Ignore {
    fn clone(&self) -> Self {
        Self {
            rules: self.rules.clone(),
            cache: Mutex::default(),
        }
    }
}

impl Ignore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, rules: impl Into<JsString>) -> &mut Self {
        for line in rules.into().split_inclusive('\n') {
            let pattern = match line.strip_suffix("\n") {
                Some(line) => line.strip_suffix("\r").unwrap_or(line),
                None => line,
            };
            if !check_pattern(&pattern) {
                continue;
            }
            let negative = pattern.starts_with("!");
            let body = if negative {
                pattern.slice(1, pattern.len())
            } else {
                pattern
            };
            let body = if body.starts_with("\\!") || body.starts_with("\\#") {
                body.slice(1, body.len())
            } else {
                body
            };
            let prefix = make_regex_prefix(&body);
            let source = regex(r"(^|\\\/)?\\\*$").replace_all_with(prefix, |captures| {
                let prefix = group(captures, 1);
                let wildcard = if prefix.is_empty() { "[^/]*" } else { "[^/]+" };
                prefix + wildcard + r"(?=$|\/$)"
            });
            self.rules.push(Rule {
                negative,
                source,
                regex: OnceLock::new(),
            });
            self.cache.get_mut().expect("ignore cache lock poisoned").clear();
        }
        self
    }

    pub fn ignores(&self, path: impl Into<JsString>) -> Result<bool, IgnoreError> {
        let path = path.into();
        check_path(&path)?;
        let mut cache = self.cache.lock().expect("ignore cache lock poisoned");
        self.test_path(&path, &mut cache)
    }

    fn test_path(&self, path: &JsString, cache: &mut BTreeMap<JsString, bool>) -> Result<bool, IgnoreError> {
        if let Some(ignored) = cache.get(path) {
            return Ok(*ignored);
        }
        let mut slices: Vec<_> = path.split('/').filter(|slice| !slice.is_empty()).collect();
        slices.pop();
        let parent_ignored = !slices.is_empty() && self.test_path(&(JsString::join(slices, "/") + "/"), cache)?;
        let ignored = parent_ignored || self.test_rules(path)?;
        cache.insert(path.clone(), ignored);
        Ok(ignored)
    }

    fn test_rules(&self, path: &JsString) -> Result<bool, IgnoreError> {
        let mut ignored = false;
        let mut unignored = false;
        for rule in &self.rules {
            let skip_decided = unignored == rule.negative && ignored != unignored;
            let skip_initial_negative = rule.negative && !ignored && !unignored;
            if skip_decided || skip_initial_negative {
                continue;
            }
            if rule.matches(path)? {
                ignored = !rule.negative;
                unignored = rule.negative;
            }
        }
        Ok(ignored)
    }
}

fn check_path(path: &JsString) -> Result<(), IgnoreError> {
    if path.is_empty() {
        return Err(IgnoreError {
            name: "TypeError".into(),
            message: "path must not be empty".into(),
        });
    }
    if regex(r"^\.{0,2}\/|^\.{1,2}$").is_match(path) {
        return Err(IgnoreError {
            name: "RangeError".into(),
            message: JsString::from("path should be a `path.relative()`d string, but got \"") + path + "\"",
        });
    }
    Ok(())
}

fn check_pattern(pattern: &JsString) -> bool {
    !pattern.is_empty()
        && !pattern.starts_with("#")
        && !regex(r"^\s+$").is_match(pattern)
        && !regex(r"(?:[^\\]|^)\\$").is_match(pattern)
}

fn regex(pattern: &str) -> JsRegex {
    JsRegex::new(pattern).expect("frozen ignore regexp is valid")
}

fn group(captures: &JsCaptures, index: usize) -> JsString {
    captures.get(index).map(|matched| matched.text()).unwrap_or_default()
}

const REPLACERS: &[&str] = &[
    r"^\uFEFF",
    r"((?:\\\\)*?)(\\?\s+)$",
    r"(\\+?)\s",
    r"[\\$.|*+(){^]",
    r"(?!\\)\?",
    r"^\/",
    r"\/",
    r"^\^*\\\*\\\*\\\/",
    r"^(?=[^^])",
    r"\\\/\\\*\\\*(?=\\\/|$)",
    r"(^|[^\\]+)(\\\*)+(?=.+)",
    r"\\\\\\(?=[$.|*+(){^])",
    r"\\\\",
    r"(\\)?\[([^\]/]*?)(\\*)($|\])",
    r"(?:[^*])$",
];

fn make_regex_prefix(pattern: &JsString) -> JsString {
    static COMPILED: OnceLock<Vec<JsRegex>> = OnceLock::new();
    let replacers = COMPILED.get_or_init(|| REPLACERS.iter().map(|source| regex(source)).collect());
    replacers
        .iter()
        .enumerate()
        .fold(pattern.clone(), |text, (step, matcher)| {
            matcher.replace_all_with(&text, |captures| replace_prefix_step(step, pattern, &text, captures))
        })
}

fn replace_prefix_step(step: usize, pattern: &JsString, text: &JsString, captures: &JsCaptures) -> JsString {
    let matched = group(captures, 0);
    match step {
        0 => JsString::default(),
        1 => group(captures, 1) + if group(captures, 2).starts_with("\\") { " " } else { "" },
        2 => even_backslashes(&group(captures, 1)) + " ",
        3 => JsString::from("\\") + matched,
        4 => "[^/]".into(),
        5 => "^".into(),
        6 => r"\/".into(),
        7 => r"^(?:.*\/)?".into(),
        8 => if regex(r"\/(?!$)").is_match(pattern) {
            "^"
        } else {
            r"(?:^|\/)"
        }
        .into(),
        9 => if captures.get(0).expect("whole match").start() + 6 < text.len() {
            r"(?:\/[^\/]+)*"
        } else {
            r"\/.+"
        }
        .into(),
        10 => group(captures, 1) + regex(r"\\\*").replace_all(group(captures, 2), r"[^\/]*"),
        11 | 12 => "\\".into(),
        13 => replace_range(captures),
        14 => {
            let suffix = if matched.ends_with("/") { "$" } else { r"(?=$|\/$)" };
            matched + suffix
        }
        _ => unreachable!("frozen replacer index"),
    }
}

fn even_backslashes(slashes: &JsString) -> JsString {
    slashes.slice(0, slashes.len() - slashes.len() % 2)
}

fn replace_range(captures: &JsCaptures) -> JsString {
    let range = group(captures, 2);
    let end_escape = group(captures, 3);
    let close = group(captures, 4);
    if group(captures, 1) == "\\" {
        return JsString::from(r"\[") + range + even_backslashes(&end_escape) + close;
    }
    if close != "]" || !end_escape.len().is_multiple_of(2) {
        return "[]".into();
    }
    let sanitized = regex(r"([0-z])-([0-z])").replace_all_with(range, |matched| {
        let from = group(matched, 1);
        let to = group(matched, 2);
        if from.units()[0] <= to.units()[0] {
            group(matched, 0)
        } else {
            JsString::default()
        }
    });
    JsString::from("[") + sanitized + end_escape + "]"
}
