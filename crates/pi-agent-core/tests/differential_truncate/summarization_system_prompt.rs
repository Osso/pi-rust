use pi_agent_core::harness::compaction::SUMMARIZATION_SYSTEM_PROMPT;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const REQUEST_ID: u64 = 13_129;

pub(crate) fn compare(oracle: &mut TsOracle) {
    let request = json!({
        "id": REQUEST_ID,
        "operation": "summarization_system_prompt",
    });
    let expected = oracle.request(&request);
    let actual = ok_response(REQUEST_ID, normalize_prompt(SUMMARIZATION_SYSTEM_PROMPT));
    assert_eq!(actual, expected, "summarization system prompt mismatch");
}

fn normalize_prompt(value: &str) -> Value {
    let line_feed_positions = value
        .char_indices()
        .filter_map(|(index, character)| (character == '\n').then_some(index))
        .collect::<Vec<_>>();
    json!({
        "value": value,
        "utf8Bytes": value.len(),
        "utf16Units": value.encode_utf16().count(),
        "lineFeedPositions": line_feed_positions,
        "hasCarriageReturn": value.contains('\r'),
        "endsWithLineFeed": value.ends_with('\n'),
    })
}
