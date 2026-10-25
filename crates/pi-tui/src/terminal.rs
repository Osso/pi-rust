//! Ported from `packages/tui/src/terminal.ts`.
//!
//! The TypeScript `Terminal` interface installs input/resize callbacks. The Rust
//! port uses a typed event queue ([`TerminalEvent`]) instead: the terminal never
//! calls back into the TUI while it is borrowed, and `ProcessTerminal` reads stdin
//! on a worker thread that forwards bytes over a channel. Callers drive the queue
//! with [`Terminal::poll_events`].

use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustix::termios::{OptionalActions, Termios, tcgetattr, tcgetwinsize, tcsetattr};

use crate::native_modifiers::{ModifierKey, is_native_modifier_pressed};
use crate::stdin_buffer::{StdinBuffer, StdinBufferEvent, StdinBufferOptions};

const TERMINAL_PROGRESS_KEEPALIVE: Duration = Duration::from_millis(1000);
const TERMINAL_PROGRESS_ACTIVE_SEQUENCE: &str = "\x1b]9;4;3\x07";
const TERMINAL_PROGRESS_CLEAR_SEQUENCE: &str = "\x1b]9;4;0;\x07";
const APPLE_TERMINAL_SHIFT_ENTER_SEQUENCE: &str = "\x1b[13;2u";
const DESIRED_KITTY_KEYBOARD_PROTOCOL_FLAGS: u32 = 7;
const KEYBOARD_PROTOCOL_RESPONSE_FRAGMENT_TIMEOUT: Duration = Duration::from_millis(150);
const BRACKETED_PASTE_ENABLE: &str = "\x1b[?2004h";
const BRACKETED_PASTE_DISABLE: &str = "\x1b[?2004l";

fn kitty_keyboard_protocol_query() -> String {
    format!("\x1b[>{DESIRED_KITTY_KEYBOARD_PROTOCOL_FLAGS}u\x1b[?u\x1b[c")
}

/// A terminal input or resize event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEvent {
    Input(JsString),
    Resize,
}

/// Minimal terminal interface for the TUI.
pub trait Terminal {
    fn start(&mut self) -> Result<(), CallbackError>;
    fn stop(&mut self) -> Result<(), CallbackError>;
    /// Process-terminal cleanup usable from a panic hook on any thread.
    fn crash_restore(&self) -> Option<TerminalRestore> {
        None
    }
    fn drain_input(&mut self, max_ms: u64, idle_ms: u64);
    fn write(&mut self, data: &JsString);
    fn columns(&self) -> usize;
    fn rows(&self) -> usize;
    fn kitty_protocol_active(&self) -> bool;
    fn move_by(&mut self, lines: isize);
    fn hide_cursor(&mut self);
    fn show_cursor(&mut self);
    fn clear_line(&mut self);
    fn clear_from_cursor(&mut self);
    fn clear_screen(&mut self);
    fn set_title(&mut self, title: &JsString);
    fn set_progress(&mut self, active: bool);
    /// Drain queued input/resize events. The default has no event source.
    fn poll_events(&mut self) -> Vec<TerminalEvent> {
        Vec::new()
    }
    /// Drain terminal I/O failures observed since the last call.
    ///
    /// Node emits these as stream `error` events; the interactive mode routes
    /// them through `terminalErrorHandler`.
    fn take_errors(&mut self) -> Vec<TerminalIoError> {
        Vec::new()
    }
}

/// A failed terminal stream operation: the syscall Node names in the error
/// message (`read` for stdin, `write` for stdout) and the OS error.
#[derive(Debug)]
pub struct TerminalIoError {
    pub syscall: &'static str,
    pub error: std::io::Error,
}

/// A parsed Kitty keyboard protocol negotiation response.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyboardProtocolNegotiationSequence {
    KittyFlags { flags: f64 },
    DeviceAttributes,
}

