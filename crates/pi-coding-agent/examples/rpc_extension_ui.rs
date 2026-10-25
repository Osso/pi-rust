//! Opt-in native RPC/TUI chat client. Host supplies a child embedding rpc-demo.
//! No child is spawned until `run`; no real Pi process is required by fixtures.
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue as Value;
use pi_tui::{
    Input, TUI,
    components::{
        input::InputOptions,
        select_list::{SelectItem, SelectList, SelectListTheme},
    },
    terminal::Terminal,
    tui::{Component, Focusable, TuiExt},
};
use std::{cell::RefCell, collections::VecDeque, path::PathBuf, rc::Rc};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const RED: &str = "\x1b[31m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";
fn field(v: &Value, key: &str, default: &str) -> JsString {
    v[key].as_string().cloned().unwrap_or_else(|| JsString::from(default))
}
fn clipped(s: impl Into<JsString>, n: usize) -> JsString {
    let s = s.into();
    s.slice(0, n.min(s.len()))
}
#[derive(Default)]
struct OutputLog {
    lines: VecDeque<JsString>,
}
impl OutputLog {
    fn append(&mut self, s: impl Into<JsString>) {
        self.lines.push_back(s.into());
        if self.lines.len() > 1000 {
            self.lines.pop_front();
        }
    }
    fn raw(&mut self, s: &JsString) {
        if let Some(line) = self.lines.back_mut() {
            line.push_str(s);
        } else {
            self.lines.push_back(s.into());
        }
    }
    fn render(&self, width: usize) -> Vec<JsString> {
        if self.lines.is_empty() {
            vec![JsString::default()]
        } else {
            self.lines.iter().map(|s| clipped(s, width)).collect()
        }
    }
}
enum Completion {
    Rpc { id: Value, confirm: bool },
    Local(&'static str),
}
enum Dialog {
    Select {
        title: JsString,
        list: SelectList,
        completion: Completion,
    },
    Input {
        title: JsString,
        input: Input,
        completion: Completion,
    },
}
enum Action {
    Submit(JsString),
    Escape,
    Complete(Option<JsString>),
}
/// Owns real Input/SelectList components. Network and terminal events stay on the UI thread.
pub struct RpcExtensionUi {
    log: OutputLog,
    prompt: Input,
    dialog: Option<Dialog>,
    actions: Rc<RefCell<VecDeque<Action>>>,
    commands: Vec<Value>,
    streaming: bool,
    has_text: bool,
    dots: usize,
    exit: bool,
    focused: bool,
}
impl RpcExtensionUi {
    pub fn new() -> Self {
        let actions = Rc::new(RefCell::new(VecDeque::new()));
        let mut prompt = Input::new(InputOptions::default());
        let a = actions.clone();
        prompt.on_submit = Some(Rc::new(move |s| a.borrow_mut().push_back(Action::Submit(s.into()))));
        let a = actions.clone();
        prompt.on_escape = Some(Rc::new(move || a.borrow_mut().push_back(Action::Escape)));
        let mut log = OutputLog::default();
        log.append(format!("{BOLD}RPC Chat{RESET}"));
        log.append(format!(
            "{DIM}Type a message and press Enter. Esc to abort or exit. Ctrl+D to quit.{RESET}"
        ));
        log.append(format!("{DIM}Slash commands: /select /confirm /input /editor{RESET}"));
        log.append("");
        Self {
            log,
            prompt,
            dialog: None,
            actions,
            commands: vec![],
            streaming: false,
            has_text: false,
            dots: 1,
            exit: false,
            focused: false,
        }
    }
    pub fn exit_requested(&self) -> bool {
        self.exit
    }
    pub fn take_commands(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.commands)
    }
    pub fn tick(&mut self) {
        if self.streaming {
            self.dots = self.dots % 3 + 1;
        }
    }
    fn select(&mut self, title: JsString, options: Vec<JsString>, completion: Completion) {
        let theme = SelectListTheme {
            selected_prefix: Rc::new(|s| js_format!("{}{}{}", MAGENTA, s, RESET)),
            selected_text: Rc::new(|s| js_format!("{}{}{}", MAGENTA, s, RESET)),
            description: Rc::new(|s| js_format!("{}{}{}", DIM, s, RESET)),
            scroll_info: Rc::new(|s| js_format!("{}{}{}", DIM, s, RESET)),
            no_match: Rc::new(|s| js_format!("{}{}{}", YELLOW, s, RESET)),
        };
        let count = options.len().min(8);
        let items = options.into_iter().map(|s| SelectItem::new(s.clone(), s)).collect();
        let mut list = SelectList::new(items, count, theme, Default::default());
        let a = self.actions.clone();
        list.on_select = Some(Rc::new(move |item| {
            a.borrow_mut().push_back(Action::Complete(Some(item.value.clone())))
        }));
        let a = self.actions.clone();
        list.on_cancel = Some(Rc::new(move || a.borrow_mut().push_back(Action::Complete(None))));
        self.prompt.set_focused(false);
        self.dialog = Some(Dialog::Select {
            title,
            list,
            completion,
        });
    }
    fn input(&mut self, title: JsString, prefill: Option<JsString>, completion: Completion) {
        let mut input = Input::new(InputOptions::default());
        if let Some(s) = prefill {
            input.set_value(s);
        }
        input.set_focused(self.focused);
        let a = self.actions.clone();
        input.on_submit = Some(Rc::new(move |s| {
            a.borrow_mut()
                .push_back(Action::Complete((!(s).trim().is_empty()).then(|| (s).trim())))
        }));
        let a = self.actions.clone();
        input.on_escape = Some(Rc::new(move || a.borrow_mut().push_back(Action::Complete(None))));
        self.prompt.set_focused(false);
        self.dialog = Some(Dialog::Input {
            title,
            input,
            completion,
        });
    }
    fn notify(&mut self, message: &JsString) {
        self.log
            .append(js_format!("{}{}Notification:{} {}", MAGENTA, BOLD, RESET, message));
    }
    fn complete(&mut self, value: Option<JsString>) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        let completion = match dialog {
            Dialog::Select { completion, .. } | Dialog::Input { completion, .. } => completion,
        };
        self.prompt.set_focused(self.focused);
        match completion {
            Completion::Rpc { id, confirm: true } => self
                .commands
                .push(json!({"type":"extension_ui_response","id":id,"confirmed":value.as_ref().is_some_and(|value| value == "Yes")})),
            Completion::Rpc { id, confirm: false } => self.commands.push(match value {
                Some(v) => json!({"type":"extension_ui_response","id":id,"value":v}),
                None => json!({"type":"extension_ui_response","id":id,"cancelled":true}),
            }),
            Completion::Local(kind) => {
                let message = match (kind, value) {
                    ("select", Some(v)) => js_format!("You picked: {}", v),
                    ("select", None) => "Selection cancelled".into(),
                    ("confirm", v) => JsString::from(format!("Confirmed: {}", v.as_ref().is_some_and(|value| value == "Yes"))),
                    ("input", Some(v)) => js_format!("You entered: {}", v),
                    ("input", None) => "Input cancelled".into(),
                    ("editor", Some(v)) => js_format!("Submitted: {}", v),
                    _ => "Editor cancelled".into(),
                };
                self.notify(&JsString::from(&message));
            }
        }
    }
    fn submit(&mut self, value: &JsString) {
        let value = value.trim();
        if value.is_empty() {
            return;
        }
        self.prompt.set_value("");
        match &value {
            command if command == "/select" => self.select(
                "Pick a color".into(),
                ["Red", "Green", "Blue", "Yellow"]
                    .map(JsString::from)
                    .to_vec()
                    .into_iter()
                    .collect::<Vec<_>>(),
                Completion::Local("select"),
            ),
            command if command == "/confirm" => self.select(
                "Are you sure?".into(),
                ["Yes", "No"]
                    .map(JsString::from)
                    .to_vec()
                    .into_iter()
                    .collect::<Vec<_>>(),
                Completion::Local("confirm"),
            ),
            command if command == "/input" => self.input("Enter your name".into(), None, Completion::Local("input")),
            command if command == "/editor" => self.input(
                "Edit text".into(),
                Some("Hello, world!".into()),
                Completion::Local("editor"),
            ),
            _ => self.commands.push(json!({"type":"prompt","message":value})),
        }
        self.log.append(js_format!("{}{}You:{} {}", GREEN, BOLD, RESET, value));
    }
    fn drain_actions(&mut self) {
        loop {
            let action = self.actions.borrow_mut().pop_front();
            let Some(action) = action else {
                break;
            };
            match action {
                Action::Submit(s) => self.submit(&s),
                Action::Complete(value) => self.complete(value),
                Action::Escape => {
                    if self.streaming {
                        self.commands.push(json!({"type":"abort"}));
                        self.log.append(format!("{YELLOW}[aborted]{RESET}"));
                    } else {
                        self.exit = true;
                    }
                }
            }
        }
    }
    fn extension_request(&mut self, req: &Value) {
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        match field(req, "method", "") {
            matched_tag if matched_tag == "select" => self.select(
                field(req, "title", "Select"),
                req["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_string)
                    .cloned()
                    .collect(),
                Completion::Rpc { id, confirm: false },
            ),
            matched_tag if matched_tag == "confirm" => {
                let title = if let Some(message) = req["message"].as_string().filter(|s| !s.is_empty()) {
                    js_format!("{}: {}", field(req, "title", "undefined"), message)
                } else {
                    field(req, "title", "Confirm")
                };
                self.select(
                    title,
                    ["Yes", "No"].map(JsString::from).to_vec(),
                    Completion::Rpc { id, confirm: true },
                );
            }
            matched_tag if matched_tag == "input" => {
                let title = if let Some(p) = req["placeholder"].as_string().filter(|s| !s.is_empty()) {
                    js_format!("{} ({})", field(req, "title", "undefined"), p)
                } else {
                    field(req, "title", "Input")
                };
                self.input(title, None, Completion::Rpc { id, confirm: false });
            }
            matched_tag if matched_tag == "editor" => self.input(
                field(req, "title", "Editor"),
                req["prefill"].as_string().map(|s| s.replace('\n', " ")),
                Completion::Rpc { id, confirm: false },
            ),
            matched_tag if matched_tag == "notify" => {
                let color = match field(req, "notifyType", "info") {
                    matched_tag if matched_tag == "error" => RED,
                    matched_tag if matched_tag == "warning" => YELLOW,
                    _ => MAGENTA,
                };
                self.log.append(js_format!(
                    "{}{}Notification:{} {}",
                    color,
                    BOLD,
                    RESET,
                    field(req, "message", "undefined")
                ));
            }
            matched_tag if matched_tag == "setStatus" => self.log.append(js_format!(
                "{}{}Notification:{} {}[status: {}]{} {}",
                MAGENTA,
                BOLD,
                RESET,
                DIM,
                field(req, "statusKey", "undefined"),
                RESET,
                field(req, "statusText", "(cleared)")
            )),
            matched_tag if matched_tag == "setWidget" => {
                if let Some(lines) = req["widgetLines"].as_array().filter(|a| !a.is_empty()) {
                    self.log.append(js_format!(
                        "{}{}Notification:{} {}[widget: {}]{}",
                        MAGENTA,
                        BOLD,
                        RESET,
                        DIM,
                        field(req, "widgetKey", "undefined"),
                        RESET
                    ));
                    for line in lines {
                        self.log.append(js_format!(
                            "  {}{}{}",
                            DIM,
                            line.as_string().cloned().unwrap_or_else(|| JsString::from("undefined")),
                            RESET
                        ));
                    }
                }
            }
            matched_tag if matched_tag == "set_editor_text" => self.prompt.set_value(field(req, "text", "")),
            _ => {}
        }
    }
    pub fn receive(&mut self, data: &Value) {
        match field(data, "type", "") {
            matched_tag if matched_tag == "response" && (!data["success"].as_bool().unwrap_or(false)) => {
                self.log.append(js_format!(
                    "{}[error]{} {}: {}",
                    RED,
                    RESET,
                    field(data, "command", "undefined"),
                    field(data, "error", "undefined")
                ))
            }
            matched_tag if matched_tag == "agent_start" => {
                if !self.streaming {
                    self.streaming = true;
                    self.has_text = false;
                    self.dots = 1;
                }
            }
            matched_tag if matched_tag == "extension_ui_request" => self.extension_request(data),
            matched_tag
                if matched_tag == "message_update"
                    && (data["assistantMessageEvent"]["type"] == pi_js_compat::js_json!("text_delta")) =>
            {
                if !self.has_text {
                    self.has_text = true;
                    self.log.append("");
                    self.log.append(format!("{BLUE}{BOLD}Agent:{RESET}"));
                }
                for (i, part) in field(&data["assistantMessageEvent"], "delta", "")
                    .split('\n')
                    .enumerate()
                {
                    if i > 0 {
                        self.log.append("");
                    }
                    if !part.is_empty() {
                        self.log.raw(&part);
                    }
                }
            }
            matched_tag if matched_tag == "tool_execution_start" => self.log.append(js_format!(
                "{}[tool: {}]{}",
                DIM,
                field(data, "toolName", "undefined"),
                RESET
            )),
            matched_tag if matched_tag == "tool_execution_end" => self.log.append(js_format!(
                "{}[result: {}...]{}",
                DIM,
                clipped(
                    data.get("result")
                        .map(pi_js_compat::js_json::stringify_lossless)
                        .unwrap_or_else(|| "undefined".into()),
                    120
                ),
                RESET
            )),
            matched_tag if matched_tag == "agent_end" => {
                self.streaming = false;
                self.log.append("");
            }
            _ => {}
        }
    }
}

