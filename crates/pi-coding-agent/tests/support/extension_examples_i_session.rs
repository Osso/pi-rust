//! Session/model-specific seams atop A/B's real registry/runner fixtures.
use futures_util::FutureExt;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
#[path = "oracle.rs"]
mod oracle;
#[path = "extensions.rs"]
mod support;
#[path = "ui_thread.rs"]
mod ui_thread;
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxResponseStep, RegisterFauxProviderOptions,
    faux_assistant_message,
};
use pi_coding_agent::core::{
    auth_storage::AuthStorage, extensions::*, model_registry::ModelRegistry, session_manager::SessionManager,
};
use pi_coding_agent::modes::interactive::theme::{ColorMode, Theme, load_theme_from_path, set_theme_instance};
use serde_json::{Map, Value, json};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
type Trace = Arc<Mutex<Vec<Value>>>;

struct Backend;
impl ExtensionUiTuiBackend for Backend {
    fn request_component_render(&self, _identity: usize) {}
    fn show_overlay(
        &self,
        _component: Box<dyn pi_coding_agent::core::extensions::ExtensionUiComponent>,
        _options: Option<pi_coding_agent::core::extensions::ExtensionUiOverlayOptions>,
    ) -> Rc<dyn pi_coding_agent::core::extensions::ExtensionUiOverlayHandle> {
        panic!("this fixture does not mount nested overlays")
    }

    fn request_render_force(&self) {
        panic!("fixture does not force redraw")
    }
    fn stop(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("fixture does not release terminal")
    }
    fn start(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("fixture does not acquire terminal")
    }

