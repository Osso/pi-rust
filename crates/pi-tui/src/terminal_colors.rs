//! Terminal color-scheme and background-color report parsing.

use pi_js_compat::string::js_trim;

/// 8-bit RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Terminal color scheme reported by the OSC 997 color-scheme report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColorScheme {
    Dark,
    Light,
}

fn hex_to_rgb(hex: &str) -> RgbColor {
    let normalized = hex.strip_prefix('#').unwrap_or(hex);
    RgbColor {
        r: parse_hex_byte(&normalized[0..2]),
        g: parse_hex_byte(&normalized[2..4]),
        b: parse_hex_byte(&normalized[4..6]),
    }
}

fn parse_hex_byte(digits: &str) -> u8 {
    u8::from_str_radix(digits, 16).expect("hex digits are validated before conversion")
}

fn parse_osc_hex_channel(channel: &str) -> Option<u8> {
    if channel.is_empty() || !channel.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let digits = u32::try_from(channel.len()).ok()?;
    let max = 16_u64.pow(digits).checked_sub(1)?;
    if max == 0 {
        return None;
    }
    let value = u64::from_str_radix(channel, 16).ok()?;
    Some(((value as f64 / max as f64) * 255.0).round() as u8)
}

fn osc11_value(data: &str) -> Option<&str> {
    let rest = data.strip_prefix("\x1b]11;")?;
    let value = if let Some(value) = rest.strip_suffix('\x07') {
        value
    } else {
        rest.strip_suffix("\x1b\\")?
    };
    if value.contains(['\x07', '\x1b']) {
        return None;
    }
    Some(value)
}

/// Whether `data` is a strictly framed OSC 11 background-color response.
pub fn is_osc11_background_color_response(data: &str) -> bool {
    osc11_value(data).is_some()
}

/// Parse an OSC 11 background-color response into 8-bit RGB.
pub fn parse_osc11_background_color(data: &str) -> Option<RgbColor> {
    let value = js_trim(osc11_value(data)?);
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Some(hex_to_rgb(value));
        }
        if hex.len() == 12 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            let r = parse_osc_hex_channel(&hex[0..4])?;
            let g = parse_osc_hex_channel(&hex[4..8])?;
            let b = parse_osc_hex_channel(&hex[8..12])?;
            return Some(RgbColor { r, g, b });
        }
        return None;
    }

    let rgb_value = strip_rgb_prefix(value);
    let mut channels = rgb_value.split('/');
    let red = channels.next()?;
    let green = channels.next()?;
    let blue = channels.next()?;
    Some(RgbColor {
        r: parse_osc_hex_channel(red)?,
        g: parse_osc_hex_channel(green)?,
        b: parse_osc_hex_channel(blue)?,
    })
}

fn strip_rgb_prefix(value: &str) -> &str {
    let lowercase = value.to_ascii_lowercase();
    if lowercase.starts_with("rgba:") {
        &value["rgba:".len()..]
    } else if lowercase.starts_with("rgb:") {
        &value["rgb:".len()..]
    } else {
        value
    }
}

/// Parse one strictly framed OSC 997 color-scheme report.
pub fn parse_terminal_color_scheme_report(data: &str) -> Option<TerminalColorScheme> {
    match data {
        "\x1b[?997;1n" => Some(TerminalColorScheme::Dark),
        "\x1b[?997;2n" => Some(TerminalColorScheme::Light),
        _ => None,
    }
}