/// Parse `CSI ? <flags> u` or `CSI ? <params> c`.
pub fn parse_keyboard_protocol_negotiation_sequence(
    sequence: impl Into<JsString>,
) -> Option<KeyboardProtocolNegotiationSequence> {
    let sequence = sequence.into();
    if let Some(body) = sequence.strip_prefix("\x1b[?").and_then(|rest| rest.strip_suffix('u')) {
        if !body.is_empty() && body.units().iter().all(|unit| (48..=57).contains(unit)) {
            let flags = pi_js_compat::number::parse_number(
                &body
                    .units()
                    .iter()
                    .map(|unit| char::from_u32(u32::from(*unit)).expect("ASCII flags"))
                    .collect::<String>(),
            );
            return flags
                .is_finite()
                .then_some(KeyboardProtocolNegotiationSequence::KittyFlags { flags });
        }
        return None;
    }
    if let Some(body) = sequence.strip_prefix("\x1b[?").and_then(|rest| rest.strip_suffix('c'))
        && body.units().iter().all(|unit| (48..=57).contains(unit) || *unit == 59)
    {
        return Some(KeyboardProtocolNegotiationSequence::DeviceAttributes);
    }
    None
}

fn is_keyboard_protocol_negotiation_sequence_prefix(sequence: &JsString) -> bool {
    if sequence == "\x1b[" {
        return true;
    }
    sequence
        .strip_prefix("\x1b[?")
        .is_some_and(|body| body.units().iter().all(|unit| (48..=57).contains(unit) || *unit == 59))
}

/// Whether the process is Apple Terminal (`platform` is a Rust `consts::OS` value).
pub fn is_apple_terminal_session(platform: &str, term_program: Option<&str>) -> bool {
    platform == "macos" && term_program == Some("Apple_Terminal")
}

/// Rewrite bare Return only for Apple Terminal with native Shift pressed.
pub fn normalize_apple_terminal_input(
    data: impl Into<JsString>,
    is_apple_terminal: bool,
    is_shift_pressed: bool,
) -> JsString {
    let data = data.into();
    if is_apple_terminal && data == "\r" && is_shift_pressed {
        return APPLE_TERMINAL_SHIFT_ENTER_SEQUENCE.into();
    }
    data
}

/// Raw-mode switch failure with the source message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRawModeError {
    pub flag: bool,
    pub cause: String,
}

impl TerminalRawModeError {
    pub fn new(flag: bool, cause: impl Into<String>) -> Self {
        Self {
            flag,
            cause: cause.into(),
        }
    }
}

impl std::fmt::Display for TerminalRawModeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let action = if self.flag { "enable" } else { "restore" };
        write!(formatter, "Failed to {action} terminal raw mode: {}", self.cause)
    }
}

impl std::error::Error for TerminalRawModeError {}

impl From<TerminalRawModeError> for CallbackError {
    fn from(error: TerminalRawModeError) -> Self {
        let native = pi_js_compat::node_error::NativeNodeError::new("TerminalRawModeError", error.to_string())
            .with_property(
                "cause",
                pi_js_compat::js_json::JsValue::String(error.cause.clone().into()),
            );
        Self::from(native).with_original(Arc::new(error))
    }
}

