//! `minimatch` 10.2.5 (posix platform) as used by the coding-agent packages.
use crate::js_json::JsString;
use units::*;

mod ast;
mod brace;
mod class;
mod escape;

use crate::js_regex::JsRegex as Regex;

use ast::{Ast, MmPattern};
use escape::is_valid_pattern;

/// `MinimatchOptions` with the same defaults as minimatch 10.2.5.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MinimatchOptions {
    pub nocase: bool,
    pub dot: bool,
    pub match_base: bool,
    pub nobrace: bool,
    pub noglobstar: bool,
    pub noext: bool,
    pub nonegate: bool,
    pub nocomment: bool,
    pub partial: bool,
    pub flip_negate: bool,
    pub preserve_multiple_slashes: bool,
    pub optimization_level: u8,
    pub brace_expand_max: usize,
    pub max_globstar_recursion: usize,
    pub max_extglob_recursion: usize,
    pub nocase_magic_only: bool,
}

impl Default for MinimatchOptions {
    fn default() -> Self {
        Self {
            nocase: false,
            dot: false,
            match_base: false,
            nobrace: false,
            noglobstar: false,
            noext: false,
            nonegate: false,
            nocomment: false,
            partial: false,
            flip_negate: false,
            preserve_multiple_slashes: false,
            optimization_level: 1,
            brace_expand_max: brace::default_max(),
            max_globstar_recursion: 200,
            max_extglob_recursion: 2,
            nocase_magic_only: false,
        }
    }
}

/// `minimatch(path, pattern, options)`.
pub fn minimatch(path: impl Into<JsString>, pattern: impl Into<JsString>, options: &MinimatchOptions) -> bool {
    let path = path.into();
    let pattern = pattern.into();
    if !is_valid_pattern(&pattern) {
        return false;
    }
    if !options.nocomment && pattern.starts_with(HASH) {
        return false;
    }
    Minimatch::new(&pattern, options.clone()).matches(&path, options.partial)
}

/// `minimatch.braceExpand(pattern, options)`.
pub fn brace_expand(pattern: impl Into<JsString>, options: &MinimatchOptions) -> Vec<JsString> {
    let pattern = pattern.into();
    if options.nobrace {
        return vec![pattern.to_owned()];
    }
    brace::brace_expand(&pattern, options.brace_expand_max)
}

#[derive(Clone, Debug)]
enum Part {
    Literal(JsString),
    Regex(Regex, Option<FastTest>),
    GlobStar,
    Invalid,
}

#[derive(Clone, Debug)]
enum FastTest {
    Star {
        dot: bool,
    },
    StarDotExt {
        ext: JsString,
        dot: bool,
        nocase: bool,
    },
    Qmarks {
        length: usize,
        ext: JsString,
        dot: bool,
        nocase: bool,
    },
    StarDotStar {
        dot: bool,
    },
    DotStar,
}

impl FastTest {
    fn matches(&self, value: &JsString) -> bool {
        match self {
            Self::Star { dot } => star_matches(value, *dot),
            Self::StarDotExt { ext, dot, nocase } => star_dot_ext_matches(value, ext, *dot, *nocase),
            Self::Qmarks {
                length,
                ext,
                dot,
                nocase,
            } => qmarks_match(value, *length, ext, *dot, *nocase),
            Self::StarDotStar { dot } => star_dot_star_matches(value, *dot),
            Self::DotStar => dot_star_matches(value),
        }
    }
}

fn star_matches(value: &JsString, dot: bool) -> bool {
    if dot {
        return !value.is_empty() && value != "." && value != "..";
    }
    !value.is_empty() && !value.starts_with(DOT)
}

fn star_dot_ext_matches(value: &JsString, ext: &JsString, dot: bool, nocase: bool) -> bool {
    let (value, ext) = if nocase {
        (value.to_lowercase(), ext.to_lowercase())
    } else {
        (value.to_owned(), ext.to_owned())
    };
    (dot || !value.starts_with(DOT)) && value.ends_with(&ext)
}

fn qmarks_match(value: &JsString, length: usize, ext: &JsString, dot: bool, nocase: bool) -> bool {
    if value.len() != length {
        return false;
    }
    if dot {
        if value == "." || value == ".." {
            return false;
        }
    } else if value.starts_with(DOT) {
        return false;
    }
    if ext.is_empty() {
        return true;
    }
    if nocase {
        return value.to_lowercase().ends_with(&ext.to_lowercase());
    }
    value.ends_with(ext)
}

