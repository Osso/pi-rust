//! Word navigation over ICU word segments.
//!
//! Cursor positions and segments retain native UTF-16 units, including cursors
//! inside a surrogate pair.

use crate::text_units::scalar_values;
use icu_segmenter::WordSegmenter;
use icu_segmenter::options::{WordBreakInvariantOptions, WordType};
use pi_js_compat::js_json::JsString;

use crate::unicode_tables::is_mark;
use crate::utils::{is_punctuation_char_value, is_whitespace_char};

/// One word segment, mirroring `Intl.SegmentData`.
///
/// `index` and `input` are omitted: the algorithms only consume the segment
/// text and `isWordLike`, and both are derivable from the surrounding slice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordSegment {
    pub segment: JsString,
    pub is_word_like: bool,
}

/// Custom segmenter callback.
pub type SegmentCallback<'a> = &'a dyn Fn(&JsString) -> Vec<WordSegment>;

/// Options for word navigation functions.
///
/// When omitted, ICU word segmentation (the engine behind
/// `Intl.Segmenter`) supplies the default segments.
#[derive(Default)]
pub struct WordNavigationOptions<'a> {
    /// Custom segmenter returning word segments for the given text.
    pub segment: Option<SegmentCallback<'a>>,
    /// Predicate identifying atomic segments treated as single units.
    pub is_atomic_segment: Option<&'a dyn Fn(&JsString) -> bool>,
}

/// Find the cursor position after moving one word backward from `cursor`.
pub fn find_word_backward(text: &JsString, cursor: usize, options: Option<&WordNavigationOptions<'_>>) -> usize {
    let cursor = cursor.min(text.len());
    if cursor == 0 {
        return 0;
    }

    let text_before_cursor = &text.slice(0, cursor);
    let is_atomic = options.and_then(|options| options.is_atomic_segment);
    let mut segments = segment_text(text_before_cursor, options);
    let mut new_cursor = cursor;

    while let Some(last) = segments.last() {
        if is_atomic_segment(is_atomic, &last.segment) || !is_whitespace_char(&last.segment) {
            break;
        }
        new_cursor -= last.segment.len();
        segments.pop();
    }

    let Some(last) = segments.last() else {
        return new_cursor;
    };

    if is_atomic_segment(is_atomic, &last.segment) {
        new_cursor -= last.segment.len();
    } else if last.is_word_like {
        new_cursor -= word_backward_offset(&last.segment);
    } else {
        while let Some(segment) = segments.last() {
            if is_atomic_segment(is_atomic, &segment.segment)
                || segment.is_word_like
                || is_whitespace_char(&segment.segment)
            {
                break;
            }
            new_cursor -= segment.segment.len();
            segments.pop();
        }
    }

    new_cursor
}

/// Find the cursor position after moving one word forward from `cursor`.
pub fn find_word_forward(text: &JsString, cursor: usize, options: Option<&WordNavigationOptions<'_>>) -> usize {
    let cursor = cursor.min(text.len());
    if cursor >= text.len() {
        return text.len();
    }

    let text_after_cursor = &text.slice(cursor, text.len());
    let is_atomic = options.and_then(|options| options.is_atomic_segment);
    let segments = segment_text(text_after_cursor, options);
    let mut iterator = segments.iter();
    let mut new_cursor = cursor;

    let mut next = iterator.next();
    while let Some(segment) = next {
        if is_atomic_segment(is_atomic, &segment.segment) || !is_whitespace_char(&segment.segment) {
            break;
        }
        new_cursor += segment.segment.len();
        next = iterator.next();
    }

    let Some(segment) = next else {
        return new_cursor;
    };

    if is_atomic_segment(is_atomic, &segment.segment) {
        new_cursor += segment.segment.len();
    } else if segment.is_word_like {
        new_cursor += word_forward_offset(&segment.segment);
    } else {
        let mut run = Some(segment);
        while let Some(segment) = run {
            if is_atomic_segment(is_atomic, &segment.segment)
                || segment.is_word_like
                || is_whitespace_char(&segment.segment)
            {
                break;
            }
            new_cursor += segment.segment.len();
            run = iterator.next();
        }
    }

    new_cursor
}

fn segment_text(text: &JsString, options: Option<&WordNavigationOptions<'_>>) -> Vec<WordSegment> {
    match options.and_then(|options| options.segment) {
        Some(segment) => segment(text),
        None => icu_segments(text),
    }
}

fn icu_segments(text: &JsString) -> Vec<WordSegment> {
    let segmenter = WordSegmenter::new_auto(WordBreakInvariantOptions::default());
    let mut iterator = segmenter.segment_utf16(text.units());
    let mut start = 0;
    let mut segments: Vec<WordSegment> = Vec::new();

    while let Some(end) = iterator.next() {
        if end > start {
            let segment = text.slice(start, end);
            let word_type = iterator.word_type();
            // ICU4X tags ExtendNumLet/Extend runs (for example a lone
            // underscore) as Letter; ICU4C reports WORD_NONE unless the
            // segment contains an actual letter or a number.
            let is_word_like = word_type.is_word_like()
                && (matches!(word_type, WordType::Number) || scalar_values(&segment).any(char::is_alphabetic));
            // ICU4X splits combining marks after Han where ICU4C (Node) keeps
            // them attached (UAX #29 WB4). Merge those mark runs back, except
            // after a line break where WB3 keeps the break.
            if scalar_values(&segment).next().is_some_and(is_mark)
                && let Some(previous) = segments.last_mut()
                && !previous
                    .segment
                    .units()
                    .last()
                    .is_some_and(|unit| matches!(unit, 13 | 10 | 0x2028 | 0x2029))
            {
                previous.segment.push_str(&segment);
            } else {
                segments.push(WordSegment {
                    segment: segment.clone(),
                    is_word_like,
                });
            }
        }
        start = end;
    }

    segments
}

/// Segment text with the ICU word segmenter used by `Intl.Segmenter`.
pub fn word_segments(text: &JsString) -> Vec<WordSegment> {
    icu_segments(text)
}

fn is_atomic_segment(is_atomic: Option<&dyn Fn(&JsString) -> bool>, segment: &JsString) -> bool {
    is_atomic.is_some_and(|is_atomic| is_atomic(segment))
}

/// Native-unit distance to the last ASCII punctuation boundary.
fn word_backward_offset(segment: &JsString) -> usize {
    segment
        .units()
        .iter()
        .rposition(|unit| is_punctuation_unit(*unit))
        .map_or(segment.len(), |index| segment.len() - index - 1)
}

fn word_forward_offset(segment: &JsString) -> usize {
    segment
        .units()
        .iter()
        .position(|unit| is_punctuation_unit(*unit))
        .unwrap_or(segment.len())
}

fn is_punctuation_unit(unit: u16) -> bool {
    char::from_u32(u32::from(unit)).is_some_and(is_punctuation_char_value)
}
