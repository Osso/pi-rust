//! Ported from `coding-agent/src/modes/interactive/components/session-selector-search.ts`.

use pi_js_compat::string::{is_js_whitespace_char, js_trim};
use pi_tui::fuzzy_match;

use crate::core::session_manager::SessionInfo;

/// `SortMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Threaded,
    Recent,
    Relevance,
}

/// `NameFilter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameFilter {
    All,
    Named,
}

/// One search token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchToken {
    pub kind: TokenKind,
    pub value: String,
}

/// Token kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Fuzzy,
    Phrase,
}

/// `ParsedSearchQuery`.
#[derive(Debug, Clone)]
pub struct ParsedSearchQuery {
    pub mode: SearchMode,
    pub tokens: Vec<SearchToken>,
    /// `None` when regex mode has no usable pattern.
    pub regex: Option<regex::Regex>,
    /// Parsing failed; the query is treated as non-matching.
    pub error: Option<String>,
}

/// Query mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Tokens,
    Regex,
}

/// `MatchResult`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchResult {
    pub matches: bool,
    /// Every token matched as a literal substring, not only as a subsequence.
    pub literal: bool,
    /// Lower is better; only meaningful when `matches`.
    pub score: f64,
}

impl MatchResult {
    const NO_MATCH: Self = Self {
        matches: false,
        literal: false,
        score: 0.0,
    };
}

fn normalize_whitespace_lower(text: &str) -> String {
    let mut collapsed = String::with_capacity(text.len());
    let mut pending_space = false;
    for character in text.chars() {
        if is_js_whitespace_char(character) {
            pending_space = !collapsed.is_empty();
            continue;
        }
        if pending_space {
            collapsed.push(' ');
            pending_space = false;
        }
        collapsed.extend(character.to_lowercase());
    }
    collapsed
}

fn get_session_search_text(session: &SessionInfo) -> String {
    format!(
        "{} {} {} {}",
        session.id,
        session.name.as_deref().unwrap_or(""),
        session.all_messages_text,
        session.cwd
    )
}

/// `hasSessionName`.
pub fn has_session_name(session: &SessionInfo) -> bool {
    session.name.as_deref().is_some_and(|name| !js_trim(name).is_empty())
}

/// `hasSessionMessages`.
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