fn star_dot_star_matches(value: &JsString, dot: bool) -> bool {
    if dot {
        return value != "." && value != ".." && value.contains(DOT);
    }
    !value.starts_with(DOT) && value.contains(DOT)
}

fn dot_star_matches(value: &JsString) -> bool {
    value != "." && value != ".." && value.starts_with(DOT)
}

fn fast_test(pattern: &JsString, options: &MinimatchOptions) -> Option<FastTest> {
    let stars = pattern.units().iter().copied().take_while(|byte| *byte == STAR).count();
    if stars > 0 && stars == pattern.len() {
        return Some(FastTest::Star { dot: options.dot });
    }
    if stars > 0 {
        let suffix = pattern.slice(stars, pattern.len());
        if !suffix.units().iter().copied().any(|character| {
            matches!(
                character,
                PLUS | AT | BANG | QUESTION | STAR | OPEN_BRACKET | OPEN_PAREN
            )
        }) {
            return Some(FastTest::StarDotExt {
                ext: suffix.to_owned(),
                dot: options.dot,
                nocase: options.nocase,
            });
        }
    }
    let questions = pattern
        .units()
        .iter()
        .copied()
        .take_while(|byte| *byte == QUESTION)
        .count();
    if questions > 0 {
        let suffix = pattern.slice(questions, pattern.len());
        if !suffix.units().iter().copied().any(|character| {
            matches!(
                character,
                PLUS | AT | BANG | QUESTION | STAR | OPEN_BRACKET | OPEN_PAREN
            )
        }) {
            return Some(FastTest::Qmarks {
                length: pattern.len(),
                ext: suffix.to_owned(),
                dot: options.dot,
                nocase: options.nocase,
            });
        }
    }
    if let Some(rest) = pattern.strip_prefix(STAR)
        && let Some(rest) = rest.strip_prefix(".*")
        && rest.units().iter().copied().all(|byte| byte == STAR)
        && !rest.is_empty()
    {
        return Some(FastTest::StarDotStar { dot: options.dot });
    }
    if let Some(rest) = pattern.strip_prefix(".*")
        && rest.units().iter().copied().all(|byte| byte == STAR)
        && !rest.is_empty()
    {
        return Some(FastTest::DotStar);
    }
    None
}

pub struct Minimatch {
    options: MinimatchOptions,
    pattern: JsString,
    negate: bool,
    comment: bool,
    empty: bool,
    set: Vec<Vec<Part>>,
}

impl Minimatch {
    pub fn new(pattern: impl Into<JsString>, options: MinimatchOptions) -> Self {
        let pattern = pattern.into();
        let mut minimatch = Self {
            options,
            pattern: pattern.to_owned(),
            negate: false,
            comment: false,
            empty: false,
            set: Vec::new(),
        };
        minimatch.make();
        minimatch
    }

    fn make(&mut self) {
        if !self.options.nocomment && self.pattern.starts_with(HASH) {
            self.comment = true;
            return;
        }
        if self.pattern.is_empty() {
            self.empty = true;
            return;
        }
        self.parse_negate();
        let expanded = if self.options.nobrace {
            vec![self.pattern.clone()]
        } else {
            brace::brace_expand(&self.pattern, self.options.brace_expand_max)
        };
        let mut glob_set: Vec<JsString> = Vec::new();
        for expanded in expanded {
            if !glob_set.contains(&expanded) {
                glob_set.push(expanded);
            }
        }
        let glob_parts: Vec<Vec<JsString>> = glob_set
            .iter()
            .map(|pattern| slash_split(pattern, self.options.preserve_multiple_slashes))
            .collect();
        let glob_parts = self.preprocess(glob_parts);
        let mut set = Vec::new();
        for parts in &glob_parts {
            let parsed: Vec<Part> = parts.iter().map(|part| self.parse(part)).collect();
            if !parsed.iter().any(Part::is_invalid) {
                set.push(parsed);
            }
        }
        self.set = set;
    }

    fn parse_negate(&mut self) {
        if self.options.nonegate {
            return;
        }
        let mut negate = false;
        let mut offset = 0;
        for character in self.pattern.units().iter().copied() {
            if character != BANG {
                break;
            }
            negate = !negate;
            offset += 1;
        }
        if offset > 0 {
            self.pattern = self.pattern.units().iter().copied().skip(offset).collect();
        }
        self.negate = negate;
    }

