//! Ported from `packages/tui/src/terminal.ts`.
//!
//! The TypeScript `Terminal` interface installs input/resize callbacks. The Rust
//! port uses a typed event queue ([`TerminalEvent`]) instead: the terminal never
//! calls back into the TUI while it is borrowed, and `ProcessTerminal` reads stdin
//! on a worker thread that forwards bytes over a channel. Callers drive the queue
//! with [`Terminal::poll_events`].

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use rustix::termios::{OptionalActions, Termios, tcgetattr, tcgetwinsize, tcsetattr};

use crate::native_modifiers::{ModifierKey, is_native_modifier_pressed};
use crate::stdin_buffer::{StdinBuffer, StdinBufferEvent, StdinBufferOptions};

const TERMINAL_PROGRESS_KEEPALIVE: Duration = Duration::from_millis(1000);
const TERMINAL_PROGRESS_ACTIVE_SEQUENCE: &str = "\x1b]9;4;3\x07";
const TERMINAL_PROGRESS_CLEAR_SEQUENCE: &str = "\x1b]9;4;0\x07";
const NATIVE_SHIFT_ENTER_SEQUENCE: &str = "\x1b[13;2u";
const DESIRED_KITTY_KEYBOARD_PROTOCOL_FLAGS: u32 = 7;
const KEYBOARD_PROTOCOL_RESPONSE_FRAGMENT_TIMEOUT: Duration = Duration::from_millis(150);
const DEFAULT_ESCAPE_TIMEOUT_MS: f64 = 10.0;
const DEFAULT_SSH_ESCAPE_TIMEOUT_MS: f64 = 100.0;
const BRACKETED_PASTE_ENABLE: &str = "\x1b[?2004h";
const BRACKETED_PASTE_DISABLE: &str = "\x1b[?2004l";

fn kitty_keyboard_protocol_query() -> String {
    format!("\x1b[>{DESIRED_KITTY_KEYBOARD_PROTOCOL_FLAGS}u\x1b[?u\x1b[c")
}

/// A terminal input or resize event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEvent {
    Input(String),
    Resize,
}

/// Minimal terminal interface for the TUI.
pub trait Terminal {
    fn start(&mut self) -> Result<(), TerminalRawModeError>;
    fn stop(&mut self);
    fn drain_input(&mut self, max_ms: u64, idle_ms: u64);
    fn write(&mut self, data: &str);
    fn columns(&self) -> usize;
    fn rows(&self) -> usize;
    fn kitty_protocol_active(&self) -> bool;
    fn move_by(&mut self, lines: isize);
    fn hide_cursor(&mut self);
    fn show_cursor(&mut self);
    fn clear_line(&mut self);
    fn clear_from_cursor(&mut self);
    fn clear_screen(&mut self);
    fn set_title(&mut self, title: &str);
    fn set_progress(&mut self, active: bool);
    /// Drain queued input/resize events. The default has no event source.
    fn poll_events(&mut self) -> Vec<TerminalEvent> {
        Vec::new()
    }
}

/// A parsed Kitty keyboard protocol negotiation response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardProtocolNegotiationSequence {
    KittyFlags { flags: u32 },
    DeviceAttributes,
}

/// Parse `CSI ? <flags> u` or `CSI ? <params> c`.
pub fn parse_keyboard_protocol_negotiation_sequence(sequence: &str) -> Option<KeyboardProtocolNegotiationSequence> {
    if let Some(body) = sequence.strip_prefix("\x1b[?").and_then(|rest| rest.strip_suffix('u')) {
        if !body.is_empty() && body.bytes().all(|byte| byte.is_ascii_digit()) {
            return body
                .parse()
                .ok()
                .map(|flags| KeyboardProtocolNegotiationSequence::KittyFlags { flags });
        }
        return None;
    }
    if let Some(body) = sequence.strip_prefix("\x1b[?").and_then(|rest| rest.strip_suffix('c'))
        && body.bytes().all(|byte| byte.is_ascii_digit() || byte == b';')
    {
        return Some(KeyboardProtocolNegotiationSequence::DeviceAttributes);
    }
    None
}

fn is_keyboard_protocol_negotiation_sequence_prefix(sequence: &str) -> bool {
    if sequence == "\x1b[" {
        return true;
    }
    sequence
        .strip_prefix("\x1b[?")
        .is_some_and(|body| body.bytes().all(|byte| byte.is_ascii_digit() || byte == b';'))
}

/// Whether the process is Apple Terminal (`platform` is a Rust `consts::OS` value).
pub fn is_apple_terminal_session(platform: &str, term_program: Option<&str>) -> bool {
    platform == "macos" && term_program == Some("Apple_Terminal")
}

