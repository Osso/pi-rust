//! Shared harness for bundled extension tests.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::event_bus::{EventBus, EventBusController};
use pi_coding_agent::core::extensions::loader::{ExtensionFactory, load_extension_from_factory};
use pi_coding_agent::core::extensions::types::{
    Extension, ExtensionActions, ExtensionApi, ExtensionCommandContextActions, ExtensionContextActions, ExtensionMode,
    ExtensionUiContext, ExtensionUiDialogOptions, NotifyType, SessionTransitionResult, UserMessageContent,
};
use pi_coding_agent::core::extensions::{ExtensionRunner, ExtensionRuntime, create_extension_runtime};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value};

async fn model_registry() -> Arc<ModelRegistry> {
    Arc::new(
        ModelRegistry::in_memory(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(
            serde_json::Map::new(),
        ))
        .unwrap(),
    )
}

#[derive(Default)]
pub struct Calls {
    pub shutdown: usize,
    pub confirms: Vec<(JsString, JsString)>,
    pub notify: Vec<(JsString, Option<NotifyType>)>,
    pub set_status: Vec<(JsString, Option<JsString>)>,
    pub set_editor_text: Vec<JsString>,
    pub send_user_message: Vec<(
        UserMessageContent,
        Option<pi_coding_agent::core::extensions::types::SendUserMessageOptions>,
    )>,
    pub append_entry: Vec<(JsString, Option<Value>)>,
    pub restart: Vec<Option<pi_coding_agent::core::extensions::types::RestartOptions>>,
    pub sandbox_profile: Vec<(
        Option<pi_coding_agent::core::permissions::presets::SandboxProfileName>,
        pi_coding_agent::core::permissions::presets::SandboxProfileScope,
    )>,
    pub show_sandbox_selector: usize,
    pub show_approval_selector: usize,
    pub inputs: Vec<(JsString, Option<JsString>, Option<ExtensionUiDialogOptions>)>,
    pub provision: Vec<(Value, Vec<JsString>)>,
    pub run_command: Vec<(JsString, Vec<JsString>, bool)>,
    pub debug: Vec<(JsString, Option<JsString>)>,
}

#[derive(Default)]
pub struct RecordingUi {
    pub confirmed: Mutex<bool>,
    pub calls: Arc<Mutex<Calls>>,
    pub inputs: Mutex<VecDeque<Option<JsString>>>,
}

impl RecordingUi {
    pub fn calls(&self) -> Arc<Mutex<Calls>> {
        Arc::clone(&self.calls)
    }
}

impl ExtensionUiContext for RecordingUi {
    fn confirm<'a>(
        &'a self,
        title: &JsString,
        message: &JsString,
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> pi_coding_agent::core::extensions::types::BoxFuture<'a, Result<bool, pi_ai::request::CallbackError>> {
        self.calls
            .lock()
            .unwrap()
            .confirms
            .push((title.to_owned(), message.to_owned()));
        let confirmed = *self.confirmed.lock().unwrap();
        Box::pin(async move { Ok(confirmed) })
    }

    fn input<'a>(
        &'a self,
        title: &JsString,
        placeholder: Option<&JsString>,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> pi_coding_agent::core::extensions::types::BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>>
    {
        let title = title.to_owned();
        let placeholder = placeholder.map(Clone::clone);
        let opts = opts.cloned();
        Box::pin(async move {
            self.calls.lock().unwrap().inputs.push((title, placeholder, opts));
            Ok(self.inputs.lock().unwrap().pop_front().flatten())
        })
    }

    fn notify(
        &self,
        message: &JsString,
        notify_type: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.calls
            .lock()
            .unwrap()
            .notify
            .push((message.to_owned(), notify_type));
        Ok(())
    }

    fn set_status(
        &self,
        key: &JsString,
        text: Option<pi_coding_agent::core::footer_data_provider::ExtensionStatus>,
    ) -> Result<(), pi_ai::request::CallbackError> {
        let text = text.map(|text| text.evaluate(None)).transpose()?.flatten();
        self.calls.lock().unwrap().set_status.push((key.to_owned(), text));
        Ok(())
    }

    fn set_editor_text(&self, text: &JsString) -> Result<(), pi_ai::request::CallbackError> {
        self.calls.lock().unwrap().set_editor_text.push(text.to_owned());
        Ok(())
    }
}

