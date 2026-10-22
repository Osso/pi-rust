//! Ported from `packages/coding-agent/src/modes/interactive/components/session-selector-search.ts`.

use pi_js_compat::string::is_js_whitespace_unit;
use pi_js_compat::{js_format, js_json::JsString, js_regex::JsRegex};
use pi_tui::fuzzy::fuzzy_match;

use crate::core::session_manager::SessionInfo;

/// Session ordering mode in the selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortMode {
    Threaded,
    Recent,
    Relevance,
}

/// Session name filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameFilter {
    All,
    Named,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchTokenKind {
    Fuzzy,
    Phrase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchToken {
    pub kind: SearchTokenKind,
    pub value: JsString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMode {
    Tokens,
    Regex,
}

#[derive(Clone, Debug)]
pub struct ParsedSearchQuery {
    pub mode: SearchMode,
    pub tokens: Vec<SearchToken>,
    pub regex: Option<JsRegex>,
    /// Parsing failed; the query matches nothing.
    pub error: Option<JsString>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatchResult {
    pub matches: bool,
    /// Every token matched as a literal substring rather than only as a subsequence.
    pub literal: bool,
    /// Lower is better; only meaningful when `matches`.
    pub score: f64,
}

fn normalize_whitespace_lower(text: &JsString) -> JsString {
    let mut normalized = JsString::default();
    let mut pending_space = false;
    for character in text.to_lowercase().units().iter().copied() {
        if is_js_whitespace_unit(character) {
            pending_space = !normalized.is_empty();
        } else {
            if pending_space {
                normalized.push_unit(0x20);
                pending_space = false;
            }
            normalized.push_unit(character);
        }
    }
    normalized
}

fn split_js_whitespace(text: &JsString) -> Vec<JsString> {
    text.units()
        .split(|unit| is_js_whitespace_unit(*unit))
        .filter(|part| !part.is_empty())
        .map(|part| JsString::from_utf16(part.to_vec()))
        .collect()
}

fn get_session_search_text(session: &SessionInfo) -> JsString {
    js_format!(
        "{} {} {} {}",
        session.id,
        session.name.as_ref().cloned().unwrap_or_default(),
        session.first_message,
        session.cwd
    )
}

pub fn has_session_name(session: &SessionInfo) -> bool {
    session.name.as_ref().is_some_and(|name| !name.trim().is_empty())
}

pub fn has_session_messages(session: &SessionInfo) -> bool {
    session.message_count > 0
}

fn matches_name_filter(session: &SessionInfo, filter: NameFilter) -> bool {
    if !has_session_messages(session) {
        return false;
    }
    match filter {
        NameFilter::All => true,
        NameFilter::Named => has_session_name(session),
    }
}

pub fn parse_search_query(query: &JsString) -> ParsedSearchQuery {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return ParsedSearchQuery {
            mode: SearchMode::Tokens,
            tokens: Vec::new(),
            regex: None,
            error: None,
        };
    }

    if let Some(rest) = trimmed.strip_prefix("re:") {
        let pattern = rest.trim();
        if pattern.is_empty() {
            return ParsedSearchQuery {
                mode: SearchMode::Regex,
                tokens: Vec::new(),
                regex: None,
                error: Some("Empty regex".into()),
            };
        }
        return match JsRegex::with_flags(pattern, "i") {
            Ok(regex) => ParsedSearchQuery {
                mode: SearchMode::Regex,
                tokens: Vec::new(),
                regex: Some(regex),
                error: None,
            },
            Err(error) => ParsedSearchQuery {
                mode: SearchMode::Regex,
                tokens: Vec::new(),
                regex: None,
                error: Some(error.message().clone()),
            },
        };
    }

    let mut tokens: Vec<SearchToken> = Vec::new();
    let mut buffer = JsString::default();
    let mut in_quote = false;
    let mut had_unclosed_quote = false;

    fn flush(tokens: &mut Vec<SearchToken>, buffer: &mut JsString, kind: SearchTokenKind) {
        let value = buffer.trim().to_owned();
        *buffer = JsString::default();
        if value.is_empty() {
            return;
        }
        tokens.push(SearchToken { kind, value });
    }

    for character in trimmed.units().iter().copied() {
        if character == 0x22 {
            if in_quote {
                flush(&mut tokens, &mut buffer, SearchTokenKind::Phrase);
                in_quote = false;
            } else {
                flush(&mut tokens, &mut buffer, SearchTokenKind::Fuzzy);
                in_quote = true;
            }
            continue;
        }

        if !in_quote && is_js_whitespace_unit(character) {
            flush(&mut tokens, &mut buffer, SearchTokenKind::Fuzzy);
            continue;
        }

        buffer.push_unit(character);
    }

    if in_quote {
        had_unclosed_quote = true;
    }

    if had_unclosed_quote {
        let tokens = split_js_whitespace(&trimmed)
            .into_iter()
            .map(|token| token.trim())
            .filter(|token| !token.is_empty())
            .map(|token| SearchToken {
                kind: SearchTokenKind::Fuzzy,
                value: token.to_owned(),
            })
            .collect();
        return ParsedSearchQuery {
            mode: SearchMode::Tokens,
            tokens,
            regex: None,
            error: None,
        };
    }

    flush(
        &mut tokens,
        &mut buffer,
        if in_quote {
            SearchTokenKind::Phrase
        } else {
            SearchTokenKind::Fuzzy
        },
    );

    ParsedSearchQuery {
        mode: SearchMode::Tokens,
        tokens,
        regex: None,
        error: None,
    }
}

fn is_literal_token_query_match(parsed: &ParsedSearchQuery, text: &JsString) -> bool {
    let all_phrases = parsed.tokens.iter().all(|token| token.kind == SearchTokenKind::Phrase);
    if all_phrases {
        return true;
    }

    let all_fuzzy = parsed.tokens.iter().all(|token| token.kind == SearchTokenKind::Fuzzy);
    if !all_fuzzy {
        return false;
    }

    let query = normalize_whitespace_lower(&JsString::join(parsed.tokens.iter().map(|token| &token.value), " "));
    normalize_whitespace_lower(text).contains(&query)
}

fn match_token_query(text: &JsString, parsed: &ParsedSearchQuery) -> MatchResult {
    let mut total_score = 0.0;
    let mut normalized_text: Option<JsString> = None;

    for token in &parsed.tokens {
        if token.kind == SearchTokenKind::Phrase {
            let normalized = normalized_text.get_or_insert_with(|| normalize_whitespace_lower(text));
            let phrase = normalize_whitespace_lower(&token.value);
            if phrase.is_empty() {
                continue;
            }
            let Some(index) = normalized.find(&phrase) else {
                return MatchResult {
                    matches: false,
                    literal: false,
                    score: 0.0,
                };
            };
            total_score += index as f64 * 0.1;
            continue;
        }

        let matched = fuzzy_match(&token.value, text);
        if !matched.matches {
            return MatchResult {
                matches: false,
                literal: false,
                score: 0.0,
            };
        }
        total_score += matched.score;
    }

    MatchResult {
        matches: true,
        literal: is_literal_token_query_match(parsed, text),
        score: total_score,
    }
}

pub fn match_session(session: &SessionInfo, parsed: &ParsedSearchQuery) -> MatchResult {
    let text = get_session_search_text(session);

    if parsed.mode == SearchMode::Regex {
        let Some(regex) = &parsed.regex else {
            return MatchResult {
                matches: false,
                literal: false,
                score: 0.0,
            };
        };
        let Some(found) = regex.find(&text) else {
            return MatchResult {
                matches: false,
                literal: false,
                score: 0.0,
            };
        };
        return MatchResult {
            matches: true,
            literal: true,
            score: found.start() as f64 * 0.1,
        };
    }

    if parsed.tokens.is_empty() {
        return MatchResult {
            matches: true,
            literal: true,
            score: 0.0,
        };
    }

    match_token_query(&text, parsed)
}

pub fn filter_and_sort_sessions(
    sessions: &[SessionInfo],
    query: &JsString,
    sort_mode: SortMode,
    name_filter: NameFilter,
) -> Vec<SessionInfo> {
    let name_filtered: Vec<SessionInfo> = sessions
        .iter()
        .filter(|session| matches_name_filter(session, name_filter))
        .cloned()
        .collect();
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return name_filtered;
    }

    let parsed = parse_search_query(query);
    if parsed.error.is_some() {
        return Vec::new();
    }

    if sort_mode == SortMode::Recent {
        let mut literal_matches = Vec::new();
        let mut fuzzy_matches = Vec::new();
        for session in name_filtered {
            let result = match_session(&session, &parsed);
            if !result.matches {
                continue;
            }
            if result.literal {
                literal_matches.push(session);
            } else {
                fuzzy_matches.push(session);
            }
        }
        literal_matches.extend(fuzzy_matches);
        return literal_matches;
    }

    let mut scored: Vec<(SessionInfo, bool, f64)> = Vec::new();
    for session in name_filtered {
        let result = match_session(&session, &parsed);
        if !result.matches {
            continue;
        }
        scored.push((session, result.literal, result.score));
    }

    pi_js_compat::array_sort::js_array_sort(&mut scored, |left, right| {
        if left.1 != right.1 {
            return if left.1 { -1.0 } else { 1.0 };
        }
        if left.2 != right.2 {
            return left.2 - right.2;
        }
        right.0.modified - left.0.modified
    });

    scored.into_iter().map(|(session, _, _)| session).collect()
}
