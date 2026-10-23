//! Resident console command (`cli/resident-console-command.ts`).
//!
//! `parseResidentConsoleArgs`, `runResidentConsoleCommand`, and
//! `ResidentConsoleUi` are ported. The TypeScript TUI owns its render loop;
//! the Rust runner pumps `process_events`/`flush_pending` every 5 ms, matching
//! the startup-selector deviation.

use pi_ai::request::CallbackError;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use pi_agent_core::agent::types::AgentMessage;
use pi_ai::types::{TextOrImageContent, UserContent};
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_tui::TUI;
use pi_tui::components::input::InputOptions;
use pi_tui::tui::{ComponentRef, TuiExt, component};
use pi_tui::{Component, Container, Focusable, Input, Spacer, Text};
use serde::Deserialize;

use crate::cli::startup_ui::{create_startup_tui, start_startup_tui};
use crate::config::{get_agent_dir, get_user_state_root};
use crate::core::resident_console_transport::{ResidentConsoleClient, ResidentConsoleService, ResidentConsoleSnapshot};
use crate::core::runtime_message_markers::{
    RuntimeMessageMarker, is_duplicate_turn_assistant_message, is_duplicate_turn_guard_message,
};
use crate::core::session_control_db::get_control_db_path;
use crate::core::session_manager::{SessionEntry, session_entry_to_context_messages};
use crate::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use crate::modes::interactive::components::assistant_message::AssistantMessageComponent;
use crate::modes::interactive::components::user_message::UserMessageComponent;
use crate::modes::interactive::theme::{ThemeColor, with_theme};
use crate::supervisor::response_tool::SUPERVISOR_RESPONSE_TOOL_NAME;
use crate::utils::paths::process_cwd;

const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Wire subset of `AgentSessionEvent` the console renders.
///
/// The source types the client with the full `AgentSessionEvent` union; Rust
/// deserializes only the events this UI consumes (and ignores the rest).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum ResidentConsoleEvent {
    EntryAppended {
        entry: SessionEntry,
    },
    AgentStart,
    AgentEnd,
    MessageStart {
        message: AgentMessage,
        #[serde(default)]
        runtime_message_marker: Option<RuntimeMessageMarker>,
    },
    MessageUpdate {
        message: AgentMessage,
    },
    MessageEnd {
        message: AgentMessage,
        #[serde(default)]
        runtime_message_marker: Option<RuntimeMessageMarker>,
    },
    #[serde(other)]
    Other,
}

/// `ResidentConsoleCommand`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentConsoleCommand {
    pub service: ResidentConsoleService,
    pub initial_prompt: Option<String>,
}

impl ResidentConsoleService {
    /// CLI flag / socket suffix for the service.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Architect => "architect",
            Self::Supervisor => "supervisor",
        }
    }
}

/// `parseResidentConsoleArgs`.
///
/// `Ok(None)` when the first argument is not `--supervisor`/`--architect`;
/// `Err` is the thrown flag-conflict error.
pub fn parse_resident_console_args(args: &[String]) -> Result<Option<ResidentConsoleCommand>, JsString> {
    let Some(raw_service) = args.first() else {
        return Ok(None);
    };
    let service = match raw_service.as_str() {
        "--supervisor" => ResidentConsoleService::Supervisor,
        "--architect" => ResidentConsoleService::Architect,
        _ => return Ok(None),
    };
    let prompt_parts = &args[1..];
    if prompt_parts.iter().any(|part| part.starts_with('-')) {
        return Err(js_format!(
            "--{} cannot be combined with normal Pi CLI flags",
            service.as_str()
        ));
    }
    let initial_prompt = {
        let joined = prompt_parts.join(" ");
        let trimmed = pi_js_compat::string::js_trim(&joined);
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    };
    Ok(Some(ResidentConsoleCommand {
        service,
        initial_prompt,
    }))
}