/// Rewrite a bare Return to CSI-u Shift+Enter when native Shift detection applies.
pub fn normalize_native_shift_enter_input(
    data: &str,
    should_detect_native_shift_enter: bool,
    is_shift_pressed: bool,
) -> String {
    if should_detect_native_shift_enter && data == "\r" && is_shift_pressed {
        return NATIVE_SHIFT_ENTER_SEQUENCE.to_string();
    }
    data.to_string()
}

/// Apple Terminal variant of [`normalize_native_shift_enter_input`].
pub fn normalize_apple_terminal_input(data: &str, is_apple_terminal: bool, is_shift_pressed: bool) -> String {
    normalize_native_shift_enter_input(data, is_apple_terminal, is_shift_pressed)
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

/// Detect raw-mode failures by type or by the Bun-style message prefix.
pub fn is_terminal_raw_mode_failure(error: &(dyn std::error::Error + 'static)) -> bool {
    if error.downcast_ref::<TerminalRawModeError>().is_some() {
        return true;
    }
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    lower.starts_with("setrawmode failed with errno:")
}

/// Resolve the escape reassembly timeout from an environment lookup.
pub fn resolve_escape_timeout_ms(env: &dyn Fn(&str) -> Option<String>) -> f64 {
    if let Some(raw) = env("PI_TUI_ESC_TIMEOUT") {
        let configured = crate::js_number::parse_number(&raw);
        if configured.is_finite() && configured > 0.0 {
            return configured;
        }
    }
    if env("SSH_CONNECTION").is_some() || env("SSH_TTY").is_some() {
        return DEFAULT_SSH_ESCAPE_TIMEOUT_MS;
    }
    DEFAULT_ESCAPE_TIMEOUT_MS
}

fn parse_positive_dimension(value: Option<String>) -> Option<usize> {
    let parsed = crate::js_number::parse_number(&value?);
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

fn terminal_size() -> Option<(u16, u16)> {
    let stdin = std::io::stdin();
    let size = tcgetwinsize(&stdin).ok()?;
    Some((size.ws_col, size.ws_row))
}

fn environment(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn write_log_path() -> Option<PathBuf> {
    let configured = environment("PI_TUI_WRITE_LOG")?;
    if configured.is_empty() {
        return None;
    }
    let path = PathBuf::from(&configured);
    if path.is_dir() {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        return Some(path.join(format!("tui-{seconds}-{}.log", std::process::id())));
    }
    Some(path)
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

/// Real terminal over stdin/stdout.
pub struct ProcessTerminal {
    output: Box<dyn Write>,
    was_raw: Option<Termios>,
    input_handler: Option<Receiver<Vec<u8>>>,
    reader_stop: Option<Arc<AtomicBool>>,
    kitty_protocol_active: bool,
    modify_other_keys_active: bool,
    keyboard_protocol_pushed: bool,
    keyboard_protocol_negotiation_buffer: String,
    keyboard_protocol_flush_deadline: Option<Instant>,
    stdin_buffer: Option<StdinBuffer>,
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
            was_raw: None,
            input_handler: None,
            reader_stop: None,
            kitty_protocol_active: false,
            modify_other_keys_active: false,
            keyboard_protocol_pushed: false,
            keyboard_protocol_negotiation_buffer: String::new(),
            keyboard_protocol_flush_deadline: None,
            stdin_buffer: None,
            pending_events: Vec::new(),
            last_size: None,
            progress_active: false,
            progress_last_keepalive: Instant::now(),
            clock_now: Instant::now(),
            write_log_path: write_log_path(),
        }
    }

    pub fn modify_other_keys_active(&self) -> bool {
        self.modify_other_keys_active
    }

    /// Feed raw input through the same path as the stdin reader thread.
    ///
    /// Public so tests and embedders can drive the terminal without a real TTY.
    pub fn feed_stdin(&mut self, data: &str) {
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
    pub fn feed_stdin_at(&mut self, data: &str, now: Instant) {
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
        self.write(&query);
    }

    fn setup_stdin_buffer(&mut self) {
        let escape_timeout = resolve_escape_timeout_ms(&environment);
        self.stdin_buffer = Some(StdinBuffer::with_options(StdinBufferOptions {
            timeout: Duration::from_millis(50),
            escape_timeout: Duration::from_secs_f64(escape_timeout / 1000.0),
        }));
    }

    fn dispatch_stdin_events(&mut self, events: Vec<StdinBufferEvent>) {
        for event in events {
            match event {
                StdinBufferEvent::Data(sequence) => self.handle_data_sequence(&sequence),
                StdinBufferEvent::Paste(content) => {
                    self.forward_input_sequence(&format!("\x1b[200~{content}\x1b[201~"));
                }
            }
        }
    }

    fn handle_data_sequence(&mut self, sequence: &str) {
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

    fn read_keyboard_protocol_negotiation_sequence(&mut self, sequence: &str) -> KeyboardProtocolRead {
        if !self.keyboard_protocol_negotiation_buffer.is_empty() {
            let buffered = format!("{}{}", self.keyboard_protocol_negotiation_buffer, sequence);
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
            self.set_keyboard_protocol_negotiation_buffer(sequence.to_string());
            return KeyboardProtocolRead::Pending;
        }
        KeyboardProtocolRead::None
    }

    fn set_keyboard_protocol_negotiation_buffer(&mut self, sequence: String) {
        self.clear_keyboard_protocol_negotiation_flush_deadline();
        self.keyboard_protocol_negotiation_buffer = sequence;
    }

    fn clear_keyboard_protocol_negotiation_buffer(&mut self) {
        self.clear_keyboard_protocol_negotiation_flush_deadline();
        self.keyboard_protocol_negotiation_buffer.clear();
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

    fn forward_input_sequence(&mut self, sequence: &str) {
        let should_detect_native_shift_enter = sequence == "\r"
            && (is_apple_terminal_session(std::env::consts::OS, environment("TERM_PROGRAM").as_deref())
                || std::env::consts::OS == "windows");
        let input = normalize_native_shift_enter_input(
            sequence,
            should_detect_native_shift_enter,
            should_detect_native_shift_enter && is_native_modifier_pressed(ModifierKey::Shift),
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
                if flags != 0 {
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
    }

    fn disable_modify_other_keys(&mut self) {
        if !self.modify_other_keys_active {
            return;
        }
        self.write("\x1b[>4;0m");
        self.modify_other_keys_active = false;
    }

    fn start_stdin_reader(&mut self) {
        let (sender, receiver) = channel::<Vec<u8>>();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut stdin = std::io::stdin();
            let mut buffer = [0_u8; 4096];
            while !thread_stop.load(Ordering::Relaxed) {
                match stdin.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        if sender.send(buffer[..count].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        });
        self.input_handler = Some(receiver);
        self.reader_stop = Some(stop);
    }

    fn drain_stdin_channel(&mut self) {
        let Some(receiver) = self.input_handler.as_ref() else {
            return;
        };
        let chunks: Vec<Vec<u8>> = receiver.try_iter().collect();
        for chunk in chunks {
            let text = String::from_utf8_lossy(&chunk).into_owned();
            if let Some(buffer) = self.stdin_buffer.as_mut() {
                let events = buffer.process(&text);
                self.dispatch_stdin_events(events);
            }
        }
    }

    fn append_write_log(&self, data: &str) {
        let Some(path) = self.write_log_path.as_deref() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
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
    fn start(&mut self) -> Result<(), TerminalRawModeError> {
        self.was_raw = Some(open_stdin_raw()?);
        self.write(BRACKETED_PASTE_ENABLE);
        self.last_size = terminal_size();
        self.start_stdin_reader();
        self.query_and_enable_kitty_protocol();
        Ok(())
    }

    fn stop(&mut self) {
        if self.progress_active {
            self.progress_active = false;
            self.write(TERMINAL_PROGRESS_CLEAR_SEQUENCE);
        }
        self.write(BRACKETED_PASTE_DISABLE);
        let should_disable_kitty = self.keyboard_protocol_pushed || self.kitty_protocol_active;
        self.clear_keyboard_protocol_negotiation_buffer();
        if should_disable_kitty {
            self.write("\x1b[<u");
            self.keyboard_protocol_pushed = false;
            self.kitty_protocol_active = false;
            crate::keys::set_kitty_protocol_active(false);
        }
        self.disable_modify_other_keys();
        if let Some(buffer) = self.stdin_buffer.as_mut() {
            buffer.destroy();
        }
        self.stdin_buffer = None;
        if let Some(stop) = self.reader_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        self.input_handler = None;
        if let Some(termios) = self.was_raw.take() {
            let _ = restore_stdin_mode(&termios);
        }
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

        let end = Instant::now() + Duration::from_millis(max_ms);
        let idle = Duration::from_millis(idle_ms);
        let mut last_data = Instant::now();
        loop {
            let now = Instant::now();
            if now >= end {
                break;
            }
            if now.duration_since(last_data) >= idle {
                break;
            }
            self.drain_stdin_channel();
            let events = self.poll_at(now);
            if events.iter().any(|event| matches!(event, TerminalEvent::Input(_))) {
                last_data = Instant::now();
            }
            self.pending_events.clear();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn write(&mut self, data: &str) {
        let _ = self.output.write_all(data.as_bytes());
        let _ = self.output.flush();
        self.append_write_log(data);
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

    fn set_title(&mut self, title: &str) {
        self.write(&format!("\x1b]0;{title}\x07"));
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
    }

    fn poll_events(&mut self) -> Vec<TerminalEvent> {
        self.drain_stdin_channel();
        let now = Instant::now();
        let mut events = self.poll_at(now);
        self.check_resize();
        self.refresh_progress_keepalive();
        events.append(&mut self.pending_events);
        events
    }
}
