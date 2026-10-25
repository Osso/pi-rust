#[path = "../examples/extensions/plan_mode/index.rs"]
mod example;
#[path = "support/extension_examples_l_plan.rs"]
mod support;
use serde_json::{Value, json};
fn register(
    api: &pi_coding_agent::core::extensions::ExtensionApi,
    _: support::Trace,
) -> Result<(), pi_coding_agent::core::extensions::HandlerError> {
    example::plan_mode_extension(api)
}
fn assistant(text: &str) -> Value {
    json!({"role":"assistant","content":[{"type":"thinking","thinking":"hidden"},{"type":"text","text":text}],"api":"openai-completions","provider":"fixture","model":"faux","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"stop","timestamp":1})
}
#[tokio::test]
async fn planning_toggle_filter_safety_execution_and_completion_match() {
    let plan = assistant("Plan:\n1. Read the source files\n2. Verify the output tests");
    for choice in [
        Value::Null,
        json!("Stay in plan mode"),
        json!("Refine the plan"),
        json!("Execute the plan (track progress)"),
    ] {
        support::compare("plan-mode",register,json!({"fixed":choice.as_str().is_some_and(|c|c.starts_with("Execute")),"steps":[
   {"op":"event","event":{"type":"session_start","reason":"startup"}},
   {"op":"command","name":"todos"},{"op":"before"},
   {"op":"tool","name":"bash","input":{"command":"rm -rf scratch"}},
   {"op":"command","name":"plan"},{"op":"before"},
   {"op":"tool","name":"bash","input":{"command":"rm scratch"}},
   {"op":"tool","name":"bash","input":{"command":"ls -l"}},
   {"op":"tool","name":"read","input":{"path":"a"}},
   {"op":"context","messages":[{"role":"user","content":"[PLAN MODE ACTIVE] old","timestamp":0}]},
   {"op":"event","event":{"type":"agent_end","messages":[plan]},"selected":choice,"editor":"  sharpen the plan  "},
   {"op":"command","name":"todos"},{"op":"before"},
   {"op":"event","event":{"type":"turn_end","turnIndex":0,"message":assistant("[DONE:1]"),"toolResults":[]}},
   {"op":"command","name":"todos"},{"op":"before"},
   {"op":"event","event":{"type":"turn_end","turnIndex":1,"message":assistant("[DONE:2]"),"toolResults":[]}},
   {"op":"event","event":{"type":"agent_end","messages":[]}},
   {"op":"command","name":"todos"},
   {"op":"shortcut","key":"ctrl+alt+p"},{"op":"shortcut","key":"ctrl+alt+p"},
   {"op":"context","messages":[{"role":"user","content":"[PLAN MODE ACTIVE] old","timestamp":0},{"role":"user","content":[{"type":"text","text":"[PLAN MODE ACTIVE] stale"}],"timestamp":0},{"role":"user","content":"keep","timestamp":0},{"role":"custom","customType":"plan-mode-context","content":"stale","display":false,"timestamp":0},assistant("keep")]}
  ]})).await;
    }
}
#[tokio::test]
async fn restoring_execution_scans_only_after_marker_without_mutating_stored_todos() {
    let result=support::compare("plan-mode",register,json!({"fixed":true,"flags":{"plan":true},"entries":[
  {"type":"message","message":assistant("[DONE:2]")},
  {"type":"custom_message","customType":"plan-mode-execute","content":"go","display":true},
  {"type":"custom","customType":"plan-mode","data":{"enabled":false,"executing":true,"todos":[{"step":1,"text":"Read files","completed":false},{"step":2,"text":"Verify results","completed":false}]}},
  {"type":"message","message":assistant("[DONE:1]")}
 ],"steps":[{"op":"event","event":{"type":"session_start","reason":"startup"}},{"op":"command","name":"todos"},{"op":"before"}]})).await;
    assert_eq!(
        result["entries"][2]["data"]["todos"][0]["completed"].as_bool(),
        Some(false)
    );
}

#[tokio::test]
async fn truncated_astral_todo_retains_surrogate_in_ui_and_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let text = format!("Plan:\n1. {}🚀 trailing", "a".repeat(46));
    let result=support::compare("plan-mode",register,json!({"fixed":true,"checkRawPersistence":true,"sessionDir":dir.path(),"flags":{"plan":true},"entries":[{"type":"message","message":assistant("initial")}],"steps":[
 {"op":"event","event":{"type":"session_start","reason":"startup"}},
 {"op":"event","event":{"type":"agent_end","messages":[assistant(&text)]},"selected":"Execute the plan (track progress)"},
 {"op":"command","name":"todos"},
 {"op":"event","event":{"type":"session_start","reason":"resume"}},
 {"op":"before"},
 {"op":"event","event":{"type":"turn_end","turnIndex":0,"message":assistant("[DONE:1]"),"toolResults":[]}},
 {"op":"event","event":{"type":"agent_end","messages":[]}}
 ]})).await;
    assert_eq!(
        result["entries"][1]["data"]["todos"][0]["text"].as_string().unwrap(),
        &(pi_js_compat::js_json::JsString::from(format!("A{}", "a".repeat(45)))
            + pi_js_compat::js_json::JsString::from_utf16(vec![0xd83d])
            + "...")
    );
}
