//! Native clipboard access.
//!
//! The pinned source loads `@mariozechner/clipboard`, a napi wrapper that
//! vendors `clipboard-rs`. The Rust port depends on the same `clipboard-rs`
//! release directly, so the platform backends (X11/Wayland/macOS/Windows) are
//! the code the Node addon runs.

use clipboard_rs::common::RustImage;
use clipboard_rs::{Clipboard, ClipboardContext, ContentFormat};
use pi_js_compat::js_json::{JsString, encode_node_utf8};

/// A native clipboard implementation.
pub trait ClipboardModule: Send + Sync {
    fn get_text(&self) -> Result<String, ClipboardError>;
    fn set_text(&self, text: &str) -> Result<(), ClipboardError>;
    fn has_image(&self) -> Result<bool, ClipboardError>;
    fn get_image_binary(&self) -> Result<Vec<u8>, ClipboardError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardError {
    message: JsString,
}

impl ClipboardError {
    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// `error.message`.
    pub fn message(&self) -> &JsString {
        &self.message
    }
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}

impl std::error::Error for ClipboardError {}

/// The system clipboard backed by `clipboard-rs`.
///
/// Like the napi wrapper, every call opens a fresh `ClipboardContext`; the
/// X11 backend needs that to re-read the current selection owner.
pub struct SystemClipboard;

impl ClipboardModule for SystemClipboard {
    fn get_text(&self) -> Result<String, ClipboardError> {
        context()?.get_text().map_err(clipboard_error)
    }

    fn set_text(&self, text: &str) -> Result<(), ClipboardError> {
        context()?.set_text(text.to_owned()).map_err(clipboard_error)
    }

    fn has_image(&self) -> Result<bool, ClipboardError> {
        Ok(context()?.has(ContentFormat::Image))
    }

    fn get_image_binary(&self) -> Result<Vec<u8>, ClipboardError> {
        let image = context()?.get_image().map_err(clipboard_error)?;
        let png = image.to_png().map_err(clipboard_error)?;
        Ok(png.get_bytes().to_vec())
    }
}

fn context() -> Result<ClipboardContext, ClipboardError> {
    ClipboardContext::new().map_err(clipboard_error)
}

fn clipboard_error(error: impl ToString) -> ClipboardError {
    ClipboardError::new(error.to_string())
}

/// Return the first clipboard loaded by the supplied resolution roots.
pub fn load_clipboard_native(
    loaders: &[&dyn Fn() -> Option<Box<dyn ClipboardModule>>],
) -> Option<Box<dyn ClipboardModule>> {
    loaders.iter().find_map(|loader| loader())
}

/// Whether a display is available for clipboard access.
pub fn display_available(is_linux: bool, display: Option<&str>, wayland_display: Option<&str>) -> bool {
    !is_linux || non_empty(display) || non_empty(wayland_display)
}

/// Whether the current process has a display for clipboard access.
pub fn has_display() -> bool {
    display_available(
        cfg!(target_os = "linux"),
        pi_js_compat::process_env::get("DISPLAY").as_deref(),
        pi_js_compat::process_env::get("WAYLAND_DISPLAY").as_deref(),
    )
}

/// The process-wide native clipboard.
///
/// Disabled under Termux or without a display, matching the source's module
/// initialization guard.
pub fn clipboard() -> Option<Box<dyn ClipboardModule>> {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let enabled = ENABLED.get_or_init(|| {
        let in_termux = non_empty(pi_js_compat::process_env::get("TERMUX_VERSION").as_deref());
        !in_termux && has_display()
    });
    enabled.then(|| Box::new(SystemClipboard) as Box<dyn ClipboardModule>)
}

fn non_empty(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}
