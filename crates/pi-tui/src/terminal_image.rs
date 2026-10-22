//! Terminal image protocols, capabilities, and Kitty image metadata.

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::hash::{BuildHasher, Hasher};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Inline-image protocol supported by the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    Kitty,
    Iterm2,
}

/// Detected terminal capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCapabilities {
    pub images: Option<ImageProtocol>,
    pub true_color: bool,
    pub hyperlinks: bool,
}

/// Terminal cell size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellDimensions {
    pub width_px: u32,
    pub height_px: u32,
}

/// Image dimensions in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageDimensions {
    pub width_px: u32,
    pub height_px: u32,
}

/// Image size in terminal cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageCellSize {
    pub columns: u32,
    pub rows: u32,
}

/// Options for [`render_image`].
#[derive(Debug, Clone, Default)]
pub struct ImageRenderOptions {
    pub max_width_cells: Option<f64>,
    pub max_height_cells: Option<f64>,
    pub preserve_aspect_ratio: Option<bool>,
    pub image_id: Option<u32>,
    pub move_cursor: Option<bool>,
}

/// Options for [`encode_kitty`].
#[derive(Debug, Clone, Default)]
pub struct KittyEncodeOptions {
    pub columns: Option<u32>,
    pub rows: Option<u32>,
    pub image_id: Option<u32>,
    pub move_cursor: Option<bool>,
}

/// Width/height value for iTerm2's OSC 1337 metadata.
#[derive(Debug, Clone, PartialEq)]
pub enum Iterm2Size {
    Number(f64),
    Text(JsString),
}

impl Iterm2Size {
    pub fn to_js_string(&self) -> JsString {
        match self {
            Self::Number(value) => pi_js_compat::number::js_number_to_string(*value).into(),
            Self::Text(value) => value.clone(),
        }
    }
}

/// Options for [`encode_iterm2`].
#[derive(Debug, Clone, Default)]
pub struct Iterm2Options {
    pub width: Option<Iterm2Size>,
    pub height: Option<Iterm2Size>,
    pub name: Option<JsString>,
    pub preserve_aspect_ratio: Option<bool>,
    pub inline: Option<bool>,
}

/// Result of [`render_image`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedImage {
    pub sequence: JsString,
    pub columns: u32,
    pub rows: u32,
    pub image_id: Option<u32>,
}

/// Environment values consulted by capability detection.
///
/// The TypeScript function reads `process.env` directly. Rust exposes this
/// snapshot so callers and tests can supply values without mutating process
/// environment.
#[derive(Debug, Clone, Default)]
pub struct TerminalEnvironment {
    pub term_program: Option<String>,
    pub terminal_emulator: Option<String>,
    pub term: Option<String>,
    pub color_term: Option<String>,
    pub tmux: bool,
    pub kitty_window_id: bool,
    pub ghostty_resources_dir: bool,
    pub wezterm_pane: bool,
    pub warp_session_id: bool,
    pub warp_terminal_session_uuid: bool,
    pub iterm_session_id: bool,
    pub wt_session: bool,
}

impl TerminalEnvironment {
    pub fn from_process() -> Self {
        Self {
            term_program: environment_lowercase("TERM_PROGRAM"),
            terminal_emulator: environment_lowercase("TERMINAL_EMULATOR"),
            term: environment_lowercase("TERM"),
            color_term: environment_lowercase("COLORTERM"),
            tmux: environment_flag("TMUX"),
            kitty_window_id: environment_flag("KITTY_WINDOW_ID"),
            ghostty_resources_dir: environment_flag("GHOSTTY_RESOURCES_DIR"),
            wezterm_pane: environment_flag("WEZTERM_PANE"),
            warp_session_id: environment_flag("WARP_SESSION_ID"),
            warp_terminal_session_uuid: environment_flag("WARP_TERMINAL_SESSION_UUID"),
            iterm_session_id: environment_flag("ITERM_SESSION_ID"),
            wt_session: environment_flag("WT_SESSION"),
        }
    }
}

fn environment_flag(name: &str) -> bool {
    pi_js_compat::process_env::get(name).is_some_and(|value| !value.is_empty())
}

fn environment_lowercase(name: &str) -> Option<String> {
    pi_js_compat::process_env::get(name)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_lowercase())
}

