use crate::shared_oracle as oracle;
use pi_agent_core::harness::compaction::{find_cut_point, find_turn_start_index};
use pi_agent_core::harness::session::SessionTreeEntry;
use pi_js_compat::{
    js_json as json,
    js_json::{JsValue, parse_lossless},
    node_error::NativeNodeError,
    number::js_number_to_string,
};

fn number(text: &str) -> f64 {
    match text {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        other => other.parse().unwrap(),
    }
}
fn number_text(value: f64) -> String {
    if value == 0.0 && value.is_sign_negative() {
        "-0".into()
    } else {
        js_number_to_string(value)
    }
}
fn failure(error: NativeNodeError) -> JsValue {
    json!({"ok":false,"error":{"name":error.name,"message":error.message}})
}

#[test]
fn public_compaction_indices_match_all_js_number_branches() {
    let source = "packages/agent-core/src/harness/compaction/compaction.ts";
    let mut oracle = oracle::TsOracle::spawn("compaction-index-oracle.ts", source);
    let full = parse_lossless(r#"[{"type":"message","id":"u","parentId":null,"timestamp":"t","message":{"role":"user","content":"u","timestamp":0}},{"type":"thinking_level_change","id":"meta","parentId":"u","timestamp":"t","thinkingLevel":"high"},{"type":"custom_message","id":"custom","parentId":"meta","timestamp":"t","customType":"note","content":"context","display":false},{"type":"message","id":"tool","parentId":"custom","timestamp":"t","message":{"role":"toolResult","toolCallId":"c","toolName":"read","content":[],"isError":false,"timestamp":0}}]"#).unwrap();
    let metadata = JsValue::Array(vec![full[1].clone()]);
    for raw in [full, metadata, JsValue::Array(vec![])] {
        let entries = raw
            .as_array()
            .unwrap()
            .iter()
            .cloned()
            .map(|entry| SessionTreeEntry::from_value(entry).unwrap())
            .collect::<Vec<_>>();
        for entry in [
            "-Infinity",
            "-1",
            "-0",
            "0",
            "0.5",
            "1",
            "2",
            "3",
            "4",
            "Infinity",
            "NaN",
        ] {
            for start in ["-Infinity", "-1", "-0", "0", "0.5", "2", "4", "Infinity", "NaN"] {
                let input = json!({"operation":"turn","entries":raw,"entry":entry,"start":start});
                let expected = oracle.request(&input);
                let actual = find_turn_start_index(&entries, number(entry), number(start))
                    .map(|value| json!({"ok":true,"value":value,"number":number_text(value)}))
                    .unwrap_or_else(failure);
                assert_eq!(actual, expected, "turn entry={entry} start={start}");
            }
        }
        for start in [
            "-Infinity",
            "-1",
            "-0",
            "0",
            "0.5",
            "1",
            "2",
            "3",
            "4",
            "4.5",
            "Infinity",
            "NaN",
        ] {
            for end in [
                "-Infinity",
                "-1",
                "-0",
                "0",
                "0.5",
                "1",
                "1.5",
                "2",
                "2.5",
                "4",
                "4.5",
                "Infinity",
                "NaN",
            ] {
                let input = json!({"operation":"cut","entries":raw,"start":start,"end":end,"keep":"NaN"});
                let expected = oracle.request(&input);
                let actual = find_cut_point(&entries,number(start),number(end),f64::NAN)
                    .map(|value|json!({"ok":true,"value":{"firstKeptEntryIndex":value.first_kept_entry_index,"turnStartIndex":value.turn_start_index,"isSplitTurn":value.is_split_turn},"firstNumber":number_text(value.first_kept_entry_index)})).unwrap_or_else(failure);
                assert_eq!(actual, expected, "cut start={start} end={end}");
            }
        }
    }
}
