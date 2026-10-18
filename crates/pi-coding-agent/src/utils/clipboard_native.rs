/// A native clipboard implementation.
pub trait ClipboardModule: Send + Sync {
    fn get_text(&self) -> Result<String, ClipboardError>;
    fn set_text(&self, text: &str) -> Result<(), ClipboardError>;
    fn has_image(&self) -> bool;
    fn get_image_binary(&self) -> Result<Vec<u8>, ClipboardError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardError {
    message: String,
}

impl ClipboardError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ClipboardError {}

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
        std::env::var("DISPLAY").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    )
}

/// The process-wide native clipboard.
///
/// The pinned source loads the `@mariozechner/clipboard` native module. The
/// Rust port has no equivalent binding, so this returns `None`; callers can
/// supply a `ClipboardModule` through `load_clipboard_native`.
pub fn clipboard() -> Option<Box<dyn ClipboardModule>> {
    if std::env::var_os("TERMUX_VERSION").is_some() || !has_display() {
        return None;
    }
    load_clipboard_native(&[])
}

fn non_empty(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}
