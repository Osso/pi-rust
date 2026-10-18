//! Converts terminal ANSI SGR styling to inline-styled HTML.

const ANSI_COLORS: [&str; 16] = [
    "#000000", "#800000", "#008000", "#808000", "#000080", "#800080", "#008080", "#c0c0c0", "#808080", "#ff0000",
    "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff",
];

const ESCAPE: char = '\u{1b}';

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct TextStyle {
    fg: Option<String>,
    bg: Option<String>,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
}

impl TextStyle {
    fn is_empty(&self) -> bool {
        self.fg.is_none() && self.bg.is_none() && !self.bold && !self.dim && !self.italic && !self.underline
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn to_inline_css(&self) -> String {
        let mut parts = Vec::new();
        if let Some(fg) = &self.fg {
            parts.push(format!("color:{fg}"));
        }
        if let Some(bg) = &self.bg {
            parts.push(format!("background-color:{bg}"));
        }
        if self.bold {
            parts.push("font-weight:bold".to_owned());
        }
        if self.dim {
            parts.push("opacity:0.6".to_owned());
        }
        if self.italic {
            parts.push("font-style:italic".to_owned());
        }
        if self.underline {
            parts.push("text-decoration:underline".to_owned());
        }
        parts.join(";")
    }
}

/// Converts ANSI-escaped text to HTML with inline styles.
pub fn ansi_to_html(text: &str) -> String {
    let mut style = TextStyle::default();
    let mut result = String::new();
    let mut last_index = 0;
    let mut in_span = false;

    for (start, end) in escape_sequences(text) {
        let before = &text[last_index..start];
        if !before.is_empty() {
            push_escaped(&mut result, before);
        }

        if in_span {
            result.push_str("</span>");
            in_span = false;
        }
        apply_sgr(&parse_params(&text[start + 2..end]), &mut style);
        if !style.is_empty() {
            result.push_str("<span style=\"");
            result.push_str(&style.to_inline_css());
            result.push_str("\">");
            in_span = true;
        }
        last_index = end + 1;
    }

    let remaining = &text[last_index..];
    if !remaining.is_empty() {
        push_escaped(&mut result, remaining);
    }
    if in_span {
        result.push_str("</span>");
    }
    result
}

/// Converts ANSI-escaped lines to `<div class="ansi-line">` elements without extra whitespace.
pub fn ansi_lines_to_html(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| {
            let html = ansi_to_html(line);
            let content = if html.is_empty() { "&nbsp;" } else { &html };
            format!("<div class=\"ansi-line\">{content}</div>")
        })
        .collect()
}

/// Returns `(start, m_index)` for each `ESC [ params m` sequence, where `params` spans `start + 2..m_index`.
fn escape_sequences(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut sequences = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == ESCAPE as u8 && bytes.get(index + 1) == Some(&b'[') {
            let mut cursor = index + 2;
            while matches!(bytes.get(cursor), Some(b'0'..=b'9' | b';')) {
                cursor += 1;
            }
            if bytes.get(cursor) == Some(&b'm') {
                sequences.push((index, cursor));
                index = cursor + 1;
                continue;
            }
        }
        index += 1;
    }
    sequences
}

fn parse_params(params: &str) -> Vec<f64> {
    if params.is_empty() {
        return vec![0.0];
    }
    params.split(';').map(parse_leading_integer).collect()
}

/// `parseInt(part, 10) || 0`: non-numeric and zero values both become zero.
fn parse_leading_integer(part: &str) -> f64 {
    if part.is_empty() {
        return 0.0;
    }
    match part.parse::<f64>() {
        Ok(value) if value != 0.0 => value,
        _ => 0.0,
    }
}

