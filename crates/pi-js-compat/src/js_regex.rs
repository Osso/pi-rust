//! ECMAScript RegExp over raw UTF-16 strings (Unicode and legacy UCS-2 modes).
use std::ops::Range;
use std::sync::Arc;

use crate::js_json::JsString;

#[derive(Clone, Debug)]
pub struct JsRegex {
    regex: regress::Regex,
    unicode: bool,
}

/// Match offsets are UTF-16 code-unit offsets into the original native input.
#[derive(Clone, Debug)]
pub struct JsMatch {
    input: Arc<JsString>,
    range: Range<usize>,
}

impl JsMatch {
    pub fn start(&self) -> usize {
        self.range.start
    }
    pub fn end(&self) -> usize {
        self.range.end
    }
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }
    pub fn text(&self) -> JsString {
        self.input.slice(self.start(), self.end())
    }
}

#[derive(Clone, Debug)]
pub struct JsCaptures {
    input: Arc<JsString>,
    matched: regress::Match,
}

impl JsCaptures {
    pub fn get(&self, index: usize) -> Option<JsMatch> {
        self.matched.group(index).map(|range| JsMatch {
            input: self.input.clone(),
            range,
        })
    }

    pub fn name(&self, name: impl Into<JsString>) -> Option<JsMatch> {
        let name = name.into();
        let (_, range) = self.matched.named_groups().find(|(group, _)| name == *group)?;
        range.map(|range| JsMatch {
            input: self.input.clone(),
            range,
        })
    }

    /// Includes the whole-match group at index zero and unmatched groups.
    pub fn len(&self) -> usize {
        self.matched.captures.len() + 1
    }
    pub fn is_empty(&self) -> bool {
        false
    }
}

impl JsRegex {
    pub fn new(pattern: impl Into<JsString>) -> Result<Self, regress::Error> {
        Self::with_flags(pattern, "")
    }
    pub fn unicode(pattern: impl Into<JsString>) -> Result<Self, regress::Error> {
        Self::with_flags(pattern, "u")
    }

    pub fn with_flags(pattern: impl Into<JsString>, flags: &str) -> Result<Self, regress::Error> {
        let pattern = pattern.into();
        let unicode = flags.contains('u');
        let points: Vec<u32> = if unicode {
            char::decode_utf16(pattern.units().iter().copied())
                .map(|point| {
                    point
                        .map(u32::from)
                        .unwrap_or_else(|error| u32::from(error.unpaired_surrogate()))
                })
                .collect()
        } else {
            pattern.units().iter().copied().map(u32::from).collect()
        };
        let regex = regress::Regex::from_unicode(points.into_iter(), flags)?;
        Ok(Self { regex, unicode })
    }

    pub fn is_match(&self, text: impl Into<JsString>) -> bool {
        let text = text.into();
        if self.unicode {
            self.regex.find_from_utf16(text.units(), 0).next().is_some()
        } else {
            self.regex.find_from_ucs2(text.units(), 0).next().is_some()
        }
    }

    pub fn find(&self, text: impl Into<JsString>) -> Option<JsMatch> {
        self.find_from(text, 0)
    }
    pub fn find_from(&self, text: impl Into<JsString>, start: usize) -> Option<JsMatch> {
        self.captures_from(text, start)?.get(0)
    }

    pub fn captures(&self, text: impl Into<JsString>) -> Option<JsCaptures> {
        self.captures_from(text, 0)
    }
    pub fn captures_from(&self, text: impl Into<JsString>, start: usize) -> Option<JsCaptures> {
        let input = Arc::new(text.into());
        let start = self.normalize_start(&input, start);
        let matched = if self.unicode {
            self.regex.find_from_utf16(input.units(), start).next()
        } else {
            self.regex.find_from_ucs2(input.units(), start).next()
        }?;
        Some(JsCaptures { input, matched })
    }

