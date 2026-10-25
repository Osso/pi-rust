#[path = "../examples/extensions/question.rs"]
mod example;
use crate::shared_extension_forms as harness;
use serde_json::json;

#[tokio::test]
async fn question_all_branches_keys_editor_renderers_and_widths_match() {
    let params = json!({"question":"Pick a database 🚀 with a long wrapped prompt", "options":[{"label":"SQLite","description":"Embedded and local"},{"label":"Postgres","description":"Shared server"}]});
    let scripts = vec![
        vec!["\r"],
        vec!["\x1b[A", "\x1b[A", "\x1b[B", "\r"],
        vec!["\x1b"],
        vec!["\x1b[B", "\x1b[B", "\x1b[B", "\x1b[B", "\r", "  custom 🚀  ", "\r"],
        vec!["\x1b[B", "\x1b[B", "\r", "\r", "\x1b", "\r"],
        vec!["\x1b[B", "\x1b[B", "\r", "discard", "\x1b", "\x1b[A", "\r"],
    ];
    for width in [1, 2, 7, 24, 80] {
        for keys in &scripts {
            harness::compare("question",example::question_extension,json!({"params":params,"widths":[width,40],"callWidth":width,"steps":keys.iter().map(|key|json!({"input":if width <= 2 { key.replace("🚀", "text") } else { (*key).to_owned() }})).collect::<Vec<_>>()})).await;
        }
    }
    for params in [params.clone(), json!({"question":"empty","options":[]})] {
        harness::compare(
            "question",
            example::question_extension,
            json!({"mode":"print","params":params,"widths":[80]}),
        )
        .await;
    }
    harness::compare(
        "question",
        example::question_extension,
        json!({"params":{"question":"empty","options":[]},"widths":[80]}),
    )
    .await;
    let result=harness::compare("question",example::question_extension,json!({"params":params,"widths":[30],"steps":[{"input":"\x1b[B"},{"input":"\x1b[B"},{"input":"\r"},{"inputUnits":[55296]},{"invalidate":true},{"input":"\r"}]})).await;
    assert_eq!(
        result["result"]["details"]["answer"],
        pi_js_compat::js_json!(pi_js_compat::js_json::JsString::from_utf16(vec![0xd800]))
    );
    harness::compare("question", example::question_extension, json!({"params":params,"widths":[24],"steps":[{"input":"\r"}],"renderCases":[{"content":[],"details":{"question":"lookup","options":["A"],"answer":"missing","wasCustom":false}}]})).await;
    harness::compare("question", example::question_extension, json!({"params":params,"widths":[1],"steps":[{"input":"\x1b[B"},{"input":"\x1b[B"},{"input":"\r"},{"input":"🚀"}]})).await;
}
