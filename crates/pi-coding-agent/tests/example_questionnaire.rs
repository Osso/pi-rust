#[path = "../examples/extensions/questionnaire.rs"]
mod example;
use crate::shared_extension_forms as harness;
use serde_json::json;

#[tokio::test]
async fn empty_options_down_enter_are_no_ops_and_escape_cancels() {
    let actual = harness::compare(
        "questionnaire",
        example::questionnaire_extension,
        json!({
            "params":{"questions":[{"id":"none","prompt":"No choices","options":[],"allowOther":false}]},
            "widths":[24,40],
            "steps":[{"input":"\x1b[B"},{"input":"\r"},{"input":"\x1b"}]
        }),
    )
    .await;
    assert_eq!(
        actual["result"]["details"],
        pi_js_compat::js_json!({"questions":[{"id":"none","prompt":"No choices","options":[],"allowOther":false,"label":"Q1"}],"answers":[],"cancelled":true})
    );
    assert_eq!(
        actual["result"]["content"],
        pi_js_compat::js_json!([{"type":"text","text":"User cancelled the questionnaire"}])
    );
    let frames = actual["renders"].as_array().unwrap();
    assert_eq!(frames.len(), 4);
    assert!(frames.iter().all(|frame| frame == &frames[0]));
}

#[tokio::test]
async fn questionnaire_single_multi_answers_tabs_cancel_rendering_and_defaults_match() {
    let one = json!({"id":"storage","prompt":"Storage 🚀 long prompt with wrapping","options":[{"value":"sqlite","label":"SQLite","description":"Embedded and local"}]});
    let scripts = vec![
        vec!["\r"],
        vec!["\x1b"],
        vec!["\x1b[B", "\x1b[B", "\r", "\r"],
        vec!["\x1b[B", "\r", "custom 🚀", "\r"],
        vec!["\x1b[B", "\r", "discard", "\x1b", "\x1b[A", "\r"],
    ];
    for width in [1, 2, 7, 24, 80] {
        for keys in &scripts {
            harness::compare("questionnaire",example::questionnaire_extension,json!({"params":{"questions":[one]},"widths":[width,40],"callWidth":width,"steps":keys.iter().map(|key|json!({"input":if width <= 2 {key.replace("🚀", "text")}else{(*key).to_owned()}})).collect::<Vec<_>>()})).await;
        }
    }
    let two = json!({"questions":[one,{"id":"priority","label":"Priority","prompt":"Pick urgency","allowOther":false,"options":[{"value":"now","label":"Now"},{"value":"later","label":"Later"}]}]});
    for keys in [
        vec!["\r", "\r", "\r"],
        vec!["\t", "\t", "\r", "\x1b"],
        vec!["\x1b[D", "\x1b[Z", "\r", "\t", "\x1b[B", "\r", "\x1b", "\x1b"],
        vec!["\r", "\x1b[B", "\r", "\t", "\x1b[B", "\r", "changed", "\r", "\t", "\r"],
    ] {
        harness::compare("questionnaire",example::questionnaire_extension,json!({"params":two,"widths":[32,80],"steps":keys.iter().map(|key|json!({"input":key})).collect::<Vec<_>>()})).await;
    }
    harness::compare(
        "questionnaire",
        example::questionnaire_extension,
        json!({"mode":"rpc","params":two,"widths":[80]}),
    )
    .await;
    harness::compare(
        "questionnaire",
        example::questionnaire_extension,
        json!({"params":{"questions":[]},"widths":[80]}),
    )
    .await;
    let result=harness::compare("questionnaire",example::questionnaire_extension,json!({"params":{"questions":[one]},"widths":[30],"steps":[{"input":"\x1b[B"},{"input":"\r"},{"inputUnits":[55296]},{"invalidate":true},{"input":"\r"}]})).await;
    assert_eq!(
        result["result"]["details"]["answers"][0]["label"],
        pi_js_compat::js_json!(pi_js_compat::js_json::JsString::from_utf16(vec![0xd800]))
    );
    harness::compare("questionnaire", example::questionnaire_extension, json!({"params":{"questions":[one]},"widths":[24],"steps":[{"input":"\r"}],"renderCases":[{"content":[],"details":{"questions":[],"answers":[{"id":"manual","value":"v","label":"No index","wasCustom":false}],"cancelled":false}}]})).await;
    harness::compare("questionnaire", example::questionnaire_extension, json!({"params":{"questions":[one]},"widths":[1],"steps":[{"input":"\x1b[B"},{"input":"\r"},{"input":"🚀"}],"sourceError":{"name":"TypeError","message":"Cannot read properties of null (reading 'cancelled')"}})).await;
}