pub struct Harness {
    pub extension: Extension,
    pub runner: ExtensionRunner,
    pub calls: Arc<Mutex<Calls>>,
}

impl Harness {
    pub async fn load_full(
        cwd: &str,
        session_manager: pi_coding_agent::core::session_manager::SharedSessionManager,
        ui: Arc<RecordingUi>,
        control_db_path: Option<String>,
        setup: impl FnOnce(&ExtensionApi) -> Result<(), pi_coding_agent::core::extensions::types::HandlerError>
        + Send
        + 'static,
    ) -> Self {
        let runtime = create_extension_runtime();
        let calls = ui.calls();
        let extension = load(runtime.clone(), cwd, ui.clone(), calls.clone(), setup).await;
        let mut runner = ExtensionRunner::new(
            vec![extension.clone()],
            runtime,
            cwd,
            Arc::clone(&session_manager),
            model_registry().await,
            None,
        );
        runner
            .bind_core(actions(&calls), context_actions(&calls, control_db_path), None)
            .unwrap();
        runner.bind_command_context(Some(command_actions(&calls)));
        runner.set_ui_context(Some(ui.clone()), ExtensionMode::Tui);
        Self {
            extension,
            runner,
            calls,
        }
    }

    pub fn context(&self) -> pi_coding_agent::core::extensions::types::ExtensionContext {
        self.runner.create_context()
    }

    pub fn registrations(&self) -> Value {
        registrations_json(&self.extension)
    }

    pub fn take_calls(&self) -> Calls {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }
}

async fn load(
    runtime: ExtensionRuntime,
    cwd: &str,
    ui: Arc<RecordingUi>,
    calls: Arc<Mutex<Calls>>,
    setup: impl FnOnce(&ExtensionApi) -> Result<(), pi_coding_agent::core::extensions::types::HandlerError> + Send + 'static,
) -> Extension {
    let _ = (ui, calls);
    let setup = Mutex::new(Some(setup));
    let factory: ExtensionFactory = Arc::new(move |api| {
        let setup = setup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .expect("extension setup runs once");
        let result = setup(api);
        Box::pin(async move { result })
    });
    let bus: Arc<dyn EventBus> = Arc::new(EventBusController::new());
    load_extension_from_factory(factory, &JsString::from(cwd), bus, runtime, &JsString::from("<test>"))
        .await
        .expect("extension factory succeeds")
}

pub fn actions(calls: &Arc<Mutex<Calls>>) -> ExtensionActions {
    let send_user_message_calls = Arc::clone(calls);
    let append_entry_calls = Arc::clone(calls);
    ExtensionActions {
        send_message: Arc::new(|_, _| Ok(())),
        send_user_message: Arc::new(move |content, options| {
            send_user_message_calls
                .lock()
                .unwrap()
                .send_user_message
                .push((content, options));
            Ok(())
        }),
        append_entry: Arc::new(move |custom_type, data| {
            append_entry_calls
                .lock()
                .unwrap()
                .append_entry
                .push((custom_type.to_owned(), data));
            Ok(())
        }),
        request_resume_continuation: Arc::new(|| {}),
        set_session_name: Arc::new(|_| Ok(())),
        get_session_name: Arc::new(|| Ok(None)),
        set_label: Arc::new(|_, _| Ok(())),
        get_active_tools: Arc::new(|| Ok(Vec::new())),
        get_all_tools: Arc::new(|| Ok(Vec::new())),
        set_active_tools: Arc::new(|_| Ok(())),
        call_tool: Arc::new(|_, _, _, _| Box::pin(async { Ok(empty_tool_result()) })),
        call_command: Arc::new(|_, _| Box::pin(async { Ok(Value::Null) })),
        refresh_tools: Arc::new(|| Ok(())),
        get_commands: Arc::new(|| Ok(Vec::new())),
        set_model: Arc::new(|_| Box::pin(async { Ok(false) })),
        get_thinking_level: Arc::new(|| Ok(pi_agent_core::harness::ThinkingLevel::Off)),
        set_thinking_level: Arc::new(|_| Ok(())),
    }
}