fn apply_sgr(params: &[f64], style: &mut TextStyle) {
    let mut index = 0;
    while index < params.len() {
        let code = params[index];
        let extended_256 = params.get(index + 1) == Some(&5.0) && params.len() > index + 2;
        let extended_rgb = params.get(index + 1) == Some(&2.0) && params.len() > index + 4;
        if code == 0.0 {
            style.reset();
        } else if code == 1.0 {
            style.bold = true;
        } else if code == 2.0 {
            style.dim = true;
        } else if code == 3.0 {
            style.italic = true;
        } else if code == 4.0 {
            style.underline = true;
        } else if code == 22.0 {
            style.bold = false;
            style.dim = false;
        } else if code == 23.0 {
            style.italic = false;
        } else if code == 24.0 {
            style.underline = false;
        } else if (30.0..=37.0).contains(&code) {
            style.fg = Some(ANSI_COLORS[(code - 30.0) as usize].to_owned());
        } else if code == 38.0 {
            if extended_256 {
                style.fg = Some(color256_to_hex(params[index + 2]));
                index += 2;
            } else if extended_rgb {
                style.fg = Some(rgb_css(params[index + 2], params[index + 3], params[index + 4]));
                index += 4;
            }
        } else if code == 39.0 {
            style.fg = None;
        } else if (40.0..=47.0).contains(&code) {
            style.bg = Some(ANSI_COLORS[(code - 40.0) as usize].to_owned());
        } else if code == 48.0 {
            if extended_256 {
                style.bg = Some(color256_to_hex(params[index + 2]));
                index += 2;
            } else if extended_rgb {
                style.bg = Some(rgb_css(params[index + 2], params[index + 3], params[index + 4]));
                index += 4;
            }
        } else if code == 49.0 {
            style.bg = None;
        } else if (90.0..=97.0).contains(&code) {
            style.fg = Some(ANSI_COLORS[(code - 90.0 + 8.0) as usize].to_owned());
        } else if (100.0..=107.0).contains(&code) {
            style.bg = Some(ANSI_COLORS[(code - 100.0 + 8.0) as usize].to_owned());
        }
        index += 1;
    }
}

fn rgb_css(red: f64, green: f64, blue: f64) -> String {
    format!(
        "rgb({},{},{})",
        js_number_to_string(red),
        js_number_to_string(green),
        js_number_to_string(blue)
    )
}

fn color256_to_hex(index: f64) -> String {
    if index < 16.0 {
        return ANSI_COLORS[index as usize].to_owned();
    }

    if index < 232.0 {
        let cube_index = index - 16.0;
        let red = (cube_index / 36.0).floor();
        let green = ((cube_index % 36.0) / 6.0).floor();
        let blue = cube_index % 6.0;
        return format!("#{}{}{}", hex_component(red), hex_component(green), hex_component(blue));
    }

    let gray = 8.0 + (index - 232.0) * 10.0;
    let gray_hex = pad_start_hex(gray);
    format!("#{gray_hex}{gray_hex}{gray_hex}")
}

fn hex_component(value: f64) -> String {
    let component = if value == 0.0 { 0.0 } else { 55.0 + value * 40.0 };
    pad_start_hex(component)
}

fn pad_start_hex(value: f64) -> String {
    let hex = js_number_to_radix16(value);
    if hex.len() >= 2 { hex } else { format!("0{hex}") }
}

/// JavaScript `Number.prototype.toString(16)` for the non-negative integers this module produces.
fn js_number_to_radix16(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return "Infinity".to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }
    let bits = value.to_bits();
    let exponent_bits = ((bits >> 52) & 0x7FF) as i32;
    let fraction = bits & ((1 << 52) - 1);
    let (mut mantissa, mut exponent) = if exponent_bits == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1 << 52), exponent_bits - 1075)
    };
    while mantissa & 1 == 0 && exponent < 0 {
        mantissa >>= 1;
        exponent += 1;
    }
    if exponent < 0 {
        // Fractional values cannot be produced by the palette math; keep the exact integer part.
        mantissa >>= -exponent;
        exponent = 0;
    }
    let shift = exponent % 4;
    let mut digits = format!("{:x}", mantissa << shift);
    digits.push_str(&"0".repeat((exponent / 4) as usize));
    digits
}

/// JavaScript `Number.prototype.toString()` for non-negative integers and infinities.
fn js_number_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return "Infinity".to_owned();
    }
    if value < 1e21 {
        return format!("{value:.0}");
    }
    let scientific = format!("{value:e}");
    match scientific.split_once('e') {
        Some((digits, exponent)) if !exponent.starts_with('-') => format!("{digits}e+{exponent}"),
        _ => scientific,
    }
}

fn push_escaped(output: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#039;"),
            other => output.push(other),
        }
    }
}
