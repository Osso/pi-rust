#![allow(dead_code)] // Each integration test compiles its own copy and uses a subset.

//! Harness for bundled-extension behavioral tests.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use pi_agent_core::harness::ThinkingLevel;
use pi_ai::request::{BoxFuture as FetchFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_ai::types::Model;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::types::{
    BoxFuture, ContextUsage as ExtensionContextUsage, CustomMessagePayload, ExtensionUiContext,
    ExtensionUiDialogOptions, ExtensionUiFooterFactory, HandlerError, NotifyType, SendMessageOptions,
    SendUserMessageOptions, UserMessageContent,
};
use pi_coding_agent::core::extensions::{
    Extension, ExtensionActions, ExtensionContextActions, ExtensionMode, ExtensionRunner, ExtensionRuntime,
};
use pi_coding_agent::core::footer_data_provider::FooterDataProvider;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::session_manager::{SessionManager, SharedSessionManager};
use pi_coding_agent::core::settings_manager::{Settings, SettingsManager};
use pi_coding_agent::modes::interactive::theme::{
    current_theme_if_initialized, load_theme_from_path, set_theme_instance,
};
use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value};

use super::support;

/// Scripted HTTP responses with request recording.
pub struct StubFetch {
    responses: Mutex<Vec<(u16, String, Vec<u8>)>>,
    pub requests: Mutex<Vec<HttpRequest>>,
}

impl StubFetch {
    pub fn new(responses: Vec<(u16, &str, &str)>) -> Self {
        Self {
            responses: Mutex::new(
                responses
                    .into_iter()
                    .map(|(status, status_text, body)| (status, status_text.to_owned(), body.as_bytes().to_vec()))
                    .collect(),
            ),
            requests: Mutex::new(Vec::new()),
        }
    }
}

impl Fetch for StubFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> FetchFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().unwrap().push(request);
        let (status, status_text, body) = self.responses.lock().unwrap().remove(0);
        Box::pin(async move {
            Ok(HttpResponse {
                status,
                status_text,
                headers: Vec::new(),
                body: Box::new(StubBody(Some(body))),
            })
        })
    }
}

struct StubBody(Option<Vec<u8>>);

impl HttpBody for StubBody {
    fn next_chunk(&mut self) -> FetchFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = self.0.take();
        Box::pin(async move { chunk.map(Ok) })
    }
}

type AppendEntryAction = Arc<dyn Fn(&JsString, Option<Value>) -> Result<(), HandlerError> + Send + Sync>;

#[derive(Default)]
pub struct RecordingUi {
    pub notifications: Mutex<Vec<(JsString, Option<NotifyType>)>>,
    pub statuses: Mutex<BTreeMap<JsString, Option<JsString>>>,
    pub status_calls: Mutex<Vec<(JsString, Option<JsString>)>>,
    pub status_values: Mutex<BTreeMap<JsString, Option<pi_coding_agent::core::footer_data_provider::ExtensionStatus>>>,
    pub editor_texts: Mutex<Vec<JsString>>,
    pub select_result: Mutex<Option<JsString>>,
    pub select_calls: Mutex<Vec<(JsString, Vec<JsString>)>>,
    pub confirm_result: Mutex<bool>,
    pub footer_factories: Mutex<Vec<ExtensionUiFooterFactory>>,
}

impl RecordingUi {
    pub fn notifications(&self) -> Vec<(JsString, Option<NotifyType>)> {
        self.notifications.lock().unwrap().clone()
    }

    pub fn last_notification(&self) -> Option<(JsString, Option<NotifyType>)> {
        self.notifications.lock().unwrap().last().cloned()
    }

    pub fn status(&self, key: &str) -> Option<JsString> {
        self.statuses
            .lock()
            .unwrap()
            .get(&JsString::from(key))
            .cloned()
            .flatten()
    }

    pub fn select_result(&self, value: Option<&str>) {
        *self.select_result.lock().unwrap() = value.map(JsString::from);
    }

    pub fn confirm_result(&self, value: bool) {
        *self.confirm_result.lock().unwrap() = value;
    }
}