fn split_js_whitespace(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = None;
    for (index, character) in text.char_indices() {
        if is_js_whitespace_char(character) {
            if let Some(start) = start.take() {
                parts.push(&text[start..index]);
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(start) = start {
        parts.push(&text[start..]);
    }
    parts
}

/// `parseSearchQuery`.
pub fn parse_search_query(query: &str) -> ParsedSearchQuery {
    let trimmed = js_trim(query);
    if trimmed.is_empty() {
        return ParsedSearchQuery {
            mode: SearchMode::Tokens,
            tokens: Vec::new(),
            regex: None,
            error: None,
        };
    }

    if let Some(rest) = trimmed.strip_prefix("re:") {
        let pattern = js_trim(rest);
        if pattern.is_empty() {
            return ParsedSearchQuery {
                mode: SearchMode::Regex,
                tokens: Vec::new(),
                regex: None,
                error: Some("Empty regex".to_string()),
            };
        }
        return match regex::RegexBuilder::new(pattern).case_insensitive(true).build() {
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
                error: Some(error.to_string()),
            },
        };
    }

    let mut tokens = Vec::new();
    let mut buffer = String::new();
    let mut in_quote = false;
    let mut had_unclosed_quote = false;

    for character in trimmed.chars() {
        if character == '"' {
            if in_quote {
                push_token(&mut tokens, &mut buffer, TokenKind::Phrase);
                in_quote = false;
            } else {
                push_token(&mut tokens, &mut buffer, TokenKind::Fuzzy);
                in_quote = true;
            }
            continue;
        }
        if !in_quote && is_js_whitespace_char(character) {
            push_token(&mut tokens, &mut buffer, TokenKind::Fuzzy);
            continue;
        }
        buffer.push(character);
    }

    if in_quote {
        had_unclosed_quote = true;
    }
    if had_unclosed_quote {
        return ParsedSearchQuery {
            mode: SearchMode::Tokens,
            tokens: split_js_whitespace(trimmed)
                .into_iter()
                .map(|token| SearchToken {
                    kind: TokenKind::Fuzzy,
                    value: token.to_string(),
                })
                .collect(),
            regex: None,
            error: None,
        };
    }

    push_token(&mut tokens, &mut buffer, TokenKind::Fuzzy);
    ParsedSearchQuery {
        mode: SearchMode::Tokens,
        tokens,
        regex: None,
        error: None,
    }
}

fn push_token(tokens: &mut Vec<SearchToken>, buffer: &mut String, kind: TokenKind) {
    let value = js_trim(buffer).to_string();
    buffer.clear();
    if value.is_empty() {
        return;
    }
    tokens.push(SearchToken { kind, value });
}

fn is_literal_token_query_match(parsed: &ParsedSearchQuery, text: &str) -> bool {
    if parsed.tokens.iter().all(|token| token.kind == TokenKind::Phrase) {
        return true;
    }
    if !parsed.tokens.iter().all(|token| token.kind == TokenKind::Fuzzy) {
        return false;
    }
    let query = normalize_whitespace_lower(
        &parsed
            .tokens
            .iter()
            .map(|token| token.value.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    );
    normalize_whitespace_lower(text).contains(&query)
}

fn utf16_index(text: &str, byte_index: usize) -> usize {
    text[..byte_index].encode_utf16().count()
}

fn match_token_query(text: &str, parsed: &ParsedSearchQuery) -> MatchResult {
    let mut total_score = 0.0;
    let mut normalized_text: Option<String> = None;

    for token in &parsed.tokens {
        if token.kind == TokenKind::Phrase {
            let normalized = normalized_text.get_or_insert_with(|| normalize_whitespace_lower(text));
            let phrase = normalize_whitespace_lower(&token.value);
            if phrase.is_empty() {
                continue;
            }
            let Some(index) = normalized.find(&phrase) else {
                return MatchResult::NO_MATCH;
            };
            total_score += utf16_index(normalized, index) as f64 * 0.1;
            continue;
        }

        let matched = fuzzy_match(&token.value, text);
        if !matched.matches {
            return MatchResult::NO_MATCH;
        }
        total_score += matched.score;
    }

    MatchResult {
        matches: true,
        literal: is_literal_token_query_match(parsed, text),
        score: total_score,
    }
}

/// `matchSession`.
pub fn match_session(session: &SessionInfo, parsed: &ParsedSearchQuery) -> MatchResult {
    let text = get_session_search_text(session);

    if parsed.mode == SearchMode::Regex {
        let Some(regex) = &parsed.regex else {
            return MatchResult::NO_MATCH;
        };
        let Some(found) = regex.find(&text) else {
            return MatchResult::NO_MATCH;
        };
        return MatchResult {
            matches: true,
            literal: true,
            score: utf16_index(&text, found.start()) as f64 * 0.1,
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

/// `filterAndSortSessions`.
pub fn filter_and_sort_sessions(
    sessions: &[SessionInfo],
    query: &str,
    sort_mode: SortMode,
    name_filter: NameFilter,
) -> Vec<SessionInfo> {
    let name_filtered: Vec<SessionInfo> = sessions
        .iter()
        .filter(|session| matches_name_filter(session, name_filter))
        .cloned()
        .collect();
    if js_trim(query).is_empty() {
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

    scored.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .reverse()
            .then(a.2.total_cmp(&b.2))
            .then(b.0.modified.cmp(&a.0.modified))
    });

    scored.into_iter().map(|(session, _, _)| session).collect()
}
