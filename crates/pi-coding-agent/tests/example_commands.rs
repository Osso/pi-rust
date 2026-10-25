//! Compile the shipped, explicitly embedded example and compare its complete
//! registration/completion/UI transcript with the actual pinned TS example.
#[path = "../examples/extensions/commands.rs"]
mod commands;
use crate::shared_extensions as support;
use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::slash_commands::SlashCommandInfo;
use serde_json::{Map, Value, json};

#[derive(Default)]
struct CapturedUi {
    actions: Arc<Mutex<Vec<Value>>>,
    selected: Mutex<Option<String>>,
    confirmed: Mutex<bool>,
}

impl ExtensionUiContext for CapturedUi {
    fn select<'a>(
        &'a self,
        title: &JsString,
        items: &[JsString],
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        self.actions
            .lock()
            .unwrap()
            .push(json!({"type":"select", "title":title, "items":items}));
        let selected = self.selected.lock().unwrap().clone();
        Box::pin(async move { Ok(selected.map(JsString::from)) })
    }

    fn confirm<'a>(
        &'a self,
        title: &JsString,
        message: &JsString,
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, pi_ai::request::CallbackError>> {
        self.actions
            .lock()
            .unwrap()
            .push(json!({"type":"confirm", "title":title, "message":message}));
        let confirmed = *self.confirmed.lock().unwrap();
        Box::pin(async move { Ok(confirmed) })
    }

    fn notify(
        &self,
        message: &JsString,
        level: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.actions
            .lock()
            .unwrap()
            .push(json!({"type":"notify", "message":message, "level":level}));
        Ok(())
    }
}

fn command(name: &str, source: &str, description: Option<&str>, path: &str) -> Value {
    let mut value = json!({
        "name":name, "source":source,
        "sourceInfo":{"path":path,"source":"fixture","scope":"temporary","origin":"top-level"}
    });
    if let Some(description) = description {
        value["description"] = json!(description);
    }
    value
}

fn fixture() -> Value {
    let commands = json!([
        command("duplicate", "skill", Some("first original"), "/skills/first.md"),
        command("zeta", "prompt", None, ""),
        command(
            "duplicate",
            "extension",
            Some("second - description"),
            "/extensions/second.rs"
        ),
        command("alpha", "extension", Some(""), "/extensions/alpha.rs"),
        command("rocket🚀𐀀", "skill", Some("λ\nnext"), "/技巧/🚀𐀀.md"),
        command("name - suffix", "prompt", Some("split oddity"), "/prompts/split.md"),
        command("name", "skill", Some("resolved by split"), "/skills/name.md"),
        command("/slash", "extension", Some("slash oddity"), "/extensions/slash.rs")
    ]);
    let cases = [
        ("", None, false),
        ("", Some("--- Extensions ---"), true),
        ("", Some("--- Prompts ---"), true),
        ("", Some("--- Skills ---"), true),
        ("", Some(""), true),
        ("", Some("/zeta"), true),
        ("extension", Some("/duplicate - second - description"), true),
        ("\u{feff}\u{a0}extension\u{2028}", Some("/alpha"), false),
        ("prompt", Some("/name - suffix - split oddity"), true),
        ("skill", Some("/rocket🚀𐀀 - λ\nnext"), true),
        ("", Some("//slash - slash oddity"), true),
        ("", Some("/unknown - no match"), true),
        ("extensions", None, false),
        ("unknown arbitrary filter", None, false),
        ("EXTENSION", None, false),
        ("\u{85}extension\u{85}", None, false),
        ("\u{200b}skill", None, false),
    ];
    let mut invocations: Vec<Value> = cases
        .into_iter()
        .map(|(args, selected, confirm)| json!({"args":args,"selected":selected,"confirm":confirm,"commands":commands}))
        .collect();
    // The same registered handler observes a changed command list on every call.
    invocations.extend([
        json!({"args":"", "selected":null, "confirm":false, "commands":[]}),
        json!({"args":"skill", "selected":null, "confirm":false, "commands":[]}),
        json!({"args":"", "selected":"/late", "confirm":true,
            "commands":[command("late", "prompt", Some("late bound"), "/late.md")]}),
    ]);
    json!({"prefixes":["","e","extension","p","s","extensions","P"," ","🚀𐀀"],"invocations":invocations})
}

