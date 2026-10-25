#[path = "support/agent_session_fixture.rs"]
mod agent_session_fixture;
use crate::shared_oracle as oracle;

use pi_agent_core::{agent::AgentMessage, harness::types::ThinkingLevel};
use pi_ai::types::{AssistantMessage, Model, ToolResultMessage, ToolResultRole, Usage};
use pi_coding_agent::{
    core::{
        footer_data_provider::{ContextUsage, FooterDataProvider, FooterSessionOverride},
        session_manager::SessionManager,
    },
    modes::interactive::{
        components::footer::FooterComponent,
        theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance},
    },
};
use pi_js_compat::{js_json::JsString, process_env};
use pi_tui::Component;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn raw(value: &Value) -> JsString {
    JsString::from_utf16(
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_u64().unwrap() as u16)
            .collect(),
    )
}
fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| value.as_str().unwrap().parse().unwrap())
}
fn model_data(model: &Model) -> Value {
    json!({"id":model.id,"provider":model.provider,"reasoning":model.reasoning,"contextWindow":model.context_window})
}
fn display_model(value: &Value, base: &Model) -> Option<Model> {
    if value.is_null() {
        return None;
    }
    let mut model = base.clone();
    model.id = value["id"].as_str().unwrap().into();
    model.provider = value["provider"].as_str().unwrap().into();
    model.reasoning = value["reasoning"].as_bool().unwrap();
    model.context_window = number(&value["contextWindow"]);
    Some(model)
}
fn append(manager: &Arc<Mutex<SessionManager>>, value: &Value) {
    let values = value["usage"].as_array().unwrap();
    let mut usage = Usage {
        input: number(&values[0]),
        output: number(&values[1]),
        cache_read: number(&values[2]),
        cache_write: number(&values[3]),
        ..Default::default()
    };
    usage.cost.total = number(&values[4]);
    let message = if value["kind"] == "assistant" {
        let mut message = AssistantMessage::empty("faux", "faux", "faux-1", 0.0);
        message.usage = usage;
        AgentMessage::Assistant(Box::new(message))
    } else {
        AgentMessage::ToolResult(ToolResultMessage {
            role: ToolResultRole::Tag,
            tool_call_id: "tool".into(),
            tool_name: "read".into(),
            content: vec![],
            details: None,
            is_error: false,
            timestamp: 0.0,
        })
    };
    manager.lock().unwrap().append_message(message).unwrap();
}
fn seed(manager: &Arc<Mutex<SessionManager>>, data: &Value) {
    if !data["name"].is_null() {
        manager
            .lock()
            .unwrap()
            .set_session_name(Some(&raw(&data["name"])))
            .unwrap();
    }
    for entry in data["entries"].as_array().unwrap() {
        append(manager, entry);
    }
}
fn set_override(
    footer: &mut FooterComponent,
    value: &Value,
    managers: &[(&str, Arc<Mutex<SessionManager>>)],
    base: &Model,
) {
    let manager = value["manager"].as_str().unwrap();
    footer.set_session_override(FooterSessionOverride {
        cwd: raw(&value["cwd"]),
        session_manager: managers.iter().find(|(key, _)| *key == manager).map(|(_, manager)| {
            pi_coding_agent::core::readonly_session_manager::create_readonly_session_manager(manager.clone())
        }),
        model: display_model(&value["model"], base),
        thinking_level: raw(&value["thinking"]),
        context_usage: (!value["context"].is_null()).then(|| ContextUsage {
            tokens: None,
            context_window: number(&value["context"]["window"]),
            percent: (!value["context"]["percent"].is_null()).then(|| number(&value["context"]["percent"])),
        }),
    });
}
async fn compare_request(oracle: &mut oracle::TsOracle, mut request: Value) -> Option<String> {
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .map(|name| ((*name).into(), ColorValue::Index(0.0)))
        .collect();
    set_theme_instance(Arc::new(
        Theme::new(&colors, &colors, ColorMode::Color256, None, None).unwrap(),
    ));
    for (key, value) in request["env"].as_object().unwrap() {
        if let Some(value) = value.as_str() {
            process_env::set(key, value);
        } else {
            process_env::remove(key);
        }
    }
    process_env::set("PI_EXECUTABLE_NAME", "pi-footer-proof");
    let directory = tempfile::tempdir().unwrap();
    let secondary = tempfile::tempdir().unwrap();
    let main = agent_session_fixture::fixture(directory.path(), ThinkingLevel::High).await;
    let second = agent_session_fixture::fixture(secondary.path(), ThinkingLevel::High).await;
    let main_session = Arc::new(main.session);
    let second_session = Arc::new(second.session);
    let base = main_session.model();
    request["main"]["cwd"] = json!(units(directory.path().to_str().unwrap()));
    request["second"]["cwd"] = json!(units(secondary.path().to_str().unwrap()));
    request["mainModel"] = model_data(&base);
    request["secondModel"] = model_data(&second_session.model());
    let main_manager = main_session.session_manager_handle();
    let second_manager = second_session.session_manager_handle();
    let child_manager = Arc::new(Mutex::new(
        SessionManager::in_memory(Some(&raw(&request["child"]["cwd"])), None).unwrap(),
    ));
    let managers = [
        ("main", main_manager),
        ("second", second_manager),
        ("child", child_manager),
    ];
    for (key, manager) in &managers {
        seed(manager, &request[*key]);
    }
    if let Some(branch) = request["branch"].as_str() {
        std::fs::create_dir(directory.path().join(".git")).unwrap();
        std::fs::write(
            directory.path().join(".git/HEAD"),
            format!("ref: refs/heads/{branch}\n"),
        )
        .unwrap();
    }
    let provider = Arc::new(FooterDataProvider::new(directory.path().to_str().unwrap()));
    provider.set_available_provider_count(request["providerCount"].as_f64().unwrap());
    for pair in request["statuses"].as_array().unwrap() {
        provider.set_extension_status(
            &raw(&pair[0]),
            Some(pi_coding_agent::core::footer_data_provider::ExtensionStatus::Text(raw(
                &pair[1],
            ))),
        );
    }
    let mut footer = FooterComponent::new(main_session.clone(), provider.clone());
    if !request["override"].is_null() {
        set_override(&mut footer, &request["override"], &managers, &base);
    }
    let mut width = request["width"].as_u64().unwrap() as usize;
    let snapshot = |footer: &mut FooterComponent, width| {
        footer
            .render(width)
            .iter()
            .map(|line| line.units().to_vec())
            .collect::<Vec<_>>()
    };
    let mut frames = vec![snapshot(&mut footer, width)];
    for step in request["steps"].as_array().unwrap() {
        match step["op"].as_str().unwrap() {
            "credential" => {
                let session = if step["target"] == "main" {
                    &main_session
                } else {
                    &second_session
                };
                let storage = session.model_registry().auth_storage();
                let provider = step["provider"].as_str().unwrap();
                if step["value"].is_null() {
                    storage.remove(provider).unwrap();
                } else {
                    storage
                        .set(provider, pi_js_compat::js_json::JsValue::from(&step["value"]))
                        .unwrap();
                }
            }
            "append" => append(
                &managers
                    .iter()
                    .find(|(name, _)| *name == step["target"].as_str().unwrap())
                    .unwrap()
                    .1,
                &step["entry"],
            ),
            "name" => {
                managers
                    .iter()
                    .find(|(name, _)| *name == step["target"].as_str().unwrap())
                    .unwrap()
                    .1
                    .lock()
                    .unwrap()
                    .set_session_name(Some(&raw(&step["value"])))
                    .unwrap();
            }
            "invalidate" => footer.invalidate(),
            "dispose" => footer.dispose(),
            "override" => set_override(&mut footer, &step["value"], &managers, &base),
            "clearOverride" => footer.clear_session_override(),
            "session" => footer.set_session(second_session.clone()),
            "auto" => footer.set_auto_compact_enabled(step["value"].as_bool().unwrap()),
            "width" => width = step["value"].as_u64().unwrap() as usize,
            "status" => provider.set_extension_status(
                &raw(&step["key"]),
                Some(pi_coding_agent::core::footer_data_provider::ExtensionStatus::Text(raw(
                    &step["value"],
                ))),
            ),
            value => panic!("unknown step {value}"),
        }
        frames.push(snapshot(&mut footer, width));
    }
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], true, "source: {expected}");
    let actual = json!(frames);
    if actual == expected["output"] {
        return None;
    }
    let index = actual
        .as_array()
        .unwrap()
        .iter()
        .zip(expected["output"].as_array().unwrap())
        .position(|(actual, expected)| actual != expected)
        .unwrap();
    let actual_frame = &actual[index];
    let expected_frame = &expected["output"][index];
    Some(format!(
        "frame {index}: source {:?} / native {:?}",
        expected_frame, actual_frame
    ))
}
fn entry(kind: &str, usage: Value) -> Value {
    json!({"kind":kind,"usage":usage})
}
fn data(name: &str, entries: Vec<Value>) -> Value {
    json!({"cwd":units("/home/proof/child"),"name":units(name),"entries":entries})
}
fn override_(manager: &str) -> Value {
    json!({"manager":manager,"cwd":units("/home/proof/repo"),"model":{"id":"display","provider":"other","reasoning":true,"contextWindow":200000},"thinking":units("high"),"context":{"window":200000,"percent":25}})
}
fn request(entries: Vec<Value>) -> Value {
    json!({"main":data("Main",entries),"second":data("Second",vec![entry("assistant",json!([77,8,0,0,0.01]))]),"child":data("Child",vec![entry("assistant",json!([123,45,67,89,0.02]))]),"width":180,"providerCount":2,"statuses":[],"steps":[],"env":{"HOME":"/home/proof","USERPROFILE":null,"PI_EXPERIMENTAL":"0"}})
}