pub fn context_actions(calls: &Arc<Mutex<Calls>>, control_db_path: Option<String>) -> ExtensionContextActions {
    let restart_calls = Arc::clone(calls);
    let shutdown_calls = Arc::clone(calls);
    let control_db_path = Arc::new(Mutex::new(control_db_path));
    ExtensionContextActions {
        get_model: Arc::new(|| None),
        get_thinking_level: None,
        set_thinking_level: None,
        get_scoped_models: None,
        get_footer_data: None,
        is_idle: Arc::new(|| true),
        has_active_retry: Arc::new(|| false),
        is_project_trusted: Arc::new(|| true),
        get_signal: Arc::new(|| None),
        abort: Arc::new(|| {}),
        has_pending_messages: Arc::new(|| false),
        shutdown: Arc::new(move || {
            shutdown_calls.lock().unwrap().shutdown += 1;
        }),
        restart: Arc::new(move |options| {
            restart_calls.lock().unwrap().restart.push(options);
            Box::pin(async { Ok(()) })
        }),
        get_control_db_path: Some(Arc::new(move || {
            control_db_path.lock().unwrap().clone().map(JsString::from)
        })),
        get_context_usage: Arc::new(|| None),
        get_multi_agent_agent_id: None,
        get_multi_agent_parent_session_id: None,
        register_supervisor_review_cancellation: None,
        get_no_supervisor: None,
        get_multi_agent_requires_agent_id: None,
        get_multi_agent_store: None,
        get_tool_detach_registry: None,
        compact: Arc::new(|_| {}),
        get_system_prompt: Arc::new(JsString::default),
        get_system_prompt_options: None,
    }
}

