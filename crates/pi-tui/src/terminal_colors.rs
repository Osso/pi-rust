//! Terminal color-scheme and background-color report parsing over native strings.

use crate::text_units::ascii_protocol;
use pi_js_compat::js_json::JsString;

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

fn is_hex(text: &JsString) -> bool {
    !text.is_empty()
        && text
            .units()
            .iter()
            .all(|unit| matches!(unit, 48..=57 | 65..=70 | 97..=102))
}

fn parse_osc_hex_channel(channel: &JsString) -> Option<u8> {
    if !is_hex(channel) {
        return None;
    }
    let max = 16_f64.powf(channel.len() as f64) - 1.0;
    if max <= 0.0 {
        return None;
    }
    // A numeric-parser projection of already validated ASCII digits, not text state.
    let digits = ascii_protocol(channel)?;
    let value = pi_js_compat::number::parse_int(&digits, 16);
    let rounded = ((value / max) * 255.0).round();
    // Existing approved nonfinite numeric gap: RgbColor channels are typed u8.
    rounded.is_finite().then_some(rounded as u8)
}

fn osc11_value(data: &JsString) -> Option<JsString> {
    let rest = data.strip_prefix("\x1b]11;")?;
    let value = if let Some(value) = rest.strip_suffix("\x07") {
        value
    } else {
        rest.strip_suffix("\x1b\\")?
    };
    if value.contains("\x07") || value.contains("\x1b") {
        return None;
    }
    Some(value)
}

/// Whether data is a strictly framed OSC 11 background-color response.
pub fn is_osc11_background_color_response(data: impl Into<JsString>) -> bool {
    osc11_value(&data.into()).is_some()
}

/// Parse an OSC 11 background-color response into 8-bit RGB.
pub fn parse_osc11_background_color(data: impl Into<JsString>) -> Option<RgbColor> {
    let value = osc11_value(&data.into())?.trim();
    if let Some(hex) = value.strip_prefix("#") {
        if !is_hex(&hex) {
            return None;
        }
        let channel_width = match hex.len() {
            6 => 2,
            12 => 4,
            _ => return None,
        };
        return Some(RgbColor {
            r: parse_osc_hex_channel(&hex.slice(0, channel_width))?,
            g: parse_osc_hex_channel(&hex.slice(channel_width, channel_width * 2))?,
            b: parse_osc_hex_channel(&hex.slice(channel_width * 2, channel_width * 3))?,
        });
    }
    let value = strip_rgb_prefix(&value);
    let mut channels = value.split('/');
    Some(RgbColor {
        r: parse_osc_hex_channel(&channels.next()?)?,
        g: parse_osc_hex_channel(&channels.next()?)?,
        b: parse_osc_hex_channel(&channels.next()?)?,
    })
}

fn strip_rgb_prefix(value: &JsString) -> JsString {
    let lowercase = value.to_lowercase();
    if lowercase.starts_with("rgba:") {
        value.slice(5, value.len())
    } else if lowercase.starts_with("rgb:") {
        value.slice(4, value.len())
    } else {
        value.clone()
    }
}

/// Parse one strictly framed OSC 997 color-scheme report.
pub fn parse_terminal_color_scheme_report(data: impl Into<JsString>) -> Option<TerminalColorScheme> {
    let data = data.into();
    if data == "\x1b[?997;1n" {
        Some(TerminalColorScheme::Dark)
    } else if data == "\x1b[?997;2n" {
        Some(TerminalColorScheme::Light)
    } else {
        None
    }
}