#[tokio::test(flavor = "current_thread")]
async fn footer_usage_override_truthiness_and_lifecycle_match_frozen() {
    let main_usage = entry("assistant", json!([10000, 2000, 3000, 4000, 0.1234]));
    let mut no_manager = request(vec![main_usage.clone()]);
    no_manager["override"] = override_("none");
    let mut nan = request(vec![entry("assistant", json!(["NaN", "NaN", "NaN", "NaN", "NaN"]))]);
    nan["override"] = override_("main");
    let mut empty = request(vec![]);
    empty["override"] = override_("none");
    empty["override"]["model"]["id"] = json!("");
    empty["override"]["thinking"] = json!([]);
    let mut home = request(vec![]);
    home["override"] = override_("none");
    home["env"]["HOME"] = json!("");
    home["env"]["USERPROFILE"] = json!("/home/proof");
    let mut cache = request(vec![main_usage.clone()]);
    cache["branch"] = json!("proof");
    cache["steps"] = json!([
        {"op":"append","target":"main","entry":entry("assistant",json!([500,50,0,0,0.01]))},{"op":"name","target":"main","value":units("Changed")},{"op":"dispose"},{"op":"invalidate"},
        {"op":"override","value":override_("child")},{"op":"append","target":"child","entry":entry("assistant",json!([1000,100,0,0,0]))},{"op":"invalidate"},{"op":"clearOverride"},{"op":"override","value":override_("child")},{"op":"session"}
    ]);
    let mut widths = request(vec![main_usage.clone()]);
    widths["override"] = override_("child");
    widths["statuses"] = json!([
        [units("goal"), vec![55296, 32, 65]],
        [units("other"), units("other status")]
    ]);
    widths["env"]["PI_EXPERIMENTAL"] = json!("1");
    widths["steps"] = json!([{"op":"auto","value":false},{"op":"width","value":80},{"op":"width","value":40},{"op":"width","value":12},{"op":"width","value":1},{"op":"width","value":0}]);
    let mut credentials = request(vec![]);
    credentials["override"] = override_("none");
    let mut kimi_override = override_("none");
    kimi_override["model"]["provider"] = json!("kimi-coding");
    let mut no_model_override = override_("none");
    no_model_override["model"] = Value::Null;
    credentials["steps"] = json!([
        {"op":"credential","target":"main","provider":"other","value":{"type":"oauth","access":"access","refresh":"refresh","expires":9999999999999_f64}},
        {"op":"credential","target":"main","provider":"other","value":{"type":"api_key","key":"fixture"}},
        {"op":"credential","target":"main","provider":"faux","value":{"type":"oauth","access":"access","refresh":"refresh","expires":9999999999999_f64}},
        {"op":"override","value":kimi_override},
        {"op":"override","value":no_model_override},
        {"op":"clearOverride"},
        {"op":"credential","target":"main","provider":"faux","value":null},
        {"op":"credential","target":"second","provider":"faux","value":{"type":"oauth","access":"access","refresh":"refresh","expires":9999999999999_f64}},
        {"op":"session"}
    ]);
    let mut cases = vec![
        ("live-displayed-provider-credentials", credentials),
        (
            "assistant-only-usage",
            request(vec![main_usage, entry("toolResult", json!([500, 100, 200, 300, 1]))]),
        ),
        ("null-override-manager", no_manager),
        ("numeric-truthiness", nan),
        ("empty-model-and-effort", empty),
        ("empty-HOME-fallback", home),
        ("cached-lifecycle", cache),
        ("width-status-and-experimental", widths),
    ];
    for value in [
        0.0, 999.0, 1000.0, 1500.0, 9999.0, 10000.0, 12345.0, 999999.0, 1000000.0, 1500000.0, 9999999.0, 10000000.0,
        12345678.0, -0.0, -1000.0,
    ] {
        let mut numeric = request(vec![entry("assistant", json!([value, 0, 0, 0, 0]))]);
        numeric["override"] = override_("main");
        cases.push(("token-format-boundary", numeric));
    }
    for percent in [
        Value::Null,
        json!(0),
        json!(70),
        json!(70.1),
        json!(90),
        json!(90.1),
        json!("NaN"),
    ] {
        let mut context = request(vec![]);
        context["override"] = override_("none");
        context["override"]["context"]["percent"] = percent;
        context["override"]["model"]["reasoning"] = json!(false);
        cases.push(("context-threshold", context));
    }
    let mut absent = request(vec![]);
    absent["override"] = override_("none");
    absent["override"]["context"] = Value::Null;
    absent["override"]["model"] = Value::Null;
    cases.push(("no-model-no-context", absent));
    let mut raw_name = request(vec![]);
    raw_name["main"]["name"] = json!([77, 55296]);
    cases.push(("raw-session-name", raw_name));
    let mut oracle = oracle::TsOracle::spawn("footer-oracle.ts", "packages/coding-agent/src");
    let mut failed = Vec::new();
    for (name, request) in cases {
        if let Some(error) = compare_request(&mut oracle, request).await {
            eprintln!("{name}: {error}");
            failed.push(name);
        }
    }
    process_env::clear();
    assert!(failed.is_empty(), "footer mismatches: {failed:?}");
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