    pub fn captures_iter(&self, text: impl Into<JsString>) -> std::vec::IntoIter<JsCaptures> {
        let input = Arc::new(text.into());
        let matches: Vec<_> = if self.unicode {
            self.regex.find_from_utf16(input.units(), 0).collect()
        } else {
            self.regex.find_from_ucs2(input.units(), 0).collect()
        };
        matches
            .into_iter()
            .map(|matched| JsCaptures {
                input: input.clone(),
                matched,
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    pub fn find_iter(&self, text: impl Into<JsString>) -> impl Iterator<Item = JsMatch> {
        self.captures_iter(text).filter_map(|captures| captures.get(0))
    }

    pub fn replace_all(&self, text: impl Into<JsString>, replacement: impl Into<JsString>) -> JsString {
        let replacement = replacement.into();
        self.replace_all_with(text, |captures| expand_replacement(&replacement, captures))
    }

    pub fn replace_all_with(
        &self,
        text: impl Into<JsString>,
        mut replace: impl FnMut(&JsCaptures) -> JsString,
    ) -> JsString {
        let text = text.into();
        let mut result = JsString::default();
        let mut previous_end = 0;
        for captures in self.captures_iter(&text) {
            let range = &captures.matched.range;
            result.push_str(text.slice(previous_end, range.start));
            result.push_str(replace(&captures));
            previous_end = range.end;
        }
        result + text.slice(previous_end, text.len())
    }

    fn normalize_start(&self, text: &JsString, start: usize) -> usize {
        let inside_pair = start > 0
            && start < text.len()
            && (0xdc00..=0xdfff).contains(&text.units()[start])
            && (0xd800..=0xdbff).contains(&text.units()[start - 1]);
        if self.unicode && inside_pair { start - 1 } else { start }
    }
}

fn expand_replacement(replacement: &JsString, captures: &JsCaptures) -> JsString {
    let mut output = JsString::default();
    let mut index = 0;
    while index < replacement.len() {
        if replacement.units()[index] != u16::from(b'$') {
            output.push_unit(replacement.units()[index]);
            index += 1;
            continue;
        }
        match expand_substitution(replacement, index, captures) {
            Some((text, consumed)) => {
                output.push_str(text);
                index += consumed;
            }
            None => {
                output.push_unit(u16::from(b'$'));
                index += 1;
            }
        }
    }
    output
}

fn expand_substitution(replacement: &JsString, index: usize, captures: &JsCaptures) -> Option<(JsString, usize)> {
    let next = *replacement.units().get(index + 1)?;
    let matched = &captures.matched.range;
    match next {
        36 => Some((JsString::from("$"), 2)),
        38 => Some((captures.input.slice(matched.start, matched.end), 2)),
        96 => Some((captures.input.slice(0, matched.start), 2)),
        39 => Some((captures.input.slice(matched.end, captures.input.len()), 2)),
        48..=57 => expand_numbered_group(replacement, index, captures),
        60 => expand_named_group(replacement, index, captures),
        _ => None,
    }
}

fn expand_numbered_group(replacement: &JsString, index: usize, captures: &JsCaptures) -> Option<(JsString, usize)> {
    let first = usize::from(replacement.units()[index + 1] - u16::from(b'0'));
    let second = replacement
        .units()
        .get(index + 2)
        .copied()
        .filter(|unit| (48..=57).contains(unit));
    let two_digits = second.map(|second| first * 10 + usize::from(second - 48));
    let valid_pair = two_digits.filter(|number| *number > 0 && *number < captures.len());
    let (number, consumed) = match valid_pair {
        Some(number) => (number, 3),
        None if first > 0 && first < captures.len() => (first, 2),
        None => return None,
    };
    Some((
        captures.get(number).map(|group| group.text()).unwrap_or_default(),
        consumed,
    ))
}

fn expand_named_group(replacement: &JsString, index: usize, captures: &JsCaptures) -> Option<(JsString, usize)> {
    captures.matched.named_groups().next()?;
    let end = replacement.find_from(">", index + 2)?;
    let name = replacement.slice(index + 2, end);
    let text = captures.name(name).map(|group| group.text()).unwrap_or_default();
    Some((text, end + 1 - index))
}