async fn run_native(request: &Value) -> Value {
    let runtime = create_extension_runtime();
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = commands::commands_extension(api);
        Box::pin(async move { result })
    });
    let mut registry = ExtensionRegistry::new();
    registry.register("<example:commands>", factory);
    let extension = load_extension_from_factory(
        registry.get(&JsString::from("<example:commands>")).unwrap().clone(),
        &JsString::from("/fixture"),
        support::event_bus(),
        runtime.clone(),
        &JsString::from("<example:commands>"),
    )
    .await
    .unwrap();
    assert_eq!(extension.read().commands.len(), 1);
    let registered = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from("commands"))
        .unwrap()
        .clone();
    let metadata = json!([{
        "name":registered.name,"description":registered.description,
        "hasArgumentCompletions":registered.get_argument_completions.is_some(),"hasHandler":true,
    }]);
    let completions: Vec<Value> = request["prefixes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|prefix| {
            json!(registered.get_argument_completions.as_ref().unwrap()(&prefix.as_str().unwrap().into()).unwrap())
        })
        .collect();
    let model_registry = ModelRegistry::in_memory(AuthStorage::in_memory(Map::new())).unwrap();
    let mut runner = support::runner_for(vec![extension], "/fixture", runtime, Arc::new(model_registry));
    let ui = Arc::new(CapturedUi::default());
    let current_commands = Arc::new(Mutex::new(Vec::<SlashCommandInfo>::new()));
    let commands_snapshot = Arc::clone(&current_commands);
    let actions_log = Arc::clone(&ui.actions);
    let mut actions = support::actions();
    actions.get_commands = Arc::new(move || {
        actions_log.lock().unwrap().push(json!({"type":"getCommands"}));
        Ok(commands_snapshot.lock().unwrap().clone())
    });
    runner.bind_core(actions, support::context_actions(), None).unwrap();
    runner.set_ui_context(Some(ui.clone()), ExtensionMode::Tui);
    let resolved = runner.get_command("commands").unwrap();
    let mut invocations = Vec::new();
    for invocation in request["invocations"].as_array().unwrap() {
        *current_commands.lock().unwrap() = serde_json::from_value(invocation["commands"].clone()).unwrap();
        *ui.selected.lock().unwrap() = invocation["selected"].as_str().map(str::to_owned);
        *ui.confirmed.lock().unwrap() = invocation["confirm"].as_bool().unwrap();
        ui.actions.lock().unwrap().clear();
        (resolved.handler)(
            JsString::from(invocation["args"].as_str().unwrap().to_owned()),
            runner.create_command_context(),
        )
        .await
        .unwrap();
        invocations.push(ui.actions.lock().unwrap().clone());
    }
    json!({"metadata":metadata,"completions":completions,"invocations":invocations})
}

#[tokio::test]
async fn pinned_commands_example_matches_metadata_completions_and_all_actions() {
    let request = fixture();
    let mut oracle = oracle::TsOracle::spawn(
        "example-commands-oracle.ts",
        "packages/coding-agent/examples/extensions/commands.ts",
    );
    let expected = oracle.request(&request);
    assert_eq!(
        expected["metadata"],
        json!([{"name":"commands","description":"List available slash commands","hasArgumentCompletions":true,"hasHandler":true}])
    );
    assert_eq!(
        expected["completions"][0],
        json!([{"value":"extension","label":"extension"},{"value":"prompt","label":"prompt"},{"value":"skill","label":"skill"}])
    );
    assert_eq!(expected["completions"][5], Value::Null);
    assert_eq!(
        expected["invocations"][6][2],
        json!({"type":"confirm","title":"duplicate","message":"View source path?\n/skills/first.md"})
    );
    assert_eq!(
        expected["invocations"][17][1],
        json!({"type":"notify","message":"No commands found","level":"info"})
    );
    assert_eq!(run_native(&request).await, expected);
}
