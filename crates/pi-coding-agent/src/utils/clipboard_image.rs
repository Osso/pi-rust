//! Port of frozen `coding-agent/src/utils/clipboard-image.ts`.

use std::collections::BTreeMap;
use std::time::Duration;

use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

use crate::utils::child_process::{SpawnSyncOptions, spawn_sync};
use crate::utils::clipboard_native::ClipboardModule;
use crate::utils::photon::{decode_image, png_bytes};
use crate::utils::pi_user_agent::node_platform_name;

pub const SUPPORTED_IMAGE_MIME_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];
const DEFAULT_LIST_TIMEOUT: Duration = Duration::from_millis(1000);
const DEFAULT_READ_TIMEOUT: Duration = Duration::from_millis(3000);
const DEFAULT_POWERSHELL_TIMEOUT: Duration = Duration::from_millis(5000);
const DEFAULT_MAX_BUFFER_BYTES: usize = 50 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    pub bytes: Vec<u8>,
    pub mime_type: JsString,
}

/// Value environment used only for dispatch; tools inherit actual process.env.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardEnvironment {
    platform: String,
    variables: BTreeMap<String, JsString>,
}

impl ClipboardEnvironment {
    pub fn new(platform: impl Into<String>, variables: BTreeMap<String, JsString>) -> Self {
        Self {
            platform: platform.into(),
            variables,
        }
    }

    pub fn from_process() -> Self {
        Self::new(
            node_platform_name(std::env::consts::OS),
            pi_js_compat::process_env::vars()
                .into_iter()
                .map(|(name, value)| (name, value.into()))
                .collect(),
        )
    }

    pub fn platform(&self) -> &str {
        &self.platform
    }
    pub fn variable(&self, name: &str) -> Option<&JsString> {
        self.variables.get(name)
    }
    fn has(&self, name: &str) -> bool {
        self.variable(name).is_some_and(|value| !value.is_empty())
    }
}

pub fn is_wayland_session(environment: &ClipboardEnvironment) -> bool {
    environment.has("WAYLAND_DISPLAY")
        || environment
            .variable("XDG_SESSION_TYPE")
            .is_some_and(|value| value == "wayland")
}

pub fn extension_for_image_mime_type(mime_type: impl Into<JsString>) -> Option<&'static str> {
    match base_mime_type(&mime_type.into()) {
        mime_type if mime_type == "image/png" => Some("png"),
        mime_type if mime_type == "image/jpeg" => Some("jpg"),
        mime_type if mime_type == "image/webp" => Some("webp"),
        mime_type if mime_type == "image/gif" => Some("gif"),
        _ => None,
    }
}

/// Native clipboard failures and synchronous argument-validation throws reject.
/// Failed command results and unsupported conversion failures return no image.
pub async fn read_clipboard_image(
    environment: &ClipboardEnvironment,
    native: Option<&dyn ClipboardModule>,
) -> Result<Option<ClipboardImage>, CallbackError> {
    if environment.has("TERMUX_VERSION") {
        return Ok(None);
    }
    let image = if environment.platform() == "linux" {
        read_linux_clipboard_image(environment, native)?
    } else {
        read_native_clipboard_image(native)?
    };
    let Some(image) = image else {
        return Ok(None);
    };
    if is_supported_image_mime_type(&image.mime_type) {
        return Ok(Some(image));
    }
    Ok(convert_to_png(&image.bytes).map(|bytes| ClipboardImage {
        bytes,
        mime_type: "image/png".into(),
    }))
}

/// Frozen clipboard conversion does not apply image-convert.ts EXIF rotation.
fn convert_to_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = decode_image(bytes).ok()?;
    png_bytes(&image).ok()
}

fn read_linux_clipboard_image(
    environment: &ClipboardEnvironment,
    native: Option<&dyn ClipboardModule>,
) -> Result<Option<ClipboardImage>, CallbackError> {
    let wsl = is_wsl(environment);
    let wayland = is_wayland_session(environment);
    let mut image = None;
    if wayland || wsl {
        image = read_clipboard_image_via_wl_paste()?;
        if image.is_none() {
            image = read_clipboard_image_via_xclip()?;
        }
    } else if !environment.has("DISPLAY") {
        image = read_clipboard_image_via_wl_paste()?;
    }
    if image.is_none() && wsl {
        image = read_clipboard_image_via_powershell();
    }
    if image.is_none() && !wayland {
        image = read_native_clipboard_image(native)?;
    }
    Ok(image)
}

fn read_native_clipboard_image(native: Option<&dyn ClipboardModule>) -> Result<Option<ClipboardImage>, CallbackError> {
    let Some(native) = native else {
        return Ok(None);
    };
    if !native
        .has_image()
        .map_err(|error| CallbackError::from_error(error.message().clone(), error))?
    {
        return Ok(None);
    }
    let bytes = native
        .get_image_binary()
        .map_err(|error| CallbackError::from_error(error.message().clone(), error))?;
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(ClipboardImage {
        bytes,
        mime_type: "image/png".into(),
    }))
}

fn read_clipboard_image_via_wl_paste() -> Result<Option<ClipboardImage>, CallbackError> {
    let Some(listing) = run_command("wl-paste", &["--list-types".into()], DEFAULT_LIST_TIMEOUT)? else {
        return Ok(None);
    };
    let Some(selected) = select_preferred_image_mime_type(&split_types(&listing)) else {
        return Ok(None);
    };
    let Some(bytes) = run_command(
        "wl-paste",
        &["--type".into(), selected.clone(), "--no-newline".into()],
        DEFAULT_READ_TIMEOUT,
    )?
    else {
        return Ok(None);
    };
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(ClipboardImage {
        bytes,
        mime_type: base_mime_type(&selected),
    }))
}