    fn create_fixed_cell(&self) -> std::rc::Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("this component fixture must not construct an editor")
    }
    fn request_render(&self) {}
    fn terminal_rows(&self) -> usize {
        40
    }
    fn terminal_columns(&self) -> usize {
        80
    }
}
struct Ui {
    trace: Trace,
    generation_started: Arc<tokio::sync::Notify>,
    theme: Arc<Theme>,
    request: Value,
}
impl ExtensionUiContext for Ui {
    fn theme(&self) -> Arc<Theme> {
        self.theme.clone()
    }
    fn notify(
        &self,
        s: &JsString,
        level: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.trace.lock().unwrap().push(json!(["notify", s, level]));
        Ok(())
    }
    fn editor<'a>(
        &'a self,
        title: &JsString,
        prefill: Option<&JsString>,
    ) -> BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        self.trace.lock().unwrap().push(json!(["editor", title, prefill]));
        let result = self.request["editor"].as_str().map(str::to_owned);
        Box::pin(async move { Ok(result.map(JsString::from)) })
    }
    fn set_editor_text(&self, text: &JsString) -> Result<(), pi_ai::request::CallbackError> {
        self.trace.lock().unwrap().push(json!(["setEditorText", text]));
        Ok(())
    }
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
        let trace = self.trace.clone();
        let theme = self.theme.clone();
        let request = self.request.clone();
        let generation_started = self.generation_started.clone();
        ui_thread::run(move || {
            Box::pin(async move {
                set_theme_instance(theme.clone());
                let (sender, receiver) = tokio::sync::oneshot::channel();
                let sender = Arc::new(Mutex::new(Some(sender)));
                let component = factory(
                    ExtensionUiFactoryContext {
                        tui: ExtensionUiTui::new(Arc::new(Backend)),
                        theme,
                        keybindings: pi_coding_agent::core::keybindings::KeybindingsManager::new(Vec::new(), None),
                    },
                    ExtensionUiDone::new(Rc::new(move |value| {
                        if let Some(sender) = sender.lock().unwrap().take() {
                            let _ = sender.send(value);
                        }
                    })),
                )
                .expect("custom component fixture factory")
                .now_or_never()
                .expect("fixture factory must resolve immediately")
                .expect("custom component fixture factory");
                for width in [32, 80] {
                    trace
                        .lock()
                        .unwrap()
                        .push(json!(["render", width, component.render(width)]));
                }
                component.invalidate();
                component.handle_input(&JsString::from("x"));
                component.handle_input(&JsString::from(request["key"].as_str().unwrap_or("\r")));
                let result = tokio::time::timeout(std::time::Duration::from_secs(3), receiver)
                    .await
                    .expect("custom UI did not complete")
                    .expect("done sender dropped");
                if request["key"] == "\x1b" && request["noAuth"] != true {
                    tokio::time::timeout(std::time::Duration::from_secs(3), generation_started.notified())
                        .await
                        .expect("source generation must dispatch even after loader cancellation");
                }
                component.dispose();
                Ok(result)
            })
        })
    }
}
fn assistant(content: Value) -> Value {
    let mut message = serde_json::to_value(faux_assistant_message(
        "",
        FauxAssistantMessageOptions {
            timestamp: Some(0.0),
            ..Default::default()
        },
    ))
    .unwrap();
    message["content"] = content;
    message
}
pub async fn compare(name: &str, register: fn(&ExtensionApi) -> Result<(), HandlerError>, mut request: Value) {
    if request["rich"] == true {
        request["messages"] = json!([
         {"role":"user","content":[{"type":"text","text":"  goal  "},{"type":"image","mimeType":"image/png","data":"AA=="},{"type":"text","text":"second"}],"timestamp":0},
         assistant(json!([{"type":"thinking","thinking":"hidden"},{"type":"text","text":"  progress  "},{"type":"toolCall","id":"call","name":"read","arguments":{"z":1,"2":"two","1":"one","large":1e20,"nested":{"4":4,"2":2}}}])),
         {"role":"toolResult","toolCallId":"call","toolName":"read","content":[{"type":"text","text":"not summary text"}],"isError":false,"timestamp":0},
         assistant(json!([{"type":"toolCall","id":"call2","name":"bash","arguments":{}}]))
        ]);
    }
    let runtime = create_extension_runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register(
        name,
        Arc::new(move |api| {
            let result = register(api);
            Box::pin(async move { result })
        }),
    );
    let extension = load_extension_from_factory(
        registry.get(&JsString::from(name)).unwrap().clone(),
        &JsString::from("/fixture"),
        support::event_bus(),
        runtime.clone(),
        &JsString::from("<example:session>"),
    )
    .await
    .unwrap();
    let metadata = {
        let registration = extension.read();
        json!({"commands":registration.commands.values().map(|c|json!([c.name,c.description,c.get_argument_completions.is_some()])).collect::<Vec<_>>(),"tools":registration.tools.keys().collect::<Vec<_>>()})
    };
    let mut credentials = Map::new();
    if request["noAuth"] != true {
        credentials.insert("openai".into(), json!({"type":"api_key","key":"fixture-key"}));
    }
    let models = ModelRegistry::in_memory(AuthStorage::in_memory(credentials)).unwrap();
    if request["noAuth"] != true {
        models.refresh().unwrap();
    }
    let faux = pi_ai::compat::register_faux_provider(RegisterFauxProviderOptions {
        api: Some(
            pi_ai::compat::get_model("openai", "gpt-5.2")
                .expect("frozen fixture model")
                .api,
        ),
        provider: Some("openai".into()),
        models: Some(vec![FauxModelDefinition {
            id: if request["missingModel"] == true {
                "other".into()
            } else {
                "gpt-5.2".into()
            },
            name: Some("Fixture".into()),
            reasoning: Some(true),
            context_window: Some(128000.0),
            max_tokens: Some(4096.0),
            ..Default::default()
        }]),
        ..Default::default()
    });
    let calls: Trace = Arc::default();
    let log = calls.clone();
    let generation_started = Arc::new(tokio::sync::Notify::new());
    let started = generation_started.clone();
    let response=request.get("response").cloned().unwrap_or(json!([{"type":"text","text":"## Summary\n\nDone **one**."},{"type":"thinking","thinking":"hidden"},{"type":"text","text":"Next: `two`."}]));
    let error = request["error"].as_str().map(str::to_owned);
    let aborted = request["aborted"] == true;
    faux.set_responses(vec![FauxResponseStep::Factory(Arc::new(move|context,options,_,model|{
  let options=options.unwrap_or_default();let session_id=options.stream.session_id.as_ref().map(|id| {assert_eq!(id.len(),36);assert_eq!(id.slice(14, 15),"7");"<uuidv7>"});
  let mut context=serde_json::to_value(context).unwrap();context["messages"][0]["timestamp"]=json!("<clock>");
  log.lock().unwrap().push(json!({"model":[model.provider,model.id],"context":context,"options":{"cacheRetention":options.stream.cache_retention,"sessionId":session_id,"reasoningEffort":options.stream.extra.get("reasoningEffort")}}));
  started.notify_one();
  let response=response.clone();let error=error.clone();Box::pin(async move{if let Some(error)=error{return Err(pi_ai::request::CallbackError::new(error));}let mut message=faux_assistant_message("",FauxAssistantMessageOptions{timestamp:Some(0.0),..Default::default()});message.content=serde_json::from_value(response).unwrap();if aborted {message.stop_reason=pi_ai::types::StopReason::Aborted;}Ok(message)})
 }))]);
    let manager = Arc::new(Mutex::new(
        SessionManager::in_memory(Some(&JsString::from("/fixture")), None).unwrap(),
    ));
    let session_root = tempfile::tempdir().unwrap();
    if name == "handoff" {
        let path = session_root.path().join("old.jsonl");
        manager
            .lock()
            .unwrap()
            .set_session_file(&JsString::from(path.to_str().unwrap()))
            .unwrap();
        request["sessionFile"] = json!(path);
    }
    for message in request["messages"].as_array().into_iter().flatten() {
        manager
            .lock()
            .unwrap()
            .append_message(serde_json::from_value(message.clone()).unwrap())
            .unwrap();
    }
    if let Some(index) = request["compaction"].as_u64() {
        let mut manager = manager.lock().unwrap();
        let kept = manager
            .get_entries()
            .get(index as usize)
            .and_then(|entry| entry.id())
            .unwrap_or(&JsString::from("missing"))
            .to_owned();
        manager
            .append_compaction(&JsString::from("Earlier summary"), &kept, 12000.0, Default::default())
            .unwrap();
        manager
            .append_message(
                serde_json::from_value(json!({"role":"user","content":"after compaction","timestamp":0})).unwrap(),
            )
            .unwrap();
    }
    let registry = Arc::new(models);
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime.clone(),
        "/fixture",
        manager.clone(),
        registry.clone(),
        None,
    );
    let mut context = support::context_actions();
    let model = registry.find("openai", "gpt-5.2");
    if request["noModel"] != true {
        context.get_model = Arc::new(move || model.clone());
    }
    runner.bind_core(support::actions(), context, None).unwrap();
    let errors: Trace = Arc::default();
    let log = errors.clone();
    runner.on_error(Arc::new(move |e| log.lock().unwrap().push(json!([e.event, e.error]))));
    let theme = Arc::new(
        load_theme_from_path(
            &JsString::from(
                (&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json"))
                    .to_str()
                    .expect("fixture path must be UTF-8"),
            ),
            Some(ColorMode::Truecolor),
        )
        .unwrap(),
    );
    set_theme_instance(theme.clone());
    let trace: Trace = Arc::default();
    let ui = Arc::new(Ui {
        trace: trace.clone(),
        generation_started: generation_started.clone(),
        theme: theme.clone(),
        request: request.clone(),
    });
    runner.set_ui_context(
        if request["hasUI"] == false {
            None
        } else {
            Some(ui.clone())
        },
        serde_json::from_value(request.get("mode").cloned().unwrap_or(json!("tui"))).unwrap(),
    );
    let replacements: Trace = Arc::default();
    let transitions: Trace = Arc::default();
    let mut replacement = support::runner_for(vec![], "/fixture", create_extension_runtime(), registry.clone());
    support::bind(&mut replacement);
    replacement.set_ui_context(
        Some(Arc::new(Ui {
            trace: replacements.clone(),
            generation_started,
            theme,
            request: request.clone(),
        })),
        ExtensionMode::Tui,
    );
    let replacement_context = replacement.create_command_context();
    let cancelled = request["cancelSession"] == true;
    let log = transitions.clone();
    let previous = runtime.clone();
    runner.bind_command_context(Some(ExtensionCommandContextActions {
        show_approval_selector: Arc::new(|| Ok(())),
        show_sandbox_selector: Arc::new(|| Ok(())),
        wait_for_idle: Arc::new(|| Box::pin(async {})),
        new_session: Arc::new(move |options| {
            let options = options.unwrap_or_default();
            let context = replacement_context.clone();
            let log = log.clone();
            let previous = previous.clone();
            Box::pin(async move {
                log.lock().unwrap().push(json!([
                    "newSession",
                    options.parent_session,
                    options.setup.is_some(),
                    options.with_session.is_some()
                ]));
                if !cancelled {
                    previous.invalidate(None);
                    if let Some(callback) = options.with_session {
                        callback(ReplacedSessionContext {
                            command: context,
                            send_message: Arc::new(|_, _| Box::pin(async { Ok(()) })),
                            send_user_message: Arc::new(|_, _| Box::pin(async { Ok(()) })),
                        })
                        .await?;
                    }
                }
                Ok(SessionTransitionResult { cancelled })
            })
        }),
        fork: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        navigate_tree: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        switch_session: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        reload: Arc::new(|| Box::pin(async { Ok(()) })),
        restart: Arc::new(|_| Box::pin(async { Ok(()) })),
    }));
    let result = (runner.get_command(name).unwrap().handler)(
        request["args"].as_str().unwrap_or("").into(),
        runner.create_command_context(),
    )
    .await;
    faux.unregister();
    let actual = json!({"metadata":metadata,"trace":trace.lock().unwrap().clone(),"calls":calls.lock().unwrap().clone(),"errors":errors.lock().unwrap().clone(),"result":result.err().map(|e|e.message),"transitions":transitions.lock().unwrap().clone(),"replacementUi":replacements.lock().unwrap().clone()});
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "extension-examples-i-session-oracle.ts",
        &format!("packages/coding-agent/examples/extensions/{name}.ts"),
        &[("COLORTERM", "truecolor".into())],
    );
    let expected = oracle.request(&request);
    assert_eq!(actual, expected, "{name}: {request}");
}