/// Detect raw-mode failures by type or by the Bun-style message prefix.
pub fn is_terminal_raw_mode_failure(error: &(dyn std::error::Error + 'static)) -> bool {
    if error.downcast_ref::<TerminalRawModeError>().is_some() {
        return true;
    }
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    lower.starts_with("setrawmode failed with errno:")
}

fn parse_positive_dimension(value: Option<String>) -> Option<usize> {
    let parsed = pi_js_compat::number::parse_number(&value?);
    if parsed.is_finite() && parsed > 0.0 {
        Some(parsed as usize)
    } else {
        None
    }
}

/// Resolve columns: terminal size, then `COLUMNS`, then 80.
pub fn resolve_columns(winsize: Option<u16>, env_columns: Option<String>) -> usize {
    winsize
        .map(usize::from)
        .filter(|columns| *columns > 0)
        .or_else(|| parse_positive_dimension(env_columns))
        .unwrap_or(80)
}

/// Resolve rows: terminal size, then `LINES`, then 24.
pub fn resolve_rows(winsize: Option<u16>, env_lines: Option<String>) -> usize {
    winsize
        .map(usize::from)
        .filter(|rows| *rows > 0)
        .or_else(|| parse_positive_dimension(env_lines))
        .unwrap_or(24)
}

/// The live stdout terminal width, absent for redirected output.
pub fn stdout_columns() -> Option<usize> {
    let stdout = std::io::stdout();
    let size = tcgetwinsize(&stdout).ok()?;
    Some(usize::from(size.ws_col))
}

fn terminal_size() -> Option<(u16, u16)> {
    let stdin = std::io::stdin();
    let size = tcgetwinsize(&stdin).ok()?;
    Some((size.ws_col, size.ws_row))
}

fn environment(name: &str) -> Option<String> {
    pi_js_compat::process_env::get(name)
}

fn write_log_path() -> Option<PathBuf> {
    let configured = environment("PI_TUI_WRITE_LOG")?;
    if configured.is_empty() {
        return None;
    }
    let path = PathBuf::from(&configured);
    if path.is_dir() {
        let millis = pi_js_compat::date::js_now_millis() as i64;
        let date = pi_js_compat::date::local_date_fields(millis).expect("resolve local write-log date");
        let seconds = millis.div_euclid(1000).rem_euclid(60);
        let stamp = format!(
            "{}-{:02}-{:02}_{:02}-{:02}-{seconds:02}",
            date.year,
            date.month + 1,
            date.day,
            date.hours,
            date.minutes
        );
        return Some(path.join(format!("tui-{stamp}-{}.log", std::process::id())));
    }
    Some(path)
}

fn wait_for_stdin(stdin: &std::io::Stdin) -> std::io::Result<bool> {
    use rustix::event::{PollFd, PollFlags, Timespec, poll};
    let mut descriptors = [PollFd::new(stdin, PollFlags::IN)];
    let timeout = Timespec {
        tv_sec: 0,
        tv_nsec: 10_000_000,
    };
    match poll(&mut descriptors, Some(&timeout)) {
        Ok(count) => Ok(count > 0),
        Err(rustix::io::Errno::INTR) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn send_stdin_chunk(stdin: &std::io::Stdin, sender: &std::sync::mpsc::Sender<Vec<u8>>) -> std::io::Result<bool> {
    let mut buffer = [0_u8; 4096];
    match rustix::io::read(stdin, &mut buffer) {
        Ok(0) => Ok(false),
        Ok(count) => Ok(sender.send(buffer[..count].to_vec()).is_ok()),
        Err(rustix::io::Errno::INTR) => Ok(true),
        Err(error) => Err(error.into()),
    }
}

fn read_stdin_chunks(
    sender: std::sync::mpsc::Sender<Vec<u8>>,
    errors: std::sync::mpsc::Sender<std::io::Error>,
    stop: Arc<AtomicBool>,
) {
    let stdin = std::io::stdin();
    while !stop.load(Ordering::Relaxed) {
        match wait_for_stdin(&stdin) {
            Ok(false) => continue,
            Err(error) => {
                let _ = errors.send(error);
                break;
            }
            Ok(true) => {}
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        match send_stdin_chunk(&stdin, &sender) {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => {
                let _ = errors.send(error);
                break;
            }
        }
    }
}

fn decode_stdin_chunk(pending: &mut Vec<u8>, chunk: &[u8]) -> String {
    pending.extend_from_slice(chunk);
    let mut decoded = String::new();
    while !pending.is_empty() {
        match std::str::from_utf8(pending) {
            Ok(text) => {
                decoded.push_str(text);
                pending.clear();
            }
            Err(error) => {
                let valid = error.valid_up_to();
                decoded.push_str(std::str::from_utf8(&pending[..valid]).expect("validated UTF-8 prefix"));
                pending.drain(..valid);
                let Some(invalid) = error.error_len() else {
                    break;
                };
                decoded.push(char::REPLACEMENT_CHARACTER);
                pending.drain(..invalid);
            }
        }
    }
    decoded
}

fn open_stdin_raw() -> Result<Termios, TerminalRawModeError> {
    let stdin = std::io::stdin();
    let original = tcgetattr(&stdin).map_err(|error| TerminalRawModeError::new(true, error.to_string()))?;
    let mut raw = original.clone();
    raw.make_raw();
    tcsetattr(&stdin, OptionalActions::Now, &raw)
        .map_err(|error| TerminalRawModeError::new(true, error.to_string()))?;
    Ok(original)
}

fn restore_stdin_mode(termios: &Termios) -> Result<(), TerminalRawModeError> {
    let stdin = std::io::stdin();
    tcsetattr(&stdin, OptionalActions::Now, termios)
        .map_err(|error| TerminalRawModeError::new(false, error.to_string()))
}

/// Send-able copy of terminal cleanup, independent of the UI's `Rc` graph.
#[derive(Clone, Default)]
pub struct TerminalRestore {
    state: Arc<Mutex<TerminalRestoreState>>,
}

#[derive(Default)]
struct TerminalRestoreState {
    termios: Option<Termios>,
    terminal_bytes: JsString,
    before: JsString,
    after: JsString,
}

impl TerminalRestore {
    /// Publish the renderer's exact stop output on the owning UI thread.
    pub fn publish_tui(&self, before: impl Into<JsString>, after: impl Into<JsString>) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.before = before.into();
        state.after = after.into();
    }

    /// Keep terminal writes excluded through the caller's final report/exit.
    pub fn restore_then(&self, report: impl FnOnce(std::io::Result<()>)) {
        let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = restore_process_terminal(&state);
        report(result);
    }
}

fn restore_process_terminal(state: &TerminalRestoreState) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(encode_node_utf8(&state.before).as_bytes())?;
    stdout.write_all(encode_node_utf8(&state.terminal_bytes).as_bytes())?;
    stdout.flush()?;
    if let Some(termios) = &state.termios {
        tcsetattr(std::io::stdin(), OptionalActions::Now, termios)?;
    }
    stdout.write_all(encode_node_utf8(&state.after).as_bytes())?;
    stdout.flush()
}

/// Real terminal over stdin/stdout.
pub struct ProcessTerminal {
    output: Box<dyn Write>,
    crash_restore: TerminalRestore,
    was_raw: Option<Termios>,
    input_handler: Option<Receiver<Vec<u8>>>,
    read_errors: Option<Receiver<std::io::Error>>,
    pending_errors: Vec<TerminalIoError>,
    reader_stop: Option<Arc<AtomicBool>>,
    reader_thread: Option<std::thread::JoinHandle<()>>,
    kitty_protocol_active: bool,
    modify_other_keys_active: bool,
    keyboard_protocol_pushed: bool,
    keyboard_protocol_negotiation_buffer: JsString,
    keyboard_protocol_flush_deadline: Option<Instant>,
    stdin_buffer: Option<StdinBuffer>,
    stdin_utf8_buffer: Vec<u8>,
    pending_events: Vec<TerminalEvent>,
    last_size: Option<(u16, u16)>,
    progress_active: bool,
    progress_last_keepalive: Instant,
    clock_now: Instant,
    write_log_path: Option<PathBuf>,
}

impl ProcessTerminal {
    pub fn new() -> Self {
        Self::with_output(Box::new(std::io::stdout()))
    }

    /// Construct a terminal with a caller-supplied output sink (tests, embeddings).
    pub fn with_output(output: Box<dyn Write>) -> Self {
        Self {
            output,
            crash_restore: TerminalRestore::default(),
            was_raw: None,
            input_handler: None,
            read_errors: None,
            pending_errors: Vec::new(),
            reader_stop: None,
            reader_thread: None,
            kitty_protocol_active: false,
            modify_other_keys_active: false,
            keyboard_protocol_pushed: false,
            keyboard_protocol_negotiation_buffer: JsString::default(),
            keyboard_protocol_flush_deadline: None,
            stdin_buffer: None,
            stdin_utf8_buffer: Vec::new(),
            pending_events: Vec::new(),
            last_size: None,
            progress_active: false,
            progress_last_keepalive: Instant::now(),
            clock_now: Instant::now(),
            write_log_path: write_log_path(),
        }
    }

    fn publish_terminal_restore(&self) {
        let mut state = self
            .crash_restore
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.termios = self.was_raw.clone();
        state.terminal_bytes = self.stop_sequences().into();
    }

    fn stop_sequences(&self) -> String {
        let progress = if self.progress_active {
            TERMINAL_PROGRESS_CLEAR_SEQUENCE
        } else {
            ""
        };
        let kitty = if self.keyboard_protocol_pushed || self.kitty_protocol_active {
            "\x1b[<u"
        } else {
            ""
        };
        let modify = if self.modify_other_keys_active {
            "\x1b[>4;0m"
        } else {
            ""
        };
        format!("{progress}{BRACKETED_PASTE_DISABLE}{kitty}{modify}")
    }

    pub fn modify_other_keys_active(&self) -> bool {
        self.modify_other_keys_active
    }

    /// Feed raw input through the same path as the stdin reader thread.
    ///
    /// Public so tests and embedders can drive the terminal without a real TTY.
    pub fn feed_stdin(&mut self, data: impl Into<JsString>) {
        self.feed_stdin_at(data, Instant::now());
    }

    /// Process buffered input/timers as of `now`.
    pub fn poll_at(&mut self, now: Instant) -> Vec<TerminalEvent> {
        self.clock_now = now;
        self.flush_keyboard_protocol_buffer_if_due(now);
        if let Some(buffer) = self.stdin_buffer.as_mut() {
            let events = buffer.poll(now);
            self.dispatch_stdin_events(events);
        }
        std::mem::take(&mut self.pending_events)
    }

    /// Begin Kitty keyboard protocol negotiation without starting raw mode.
    pub fn begin_keyboard_protocol_negotiation(&mut self) {
        self.query_and_enable_kitty_protocol();
    }

    /// Feed input with an explicit clock, for deterministic timer tests.
    pub fn feed_stdin_at(&mut self, data: impl Into<JsString>, now: Instant) {
        self.clock_now = now;
        if let Some(buffer) = self.stdin_buffer.as_mut() {
            let events = buffer.process(data);
            self.dispatch_stdin_events(events);
        }
        let events = self.poll_at(now);
        self.pending_events.extend(events);
    }

    fn query_and_enable_kitty_protocol(&mut self) {
        self.setup_stdin_buffer();
        self.keyboard_protocol_pushed = true;
        self.clear_keyboard_protocol_negotiation_buffer();
        let query = kitty_keyboard_protocol_query();
        self.publish_terminal_restore();
        self.write(&query);
    }

    fn setup_stdin_buffer(&mut self) {
        self.stdin_buffer = Some(StdinBuffer::with_options(StdinBufferOptions {
            timeout: Duration::from_millis(10),
        }));
    }

    fn dispatch_stdin_events(&mut self, events: Vec<StdinBufferEvent>) {
        for event in events {
            match event {
                StdinBufferEvent::Data(sequence) => self.handle_data_sequence(&sequence),
                StdinBufferEvent::Paste(content) => {
                    self.forward_input_sequence(&(JsString::from("\x1b[200~") + content + "\x1b[201~"));
                }
            }
        }
    }

    fn handle_data_sequence(&mut self, sequence: &JsString) {
        match self.read_keyboard_protocol_negotiation_sequence(sequence) {
            KeyboardProtocolRead::Pending => {
                self.schedule_keyboard_protocol_buffer_flush();
                return;
            }
            KeyboardProtocolRead::Sequence(negotiation) => {
                if self.handle_keyboard_protocol_negotiation_sequence(negotiation) {
                    return;
                }
            }
            KeyboardProtocolRead::None => {}
        }
        self.forward_input_sequence(sequence);
    }

    fn read_keyboard_protocol_negotiation_sequence(&mut self, sequence: &JsString) -> KeyboardProtocolRead {
        if !self.keyboard_protocol_negotiation_buffer.is_empty() {
            let buffered = self.keyboard_protocol_negotiation_buffer.clone() + sequence;
            if let Some(negotiation) = parse_keyboard_protocol_negotiation_sequence(&buffered) {
                self.clear_keyboard_protocol_negotiation_buffer();
                return KeyboardProtocolRead::Sequence(Some(negotiation));
            }
            if is_keyboard_protocol_negotiation_sequence_prefix(&buffered) {
                self.set_keyboard_protocol_negotiation_buffer(buffered);
                return KeyboardProtocolRead::Pending;
            }
            self.flush_keyboard_protocol_negotiation_buffer_as_input();
        }

        if let Some(negotiation) = parse_keyboard_protocol_negotiation_sequence(sequence) {
            return KeyboardProtocolRead::Sequence(Some(negotiation));
        }
        if is_keyboard_protocol_negotiation_sequence_prefix(sequence) {
            self.set_keyboard_protocol_negotiation_buffer(sequence.clone());
            return KeyboardProtocolRead::Pending;
        }
        KeyboardProtocolRead::None
    }

    fn set_keyboard_protocol_negotiation_buffer(&mut self, sequence: JsString) {
        self.clear_keyboard_protocol_negotiation_flush_deadline();
        self.keyboard_protocol_negotiation_buffer = sequence;
    }

    fn clear_keyboard_protocol_negotiation_buffer(&mut self) {
        self.clear_keyboard_protocol_negotiation_flush_deadline();
        self.keyboard_protocol_negotiation_buffer = JsString::default();
    }

    fn flush_keyboard_protocol_negotiation_buffer_as_input(&mut self) {
        if self.keyboard_protocol_negotiation_buffer.is_empty() {
            return;
        }
        let sequence = std::mem::take(&mut self.keyboard_protocol_negotiation_buffer);
        self.clear_keyboard_protocol_negotiation_flush_deadline();
        self.forward_input_sequence(&sequence);
    }

    fn schedule_keyboard_protocol_buffer_flush(&mut self) {
        if self.keyboard_protocol_negotiation_buffer.is_empty() || self.keyboard_protocol_flush_deadline.is_some() {
            return;
        }
        self.keyboard_protocol_flush_deadline = Some(self.clock_now + KEYBOARD_PROTOCOL_RESPONSE_FRAGMENT_TIMEOUT);
    }

    fn clear_keyboard_protocol_negotiation_flush_deadline(&mut self) {
        self.keyboard_protocol_flush_deadline = None;
    }

    fn flush_keyboard_protocol_buffer_if_due(&mut self, now: Instant) {
        let Some(deadline) = self.keyboard_protocol_flush_deadline else {
            return;
        };
        if now < deadline {
            return;
        }
        self.flush_keyboard_protocol_negotiation_buffer_as_input();
    }

    fn forward_input_sequence(&mut self, sequence: &JsString) {
        let is_apple_terminal =
            sequence == "\r" && is_apple_terminal_session(std::env::consts::OS, environment("TERM_PROGRAM").as_deref());
        let input = normalize_apple_terminal_input(
            sequence,
            is_apple_terminal,
            is_apple_terminal && is_native_modifier_pressed(ModifierKey::Shift),
        );
        self.pending_events.push(TerminalEvent::Input(input));
    }

    fn handle_keyboard_protocol_negotiation_sequence(
        &mut self,
        negotiation: Option<KeyboardProtocolNegotiationSequence>,
    ) -> bool {
        let Some(negotiation) = negotiation else {
            return false;
        };
        self.clear_keyboard_protocol_negotiation_buffer();
        match negotiation {
            KeyboardProtocolNegotiationSequence::KittyFlags { flags } => {
                if flags != 0.0 {
                    self.disable_modify_other_keys();
                    if !self.kitty_protocol_active {
                        self.kitty_protocol_active = true;
                        crate::keys::set_kitty_protocol_active(true);
                    }
                } else {
                    self.enable_modify_other_keys();
                }
                true
            }
            KeyboardProtocolNegotiationSequence::DeviceAttributes => {
                if !self.kitty_protocol_active {
                    self.enable_modify_other_keys();
                }
                true
            }
        }
    }

    fn enable_modify_other_keys(&mut self) {
        if self.kitty_protocol_active || self.modify_other_keys_active {
            return;
        }
        self.write("\x1b[>4;2m");
        self.modify_other_keys_active = true;
        self.publish_terminal_restore();
    }

    fn disable_modify_other_keys(&mut self) {
        if !self.modify_other_keys_active {
            return;
        }
        self.write("\x1b[>4;0m");
        self.modify_other_keys_active = false;
        self.publish_terminal_restore();
    }

    fn start_stdin_reader(&mut self) {
        let (sender, receiver) = channel::<Vec<u8>>();
        let (error_sender, error_receiver) = channel::<std::io::Error>();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        self.reader_thread = Some(std::thread::spawn(move || {
            read_stdin_chunks(sender, error_sender, thread_stop)
        }));
        self.input_handler = Some(receiver);
        self.read_errors = Some(error_receiver);
        self.reader_stop = Some(stop);
    }

    fn collect_read_errors(&mut self) {
        let Some(receiver) = self.read_errors.as_ref() else {
            return;
        };
        for error in receiver.try_iter() {
            self.pending_errors.push(TerminalIoError { syscall: "read", error });
        }
    }

    fn drain_stdin_channel(&mut self) -> bool {
        let Some(receiver) = self.input_handler.as_ref() else {
            return false;
        };
        let chunks: Vec<Vec<u8>> = receiver.try_iter().collect();
        let received = !chunks.is_empty();
        for chunk in chunks {
            let text = decode_stdin_chunk(&mut self.stdin_utf8_buffer, &chunk);
            if text.is_empty() {
                continue;
            }
            if let Some(buffer) = self.stdin_buffer.as_mut() {
                let events = buffer.process(&text);
                self.dispatch_stdin_events(events);
            }
        }
        received
    }

    fn append_write_log(&self, data: &str) {
        let Some(path) = self.write_log_path.as_deref() else {
            return;
        };
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(data.as_bytes());
        }
    }

    fn refresh_progress_keepalive(&mut self) {
        if !self.progress_active {
            return;
        }
        if self.progress_last_keepalive.elapsed() >= TERMINAL_PROGRESS_KEEPALIVE {
            self.progress_last_keepalive = Instant::now();
            self.write(TERMINAL_PROGRESS_ACTIVE_SEQUENCE);
        }
    }

    fn check_resize(&mut self) {
        let size = terminal_size();
        if size != self.last_size {
            self.last_size = size;
            if size.is_some() {
                self.pending_events.push(TerminalEvent::Resize);
            }
        }
    }
}

impl Default for ProcessTerminal {
    fn default() -> Self {
        Self::new()
    }
}

enum KeyboardProtocolRead {
    None,
    Pending,
    Sequence(Option<KeyboardProtocolNegotiationSequence>),
}

impl Terminal for ProcessTerminal {
    fn start(&mut self) -> Result<(), CallbackError> {
        self.was_raw = if rustix::termios::isatty(std::io::stdin()) {
            Some(open_stdin_raw()?)
        } else {
            None
        };
        self.publish_terminal_restore();
        self.write(BRACKETED_PASTE_ENABLE);
        self.last_size = terminal_size();
        self.start_stdin_reader();
        self.query_and_enable_kitty_protocol();
        Ok(())
    }

    fn crash_restore(&self) -> Option<TerminalRestore> {
        Some(self.crash_restore.clone())
    }

    fn stop(&mut self) -> Result<(), CallbackError> {
        self.write(self.stop_sequences());
        self.progress_active = false;
        self.clear_keyboard_protocol_negotiation_buffer();
        self.keyboard_protocol_pushed = false;
        self.kitty_protocol_active = false;
        crate::keys::set_kitty_protocol_active(false);
        self.modify_other_keys_active = false;
        if let Some(buffer) = self.stdin_buffer.as_mut() {
            buffer.destroy();
        }
        self.stdin_buffer = None;
        self.stdin_utf8_buffer.clear();
        if let Some(stop) = self.reader_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(thread) = self.reader_thread.take() {
            thread.join().expect("stdin reader must stop before restart");
        }
        self.input_handler = None;
        self.read_errors = None;
        if let Some(termios) = self.was_raw.take() {
            restore_stdin_mode(&termios)?;
        }
        self.publish_terminal_restore();
        Ok(())
    }

    fn drain_input(&mut self, max_ms: u64, idle_ms: u64) {
        let should_disable_kitty = self.keyboard_protocol_pushed || self.kitty_protocol_active;
        self.clear_keyboard_protocol_negotiation_buffer();
        if should_disable_kitty {
            self.write("\x1b[<u");
            self.keyboard_protocol_pushed = false;
            self.kitty_protocol_active = false;
            crate::keys::set_kitty_protocol_active(false);
        }
        self.disable_modify_other_keys();
        self.publish_terminal_restore();

        let end = Instant::now() + Duration::from_millis(max_ms);
        let idle = Duration::from_millis(idle_ms);
        let mut last_data = Instant::now();
        loop {
            let now = Instant::now();
            if now >= end {
                break;
            }
            if self.drain_stdin_channel() {
                last_data = now;
            }
            self.poll_at(now);
            if now.duration_since(last_data) >= idle {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn write(&mut self, data: &JsString) {
        ProcessTerminal::write(self, data);
    }

    fn columns(&self) -> usize {
        resolve_columns(terminal_size().map(|(columns, _)| columns), environment("COLUMNS"))
    }

    fn rows(&self) -> usize {
        resolve_rows(terminal_size().map(|(_, rows)| rows), environment("LINES"))
    }

    fn kitty_protocol_active(&self) -> bool {
        self.kitty_protocol_active
    }

    fn move_by(&mut self, lines: isize) {
        if lines > 0 {
            let sequence = format!("\x1b[{lines}B");
            self.write(&sequence);
        } else if lines < 0 {
            let sequence = format!("\x1b[{}A", -lines);
            self.write(&sequence);
        }
    }

    fn hide_cursor(&mut self) {
        self.write("\x1b[?25l");
    }

    fn show_cursor(&mut self) {
        self.write("\x1b[?25h");
    }

    fn clear_line(&mut self) {
        self.write("\x1b[K");
    }

    fn clear_from_cursor(&mut self) {
        self.write("\x1b[J");
    }

    fn clear_screen(&mut self) {
        self.write("\x1b[2J\x1b[H");
    }

    fn set_title(&mut self, title: &JsString) {
        self.write(JsString::from("\x1b]0;") + title + "\x07");
    }

    fn set_progress(&mut self, active: bool) {
        if active {
            self.write(TERMINAL_PROGRESS_ACTIVE_SEQUENCE);
            self.progress_active = true;
            self.progress_last_keepalive = Instant::now();
        } else {
            self.progress_active = false;
            self.write(TERMINAL_PROGRESS_CLEAR_SEQUENCE);
        }
        self.publish_terminal_restore();
    }

    fn poll_events(&mut self) -> Vec<TerminalEvent> {
        self.drain_stdin_channel();
        self.collect_read_errors();
        let now = Instant::now();
        let mut events = self.poll_at(now);
        self.check_resize();
        self.refresh_progress_keepalive();
        events.append(&mut self.pending_events);
        events
    }

    fn take_errors(&mut self) -> Vec<TerminalIoError> {
        std::mem::take(&mut self.pending_errors)
    }
}

impl ProcessTerminal {
    pub fn write(&mut self, data: impl Into<JsString>) {
        let data = encode_node_utf8(&data.into());
        let recovery = self.crash_restore.clone();
        let _writing = recovery.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Err(error) = self.output.write_all(data.as_bytes()) {
            self.pending_errors.push(TerminalIoError {
                syscall: "write",
                error,
            });
        }
        if let Err(error) = self.output.flush() {
            self.pending_errors.push(TerminalIoError {
                syscall: "write",
                error,
            });
        }
        self.append_write_log(&data);
    }
}