static CAPABILITIES: Mutex<Option<TerminalCapabilities>> = Mutex::new(None);
static CELL_DIMENSIONS: Mutex<CellDimensions> = Mutex::new(CellDimensions {
    width_px: 9,
    height_px: 18,
});

fn capabilities_lock() -> std::sync::MutexGuard<'static, Option<TerminalCapabilities>> {
    CAPABILITIES.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn cell_dimensions_lock() -> std::sync::MutexGuard<'static, CellDimensions> {
    CELL_DIMENSIONS.lock().unwrap_or_else(|poison| poison.into_inner())
}

pub fn get_cell_dimensions() -> CellDimensions {
    *cell_dimensions_lock()
}

pub fn set_cell_dimensions(dimensions: CellDimensions) {
    *cell_dimensions_lock() = dimensions;
}

fn probe_tmux_hyperlinks() -> bool {
    let Ok(mut child) = Command::new("tmux")
        .args(["display-message", "-p", "#{client_termfeatures}"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < Duration::from_millis(250) => {
                std::thread::sleep(Duration::from_millis(5));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }

    let Ok(output) = child.wait_with_output() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let Ok(stdout) = String::from_utf8(output.stdout) else {
        return false;
    };
    stdout.split(',').map(str::trim).any(|feature| feature == "hyperlinks")
}

/// Detect capabilities from an explicit environment snapshot.
pub fn detect_capabilities_in(
    environment: &TerminalEnvironment,
    tmux_forwards_hyperlink: impl FnOnce() -> bool,
) -> TerminalCapabilities {
    let term_program = environment.term_program.as_deref().unwrap_or("");
    let terminal_emulator = environment.terminal_emulator.as_deref().unwrap_or("");
    let term = environment.term.as_deref().unwrap_or("");
    let color_term = environment.color_term.as_deref().unwrap_or("");
    let has_true_color_hint = color_term == "truecolor" || color_term == "24bit";

    if environment.tmux || term.starts_with("tmux") {
        return TerminalCapabilities {
            images: None,
            true_color: has_true_color_hint,
            hyperlinks: tmux_forwards_hyperlink(),
        };
    }
    if term.starts_with("screen") {
        return TerminalCapabilities {
            images: None,
            true_color: has_true_color_hint,
            hyperlinks: false,
        };
    }
    if environment.kitty_window_id || term_program == "kitty" {
        return TerminalCapabilities {
            images: Some(ImageProtocol::Kitty),
            true_color: true,
            hyperlinks: true,
        };
    }
    if term_program == "ghostty" || term.contains("ghostty") || environment.ghostty_resources_dir {
        return TerminalCapabilities {
            images: Some(ImageProtocol::Kitty),
            true_color: true,
            hyperlinks: true,
        };
    }
    if environment.wezterm_pane || term_program == "wezterm" {
        return TerminalCapabilities {
            images: Some(ImageProtocol::Kitty),
            true_color: true,
            hyperlinks: true,
        };
    }
    if term_program == "warpterminal" || environment.warp_session_id || environment.warp_terminal_session_uuid {
        return TerminalCapabilities {
            images: Some(ImageProtocol::Kitty),
            true_color: true,
            hyperlinks: true,
        };
    }
    if environment.iterm_session_id || term_program == "iterm.app" {
        return TerminalCapabilities {
            images: Some(ImageProtocol::Iterm2),
            true_color: true,
            hyperlinks: true,
        };
    }
    if environment.wt_session {
        return TerminalCapabilities {
            images: None,
            true_color: true,
            hyperlinks: true,
        };
    }
    if term_program == "vscode" {
        return TerminalCapabilities {
            images: None,
            true_color: true,
            hyperlinks: true,
        };
    }
    if term_program == "alacritty" {
        return TerminalCapabilities {
            images: None,
            true_color: true,
            hyperlinks: true,
        };
    }
    if terminal_emulator == "jetbrains-jediterm" {
        return TerminalCapabilities {
            images: None,
            true_color: true,
            hyperlinks: false,
        };
    }
    TerminalCapabilities {
        images: None,
        true_color: has_true_color_hint,
        hyperlinks: false,
    }
}

/// Detect capabilities from the process environment, probing tmux if present.
pub fn detect_capabilities() -> TerminalCapabilities {
    detect_capabilities_in(&TerminalEnvironment::from_process(), probe_tmux_hyperlinks)
}

pub fn get_capabilities() -> TerminalCapabilities {
    let mut cached = capabilities_lock();
    *cached.get_or_insert_with(detect_capabilities)
}

pub fn reset_capabilities_cache() {
    *capabilities_lock() = None;
}

/// Override the cached capabilities.
pub fn set_capabilities(capabilities: TerminalCapabilities) {
    *capabilities_lock() = Some(capabilities);
}

const KITTY_PREFIX: &str = "\x1b_G";
const ITERM2_PREFIX: &str = "\x1b]1337;File=";

pub fn is_image_line(line: impl Into<pi_js_compat::js_json::JsString>) -> bool {
    let line = line.into();
    line.starts_with(KITTY_PREFIX)
        || line.starts_with(ITERM2_PREFIX)
        || line.contains(KITTY_PREFIX)
        || line.contains(ITERM2_PREFIX)
}

/// Generate a random image ID in `[1, 0xffffffff]`.
pub fn allocate_image_id() -> u32 {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    hasher.write_u64(timestamp);
    hasher.write_u32(std::process::id());
    ((hasher.finish() % 0xffff_fffe) + 1) as u32
}

pub fn encode_kitty(base64_data: impl Into<JsString>, options: &KittyEncodeOptions) -> JsString {
    let base64_data = &base64_data.into();
    const CHUNK_SIZE: usize = 4096;

    let mut params = vec![JsString::from("a=T"), JsString::from("f=100"), JsString::from("q=2")];
    if options.move_cursor == Some(false) {
        params.push(JsString::from("C=1"));
    }
    if let Some(columns) = options.columns.filter(|value| *value != 0) {
        params.push(JsString::from(format!("c={columns}")));
    }
    if let Some(rows) = options.rows.filter(|value| *value != 0) {
        params.push(JsString::from(format!("r={rows}")));
    }
    if let Some(image_id) = options.image_id.filter(|value| *value != 0) {
        params.push(JsString::from(format!("i={image_id}")));
    }
    let parameters = JsString::join(params.iter(), ",");

    if base64_data.len() <= CHUNK_SIZE {
        return JsString::from("\x1b_G") + &parameters + ";" + base64_data + "\x1b\\";
    }

    let mut chunks = Vec::new();
    let mut offset = 0;
    let mut is_first = true;
    while offset < base64_data.len() {
        let end = (offset + CHUNK_SIZE).min(base64_data.len());
        let chunk = base64_data.slice(offset, end);
        let is_last = offset + CHUNK_SIZE >= base64_data.len();
        if is_first {
            chunks.push(JsString::from("\x1b_G") + &parameters + ",m=1;" + chunk + "\x1b\\");
            is_first = false;
        } else if is_last {
            chunks.push(JsString::from("\x1b_Gm=0;") + chunk + "\x1b\\");
        } else {
            chunks.push(JsString::from("\x1b_Gm=1;") + chunk + "\x1b\\");
        }
        offset += CHUNK_SIZE;
    }
    JsString::join(chunks, "")
}

pub fn delete_kitty_image(image_id: u32) -> JsString {
    JsString::from(format!("\x1b_Ga=d,d=I,i={image_id},q=2\x1b\\"))
}

pub fn delete_all_kitty_images() -> JsString {
    JsString::from("\x1b_Ga=d,d=A,q=2\x1b\\")
}

pub fn encode_iterm2(base64_data: impl Into<JsString>, options: &Iterm2Options) -> JsString {
    let base64_data = &base64_data.into();
    let mut params = vec![JsString::from(format!(
        "inline={}",
        if options.inline == Some(false) { 0 } else { 1 }
    ))];

    if let Some(width) = &options.width {
        params.push(JsString::from("width=") + width.to_js_string());
    }
    if let Some(height) = &options.height {
        params.push(JsString::from("height=") + height.to_js_string());
    }
    if let Some(name) = &options.name
        && !name.is_empty()
    {
        params.push(JsString::from(format!(
            "name={}",
            pi_js_compat::base64::btoa(encode_node_utf8(name).as_bytes())
        )));
    }
    if options.preserve_aspect_ratio == Some(false) {
        params.push(JsString::from("preserveAspectRatio=0"));
    }

    JsString::from("\x1b]1337;File=") + JsString::join(params.iter(), ";") + ":" + base64_data + "\x07"
}

pub fn calculate_image_cell_size(
    image_dimensions: ImageDimensions,
    max_width_cells: f64,
    max_height_cells: Option<f64>,
    cell_dimensions: CellDimensions,
) -> ImageCellSize {
    let max_width = max_width_cells.floor().max(1.0);
    let max_height = max_height_cells.map(|value| value.floor().max(1.0));
    let image_width = f64::from(image_dimensions.width_px.max(1));
    let image_height = f64::from(image_dimensions.height_px.max(1));

    let width_scale = max_width * f64::from(cell_dimensions.width_px) / image_width;
    let height_scale = max_height.map_or(width_scale, |max_height| {
        max_height * f64::from(cell_dimensions.height_px) / image_height
    });
    let scale = width_scale.min(height_scale);

    let scaled_width = image_width * scale;
    let scaled_height = image_height * scale;
    let columns = (scaled_width / f64::from(cell_dimensions.width_px)).ceil();
    let rows = (scaled_height / f64::from(cell_dimensions.height_px)).ceil();

    let columns = 1.0_f64.max(max_width.min(columns));
    let rows = 1.0_f64.max(max_height.map_or(rows, |max_height| max_height.min(rows)));

    ImageCellSize {
        columns: columns as u32,
        rows: rows as u32,
    }
}

pub fn calculate_image_rows(
    image_dimensions: ImageDimensions,
    target_width_cells: f64,
    cell_dimensions: CellDimensions,
) -> u32 {
    calculate_image_cell_size(image_dimensions, target_width_cells, None, cell_dimensions).rows
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(offset)?, *bytes.get(offset + 1)?]))
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
        *bytes.get(offset + 2)?,
        *bytes.get(offset + 3)?,
    ]))
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*bytes.get(offset)?, *bytes.get(offset + 1)?]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
        *bytes.get(offset + 2)?,
        *bytes.get(offset + 3)?,
    ]))
}

