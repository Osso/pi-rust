use crate::shared_ai_oracle as oracle;

use pi_coding_agent::core::tools::edit_primitives::{
    Edit, apply_edits_to_normalized_content, generate_diff_string, generate_unified_patch,
};
use pi_js_compat::js_json::JsString;

#[test]
fn raw_word_diff_flags_and_values_match_frozen_dependency() {
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-edit-oracle.ts",
        "packages/coding-agent/src/core/tools/edit-diff.ts",
    );
    let high = JsString::from_utf16(vec![0xd800]);
    let low = JsString::from_utf16(vec![0xdfff]);
    let cases = [
        (JsString::from("a ") + &high + " b", JsString::from("a ") + &low + " b"),
        (JsString::from("😀x"), JsString::from("😀") + &high),
        (high.clone(), high + " — "),
    ];
    for (old, new) in cases {
        let reference = oracle.request(
            &serde_json::json!({"kind":"words","content":[],"oldText":old.units(),"newText":new.units(),"path":[]}),
        );
        let actual = pi_coding_agent::core::tools::jsdiff::diff_words(&old, &new)
            .into_iter()
            .map(|part| pi_js_compat::js_json!({"added":part.added,"removed":part.removed,"value":part.value}))
            .collect::<Vec<_>>();
        let expected = pi_js_compat::js_json::parse_lossless(reference["wire"].as_str().unwrap()).unwrap();
        assert_eq!(pi_js_compat::js_json::JsValue::Array(actual), expected);
    }
}

#[test]
fn raw_edits_diff_and_patch_match_frozen_coding_agent() {
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-edit-oracle.ts",
        "packages/coding-agent/src/core/tools/edit-diff.ts",
    );
    let path = JsString::from("file-") + JsString::from_utf16(vec![0xdfff]);
    let cases = [
        (
            JsString::from("😀"),
            JsString::from_utf16(vec![0xde00]),
            JsString::from("x"),
        ),
        (
            JsString::from("😀"),
            JsString::from_utf16(vec![0xd83d]),
            JsString::from("x"),
        ),
        (
            JsString::from("old"),
            JsString::from("old"),
            JsString::from_utf16(vec![0xd800]),
        ),
        (
            JsString::from("a\nb\n"),
            JsString::from("b"),
            JsString::from_utf16(vec![0xdfff, 0x0]),
        ),
    ];
    for (content, old_text, new_text) in cases {
        let reference = oracle.request(&serde_json::json!({"content":content.units(),"oldText":old_text.units(),"newText":new_text.units(),"path":path.units()}));
        let applied = apply_edits_to_normalized_content(&content, &[Edit { old_text, new_text }], &path).unwrap();
        let actual_diff = generate_diff_string(&applied.base_content, &applied.new_content, 4);
        let mut diff = pi_js_compat::js_json!({"diff":actual_diff.diff});
        if let Some(line) = actual_diff.first_changed_line {
            diff["firstChangedLine"] = pi_js_compat::js_json!(line);
        }
        let patch = generate_unified_patch(&path, &applied.base_content, &applied.new_content, 4);
        let actual = pi_js_compat::js_json!({"baseContent":applied.base_content,"newContent":applied.new_content,"diff":diff,"patch":patch});
        let expected = pi_js_compat::js_json::parse_lossless(reference["wire"].as_str().unwrap()).unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn an_edit_can_match_one_utf16_unit_inside_a_valid_surrogate_pair() {
    let content = JsString::from("😀");
    let edits = [Edit {
        old_text: JsString::from_utf16(vec![0xde00]),
        new_text: JsString::from("x"),
    }];
    let path = JsString::from("file");
    let result = apply_edits_to_normalized_content(&content, &edits, &path).unwrap();
    assert_eq!(result.new_content.units(), &[0xd83d, 0x78]);
    let diff = generate_diff_string(&result.base_content, &result.new_content, 4);
    assert!(diff.diff.contains(JsString::from_utf16(vec![0xd83d])));
    let patch = generate_unified_patch(&path, &result.base_content, &result.new_content, 4);
    assert!(patch.contains(JsString::from_utf16(vec![0xd83d])));
}

#[test]
fn raw_new_text_and_path_reach_diff_and_errors_without_utf8_projection() {
    let path = JsString::from_utf16(vec![0xdfff]);
    let replacement = JsString::from_utf16(vec![0xd800]);
    let edits = [Edit {
        old_text: "old".into(),
        new_text: replacement.clone(),
    }];
    let result = apply_edits_to_normalized_content(JsString::from("old"), &edits, &path).unwrap();
    assert_eq!(result.new_content, replacement);
    let missing = [Edit {
        old_text: "missing".into(),
        new_text: "replacement".into(),
    }];
    let error = apply_edits_to_normalized_content(JsString::from("old"), &missing, &path).unwrap_err();
    assert!(error.message.contains(&path));
}