    fn preprocess(&self, mut glob_parts: Vec<Vec<JsString>>) -> Vec<Vec<JsString>> {
        if self.options.noglobstar {
            for parts in &mut glob_parts {
                for part in parts {
                    if part == "**" {
                        *part = JsString::from("*");
                    }
                }
            }
        }
        if self.options.optimization_level >= 1 {
            level_one_optimize(glob_parts)
        } else {
            adjacent_globstar_optimize(glob_parts)
        }
    }

    fn parse(&self, pattern: &JsString) -> Part {
        if pattern == "**" {
            return Part::GlobStar;
        }
        if pattern.is_empty() {
            return Part::Literal(JsString::default());
        }
        let mut ast = Ast::from_glob(pattern, &self.options);
        match ast.mm_pattern() {
            MmPattern::Literal(value) => Part::Literal(value),
            MmPattern::Regex(regex) => Part::Regex(regex, fast_test(pattern, &self.options)),
            MmPattern::Invalid => Part::Invalid,
        }
    }

    pub fn matches(&self, path: impl Into<JsString>, partial: bool) -> bool {
        let path = path.into();
        if self.comment {
            return false;
        }
        if self.empty {
            return path.is_empty();
        }
        if path == "/" && partial {
            return true;
        }
        let file = slash_split(&path, self.options.preserve_multiple_slashes);
        let filename = file
            .iter()
            .rev()
            .find(|segment| !segment.is_empty())
            .cloned()
            .unwrap_or_default();
        match self.match_pattern_set(&file, &filename, partial) {
            Some(result) => result,
            None => !self.options.flip_negate && self.negate,
        }
    }

    fn match_pattern_set(&self, file: &[JsString], filename: &JsString, partial: bool) -> Option<bool> {
        for pattern in &self.set {
            let file = if self.options.match_base && pattern.len() == 1 {
                vec![filename.to_owned()]
            } else {
                file.to_vec()
            };
            if self.match_one(&file, pattern, partial) {
                return Some(self.options.flip_negate || !self.negate);
            }
        }
        None
    }

    fn match_one(&self, file: &[JsString], pattern: &[Part], partial: bool) -> bool {
        if pattern.iter().any(|part| matches!(part, Part::GlobStar)) {
            self.match_globstar(file, pattern, partial, 0)
        } else {
            match_one_inner(file, pattern, partial, 0, 0)
        }
    }

    fn match_globstar(&self, file: &[JsString], pattern: &[Part], partial: bool, pattern_index: usize) -> bool {
        let first_globstar = pattern[pattern_index..]
            .iter()
            .position(|part| matches!(part, Part::GlobStar))
            .map(|index| index + pattern_index);
        let Some(first_globstar) = first_globstar else {
            return false;
        };
        let last_globstar = pattern.iter().rposition(|part| matches!(part, Part::GlobStar)).unwrap();
        let (head, body, tail) = if partial {
            (
                &pattern[pattern_index..first_globstar],
                &pattern[first_globstar + 1..],
                &pattern[pattern.len()..],
            )
        } else {
            (
                &pattern[pattern_index..first_globstar],
                if first_globstar < last_globstar {
                    &pattern[first_globstar + 1..last_globstar]
                } else {
                    &[]
                },
                &pattern[last_globstar + 1..],
            )
        };
        let mut file_index = 0usize;
        if !head.is_empty() {
            let file_head = &file[file_index..(file_index + head.len()).min(file.len())];
            if !match_one_inner(file_head, head, partial, 0, 0) {
                return false;
            }
            file_index += head.len();
        }
        let mut file_tail_match = 0usize;
        if !tail.is_empty() {
            if tail.len() + file_index > file.len() {
                return false;
            }
            let mut tail_start = file.len() - tail.len();
            if match_one_inner(file, tail, partial, tail_start, 0) {
                file_tail_match = tail.len();
            } else {
                if file.last().is_none_or(|segment| !segment.is_empty()) || file_index + tail.len() == file.len() {
                    return false;
                }
                tail_start -= 1;
                if !match_one_inner(file, tail, partial, tail_start, 0) {
                    return false;
                }
                file_tail_match = tail.len() + 1;
            }
        }
        if body.is_empty() {
            let mut saw_some = file_tail_match != 0;
            for segment in &file[file_index..file.len() - file_tail_match] {
                saw_some = true;
                if is_bad_dot(segment, self.options.dot) {
                    return false;
                }
            }
            return partial || saw_some;
        }
        let mut body_segments: Vec<(Vec<Part>, isize)> = vec![(Vec::new(), 0)];
        let mut non_globstar_parts = 0usize;
        let mut non_globstar_sums = vec![0usize];
        for part in body {
            if matches!(part, Part::GlobStar) {
                non_globstar_sums.push(non_globstar_parts);
                body_segments.push((Vec::new(), 0));
            } else {
                body_segments.last_mut().unwrap().0.push(part.clone());
                non_globstar_parts += 1;
            }
        }
        let file_length = file.len() - file_tail_match;
        let mut sum_index = body_segments.len() - 1;
        for (parts, after) in &mut body_segments {
            *after = file_length as isize - (non_globstar_sums[sum_index] + parts.len()) as isize;
            sum_index = sum_index.saturating_sub(1);
        }
        let context = GlobstarContext {
            file,
            body_segments: &body_segments,
            partial,
            max_depth: self.options.max_globstar_recursion,
            dot: self.options.dot,
        };
        match_globstar_body_sections(&context, file_index, 0, 0, file_tail_match != 0).unwrap_or(false)
    }
}

