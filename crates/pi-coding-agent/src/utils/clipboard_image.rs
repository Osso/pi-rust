//! Reading images from the system clipboard.
//!
//! Platform tools are invoked with the exact argv and timeouts used by the
//! pinned source: `wl-paste`/`xclip` on Wayland and X11, PowerShell on WSL,
//! and the native clipboard elsewhere.

use std::collections::BTreeMap;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::utils::clipboard_native::{ClipboardError, ClipboardModule};
use crate::utils::image_convert::convert_image_bytes_to_png;
use crate::utils::pi_user_agent::node_platform_name;

pub const SUPPORTED_IMAGE_MIME_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];

const DEFAULT_LIST_TIMEOUT: Duration = Duration::from_millis(1000);
const DEFAULT_READ_TIMEOUT: Duration = Duration::from_millis(3000);
const DEFAULT_POWERSHELL_TIMEOUT: Duration = Duration::from_millis(5000);
const DEFAULT_MAX_BUFFER_BYTES: usize = 50 * 1024 * 1024;

/// Clipboard bytes with their MIME type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

/// The environment inputs the source reads through `process.env` and `process.platform`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardEnvironment {
    platform: String,
    variables: BTreeMap<String, String>,
}

impl ClipboardEnvironment {
    pub fn new(platform: impl Into<String>, variables: BTreeMap<String, String>) -> Self {
        Self {
            platform: platform.into(),
            variables,
        }
    }

    pub fn from_process() -> Self {
        Self::new(
            node_platform_name(std::env::consts::OS),
            pi_js_compat::process_env::vars().into_iter().collect(),
        )
    }

    pub fn platform(&self) -> &str {
        &self.platform
    }

    pub fn variable(&self, name: &str) -> Option<&str> {
        self.variables.get(name).map(String::as_str)
    }

    fn has(&self, name: &str) -> bool {
        self.variable(name).is_some_and(|value| !value.is_empty())
    }
}

/// Whether the session is Wayland (`WAYLAND_DISPLAY` or `XDG_SESSION_TYPE=wayland`).
pub fn is_wayland_session(environment: &ClipboardEnvironment) -> bool {
    environment.has("WAYLAND_DISPLAY") || environment.variable("XDG_SESSION_TYPE") == Some("wayland")
}

/// File extension for a supported image MIME type.
pub fn extension_for_image_mime_type(mime_type: &str) -> Option<&'static str> {
    match base_mime_type(mime_type).as_str() {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        _ => None,
    }
}

/// Reads an image from the clipboard, or `None` when no source has one.
///
/// Native clipboard errors propagate, matching the source's rejected promise.
pub async fn read_clipboard_image(
    environment: &ClipboardEnvironment,
    native: Option<&dyn ClipboardModule>,
) -> Result<Option<ClipboardImage>, ClipboardError> {
    if environment.has("TERMUX_VERSION") {
        return Ok(None);
    }

    let image = if environment.platform() == "linux" {
        read_linux_clipboard_image(environment, native).await?
    } else {
        read_native_clipboard_image(native)?
    };

    let Some(image) = image else {
        return Ok(None);
    };

    if is_supported_image_mime_type(&image.mime_type) {
        return Ok(Some(image));
    }

    // Convert unsupported formats (for example BMP from WSLg) to PNG.
    Ok(convert_image_bytes_to_png(&image.bytes).map(|bytes| ClipboardImage {
        bytes,
        mime_type: "image/png".to_owned(),
    }))
}

async fn read_linux_clipboard_image(
    environment: &ClipboardEnvironment,
    native: Option<&dyn ClipboardModule>,
) -> Result<Option<ClipboardImage>, ClipboardError> {
    let wsl = is_wsl(environment);
    let wayland = is_wayland_session(environment);

    let mut image = if wayland || wsl {
        match read_clipboard_image_via_wl_paste().await {
            Some(image) => Some(image),
            None => read_clipboard_image_via_xclip().await,
        }
    } else {
        None
    };

    if image.is_none() && wsl {
        image = read_clipboard_image_via_powershell().await;
    }

    if image.is_none() && !wayland {
        image = match read_native_clipboard_image(native)? {
            Some(image) => Some(image),
            None => read_clipboard_image_via_xclip().await,
        };
    }

    Ok(image)
}

fn read_native_clipboard_image(native: Option<&dyn ClipboardModule>) -> Result<Option<ClipboardImage>, ClipboardError> {
    let Some(native) = native else {
        return Ok(None);
    };
    if !native.has_image()? {
        return Ok(None);
    }

    let bytes = native.get_image_binary()?;
    if bytes.is_empty() {
        return Ok(None);
    }

    Ok(Some(ClipboardImage {
        bytes,
        mime_type: "image/png".to_owned(),
    }))
}

async fn read_clipboard_image_via_wl_paste() -> Option<ClipboardImage> {
    let listing = run_command("wl-paste", &["--list-types"], DEFAULT_LIST_TIMEOUT).await?;
    let types = split_types(&listing);
    let selected = select_preferred_image_mime_type(&types)?;

    let bytes = run_command("wl-paste", &["--type", &selected, "--no-newline"], DEFAULT_READ_TIMEOUT).await?;
    if bytes.is_empty() {
        return None;
    }

    Some(ClipboardImage {
        bytes,
        mime_type: base_mime_type(&selected),
    })
}