impl ExtensionUiContext for RecordingUi {
    fn select<'a>(
        &'a self,
        title: &JsString,
        options: &[JsString],
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        let title = title.to_owned();
        let options = options.to_vec();
        self.select_calls.lock().unwrap().push((title, options));
        let result = self.select_result.lock().unwrap().clone();
        Box::pin(async move { Ok(result) })
    }

    fn confirm<'a>(
        &'a self,
        _title: &JsString,
        _message: &JsString,
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, pi_ai::request::CallbackError>> {
        let result = *self.confirm_result.lock().unwrap();
        Box::pin(async move { Ok(result) })
    }

    fn notify(
        &self,
        message: &JsString,
        notify_type: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.notifications
            .lock()
            .unwrap()
            .push((message.to_owned(), notify_type));
        Ok(())
    }

    fn set_status(
        &self,
        key: &JsString,
        text: Option<pi_coding_agent::core::footer_data_provider::ExtensionStatus>,
    ) -> Result<(), pi_ai::request::CallbackError> {
        self.status_values.lock().unwrap().insert(key.to_owned(), text.clone());
        let value = text.map(|text| text.evaluate(None)).transpose()?.flatten();
        self.statuses.lock().unwrap().insert(key.to_owned(), value.clone());
        self.status_calls.lock().unwrap().push((key.to_owned(), value));
        Ok(())
    }

    fn set_editor_text(&self, text: &JsString) -> Result<(), pi_ai::request::CallbackError> {
        self.editor_texts.lock().unwrap().push(text.to_owned());
        Ok(())
    }

    fn set_default_footer(&self, factory: Option<ExtensionUiFooterFactory>) {
        if let Some(factory) = factory {
            self.footer_factories.lock().unwrap().push(factory);
        }
    }
}

#[derive(Default)]
pub struct RecordedActions {
    pub messages: Mutex<Vec<CustomMessagePayload>>,
    pub user_messages: Mutex<Vec<(UserMessageContent, Option<SendUserMessageOptions>)>>,
    pub entries: Mutex<Vec<(JsString, Option<Value>)>>,
    pub active_tools: Mutex<Vec<String>>,
    pub tool_updates: Mutex<Vec<Vec<String>>>,
}

pub struct HarnessOptions {
    pub cwd: JsString,
    pub session_manager: SharedSessionManager,
    pub model: Option<Model>,
    pub thinking_level: ThinkingLevel,
    pub settings: Option<Settings>,
    pub auth: Option<JsObject>,
    pub context_usage: Option<ExtensionContextUsage>,
    pub multi_agent_agent_id: Option<JsString>,
    pub multi_agent_requires_agent_id: bool,
}

impl HarnessOptions {
    pub fn new(cwd: &str) -> Self {
        Self {
            cwd: cwd.into(),
            session_manager: Arc::new(Mutex::new(
                SessionManager::in_memory(Some(&JsString::from(cwd)), None).expect("in-memory session manager"),
            )),
            model: None,
            thinking_level: ThinkingLevel::Off,
            settings: None,
            auth: None,
            context_usage: None,
            multi_agent_agent_id: None,
            multi_agent_requires_agent_id: false,
        }
    }
}

pub struct Harness {
    pub runner: ExtensionRunner,
    pub ui: Arc<RecordingUi>,
    pub actions: Arc<RecordedActions>,
    pub thinking: Arc<Mutex<ThinkingLevel>>,
}

