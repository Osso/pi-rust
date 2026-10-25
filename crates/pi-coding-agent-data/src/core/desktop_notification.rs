//! Linux desktop notifications (`core/desktop-notification.ts`).

use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

pub const DEFAULT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS: i64 = -1;
pub const NEVER_EXPIRE_DESKTOP_NOTIFICATION_MS: i64 = 0;
pub const PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS: i64 = 999_999;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopNotificationUrgency {
    Critical,
    Low,
    Normal,
}

impl DesktopNotificationUrgency {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Low => "low",
            Self::Normal => "normal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopNotification {
    pub body: JsString,
    pub expire_time_ms: Option<i64>,
    pub title: JsString,
    pub urgency: Option<DesktopNotificationUrgency>,
}

pub trait DesktopNotificationHandle: Send + Sync {
    fn close(&self) -> Result<(), CallbackError>;
}

/// A host-provided notifier; `None` means the notification was not delivered.
pub trait DesktopNotifier: Send + Sync {
    fn notify(
        &self,
        notification: &DesktopNotification,
    ) -> Result<Option<Box<dyn DesktopNotificationHandle>>, CallbackError>;
}

/// Frozen catches log the original throw; native errors omit V8 stacks.
pub fn report_callback_error(prefix: impl Into<JsString>, error: &CallbackError) {
    use pi_js_compat::abort::AbortReason;
    use pi_js_compat::js_json::JsValue;
    use pi_js_compat::node_fs::inspect_console_value;

    let value = match error.thrown_value() {
        AbortReason::Value(JsValue::String(value)) => value,
        AbortReason::Value(value) => inspect_console_value(&value),
        AbortReason::Error(error) | AbortReason::DomException(error) => {
            let mut properties = error.value().as_object().expect("native error object").clone();
            properties.shift_remove(&JsString::from("name"));
            properties.shift_remove(&JsString::from("message"));
            let header = error.string_value();
            if properties.is_empty() {
                header
            } else {
                pi_js_compat::js_format!("{} {}", header, inspect_console_value(&JsValue::Object(properties)))
            }
        }
    };
    eprintln!("{} {}", encode_node_utf8(&prefix.into()), encode_node_utf8(&value));
}

/// Send a notification through `notify-send`; `None` outside Linux.
pub fn send_desktop_notification(notification: &DesktopNotification) -> Option<Box<dyn DesktopNotificationHandle>> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let pending = PendingNotification::new();
    let expire_time_ms = notification
        .expire_time_ms
        .unwrap_or(DEFAULT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS);
    let urgency = notification.urgency.unwrap_or(DesktopNotificationUrgency::Critical);
    let mut child = Command::new("notify-send")
        .arg("--print-id")
        .arg("--app-name=Pi")
        .arg(format!("--expire-time={expire_time_ms}"))
        .arg(format!("--urgency={}", urgency.as_str()))
        .arg(encode_node_utf8(&notification.title))
        .arg(encode_node_utf8(&notification.body))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let state = Arc::clone(&pending.state);
    std::thread::spawn(move || {
        let mut output = String::new();
        let _ = stdout.read_to_string(&mut output);
        let _ = child.wait();
        if let Some(notification_id) = parse_notification_id(&output) {
            state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .resolve(notification_id);
        }
    });
    Some(Box::new(pending))
}

fn parse_notification_id(output: &str) -> Option<u32> {
    let trimmed = output.trim_start();
    let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

struct PendingNotification {
    state: Arc<Mutex<NotificationState>>,
}

#[derive(Default)]
struct NotificationState {
    close_requested: bool,
    resolved_id: Option<u32>,
}

impl PendingNotification {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(NotificationState::default())),
        }
    }
}

impl NotificationState {
    fn resolve(&mut self, notification_id: u32) {
        self.resolved_id = Some(notification_id);
        if self.close_requested {
            close_desktop_notification(notification_id);
        }
    }

    fn request_close(&mut self) {
        self.close_requested = true;
        if let Some(notification_id) = self.resolved_id {
            close_desktop_notification(notification_id);
        }
    }
}

impl DesktopNotificationHandle for PendingNotification {
    fn close(&self) -> Result<(), CallbackError> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .request_close();
        Ok(())
    }
}

fn close_desktop_notification(notification_id: u32) {
    // `detached: true` + `unref()`: a new session that outlives this call.
    let _ = Command::new("setsid")
        .arg("busctl")
        .args([
            "--user",
            "call",
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            "CloseNotification",
            "u",
        ])
        .arg(notification_id.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}