async fn read_clipboard_image_via_xclip() -> Option<ClipboardImage> {
    let targets = run_command(
        "xclip",
        &["-selection", "clipboard", "-t", "TARGETS", "-o"],
        DEFAULT_LIST_TIMEOUT,
    )
    .await;

    let candidate_types = targets.map(|bytes| split_types(&bytes)).unwrap_or_default();
    let preferred = select_preferred_image_mime_type(&candidate_types);
    let try_types = match preferred {
        Some(preferred) => std::iter::once(preferred)
            .chain(SUPPORTED_IMAGE_MIME_TYPES.iter().map(|value| (*value).to_owned()))
            .collect::<Vec<_>>(),
        None => SUPPORTED_IMAGE_MIME_TYPES
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
    };

    for mime_type in try_types {
        let bytes = run_command(
            "xclip",
            &["-selection", "clipboard", "-t", &mime_type, "-o"],
            DEFAULT_READ_TIMEOUT,
        )
        .await;
        if let Some(bytes) = bytes.filter(|bytes| !bytes.is_empty()) {
            return Some(ClipboardImage {
                bytes,
                mime_type: base_mime_type(&mime_type),
            });
        }
    }

    None
}

/// On WSL the Linux clipboard does not receive Windows screenshots, so
/// PowerShell reads the Windows clipboard directly.
async fn read_clipboard_image_via_powershell() -> Option<ClipboardImage> {
    let temp_file = unique_temp_path();
    let result = read_powershell_clipboard(&temp_file).await;
    let _ = std::fs::remove_file(&temp_file);
    result
}

async fn read_powershell_clipboard(temp_file: &std::path::Path) -> Option<ClipboardImage> {
    let temp_path = temp_file.to_str()?;
    let windows_path = run_command("wslpath", &["-w", temp_path], DEFAULT_LIST_TIMEOUT).await?;
    let windows_path = String::from_utf8_lossy(&windows_path).trim().to_owned();
    if windows_path.is_empty() {
        return None;
    }

    let script = powershell_script(&windows_path);
    let output = run_command(
        "powershell.exe",
        &["-NoProfile", "-Command", &script],
        DEFAULT_POWERSHELL_TIMEOUT,
    )
    .await?;
    if String::from_utf8_lossy(&output).trim() != "ok" {
        return None;
    }

    let bytes = std::fs::read(temp_file).ok()?;
    if bytes.is_empty() {
        return None;
    }

    Some(ClipboardImage {
        bytes,
        mime_type: "image/png".to_owned(),
    })
}

fn powershell_script(windows_path: &str) -> String {
    let quoted = windows_path.replace('\'', "''");
    [
        "Add-Type -AssemblyName System.Windows.Forms",
        "Add-Type -AssemblyName System.Drawing",
        &format!("$path = '{quoted}'"),
        "$img = [System.Windows.Forms.Clipboard]::GetImage()",
        "if ($img) { $img.Save($path, [System.Drawing.Imaging.ImageFormat]::Png); Write-Output 'ok' } else { Write-Output 'empty' }",
    ]
    .join("; ")
}

fn unique_temp_path() -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("pi-wsl-clip-{}-{nanos}.png", std::process::id()))
}

fn is_wsl(environment: &ClipboardEnvironment) -> bool {
    if environment.has("WSL_DISTRO_NAME") || environment.has("WSLENV") {
        return true;
    }

    std::fs::read_to_string("/proc/version")
        .map(|release| {
            let release = release.to_lowercase();
            release.contains("microsoft") || release.contains("wsl")
        })
        .unwrap_or(false)
}

fn split_types(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

/// First supported type in preference order, else any `image/*` type.
fn select_preferred_image_mime_type(mime_types: &[String]) -> Option<String> {
    for preferred in SUPPORTED_IMAGE_MIME_TYPES {
        if let Some(found) = mime_types
            .iter()
            .find(|mime_type| base_mime_type(mime_type) == preferred)
        {
            return Some(found.clone());
        }
    }

    mime_types
        .iter()
        .find(|mime_type| base_mime_type(mime_type).starts_with("image/"))
        .cloned()
}

fn is_supported_image_mime_type(mime_type: &str) -> bool {
    let base = base_mime_type(mime_type);
    SUPPORTED_IMAGE_MIME_TYPES.contains(&base.as_str())
}

fn base_mime_type(mime_type: &str) -> String {
    mime_type.split(';').next().unwrap_or(mime_type).trim().to_lowercase()
}

/// Runs a command and returns its stdout when it exits successfully.
async fn run_command(program: &str, arguments: &[&str], timeout: Duration) -> Option<Vec<u8>> {
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;

    let mut stdout = child.stdout.take()?;
    let mut buffer = Vec::new();
    let collect = async {
        let mut limited = (&mut stdout).take(DEFAULT_MAX_BUFFER_BYTES as u64 + 1);
        limited.read_to_end(&mut buffer).await.ok()?;
        let status = child.wait().await.ok()?;
        Some(status)
    };

    let status = tokio::time::timeout(timeout, collect).await.ok()??;
    if !status.success() || buffer.len() > DEFAULT_MAX_BUFFER_BYTES {
        return None;
    }
    Some(buffer)
}