pub fn command_actions(calls: &Arc<Mutex<Calls>>) -> ExtensionCommandContextActions {
    let selector_calls = Arc::clone(calls);
    let approval_calls = Arc::clone(calls);
    let restart_calls = Arc::clone(calls);
    ExtensionCommandContextActions {
        show_approval_selector: Arc::new(move || {
            approval_calls.lock().unwrap().show_approval_selector += 1;
            Ok(())
        }),
        show_sandbox_selector: Arc::new(move || {
            selector_calls.lock().unwrap().show_sandbox_selector += 1;
            Ok(())
        }),
        wait_for_idle: Arc::new(|| Box::pin(async {})),
        new_session: Arc::new(|_| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        fork: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        navigate_tree: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        switch_session: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        reload: Arc::new(|| Box::pin(async { Ok(()) })),
        restart: Arc::new(move |options| {
            restart_calls.lock().unwrap().restart.push(options);
            Box::pin(async { Ok(()) })
        }),
    }
}

pub fn empty_tool_result() -> pi_agent_core::agent::types::AgentToolResult {
    pi_agent_core::agent::types::AgentToolResult {
        content: Vec::new(),
        details: None,
        is_error: None,
        terminate: None,
    }
}

pub fn calls_json(calls: &Calls) -> Value {
    let send_user_message: Vec<Value> = calls
        .send_user_message
        .iter()
        .map(|(content, options)| {
            let content = match content {
                UserMessageContent::Text(text) => Value::String(text.clone()),
                UserMessageContent::Blocks(blocks) => {
                    pi_js_compat::js_json::to_value(blocks).expect("blocks serialize")
                }
            };
            let options = options
                .as_ref()
                .map(|options| pi_js_compat::js_json::to_value(options).expect("options serialize"))
                .filter(|value| value != &Value::Null);
            json!([content, options])
        })
        .collect();
    let restart: Vec<Value> = calls
        .restart
        .iter()
        .map(|options| match options {
            Some(options) => {
                let mut object = JsObject::new();
                if let Some(notice) = &options.notice {
                    object.insert("notice".into(), Value::String(notice.clone()));
                }
                if let Some(process) = options.process {
                    object.insert("process".into(), Value::Bool(process));
                }
                Value::Object(object)
            }
            None => Value::Null,
        })
        .collect();
    let inputs: Vec<Value> = calls
        .inputs
        .iter()
        .map(|(title, placeholder, options)| {
            let mut fields = pi_js_compat::js_json::JsObject::new();
            if let Some(options) = options {
                if options.signal.is_some() {
                    fields.insert("signal".into(), json!({}));
                }
                if let Some(timeout) = options.timeout {
                    fields.insert("timeout".into(), json!(timeout));
                }
                if let Some(secret) = options.secret {
                    fields.insert("secret".into(), json!(secret));
                }
            }
            let options = Value::Object(fields);
            json!([title, placeholder, options])
        })
        .collect();
    let provision: Vec<Value> = calls
        .provision
        .iter()
        .map(|(request, values)| json!([request, values]))
        .collect();
    let run_command: Vec<Value> = calls
        .run_command
        .iter()
        .map(|(executable, args, aborted)| json!([executable, args, aborted]))
        .collect();
    let debug: Vec<Value> = calls
        .debug
        .iter()
        .map(|(action, session_id)| json!([action, session_id]))
        .collect();
    json!({
        "shutdown": calls.shutdown,
        "confirms": calls.confirms,
        "notify": calls.notify.iter().map(|(message, kind)| json!([
            message,
            kind.map(|kind| match kind {
                NotifyType::Info => "info",
                NotifyType::Warning => "warning",
                NotifyType::Error => "error",
            })
        ])).collect::<Vec<Value>>(),
        "setStatus": calls.set_status.iter().map(|(key, text)| json!([key, text])).collect::<Vec<Value>>(),
        "setEditorText": calls.set_editor_text,
        "sendUserMessage": send_user_message,
        "appendEntry": calls.append_entry.iter().map(|(custom_type, data)| json!([custom_type, data])).collect::<Vec<Value>>(),
        "restart": restart,
        "sandboxProfile": calls.sandbox_profile.iter().map(|(profile, scope)| json!([
            profile.map(pi_coding_agent::core::permissions::presets::SandboxProfileName::as_str),
            serde_json::to_value(scope).expect("scope serializes")
        ])).collect::<Vec<Value>>(),
        "showSandboxSelector": calls.show_sandbox_selector,
        "showApprovalSelector": calls.show_approval_selector,
        "inputs": inputs,
        "provision": provision,
        "runCommand": run_command,
        "debug": debug,
        "gate": Vec::<Value>::new(),
        "errors": Vec::<Value>::new(),
    })
}

pub fn registrations_json(extension: &Extension) -> Value {
    let tools: Vec<Value> = extension
        .read()
        .tools
        .values()
        .map(|registered| {
            let tool = &registered.definition;
            json!({
                "name": tool.name,
                "label": tool.label,
                "description": tool.description,
                "promptSnippet": tool.prompt_snippet,
                "promptGuidelines": tool.prompt_guidelines,
                "approvalRequired": tool.approval_required,
                "parameters": tool.parameters,
            })
        })
        .collect();
    let commands: Vec<Value> = extension
        .read()
        .commands
        .values()
        .map(|command| {
            json!({
                "name": command.name,
                "description": command.description,
                "hasCompletions": command.get_argument_completions.is_some(),
            })
        })
        .collect();
    json!({
        "tools": tools,
        "commands": commands,
        "flags": [],
        "shortcuts": Vec::<String>::new(),
        "handlers": handler_names(extension),
        "toolGates": extension.read().tool_gates.len(),
    })
}

fn handler_names(extension: &Extension) -> Vec<&'static str> {
    let handlers = &extension.read().handlers;
    let mut names = Vec::new();
    if !handlers.session_start.is_empty() {
        names.push("session_start");
    }
    if !handlers.session_shutdown.is_empty() {
        names.push("session_shutdown");
    }
    if !handlers.before_agent_start.is_empty() {
        names.push("before_agent_start");
    }
    if !handlers.agent_end.is_empty() {
        names.push("agent_end");
    }
    names
}