pub fn get_png_dimensions(base64_data: impl Into<JsString>) -> Option<ImageDimensions> {
    let base64_data = &base64_data.into();
    let bytes = pi_js_compat::base64::buffer_from_base64(base64_data);
    if bytes.len() < 24 {
        return None;
    }
    if bytes[0..4] != [0x89, 0x50, 0x4e, 0x47] {
        return None;
    }
    Some(ImageDimensions {
        width_px: read_u32_be(&bytes, 16)?,
        height_px: read_u32_be(&bytes, 20)?,
    })
}

pub fn get_jpeg_dimensions(base64_data: impl Into<JsString>) -> Option<ImageDimensions> {
    let base64_data = &base64_data.into();
    let bytes = pi_js_compat::base64::buffer_from_base64(base64_data);
    if bytes.len() < 2 || bytes[0] != 0xff || bytes[1] != 0xd8 {
        return None;
    }

    let mut offset = 2;
    while offset + 9 < bytes.len() {
        if bytes[offset] != 0xff {
            offset += 1;
            continue;
        }
        let marker = bytes[offset + 1];
        if (0xc0..=0xc2).contains(&marker) {
            return Some(ImageDimensions {
                width_px: u32::from(read_u16_be(&bytes, offset + 7)?),
                height_px: u32::from(read_u16_be(&bytes, offset + 5)?),
            });
        }
        if offset + 3 >= bytes.len() {
            return None;
        }
        let length = read_u16_be(&bytes, offset + 2)?;
        if length < 2 {
            return None;
        }
        offset += 2 + usize::from(length);
    }
    None
}