struct GlobstarContext<'a> {
    file: &'a [JsString],
    body_segments: &'a [(Vec<Part>, isize)],
    partial: bool,
    max_depth: usize,
    dot: bool,
}

fn match_globstar_body_sections(
    context: &GlobstarContext<'_>,
    file_index: usize,
    body_index: usize,
    globstar_depth: usize,
    mut saw_tail: bool,
) -> Option<bool> {
    let file = context.file;
    let partial = context.partial;
    let Some((body, after)) = context.body_segments.get(body_index) else {
        for segment in &file[file_index..] {
            saw_tail = true;
            if is_bad_dot(segment, context.dot) {
                return Some(false);
            }
        }
        return Some(saw_tail);
    };
    let mut file_index = file_index;
    while (file_index as isize) <= *after {
        let end = (file_index + body.len()).min(file.len());
        let matched = match_one_inner(&file[..end], body, partial, file_index, 0);
        if matched && globstar_depth < context.max_depth {
            let result = match_globstar_body_sections(
                context,
                file_index + body.len(),
                body_index + 1,
                globstar_depth + 1,
                saw_tail,
            );
            if result != Some(false) {
                return result;
            }
        }
        if let Some(segment) = file.get(file_index)
            && is_bad_dot(segment, context.dot)
        {
            return Some(false);
        }
        file_index += 1;
    }
    if partial { Some(true) } else { None }
}

fn is_bad_dot(segment: &JsString, dot: bool) -> bool {
    segment == "." || segment == ".." || (!dot && segment.starts_with(DOT))
}

fn match_one_inner(
    file: &[JsString],
    pattern: &[Part],
    partial: bool,
    file_index: usize,
    pattern_index: usize,
) -> bool {
    let mut file_index = file_index;
    let mut pattern_index = pattern_index;
    while file_index < file.len() && pattern_index < pattern.len() {
        let part = &pattern[pattern_index];
        let segment = &file[file_index];
        let hit = match part {
            Part::Literal(value) => segment == value,
            Part::Regex(regex, fast) => match fast {
                Some(fast) => fast.matches(segment),
                None => regex.is_match(segment),
            },
            Part::GlobStar | Part::Invalid => return false,
        };
        if !hit {
            return false;
        }
        file_index += 1;
        pattern_index += 1;
    }
    if file_index == file.len() && pattern_index == pattern.len() {
        true
    } else if file_index == file.len() {
        partial
    } else {
        pattern_index == pattern.len() && file_index == file.len() - 1 && file[file_index].is_empty()
    }
}

fn adjacent_globstar_optimize(glob_parts: Vec<Vec<JsString>>) -> Vec<Vec<JsString>> {
    glob_parts
        .into_iter()
        .map(|parts| {
            let mut output: Vec<JsString> = Vec::new();
            for part in parts {
                if part == "**" && output.last().is_some_and(|last| last == "**") {
                    continue;
                }
                output.push(part);
            }
            output
        })
        .collect()
}