/// `runResidentConsoleCommand`.
pub async fn run_resident_console_command(command: ResidentConsoleCommand) -> Result<(), CallbackError> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(js_format!("--{} requires an interactive terminal", command.service.as_str()).into());
    }
    let agent_dir = JsString::from(get_agent_dir().map_err(CallbackError::from)?.display().to_string());
    let socket_path = js_format!(
        "{}.{}-console.sock",
        get_control_db_path(&get_user_state_root().map_err(CallbackError::from)?),
        command.service.as_str()
    );
    let client =
        ResidentConsoleClient::<SessionEntry, ResidentConsoleEvent>::connect(socket_path, command.service).await?;
    let cwd = process_cwd().map_err(CallbackError::from)?;
    let settings_manager = SettingsManager::create(
        &cwd,
        Some(&agent_dir),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    )
    .map_err(CallbackError::from)?;
    let env: BTreeMap<String, String> = pi_js_compat::process_env::vars().into_iter().collect();
    let terminal = Rc::new(RefCell::new(pi_tui::terminal::ProcessTerminal::new()));
    let ui = create_startup_tui(&settings_manager, terminal, &env).await?;
    let console = ResidentConsoleUi::new(command.service, client.snapshot());
    let console_component = component(ConsoleComponent {
        ui: Rc::clone(&console),
    });
    ui.add_child(Rc::clone(&console_component));
    ui.set_focus(Some(console_component));
    start_startup_tui(&ui, &settings_manager, &env)?;

    let (event_sender, event_receiver) = channel();
    let _event_subscription = client.on_event(std::sync::Arc::new(move |event| {
        let _ = event_sender.send(event.event.clone());
    }));
    let (disconnect_sender, disconnect_receiver) = channel();
    let _disconnect_subscription = client.on_disconnect(std::sync::Arc::new(move |error| {
        let _ = disconnect_sender.send(error);
    }));
    let (submit_sender, submit_receiver) = channel::<JsString>();
    console.set_on_submit(Rc::new(move |value: &JsString| {
        let _ = submit_sender.send(value.clone());
    }));

    pump_console(
        &ui,
        &console,
        &client,
        command.initial_prompt.map(JsString::from).as_ref(),
        &event_receiver,
        &disconnect_receiver,
        &submit_receiver,
    )
    .await;

    ui.stop();
    client.close().await;
    Ok(())
}