pub fn get_gif_dimensions(base64_data: impl Into<JsString>) -> Option<ImageDimensions> {
    let base64_data = &base64_data.into();
    let bytes = pi_js_compat::base64::buffer_from_base64(base64_data);
    if bytes.len() < 10 {
        return None;
    }
    let signature = bytes[0..6].iter().map(|byte| byte & 0x7f).collect::<Vec<_>>();
    if signature != b"GIF87a" && signature != b"GIF89a" {
        return None;
    }
    Some(ImageDimensions {
        width_px: u32::from(read_u16_le(&bytes, 6)?),
        height_px: u32::from(read_u16_le(&bytes, 8)?),
    })
}

pub fn get_webp_dimensions(base64_data: impl Into<JsString>) -> Option<ImageDimensions> {
    let base64_data = &base64_data.into();
    let bytes = pi_js_compat::base64::buffer_from_base64(base64_data);
    if bytes.len() < 30 {
        return None;
    }
    let ascii_header = bytes[..16].iter().map(|byte| byte & 0x7f).collect::<Vec<_>>();
    if &ascii_header[0..4] != b"RIFF" || &ascii_header[8..12] != b"WEBP" {
        return None;
    }

    match &ascii_header[12..16] {
        b"VP8 " => Some(ImageDimensions {
            width_px: u32::from(read_u16_le(&bytes, 26)? & 0x3fff),
            height_px: u32::from(read_u16_le(&bytes, 28)? & 0x3fff),
        }),
        b"VP8L" => {
            if bytes.len() < 25 {
                return None;
            }
            let bits = read_u32_le(&bytes, 21)?;
            Some(ImageDimensions {
                width_px: (bits & 0x3fff) + 1,
                height_px: ((bits >> 14) & 0x3fff) + 1,
            })
        }
        b"VP8X" => {
            let width = u32::from(bytes[24]) | (u32::from(bytes[25]) << 8) | (u32::from(bytes[26]) << 16);
            let height = u32::from(bytes[27]) | (u32::from(bytes[28]) << 8) | (u32::from(bytes[29]) << 16);
            Some(ImageDimensions {
                width_px: width + 1,
                height_px: height + 1,
            })
        }
        _ => None,
    }
}

