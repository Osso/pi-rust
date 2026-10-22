//! Fuzzy matching utilities.
//!
//! Matches if all query characters appear in order (not necessarily consecutive).
//! Lower score is better. Ported from `packages/tui/src/fuzzy.ts`; scoring runs
//! over UTF-16 code units so astral characters keep the TypeScript index weights.

use pi_js_compat::js_json::JsString;
use pi_js_compat::string::is_js_whitespace_unit;

/// Result of a fuzzy match attempt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuzzyMatch {
    pub matches: bool,
    pub score: f64,
}

const NO_MATCH: FuzzyMatch = FuzzyMatch {
    matches: false,
    score: 0.0,
};

/// Match `query` against `text` using the TypeScript algorithm.
pub fn fuzzy_match(query: impl Into<JsString>, text: impl Into<JsString>) -> FuzzyMatch {
    let query = query.into();
    let text = text.into();
    let query_lower = query.to_lowercase();
    let text_lower = text.to_lowercase();

    let primary_match = match_query(&query_lower, &text_lower);
    if primary_match.matches {
        return primary_match;
    }

    let Some(swapped_query) = swapped_alphanumeric(&query_lower) else {
        return primary_match;
    };

    let swapped_match = match_query(&swapped_query, &text_lower);
    if !swapped_match.matches {
        return primary_match;
    }

    FuzzyMatch {
        matches: true,
        score: swapped_match.score + 5.0,
    }
}

fn match_query(normalized_query: &JsString, text_lower: &JsString) -> FuzzyMatch {
    let query = normalized_query.units();
    let text = text_lower.units();

    if query.is_empty() {
        return FuzzyMatch {
            matches: true,
            score: 0.0,
        };
    }
    if query.len() > text.len() {
        return NO_MATCH;
    }

    let mut query_index = 0;
    let mut score = 0.0;
    // The TypeScript source starts this at -1, so the first match counts as
    // consecutive (lastMatchIndex === i - 1 at i === 0). Preserve that scoring.
    let mut last_match_index: i64 = -1;
    let mut consecutive_matches = 0;

    for (i, unit) in text.iter().enumerate() {
        if query_index >= query.len() {
            break;
        }
        if *unit != query[query_index] {
            continue;
        }

        let is_word_boundary = i == 0 || is_word_boundary_unit(text[i - 1]);
        let is_consecutive = last_match_index == i as i64 - 1;

        if is_consecutive {
            consecutive_matches += 1;
            score -= f64::from(consecutive_matches) * 5.0;
        } else {
            consecutive_matches = 0;
            if last_match_index >= 0 {
                score += (i as i64 - last_match_index - 1) as f64 * 2.0;
            }
        }

        if is_word_boundary {
            score -= 10.0;
        }

        score += i as f64 * 0.1;

        last_match_index = i as i64;
        query_index += 1;
    }

    if query_index < query.len() {
        return NO_MATCH;
    }

    if normalized_query == text_lower {
        score -= 100.0;
    }

    FuzzyMatch { matches: true, score }
}

fn is_word_boundary_unit(unit: u16) -> bool {
    matches!(unit, 0x20 | 0x2d | 0x5f | 0x2e | 0x2f | 0x3a) || is_js_whitespace_unit(unit)
}

fn swapped_alphanumeric(query_lower: &JsString) -> Option<JsString> {
    let bytes = query_lower.units();
    let letters_then_digits = bytes.iter().position(|byte| (48..=57).contains(byte));
    if let Some(split) = letters_then_digits
        && split > 0
        && bytes[..split].iter().all(|unit| (97..=122).contains(unit))
        && bytes[split..].iter().all(|unit| (48..=57).contains(unit))
    {
        return Some(query_lower.slice(split, query_lower.len()) + query_lower.slice(0, split));
    }

    let digits_then_letters = bytes.iter().position(|byte| (97..=122).contains(byte));
    if let Some(split) = digits_then_letters
        && split > 0
        && bytes[..split].iter().all(|unit| (48..=57).contains(unit))
        && bytes[split..].iter().all(|unit| (97..=122).contains(unit))
    {
        return Some(query_lower.slice(split, query_lower.len()) + query_lower.slice(0, split));
    }

    None
}

/// Filter and sort items by fuzzy match quality (best matches first).
///
/// Supports whitespace- and slash-separated tokens: all tokens must match.
/// Each token is scored against every candidate returned by `get_text`
/// independently; candidates are never concatenated.
pub fn fuzzy_filter<T: Clone, S: Into<JsString>>(
    items: &[T],
    query: impl Into<JsString>,
    get_text: impl Fn(&T) -> Vec<S>,
) -> Vec<T> {
    let trimmed_query = query.into().trim();
    if trimmed_query.is_empty() {
        return items.to_vec();
    }

    let tokens: Vec<JsString> = trimmed_query
        .units()
        .split(|unit| is_js_whitespace_unit(*unit) || *unit == u16::from(b'/'))
        .filter(|token| !token.is_empty())
        .map(|token| JsString::from_utf16(token.to_vec()))
        .collect();
    if tokens.is_empty() {
        return items.to_vec();
    }

    let mut results: Vec<(T, f64)> = Vec::new();
    for item in items {
        let candidates: Vec<JsString> = get_text(item).into_iter().map(Into::into).collect();
        let mut total_score = 0.0;
        let mut all_match = true;

        for token in &tokens {
            let best = best_token_match(token, &candidates);
            if best.matches {
                total_score += best.score;
            } else {
                all_match = false;
                break;
            }
        }

        if all_match {
            results.push((item.clone(), total_score));
        }
    }

    results.sort_by(|left, right| left.1.partial_cmp(&right.1).expect("fuzzy scores are finite"));
    results.into_iter().map(|(item, _)| item).collect()
}

fn best_token_match(token: &JsString, candidates: &[JsString]) -> FuzzyMatch {
    let mut best = NO_MATCH;
    for candidate in candidates {
        let matched = fuzzy_match(token, candidate);
        if matched.matches && (!best.matches || matched.score < best.score) {
            best = matched;
        }
    }
    best
}