async fn pump_console(
    ui: &TUI,
    console: &ResidentConsoleUi,
    client: &ResidentConsoleClient<SessionEntry, ResidentConsoleEvent>,
    initial_prompt: Option<&JsString>,
    events: &Receiver<ResidentConsoleEvent>,
    disconnects: &Receiver<Option<JsString>>,
    submissions: &Receiver<JsString>,
) {
    if let Some(prompt) = initial_prompt {
        submit(ui, console, client, prompt).await;
    }
    loop {
        ui.process_events();
        ui.flush_pending();
        while let Ok(event) = events.try_recv() {
            console.handle_event(event);
            ui.request_render(false);
        }
        while let Ok(value) = submissions.try_recv() {
            submit(ui, console, client, &value).await;
        }
        match disconnects.try_recv() {
            Ok(error) => {
                console.set_status(error.unwrap_or_else(|| "Resident service disconnected".into()));
                console.close();
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {}
        }
        if console.is_closed() {
            return;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn submit(
    ui: &TUI,
    console: &ResidentConsoleUi,
    client: &ResidentConsoleClient<SessionEntry, ResidentConsoleEvent>,
    value: &JsString,
) {
    let text = value.trim();
    if text.is_empty() {
        return;
    }
    console.clear_input();
    console.set_status(with_theme(|theme| theme.fg(ThemeColor::Muted, "Queued…")));
    ui.request_render(false);
    match client.prompt(pi_ai::utils::random_uuid::random_uuid(), text).await {
        Ok(()) => console.set_status(with_theme(|theme| theme.fg(ThemeColor::Muted, "Working…"))),
        Err(error) => console.set_status(with_theme(|theme| theme.fg(ThemeColor::Error, error))),
    }
    ui.request_render(false);
}

/// `ResidentConsoleUi`: chat transcript, status line, and prompt input.
pub struct ResidentConsoleUi {
    root: Rc<RefCell<Container>>,
    input: Rc<RefCell<Input>>,
    chat: Rc<RefCell<Container>>,
    status: Rc<RefCell<Text>>,
    streaming_assistant: RefCell<Option<(Rc<RefCell<AssistantMessageComponent>>, ComponentRef)>>,
    closed: Cell<bool>,
}

impl ResidentConsoleUi {
    /// Builds the console from the attach snapshot.
    pub fn new(service: ResidentConsoleService, snapshot: &ResidentConsoleSnapshot<SessionEntry>) -> Rc<Self> {
        let (root, _) = SharedContainer::create();
        let (chat, chat_ref) = SharedContainer::create();
        let status = Rc::new(RefCell::new(Text::new("", 1, 0)));
        let status_ref = component(SharedText(Rc::clone(&status)));
        let input = Rc::new(RefCell::new(Input::new(InputOptions::default())));
        let input_ref = component(SharedInput(Rc::clone(&input)));

        {
            let mut root = root.borrow_mut();
            root.add_child(component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Accent, js_format!("{} resident console", service.as_str()))),
                1,
                0,
            )));
            root.add_child(component(Text::new(
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Dim,
                        js_format!("{} · pid {}", snapshot.cwd, snapshot.generation),
                    )
                }),
                1,
                0,
            )));
            root.add_child(component(Spacer::new(1)));
            root.add_child(chat_ref);
            root.add_child(status_ref);
            root.add_child(component(Spacer::new(1)));
            root.add_child(input_ref);
        }

        let console = Rc::new(Self {
            root,
            input,
            chat,
            status,
            streaming_assistant: RefCell::new(None),
            closed: Cell::new(false),
        });
        for entry in &snapshot.branch {
            console.append_entry(entry);
        }
        console
    }

    pub fn set_on_submit(&self, on_submit: Rc<dyn Fn(&JsString)>) {
        self.input.borrow_mut().on_submit = Some(on_submit);
    }

    /// Renders the console root at `width`.
    pub fn render(&self, width: usize) -> Vec<JsString> {
        self.root.borrow_mut().render(width)
    }

    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }

    pub fn close(&self) {
        self.closed.set(true);
    }

    fn clear_input(&self) {
        self.input.borrow_mut().set_value("");
    }

    fn set_status(&self, text: impl Into<JsString>) {
        self.status.borrow_mut().set_text(text);
    }

    fn handle_input(&self, data: &JsString) {
        if pi_tui::matches_key(data.clone(), pi_tui::Key::ctrl("c"))
            || pi_tui::matches_key(data.clone(), pi_tui::Key::ctrl("d"))
        {
            self.close();
            return;
        }
        self.input.borrow_mut().handle_input(data);
    }

    /// `handleEvent`.
    /// `handleEvent`: applies one wire event to the transcript.
    pub fn handle_event(&self, event: ResidentConsoleEvent) {
        match event {
            ResidentConsoleEvent::EntryAppended { entry } => self.append_entry(&entry),
            ResidentConsoleEvent::AgentStart => {
                self.set_status(with_theme(|theme| theme.fg(ThemeColor::Muted, "Working…")));
            }
            ResidentConsoleEvent::AgentEnd => self.set_status(""),
            ResidentConsoleEvent::MessageStart {
                message,
                runtime_message_marker,
            } => self.start_message(&message, runtime_message_marker),
            ResidentConsoleEvent::MessageUpdate { message, .. } => {
                if let AgentMessage::Assistant(assistant) = &message {
                    self.update_streaming_assistant(assistant);
                }
            }
            ResidentConsoleEvent::MessageEnd {
                message,
                runtime_message_marker,
            } => {
                if let AgentMessage::Assistant(assistant) = &message {
                    if is_duplicate_turn_assistant_message(None, runtime_message_marker) {
                        self.remove_streaming_assistant();
                    } else {
                        self.update_streaming_assistant(assistant);
                        *self.streaming_assistant.borrow_mut() = None;
                    }
                }
            }
            ResidentConsoleEvent::Other => {}
        }
    }

    fn start_message(&self, message: &AgentMessage, marker: Option<RuntimeMessageMarker>) {
        match message {
            AgentMessage::User(user) => {
                if is_duplicate_turn_guard_message(None, marker) {
                    return;
                }
                self.chat.borrow_mut().add_child(component(UserMessageComponent::new(
                    read_user_content(&user.content),
                    None,
                    1,
                )));
            }
            AgentMessage::ToolResult(result) if result.tool_name == SUPERVISOR_RESPONSE_TOOL_NAME => {
                self.chat
                    .borrow_mut()
                    .add_child(component(Text::new(read_blocks(&result.content), 1, 0)));
            }
            AgentMessage::Assistant(assistant) => {
                let assistant_component = Rc::new(RefCell::new(AssistantMessageComponent::new(
                    Some(assistant.as_ref().clone()),
                    false,
                    None,
                    None,
                    1,
                )));
                let handle = component(SharedAssistant(Rc::clone(&assistant_component)));
                self.chat.borrow_mut().add_child(Rc::clone(&handle));
                *self.streaming_assistant.borrow_mut() = Some((assistant_component, handle));
            }
            _ => {}
        }
    }

    fn update_streaming_assistant(&self, assistant: &pi_ai::types::AssistantMessage) {
        if let Some((component, _)) = self.streaming_assistant.borrow().as_ref() {
            component.borrow_mut().update_content(assistant.clone());
        }
    }

    fn remove_streaming_assistant(&self) {
        if let Some((_, handle)) = self.streaming_assistant.borrow_mut().take() {
            self.chat.borrow_mut().remove_child(&handle);
        }
    }

    fn append_entry(&self, entry: &SessionEntry) {
        for message in session_entry_to_context_messages(entry) {
            match message {
                AgentMessage::Assistant(assistant) => {
                    self.chat
                        .borrow_mut()
                        .add_child(component(AssistantMessageComponent::new(
                            Some(assistant.as_ref().clone()),
                            false,
                            None,
                            None,
                            1,
                        )));
                }
                AgentMessage::User(user) => {
                    self.chat.borrow_mut().add_child(component(UserMessageComponent::new(
                        read_user_content(&user.content),
                        None,
                        1,
                    )));
                }
                AgentMessage::ToolResult(result) => {
                    self.chat
                        .borrow_mut()
                        .add_child(component(Text::new(read_blocks(&result.content), 1, 0)));
                }
                AgentMessage::Custom(object) => {
                    let content = object.get("content").map(read_value_content).unwrap_or_default();
                    self.chat.borrow_mut().add_child(component(Text::new(content, 1, 0)));
                }
            }
        }
    }
}

