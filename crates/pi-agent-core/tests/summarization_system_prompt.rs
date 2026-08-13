use pi_agent_core::harness::compaction::SUMMARIZATION_SYSTEM_PROMPT;

const EXPECTED_PROMPT: &str = "You are a context summarization assistant. Your task is to read a conversation between a user and an AI assistant, then produce a structured summary following the exact format specified.\n\nDo NOT continue the conversation. Do NOT respond to any questions in the conversation. ONLY output the structured summary.";
const EXPECTED_LENGTH: usize = 310;
const EXPECTED_LINE_FEEDS: usize = 2;
const EXPECTED_LINE_FEED_POSITIONS: [usize; EXPECTED_LINE_FEEDS] = [186, 187];

#[test]
fn preserves_the_exact_static_prompt_value() {
    assert_eq!(SUMMARIZATION_SYSTEM_PROMPT, EXPECTED_PROMPT);
}

#[test]
fn preserves_exact_ascii_length_and_line_structure() {
    assert_eq!(SUMMARIZATION_SYSTEM_PROMPT.len(), EXPECTED_LENGTH);
    assert_eq!(SUMMARIZATION_SYSTEM_PROMPT.chars().count(), EXPECTED_LENGTH);
    assert_eq!(SUMMARIZATION_SYSTEM_PROMPT.encode_utf16().count(), EXPECTED_LENGTH);
    assert_eq!(
        SUMMARIZATION_SYSTEM_PROMPT
            .chars()
            .filter(|character| *character == '\n')
            .count(),
        EXPECTED_LINE_FEEDS
    );
    assert_eq!(
        SUMMARIZATION_SYSTEM_PROMPT
            .char_indices()
            .filter_map(|(index, character)| (character == '\n').then_some(index))
            .collect::<Vec<_>>(),
        EXPECTED_LINE_FEED_POSITIONS
    );
    assert_eq!(SUMMARIZATION_SYSTEM_PROMPT.lines().count(), 3);
    assert_eq!(SUMMARIZATION_SYSTEM_PROMPT.split('\n').collect::<Vec<_>>()[1], "");
}

#[test]
fn contains_only_ascii_lf_separators_and_no_trailing_whitespace() {
    assert!(SUMMARIZATION_SYSTEM_PROMPT.is_ascii());
    assert!(!SUMMARIZATION_SYSTEM_PROMPT.contains('\r'));
    assert!(!SUMMARIZATION_SYSTEM_PROMPT.ends_with('\n'));
    assert!(
        SUMMARIZATION_SYSTEM_PROMPT.lines().all(|line| {
            line.is_empty() || line.chars().last().map_or(true, |character| !character.is_whitespace())
        })
    );
}

#[test]
fn is_usable_as_a_compile_time_constant() {
    const PROMPT: &str = SUMMARIZATION_SYSTEM_PROMPT;

    assert_eq!(PROMPT, EXPECTED_PROMPT);
}
