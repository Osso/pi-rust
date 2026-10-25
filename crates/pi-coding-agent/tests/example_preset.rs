#[path = "../examples/extensions/preset.rs"]
mod example;
#[path = "support/extension_examples_g.rs"]
mod harness;
use pi_coding_agent::core::extensions::*;
use serde_json::{Value, json};
use std::sync::Arc;
fn register(api: &ExtensionApi, trace: harness::Trace) -> Result<(), HandlerError> {
    example::register_with_logger(
        api,
        Arc::new(move |message| trace.lock().unwrap().push(json!(["logError", message]))),
    )
}
fn register_default(api: &ExtensionApi, _: harness::Trace) -> Result<(), HandlerError> {
    example::preset_extension(api)
}
fn model() -> Value {
    json!({"id":"faux","name":"Faux","api":"openai-completions","provider":"fixture","baseUrl":"https://fixture.invalid/v1","reasoning":true,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000,"maxTokens":1000})
}
fn start() -> Value {
    json!({"op":"event","event":{"type":"session_start","reason":"startup"}})
}
fn command(args: &str) -> Value {
    json!({"op":"command","name":"preset","args":args})
}
#[tokio::test]
async fn flags_commands_cycles_selector_models_prompts_persistence_and_errors_match() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let agent = std::path::PathBuf::from(pi_coding_agent::config::get_agent_dir().unwrap().to_utf8().unwrap());
    std::fs::create_dir_all(&agent).unwrap();
    let project = temp
        .path()
        .join(pi_coding_agent::config::config_dir_name().to_utf8().unwrap());
    std::fs::create_dir(&project).unwrap();
    let global = agent.join("presets.json");
    let local = project.join("presets.json");
    for steps in
        [json!([start(),command(""),{"op":"shortcut","key":"ctrl+shift+u"},command("missing"),{"op":"before"}])]
    {
        harness::compare("preset", register_default, json!({"cwd":cwd,"steps":steps})).await;
    }
    std::fs::write(&global,serde_json::to_string(&json!({"zeta":{"tools":["read"],"instructions":"global replaced"},"alpha":{"provider":"fixture","model":"faux","thinkingLevel":"high","tools":["read","missing","custom"],"instructions":"Plan carefully, preserve all of the original behavior."},"empty":{"tools":[]},"invalid":{"provider":"fixture","model":"absent","tools":["missing"]}})).unwrap()).unwrap();
    std::fs::write(
        &local,
        r#"{"zeta":{"thinkingLevel":"low","tools":["bash"],"instructions":"local instructions"}}"#,
    )
    .unwrap();
    for success in [true, false] {
        harness::compare("preset",register,json!({"cwd":cwd,"models":[model()],"modelSuccess":success,"steps":[start(),command(" alpha "),{"op":"before","prompt":"original"},{"op":"event","event":{"type":"turn_start","turnIndex":0,"timestamp":1}},command("invalid"),command("empty"),command("zeta"),{"op":"before"},command("missing"),{"op":"shortcut","key":"ctrl+shift+u"},{"op":"shortcut","key":"ctrl+shift+u"}, {"op":"command","name":"preset","keys":["\u{1b}[B","\u{1b}[B","\u{1b}[B","\u{1b}[B","\r"]},{"op":"before"}]})).await;
    }
    for keys in [json!(["\u{1b}"]), json!(["\r"]), json!(["\u{1b}[B", "\r"])] {
        harness::compare("preset",register,json!({"cwd":cwd,"models":[model()],"flags":{"preset":"alpha"},"steps":[start(),{"op":"command","name":"preset","keys":keys},{"op":"before"}]})).await;
    }
    for flag in [json!(null), json!("unknown"), json!("")] {
        let mut request = json!({"cwd":cwd,"models":[model()],"entries":[{"type":"custom","customType":"preset-state","data":{"name":"zeta"}}],"steps":[start(),{"op":"before"},{"op":"event","event":{"type":"turn_start","turnIndex":0,"timestamp":1}}]});
        if !flag.is_null() {
            request["flags"] = json!({"preset":flag});
        }
        harness::compare("preset", register, request).await;
    }
    // Empty-start cycles through every sorted preset and restores original state.
    harness::compare("preset",register,json!({"cwd":cwd,"models":[model()],"steps":[start(),{"op":"shortcut","key":"ctrl+shift+u"},{"op":"shortcut","key":"ctrl+shift+u"},{"op":"shortcut","key":"ctrl+shift+u"},{"op":"shortcut","key":"ctrl+shift+u"},{"op":"shortcut","key":"ctrl+shift+u"},{"op":"before"}]})).await;
    harness::compare("preset",register,json!({"cwd":cwd,"models":[model()],"initialModel":model(),"thinking":"medium","steps":[start(),command("alpha"),{"op":"command","name":"preset","keys":["\u{1b}[B","\u{1b}[B","\u{1b}[B","\u{1b}[B","\r"]},{"op":"before"}]})).await;
    harness::compare("preset",register,json!({"cwd":cwd,"models":[model()],"entries":[{"type":"custom","customType":"preset-state","data":{"name":"zeta"}}],"steps":[start(),{"op":"command","name":"preset","keys":["\u{1b}[B","\u{1b}[B","\u{1b}[B","\u{1b}[B","\r"]}]})).await;
    std::fs::write(&global, "{broken").unwrap();
    std::fs::write(&local, "{also broken").unwrap();
    harness::compare("preset", register, json!({"cwd":cwd,"steps":[start(),command("")]})).await;
    std::fs::remove_file(global).unwrap();
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