fn read_clipboard_image_via_xclip() -> Result<Option<ClipboardImage>, CallbackError> {
    let targets = run_command(
        "xclip",
        &[
            "-selection".into(),
            "clipboard".into(),
            "-t".into(),
            "TARGETS".into(),
            "-o".into(),
        ],
        DEFAULT_LIST_TIMEOUT,
    )?;
    let candidates = targets.as_deref().map(split_types).unwrap_or_default();
    let preferred = select_preferred_image_mime_type(&candidates);
    let try_types = preferred
        .into_iter()
        .chain(SUPPORTED_IMAGE_MIME_TYPES.into_iter().map(JsString::from));
    for mime_type in try_types {
        if let Some(bytes) = run_command(
            "xclip",
            &[
                "-selection".into(),
                "clipboard".into(),
                "-t".into(),
                mime_type.clone(),
                "-o".into(),
            ],
            DEFAULT_READ_TIMEOUT,
        )? && !bytes.is_empty()
        {
            return Ok(Some(ClipboardImage {
                bytes,
                mime_type: base_mime_type(&mime_type),
            }));
        }
    }
    Ok(None)
}

fn read_clipboard_image_via_powershell() -> Option<ClipboardImage> {
    let temp_file = crate::utils::paths::node_join(
        pi_js_compat::node_os::tmpdir(),
        pi_js_compat::js_format!("pi-wsl-clip-{}.png", pi_ai::utils::random_uuid::random_uuid()),
    );
    // Frozen catches every failure in the PowerShell try and cleanup blocks.
    let result = read_powershell_clipboard(&temp_file).ok().flatten();
    let _ = std::fs::remove_file(encode_node_utf8(&temp_file));
    result
}

fn read_powershell_clipboard(temp_file: &JsString) -> Result<Option<ClipboardImage>, CallbackError> {
    let Some(windows_path) = run_command("wslpath", &["-w".into(), temp_file.clone()], DEFAULT_LIST_TIMEOUT)? else {
        return Ok(None);
    };
    let windows_path = decode_buffer_utf8(&windows_path).trim();
    if windows_path.is_empty() {
        return Ok(None);
    }
    let script = powershell_script(&windows_path);
    let Some(output) = run_command(
        "powershell.exe",
        &["-NoProfile".into(), "-Command".into(), script],
        DEFAULT_POWERSHELL_TIMEOUT,
    )?
    else {
        return Ok(None);
    };
    if decode_buffer_utf8(&output).trim() != "ok" {
        return Ok(None);
    }
    let Ok(bytes) = std::fs::read(encode_node_utf8(temp_file)) else {
        return Ok(None);
    };
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(ClipboardImage {
        bytes,
        mime_type: "image/png".into(),
    }))
}

fn powershell_script(windows_path: &JsString) -> JsString {
    let quoted = windows_path.replace("'", "''");
    JsString::join([
        JsString::from("Add-Type -AssemblyName System.Windows.Forms"),
        "Add-Type -AssemblyName System.Drawing".into(),
        pi_js_compat::js_format!("$path = '{}'", quoted),
        "$img = [System.Windows.Forms.Clipboard]::GetImage()".into(),
        "if ($img) { $img.Save($path, [System.Drawing.Imaging.ImageFormat]::Png); Write-Output 'ok' } else { Write-Output 'empty' }".into(),
    ], "; ")
}

fn is_wsl(environment: &ClipboardEnvironment) -> bool {
    if environment.has("WSL_DISTRO_NAME") || environment.has("WSLENV") {
        return true;
    }
    std::fs::read("/proc/version")
        .map(|bytes| {
            let release = decode_buffer_utf8(&bytes).to_lowercase();
            release.contains("microsoft") || release.contains("wsl")
        })
        .unwrap_or(false)
}

/// Buffer.toString('utf-8') replaces invalid input bytes; no JS string is projected.
fn decode_buffer_utf8(bytes: &[u8]) -> JsString {
    JsString::from(String::from_utf8_lossy(bytes).as_ref())
}

fn split_types(bytes: &[u8]) -> Vec<JsString> {
    decode_buffer_utf8(bytes)
        .split('\n')
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect()
}

fn select_preferred_image_mime_type(mime_types: &[JsString]) -> Option<JsString> {
    let normalized: Vec<_> = mime_types
        .iter()
        .map(JsString::trim)
        .filter(|value| !value.is_empty())
        .collect();
    for preferred in SUPPORTED_IMAGE_MIME_TYPES {
        if let Some(found) = normalized.iter().find(|value| base_mime_type(value) == preferred) {
            return Some(found.clone());
        }
    }
    normalized
        .into_iter()
        .find(|value| base_mime_type(value).starts_with("image/"))
}

fn is_supported_image_mime_type(mime_type: &JsString) -> bool {
    let base = base_mime_type(mime_type);
    SUPPORTED_IMAGE_MIME_TYPES.iter().any(|mime_type| base == *mime_type)
}

fn base_mime_type(mime_type: &JsString) -> JsString {
    mime_type
        .split(';')
        .next()
        .unwrap_or_else(|| mime_type.clone())
        .trim()
        .to_lowercase()
}

fn run_command(program: &str, arguments: &[JsString], timeout: Duration) -> Result<Option<Vec<u8>>, CallbackError> {
    let result = spawn_sync(
        &program.into(),
        arguments,
        SpawnSyncOptions {
            timeout: Some(timeout),
            max_buffer: Some(DEFAULT_MAX_BUFFER_BYTES),
            ..Default::default()
        },
    )
    .map_err(|error| CallbackError::from(*error))?;
    if result.error.is_some() || result.status != Some(0) {
        return Ok(None);
    }
    Ok(result.stdout)
}
