//! Opt-in native RPC/TUI chat client. Host supplies a child embedding rpc-demo.
//! No child is spawned until `run`; no real Pi process is required by fixtures.
use pi_js_compat::string::js_trim;
use pi_tui::{
    Input,
    components::{
        input::InputOptions,
        select_list::{SelectItem, SelectList, SelectListTheme},
    },
    terminal::Terminal,
    tui::{Component, Focusable, TuiExt, TuiMainScreen},
};
use serde_json::{Value, json};
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
fn field<'a>(v: &'a Value, key: &str, default: &'a str) -> &'a str {
    v[key].as_str().unwrap_or(default)
}
fn clipped(s: &str, n: usize) -> String {
    String::from_utf16_lossy(&s.encode_utf16().take(n).collect::<Vec<_>>())
}
#[derive(Default)]
struct OutputLog {
    lines: VecDeque<String>,
}
impl OutputLog {
    fn append(&mut self, s: impl Into<String>) {
        self.lines.push_back(s.into());
        if self.lines.len() > 1000 {
            self.lines.pop_front();
        }
    }
    fn raw(&mut self, s: &str) {
        if let Some(line) = self.lines.back_mut() {
            line.push_str(s);
        } else {
            self.lines.push_back(s.into());
        }
    }
    fn render(&self, width: usize) -> Vec<String> {
        if self.lines.is_empty() {
            vec![String::new()]
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
        title: String,
        list: SelectList,
        completion: Completion,
    },
    Input {
        title: String,
        input: Input,
        completion: Completion,
    },
}
enum Action {
    Submit(String),
    Escape,
    Complete(Option<String>),
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
    fn select(&mut self, title: String, options: Vec<String>, completion: Completion) {
        let theme = SelectListTheme {
            selected_prefix: Rc::new(|s| format!("{MAGENTA}{s}{RESET}")),
            selected_text: Rc::new(|s| format!("{MAGENTA}{s}{RESET}")),
            description: Rc::new(|s| format!("{DIM}{s}{RESET}")),
            scroll_info: Rc::new(|s| format!("{DIM}{s}{RESET}")),
            no_match: Rc::new(|s| format!("{YELLOW}{s}{RESET}")),
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
    fn input(&mut self, title: String, prefill: Option<String>, completion: Completion) {
        let mut input = Input::new(InputOptions::default());
        if let Some(s) = prefill {
            input.set_value(s);
        }
        input.set_focused(self.focused);
        let a = self.actions.clone();
        input.on_submit = Some(Rc::new(move |s| {
            a.borrow_mut()
                .push_back(Action::Complete((!js_trim(s).is_empty()).then(|| js_trim(s).into())))
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
    fn notify(&mut self, message: &str) {
        self.log
            .append(format!("{MAGENTA}{BOLD}Notification:{RESET} {message}"));
    }
    fn complete(&mut self, value: Option<String>) {
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
                .push(json!({"type":"extension_ui_response","id":id,"confirmed":value.as_deref()==Some("Yes")})),
            Completion::Rpc { id, confirm: false } => self.commands.push(match value {
                Some(v) => json!({"type":"extension_ui_response","id":id,"value":v}),
                None => json!({"type":"extension_ui_response","id":id,"cancelled":true}),
            }),
            Completion::Local(kind) => {
                let message = match (kind, value) {
                    ("select", Some(v)) => format!("You picked: {v}"),
                    ("select", None) => "Selection cancelled".into(),
                    ("confirm", v) => format!("Confirmed: {}", v.as_deref() == Some("Yes")),
                    ("input", Some(v)) => format!("You entered: {v}"),
                    ("input", None) => "Input cancelled".into(),
                    ("editor", Some(v)) => format!("Submitted: {v}"),
                    _ => "Editor cancelled".into(),
                };
                self.notify(&message);
            }
        }
    }
    fn submit(&mut self, value: &str) {
        let value = js_trim(value);
        if value.is_empty() {
            return;
        }
        self.prompt.set_value("");
        match value {
            "/select" => self.select(
                "Pick a color".into(),
                ["Red", "Green", "Blue", "Yellow"].map(str::to_owned).to_vec(),
                Completion::Local("select"),
            ),
            "/confirm" => self.select(
                "Are you sure?".into(),
                ["Yes", "No"].map(str::to_owned).to_vec(),
                Completion::Local("confirm"),
            ),
            "/input" => self.input("Enter your name".into(), None, Completion::Local("input")),
            "/editor" => self.input(
                "Edit text".into(),
                Some("Hello, world!".into()),
                Completion::Local("editor"),
            ),
            _ => self.commands.push(json!({"type":"prompt","message":value})),
        }
        self.log.append(format!("{GREEN}{BOLD}You:{RESET} {value}"));
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
            "select" => self.select(
                field(req, "title", "Select").into(),
                req["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                Completion::Rpc { id, confirm: false },
            ),
            "confirm" => {
                let title = if let Some(message) = req["message"].as_str().filter(|s| !s.is_empty()) {
                    format!("{}: {message}", field(req, "title", "undefined"))
                } else {
                    field(req, "title", "Confirm").into()
                };
                self.select(
                    title,
                    ["Yes", "No"].map(str::to_owned).to_vec(),
                    Completion::Rpc { id, confirm: true },
                );
            }
            "input" => {
                let title = if let Some(p) = req["placeholder"].as_str().filter(|s| !s.is_empty()) {
                    format!("{} ({p})", field(req, "title", "undefined"))
                } else {
                    field(req, "title", "Input").into()
                };
                self.input(title, None, Completion::Rpc { id, confirm: false });
            }
            "editor" => self.input(
                field(req, "title", "Editor").into(),
                req["prefill"].as_str().map(|s| s.replace('\n', " ")),
                Completion::Rpc { id, confirm: false },
            ),
            "notify" => {
                let color = match field(req, "notifyType", "info") {
                    "error" => RED,
                    "warning" => YELLOW,
                    _ => MAGENTA,
                };
                self.log.append(format!(
                    "{color}{BOLD}Notification:{RESET} {}",
                    field(req, "message", "undefined")
                ));
            }
            "setStatus" => self.log.append(format!(
                "{MAGENTA}{BOLD}Notification:{RESET} {DIM}[status: {}]{RESET} {}",
                field(req, "statusKey", "undefined"),
                field(req, "statusText", "(cleared)")
            )),
            "setWidget" => {
                if let Some(lines) = req["widgetLines"].as_array().filter(|a| !a.is_empty()) {
                    self.log.append(format!(
                        "{MAGENTA}{BOLD}Notification:{RESET} {DIM}[widget: {}]{RESET}",
                        field(req, "widgetKey", "undefined")
                    ));
                    for line in lines {
                        self.log
                            .append(format!("  {DIM}{}{RESET}", line.as_str().unwrap_or("undefined")));
                    }
                }
            }
            "set_editor_text" => self.prompt.set_value(field(req, "text", "")),
            _ => {}
        }
    }
    pub fn receive(&mut self, data: &Value) {
        match field(data, "type", "") {
            "response" if !data["success"].as_bool().unwrap_or(false) => self.log.append(format!(
                "{RED}[error]{RESET} {}: {}",
                field(data, "command", "undefined"),
                field(data, "error", "undefined")
            )),
            "agent_start" => {
                if !self.streaming {
                    self.streaming = true;
                    self.has_text = false;
                    self.dots = 1;
                }
            }
            "extension_ui_request" => self.extension_request(data),
            "message_update" if data["assistantMessageEvent"]["type"] == "text_delta" => {
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
                        self.log.raw(part);
                    }
                }
            }
            "tool_execution_start" => self
                .log
                .append(format!("{DIM}[tool: {}]{RESET}", field(data, "toolName", "undefined"))),
            "tool_execution_end" => self.log.append(format!(
                "{DIM}[result: {}...]{RESET}",
                clipped(
                    &data
                        .get("result")
                        .map(Value::to_string)
                        .unwrap_or_else(|| "undefined".into()),
                    120
                )
            )),
            "agent_end" => {
                self.streaming = false;
                self.log.append("");
            }
            _ => {}
        }
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
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = self.log.render(width);
        if self.streaming {
            lines.push(format!(
                "{BLUE}{BOLD}Agent:{RESET} {DIM}Working{}{RESET}",
                ".".repeat(self.dots)
            ));
        }
        match &mut self.dialog {
            Some(Dialog::Select { title, list, .. }) => {
                lines.push(format!("{MAGENTA}{BOLD}{title}{RESET}"));
                lines.extend(list.render(width));
                lines.push(format!("{DIM}Up/Down, Enter to select, Esc to cancel{RESET}"));
            }
            Some(Dialog::Input { title, input, .. }) => {
                lines.push(format!("{MAGENTA}{BOLD}{title}{RESET}"));
                lines.extend(input.render(width));
                lines.push(format!("{DIM}Enter to submit, Esc to cancel{RESET}"));
            }
            None => {
                lines.push(format!("{GREEN}{BOLD}You:{RESET}"));
                lines.extend(self.prompt.render(width));
            }
        }
        lines
    }
    fn handle_input(&mut self, data: &str) {
        match &mut self.dialog {
            Some(Dialog::Select { list, .. }) => list.handle_input(data),
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
    pub prefix: Vec<String>,
    pub extension: String,
    pub cwd: PathBuf,
}
pub struct RunOutcome {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}
/// Drives the real TuiMainScreen and real child pipes on the caller's UI thread.
pub async fn run(invocation: Invocation, terminal: Rc<RefCell<dyn Terminal>>) -> Result<RunOutcome, String> {
    let mut child = tokio::process::Command::new(invocation.program)
        .args(invocation.prefix)
        .args([
            "--mode",
            "rpc",
            "--no-session",
            "--no-extension",
            "--extension",
            &invocation.extension,
        ])
        .current_dir(invocation.cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stderr = child.stderr.take().expect("piped stderr");
    let errors = tokio::spawn(async move {
        let mut bytes = vec![];
        stderr
            .read_to_end(&mut bytes)
            .await
            .map(|_| String::from_utf8_lossy(&bytes).into_owned())
    });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    if child.try_wait().map_err(|e| e.to_string())?.is_some() {
        let stderr = errors.await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        return Ok(RunOutcome {
            code: 1,
            stdout: String::new(),
            stderr: format!("Agent exited immediately. Stderr:\n{stderr}\n"),
        });
    }
    let tui = TuiMainScreen::new(terminal);
    let app = Rc::new(RefCell::new(RpcExtensionUi::new()));
    let component: pi_tui::tui::ComponentRef = app.clone();
    tui.add_child(component.clone());
    tui.set_focus(Some(component));
    tui.start().map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let mut reader = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
    let mut input = tokio::time::interval(std::time::Duration::from_millis(16));
    let mut timer = tokio::time::Instant::now();
    let mut was_streaming = false;
    let event_loop=async{
  loop{
   tokio::select!{
    line=reader.next_line()=>{let Some(line)=line.map_err(|e|e.to_string())?else{break;};if let Ok(data)=serde_json::from_str(&line){app.borrow_mut().receive(&data);tui.request_render(false);}},
    _=input.tick()=>{tui.process_events();let streaming=app.borrow().streaming;if streaming&&!was_streaming{timer=tokio::time::Instant::now();}if streaming&&timer.elapsed()>=std::time::Duration::from_millis(400){app.borrow_mut().tick();timer=tokio::time::Instant::now();tui.request_render(false);}was_streaming=streaming;tui.flush_pending();},
   }
   let commands=app.borrow_mut().take_commands();for command in commands{let mut bytes=serde_json::to_vec(&command).map_err(|e|e.to_string())?;bytes.push(b'\n');stdin.write_all(&bytes).await.map_err(|e|e.to_string())?;}
   if app.borrow().exit_requested(){if let Some(pid)=child.id().and_then(|p|rustix::process::Pid::from_raw(p as i32)){let _=rustix::process::kill_process(pid,rustix::process::Signal::TERM);}break;}
  }Ok::<_,String>(())
 }.await;
    tui.flush_pending();
    tui.stop();
    if event_loop.is_err() {
        let _ = child.start_kill();
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    let stderr = errors.await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    event_loop?;
    if app.borrow().exit_requested() {
        return Ok(RunOutcome {
            code: 0,
            stdout: String::new(),
            stderr: String::new(),
        });
    }
    Ok(RunOutcome {
        code: status.code().unwrap_or(0),
        stdout: format!(
            "Agent exited with code {}\n",
            status.code().map(|c| c.to_string()).unwrap_or_else(|| "null".into())
        ),
        stderr: if stderr.is_empty() {
            stderr
        } else {
            format!("{stderr}\n")
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
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let invocation = Invocation {
        program: program.into(),
        prefix: args.collect(),
        extension,
        cwd,
    };
    let terminal = Rc::new(RefCell::new(pi_tui::terminal::ProcessTerminal::new()));
    match run(invocation, terminal).await {
        Ok(outcome) => {
            print!("{}", outcome.stdout);
            eprint!("{}", outcome.stderr);
            std::process::exit(outcome.code);
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