fn read_user_content(content: &UserContent) -> JsString {
    match content {
        UserContent::Text(text) => text.clone(),
        UserContent::Blocks(blocks) => read_blocks(blocks),
    }
}

fn read_blocks(blocks: &[TextOrImageContent]) -> JsString {
    let texts = blocks.iter().filter_map(|block| match block {
        TextOrImageContent::Text(text) => Some(text.text.clone()),
        TextOrImageContent::Image(_) => None,
    });
    JsString::join(texts, "\n")
}

fn read_value_content(content: &JsValue) -> JsString {
    match content {
        JsValue::String(text) => text.clone(),
        JsValue::Array(parts) => JsString::join(
            parts.iter().filter_map(|part| {
                let object = part.as_object()?;
                if *object.get("type")?.as_string()? != *"text" {
                    return None;
                }
                object.get("text")?.as_string().cloned()
            }),
            "\n",
        ),
        _ => JsString::default(),
    }
}

struct ConsoleComponent {
    ui: Rc<ResidentConsoleUi>,
}

impl Component for ConsoleComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.ui.root.borrow_mut().render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.ui.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.ui.root.borrow_mut().invalidate();
    }
}

struct SharedContainer(Rc<RefCell<Container>>);

impl SharedContainer {
    fn create() -> (Rc<RefCell<Container>>, ComponentRef) {
        let container = Rc::new(RefCell::new(Container::new()));
        let component = component(Self(Rc::clone(&container)));
        (container, component)
    }
}

impl Component for SharedContainer {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0.borrow_mut().render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.0.borrow_mut().invalidate();
    }
}

struct SharedText(Rc<RefCell<Text>>);

impl Component for SharedText {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0.borrow_mut().render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.0.borrow_mut().invalidate();
    }
}

struct SharedInput(Rc<RefCell<Input>>);

impl Component for SharedInput {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0.borrow_mut().render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.0.borrow_mut().handle_input(data);
    }

    fn wants_input(&self) -> bool {
        self.0.borrow().focused()
    }

    fn invalidate(&mut self) {
        self.0.borrow_mut().invalidate();
    }
}

struct SharedAssistant(Rc<RefCell<AssistantMessageComponent>>);

impl Component for SharedAssistant {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0.borrow_mut().render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.0.borrow_mut().handle_input(data);
    }

    fn invalidate(&mut self) {
        self.0.borrow_mut().invalidate();
    }
}