impl Default for RpcExtensionUi {
    fn default() -> Self {
        Self::new()
    }
}
impl Focusable for RpcExtensionUi {
    fn focused(&self) -> bool {
        self.focused
    }
    fn set_focused(&mut self, value: bool) {
        self.focused = value;
        match &mut self.dialog {
            Some(Dialog::Input { input, .. }) => input.set_focused(value),
            Some(Dialog::Select { .. }) => {}
            None => self.prompt.set_focused(value),
        }
    }
}
impl Component for RpcExtensionUi {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = self.log.render(width);
        if self.streaming {
            lines.push(JsString::from(format!(
                "{BLUE}{BOLD}Agent:{RESET} {DIM}Working{}{RESET}",
                ".".repeat(self.dots)
            )));
        }
        match &mut self.dialog {
            Some(Dialog::Select { title, list, .. }) => {
                lines.push(js_format!("{}{}{}{}", MAGENTA, BOLD, &*title, RESET));
                lines.extend(list.render(width));
                lines.push(JsString::from(format!(
                    "{DIM}Up/Down, Enter to select, Esc to cancel{RESET}"
                )));
            }
            Some(Dialog::Input { title, input, .. }) => {
                lines.push(js_format!("{}{}{}{}", MAGENTA, BOLD, &*title, RESET));
                lines.extend(input.render(width));
                lines.push(JsString::from(format!("{DIM}Enter to submit, Esc to cancel{RESET}")));
            }
            None => {
                lines.push(JsString::from(format!("{GREEN}{BOLD}You:{RESET}")));
                lines.extend(self.prompt.render(width));
            }
        }
        lines.into_iter().collect::<Vec<_>>()
    }
    fn handle_input(&mut self, data: &JsString) {
        match &mut self.dialog {
            Some(Dialog::Select { list, .. }) => list.handle_input(&JsString::from(data)),
            Some(Dialog::Input { input, .. }) => input.handle_input(data),
            None => self.prompt.handle_input(data),
        }
        self.drain_actions();
    }
    fn invalidate(&mut self) {
        self.prompt.invalidate();
        match &mut self.dialog {
            Some(Dialog::Select { list, .. }) => list.invalidate(),
            Some(Dialog::Input { input, .. }) => input.invalidate(),
            None => {}
        }
    }
    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}