fn level_one_optimize(glob_parts: Vec<Vec<JsString>>) -> Vec<Vec<JsString>> {
    glob_parts
        .into_iter()
        .map(|parts| {
            let mut output: Vec<JsString> = Vec::new();
            for part in parts {
                if part == "**" && output.last().is_some_and(|last| last == "**") {
                    continue;
                }
                if part == ".."
                    && let Some(previous) = output.last()
                    && previous != ".."
                    && previous != "."
                    && previous != "**"
                {
                    output.pop();
                    continue;
                }
                output.push(part);
            }
            if output.is_empty() {
                vec![JsString::default()]
            } else {
                output
            }
        })
        .collect()
}

fn slash_split(path: &JsString, preserve_multiple_slashes: bool) -> Vec<JsString> {
    if preserve_multiple_slashes {
        return path.split('/').collect();
    }
    let mut parts = Vec::new();
    let mut current = JsString::default();
    let mut previous_was_slash = false;
    for character in path.units().iter().copied() {
        if character == SLASH {
            if !previous_was_slash {
                parts.push(std::mem::take(&mut current));
                previous_was_slash = true;
            }
        } else {
            current.push_unit(character);
            previous_was_slash = false;
        }
    }
    parts.push(current);
    parts
}

impl Part {
    fn is_invalid(&self) -> bool {
        matches!(self, Self::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::{MinimatchOptions, brace_expand, minimatch};

    #[test]
    fn matches_simple_globs() {
        let options = MinimatchOptions::default();
        assert!(minimatch("a/b/c.js", "**/*.js", &options));
        assert!(!minimatch("a/b/c.ts", "**/*.js", &options));
        assert!(minimatch("a/b", "a/*", &options));
        assert!(!minimatch("a/.b", "a/*", &options));
        assert!(minimatch("a/.b", "a/.*", &options));
    }

    #[test]
    fn expands_braces() {
        let options = MinimatchOptions::default();
        assert_eq!(brace_expand("a{b,c}d", &options), vec!["abd", "acd"]);
        assert_eq!(brace_expand("a{1..3}b", &options), vec!["a1b", "a2b", "a3b"]);
    }
}

impl From<u16> for JsString {
    fn from(unit: u16) -> Self {
        Self::from_utf16(vec![unit])
    }
}
impl FromIterator<u16> for JsString {
    fn from_iter<T: IntoIterator<Item = u16>>(units: T) -> Self {
        Self::from_utf16(units.into_iter().collect())
    }
}
impl<'a> FromIterator<&'a u16> for JsString {
    fn from_iter<T: IntoIterator<Item = &'a u16>>(units: T) -> Self {
        units.into_iter().copied().collect()
    }
}
impl FromIterator<JsString> for JsString {
    fn from_iter<T: IntoIterator<Item = JsString>>(parts: T) -> Self {
        Self::join(parts, "")
    }
}

mod units {
    pub(super) const NUL: u16 = 0;
    pub(super) const BANG: u16 = b'!' as u16;
    pub(super) const HASH: u16 = b'#' as u16;
    pub(super) const DOLLAR: u16 = b'$' as u16;
    pub(super) const OPEN_PAREN: u16 = b'(' as u16;
    pub(super) const CLOSE_PAREN: u16 = b')' as u16;
    pub(super) const STAR: u16 = b'*' as u16;
    pub(super) const PLUS: u16 = b'+' as u16;
    pub(super) const COMMA: u16 = b',' as u16;
    pub(super) const DASH: u16 = b'-' as u16;
    pub(super) const DOT: u16 = b'.' as u16;
    pub(super) const SLASH: u16 = b'/' as u16;
    pub(super) const ZERO: u16 = b'0' as u16;
    pub(super) const NINE: u16 = b'9' as u16;
    pub(super) const QUESTION: u16 = b'?' as u16;
    pub(super) const AT: u16 = b'@' as u16;
    pub(super) const UPPER_A: u16 = b'A' as u16;
    pub(super) const UPPER_Z: u16 = b'Z' as u16;
    pub(super) const OPEN_BRACKET: u16 = b'[' as u16;
    pub(super) const BACKSLASH: u16 = b'\\' as u16;
    pub(super) const CLOSE_BRACKET: u16 = b']' as u16;
    pub(super) const CARET: u16 = b'^' as u16;
    pub(super) const LOWER_A: u16 = b'a' as u16;
    pub(super) const LOWER_Z: u16 = b'z' as u16;
    pub(super) const OPEN_BRACE: u16 = b'{' as u16;
    pub(super) const PIPE: u16 = b'|' as u16;
    pub(super) const CLOSE_BRACE: u16 = b'}' as u16;
}