pub fn get_image_dimensions(
    base64_data: impl Into<JsString>,
    mime_type: impl Into<JsString>,
) -> Option<ImageDimensions> {
    let base64_data = base64_data.into();
    let mime_type = mime_type.into();
    if mime_type == "image/png" {
        get_png_dimensions(&base64_data)
    } else if mime_type == "image/jpeg" {
        get_jpeg_dimensions(&base64_data)
    } else if mime_type == "image/gif" {
        get_gif_dimensions(&base64_data)
    } else if mime_type == "image/webp" {
        get_webp_dimensions(&base64_data)
    } else {
        None
    }
}

pub fn render_image(
    base64_data: impl Into<JsString>,
    image_dimensions: ImageDimensions,
    options: &ImageRenderOptions,
) -> Option<RenderedImage> {
    let base64_data = &base64_data.into();
    let capabilities = get_capabilities();
    let protocol = capabilities.images?;

    let max_width = options.max_width_cells.unwrap_or(80.0);
    let size = calculate_image_cell_size(
        image_dimensions,
        max_width,
        options.max_height_cells,
        get_cell_dimensions(),
    );

    match protocol {
        ImageProtocol::Kitty => {
            let sequence = encode_kitty(
                base64_data,
                &KittyEncodeOptions {
                    columns: Some(size.columns),
                    rows: Some(size.rows),
                    image_id: options.image_id,
                    move_cursor: options.move_cursor,
                },
            );
            Some(RenderedImage {
                sequence,
                columns: size.columns,
                rows: size.rows,
                image_id: options.image_id,
            })
        }
        ImageProtocol::Iterm2 => {
            let sequence = encode_iterm2(
                base64_data,
                &Iterm2Options {
                    width: Some(Iterm2Size::Number(f64::from(size.columns))),
                    height: Some(Iterm2Size::Text(JsString::from("auto"))),
                    preserve_aspect_ratio: Some(options.preserve_aspect_ratio.unwrap_or(true)),
                    ..Iterm2Options::default()
                },
            );
            Some(RenderedImage {
                sequence,
                columns: size.columns,
                rows: size.rows,
                image_id: None,
            })
        }
    }
}

/// Wrap text in an OSC 8 hyperlink.
pub fn hyperlink(text: impl Into<JsString>, url: impl Into<JsString>) -> JsString {
    let text = text.into();
    let url = url.into();
    JsString::from("\x1b]8;;") + url + "\x1b\\" + text + "\x1b]8;;\x1b\\"
}

/// Text fallback when the terminal cannot render inline images.
pub fn image_fallback(
    mime_type: impl Into<JsString>,
    dimensions: Option<ImageDimensions>,
    filename: Option<&JsString>,
) -> JsString {
    let mut parts = Vec::new();
    if let Some(filename) = filename.filter(|filename| !filename.is_empty()) {
        parts.push(filename.clone());
    }
    parts.push(JsString::from("[") + mime_type.into() + "]");
    if let Some(dimensions) = dimensions {
        parts.push(JsString::from(format!(
            "{}x{}",
            dimensions.width_px, dimensions.height_px
        )));
    }
    JsString::from("[Image: ") + JsString::join(parts, " ") + "]"
}