pub struct Invocation {
    pub program: PathBuf,
    pub prefix: Vec<JsString>,
    pub extension: JsString,
    pub cwd: PathBuf,
}
pub struct RunOutcome {
    pub code: i32,
    pub stdout: JsString,
    pub stderr: JsString,
}
/// Drives the real TUI and real child pipes on the caller's UI thread.
pub async fn run(
    invocation: Invocation,
    terminal: Rc<RefCell<dyn Terminal>>,
) -> Result<RunOutcome, pi_js_compat::callback_error::CallbackError> {
    let mut child = tokio::process::Command::new(invocation.program)
        .args(invocation.prefix.iter().map(pi_js_compat::js_json::encode_node_utf8))
        .args([
            "--mode",
            "rpc",
            "--no-session",
            "--no-extension",
            "--extension",
            &pi_js_compat::js_json::encode_node_utf8(&invocation.extension),
        ])
        .current_dir(invocation.cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stderr = child.stderr.take().expect("piped stderr");
    let mut errors = Vec::new();
    let mut stderr_open = true;
    let startup = tokio::time::sleep(std::time::Duration::from_millis(500));
    tokio::pin!(startup);
    loop {
        tokio::select! {
            biased;
            read = stderr.read_buf(&mut errors), if stderr_open => {
                stderr_open = read.map_err(|e| e.to_string())? != 0;
            }
            _ = &mut startup => break,
        }
    }
    if child.try_wait().map_err(|e| e.to_string())?.is_some() {
        let stderr = String::from_utf8_lossy(&errors);
        return Ok(RunOutcome {
            code: 1,
            stdout: JsString::default(),
            stderr: JsString::from(format!("Agent exited immediately. Stderr:\n{stderr}\n")),
        });
    }
    let tui = TUI::new(terminal);
    let app = Rc::new(RefCell::new(RpcExtensionUi::new()));
    let component: pi_tui::tui::ComponentRef = app.clone();
    tui.add_child(component.clone());
    tui.set_focus(Some(component));
    tui.start().map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let mut reader = BufReader::new(child.stdout.take().expect("piped stdout"));
    let mut line = Vec::new();
    let mut stdout_open = true;
    let mut input = tokio::time::interval(std::time::Duration::from_millis(16));
    let mut timer = tokio::time::Instant::now();
    let mut was_streaming = false;
    let event_loop = async {
        loop {
            tokio::select! {
                biased;
                read = reader.read_until(b'\n', &mut line), if stdout_open => {
                    stdout_open = read.map_err(|e| e.to_string())? != 0;
                    if let Ok(data) = serde_json::from_str(&String::from_utf8_lossy(&line)) {
                        app.borrow_mut().receive(&data);
                        tui.request_render(false);
                    }
                    line.clear();
                }
                read = stderr.read_buf(&mut errors), if stderr_open => {
                    stderr_open = read.map_err(|e| e.to_string())? != 0;
                }
                status = child.wait() => return status.map(Some).map_err(|e| e.to_string()),
                _ = input.tick() => {
                    tui.process_events();
                    let streaming = app.borrow().streaming;
                    if streaming && !was_streaming {
                        timer = tokio::time::Instant::now();
                    }
                    if streaming && timer.elapsed() >= std::time::Duration::from_millis(400) {
                        app.borrow_mut().tick();
                        timer = tokio::time::Instant::now();
                        tui.request_render(false);
                    }
                    was_streaming = streaming;
                    tui.flush_pending().unwrap_or_else(|error| pi_coding_agent::utils::uncaught::throw_callback_uncaught(error));
                }
            }
            let commands = app.borrow_mut().take_commands();
            for command in commands {
                let mut bytes = serde_json::to_vec(&command).map_err(|e| e.to_string())?;
                bytes.push(b'\n');
                stdin.write_all(&bytes).await.map_err(|e| e.to_string())?;
            }
            if app.borrow().exit_requested() {
                return Ok(None);
            }
        }
    }
    .await;
    tui.flush_pending()
        .unwrap_or_else(|error| pi_coding_agent::utils::uncaught::throw_callback_uncaught(error));
    tui.stop()?;
    if event_loop.is_err() {
        child.start_kill().map_err(|e| e.to_string())?;
        child.wait().await.map_err(|e| e.to_string())?;
    }
    let Some(status) = event_loop? else {
        if let Some(pid) = child.id().and_then(|p| rustix::process::Pid::from_raw(p as i32)) {
            rustix::process::kill_process(pid, rustix::process::Signal::TERM).map_err(|e| e.to_string())?;
        }
        return Ok(RunOutcome {
            code: 0,
            stdout: JsString::default(),
            stderr: JsString::default(),
        });
    };
    let stderr = JsString::from(String::from_utf8_lossy(&errors).as_ref());
    Ok(RunOutcome {
        code: status.code().unwrap_or(0),
        stdout: JsString::from(format!(
            "Agent exited with code {}\n",
            status.code().map(|c| c.to_string()).unwrap_or_else(|| "null".into())
        )),
        stderr: if stderr.is_empty() {
            stderr
        } else {
            js_format!("{}\n", stderr)
        },
    })
}

/// Explicit native invocation replaces the original Node/dist entry point.
#[cfg(not(test))]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(program), Some(extension)) = (args.next(), args.next()) else {
        eprintln!("Usage: rpc_extension_ui <native-host> <rpc-demo-registry-id> [host-prefix-args...]");
        std::process::exit(1);
    };
    let cwd = match pi_js_compat::process_cwd::cwd() {
        Ok(cwd) => std::path::PathBuf::from(pi_js_compat::js_json::encode_node_utf8(&cwd)),
        Err(error) => {
            eprintln!("{}", pi_js_compat::js_json::encode_node_utf8(&error.string_value()));
            std::process::exit(1);
        }
    };
    let invocation = Invocation {
        program: program.into(),
        prefix: args.map(JsString::from).collect(),
        extension: JsString::from(extension),
        cwd,
    };
    let terminal = Rc::new(RefCell::new(pi_tui::terminal::ProcessTerminal::new()));
    match run(invocation, terminal).await {
        Ok(outcome) => {
            print!("{}", pi_js_compat::js_json::encode_node_utf8(&outcome.stdout));
            eprint!("{}", pi_js_compat::js_json::encode_node_utf8(&outcome.stderr));
            std::process::exit(outcome.code);
        }
        Err(error) => {
            let message = error
                .error_or_string_message()
                .unwrap_or_else(|error| pi_coding_agent::utils::uncaught::throw_callback_uncaught(error));
            eprintln!("{}", pi_js_compat::js_json::encode_node_utf8(&message));
            std::process::exit(1);
        }
    }
}