fn ensure_theme() {
    if current_theme_if_initialized().is_some() {
        return;
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = load_theme_from_path(&JsString::from(path.to_str().expect("utf-8 manifest path")), None)
        .expect("dark theme fixture");
    set_theme_instance(Arc::new(theme));
}

impl Harness {
    pub async fn new(extensions: Vec<Extension>, runtime: ExtensionRuntime, options: HarnessOptions) -> Self {
        ensure_theme();
        let ui = Arc::new(RecordingUi::default());
        let model = Arc::new(Mutex::new(options.model));
        let thinking = Arc::new(Mutex::new(options.thinking_level));
        let actions = Arc::new(RecordedActions::default());
        let footer_data = Arc::new(FooterDataProvider::new(&options.cwd));
        let context_usage = options.context_usage;
        let multi_agent_agent_id = options.multi_agent_agent_id;
        let multi_agent_requires_agent_id = options.multi_agent_requires_agent_id;

        let registry = model_registry_with(options.auth).await;
        let settings = options
            .settings
            .map(|settings| Arc::new(Mutex::new(SettingsManager::in_memory(settings, Default::default()))));
        let mut runner = ExtensionRunner::new(
            extensions,
            runtime,
            options.cwd.clone(),
            options.session_manager,
            registry,
            settings,
        );

        let recorded = Arc::clone(&actions);
        let send_message: Arc<dyn Fn(CustomMessagePayload, Option<SendMessageOptions>) -> Result<(), _> + Send + Sync> =
            Arc::new(move |message, _options| {
                recorded.messages.lock().unwrap().push(message);
                Ok(())
            });
        let recorded = Arc::clone(&actions);
        let send_user_message: Arc<
            dyn Fn(UserMessageContent, Option<SendUserMessageOptions>) -> Result<(), _> + Send + Sync,
        > = Arc::new(move |content, options| {
            recorded.user_messages.lock().unwrap().push((content, options));
            Ok(())
        });
        let recorded = Arc::clone(&actions);
        let append_entry: AppendEntryAction = Arc::new(move |custom_type, data| {
            recorded.entries.lock().unwrap().push((custom_type.to_owned(), data));
            Ok(())
        });
        let mut extension_actions: ExtensionActions = support::actions();
        extension_actions.send_message = send_message;
        extension_actions.send_user_message = send_user_message;
        extension_actions.append_entry = append_entry;
        let recorded = Arc::clone(&actions);
        extension_actions.get_active_tools = Arc::new(move || Ok(recorded.active_tools.lock().unwrap().clone()));
        let recorded = Arc::clone(&actions);
        extension_actions.set_active_tools = Arc::new(move |names| {
            *recorded.active_tools.lock().unwrap() = names.to_vec();
            recorded.tool_updates.lock().unwrap().push(names.to_vec());
            Ok(())
        });

        let model_getter = Arc::clone(&model);
        let thinking_getter = Arc::clone(&thinking);
        let thinking_setter = Arc::clone(&thinking);
        let footer = Arc::clone(&footer_data);
        let context_actions = ExtensionContextActions {
            get_model: Arc::new(move || model_getter.lock().unwrap().clone()),
            get_thinking_level: Some(Arc::new(move || *thinking_getter.lock().unwrap())),
            set_thinking_level: Some(Arc::new(move |level| {
                *thinking_setter.lock().unwrap() = level;
                Ok(())
            })),
            get_footer_data: Some(Arc::new(move || Some(Arc::clone(&footer)))),
            get_context_usage: Arc::new(move || context_usage.clone()),
            get_multi_agent_agent_id: Some(Arc::new(move || multi_agent_agent_id.clone())),
            get_no_supervisor: None,
            get_multi_agent_requires_agent_id: Some(Arc::new(move || multi_agent_requires_agent_id)),
            ..support::context_actions()
        };

        runner.bind_core(extension_actions, context_actions, None).unwrap();
        runner.set_ui_context(Some(ui.clone() as Arc<dyn ExtensionUiContext>), ExtensionMode::Rpc);
        Self {
            runner,
            ui,
            actions,
            thinking,
        }
    }

    pub fn command_context(&self) -> pi_coding_agent::core::extensions::ExtensionCommandContext {
        self.runner.create_command_context()
    }

    pub fn context(&self) -> pi_coding_agent::core::extensions::ExtensionContext {
        self.runner.create_context()
    }
}

pub async fn model_registry() -> Arc<ModelRegistry> {
    support::model_registry().await
}

async fn model_registry_with(auth: Option<JsObject>) -> Arc<ModelRegistry> {
    match auth {
        Some(auth) => Arc::new(ModelRegistry::in_memory(AuthStorage::in_memory(auth)).unwrap()),
        None => support::model_registry().await,
    }
}

pub fn test_model(reasoning: bool) -> Model {
    let mut thinking_level_map = pi_ai::types::ThinkingLevelMap::new();
    thinking_level_map.insert(pi_ai::types::ModelThinkingLevel::Xhigh, Some("xhigh".into()));
    thinking_level_map.insert(pi_ai::types::ModelThinkingLevel::Max, Some("max".into()));
    thinking_level_map.insert(pi_ai::types::ModelThinkingLevel::Ultra, Some("max".into()));
    Model {
        properties: Default::default(),
        id: "reasoner".to_owned(),
        name: "Reasoner".into(),
        api: "openai-codex-responses".to_owned(),
        provider: "openai-codex".to_owned(),
        base_url: "https://example.test".into(),
        reasoning,
        thinking_level_map: Some(thinking_level_map),
        input: vec![pi_ai::types::Modality::Text],
        cost: pi_ai::types::ModelCost {
            properties: Default::default(),
            rates: pi_ai::types::ModelCostRates {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
            },
            tiers: None,
        },
        context_window: 200_000.0,
        auto_compaction_threshold: None,
        max_tokens: 1000.0,
        sampling_params: None,
        headers: None,
        compat: None,
    }
}
