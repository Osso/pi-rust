#[path = "support/oracle.rs"]
mod oracle;
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::terminal_image::{KittyEncodeOptions, encode_kitty};
use serde_json::json;

#[test]
fn kitty_chunking_keeps_native_units_when_a_pair_crosses_the_source_boundary() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let text = "A".repeat(4095) + "😀Z";
    let expected = oracle.request(&json!({"operation":"kitty_encode","units":JsString::from(&text).units()}));
    assert_eq!(expected["ok"], true, "{expected}");
    let result = encode_kitty(&text, &KittyEncodeOptions::default());
    let actual = serde_json::to_string(&result).unwrap();
    assert_eq!(
        parse_lossless(actual).unwrap(),
        parse_lossless(expected["json"].as_str().unwrap()).unwrap()
    );
}

static CAPS: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn image_components_cache_styles_and_protocol_metadata_keep_native_units() {
    use pi_tui::components::image::{Image, ImageOptions, ImageTheme};
    use pi_tui::terminal_image::*;
    use pi_tui::tui::Component;
    use std::{cell::RefCell, rc::Rc};
    let _lock = CAPS.lock().unwrap();
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for units in [
        vec![0xd800],
        vec![0xdc00],
        vec![0xd83d, 0xde00],
        vec![97, 0xd800, 98, 0xdc00],
        vec![0xd800, 27, 7, 0xdc00],
    ] {
        let raw = JsString::from_utf16(units.clone());
        for protocol in [None, Some(ImageProtocol::Kitty), Some(ImageProtocol::Iterm2)] {
            let protocol_name = match protocol {
                None => "none",
                Some(ImageProtocol::Kitty) => "kitty",
                Some(ImageProtocol::Iterm2) => "iterm2",
            };
            set_capabilities(TerminalCapabilities {
                images: protocol,
                true_color: true,
                hyperlinks: true,
            });
            set_cell_dimensions(CellDimensions {
                width_px: 9,
                height_px: 18,
            });
            for width in [1, 3, 8] {
                let expected = oracle
                    .request(&json!({"operation":"image_values","units":units,"protocol":protocol_name,"width":width}));
                assert_eq!(expected["ok"], true, "{expected}");
                let dimensions = ImageDimensions {
                    width_px: 18,
                    height_px: 36,
                };
                let kitty = encode_kitty(
                    &raw,
                    &KittyEncodeOptions {
                        columns: Some(2.0),
                        rows: Some(3.0),
                        image_id: Some(9),
                        move_cursor: Some(false),
                    },
                );
                let iterm = encode_iterm2(
                    &raw,
                    &Iterm2Options {
                        width: Some(Iterm2Size::Text(raw.clone())),
                        height: Some(Iterm2Size::Text(raw.clone() + "%")),
                        name: Some(raw.clone() + ".png"),
                        preserve_aspect_ratio: Some(false),
                        inline: Some(false),
                    },
                );
                let link = hyperlink(&raw, JsString::from("https://host/") + &raw);
                let mime = JsString::from("image/") + &raw;
                let filename = raw.clone() + ".png";
                let fallback = image_fallback(&mime, Some(dimensions), Some(&filename));
                let rendered = render_image(
                    &raw,
                    dimensions,
                    &ImageRenderOptions {
                        max_width_cells: Some(6.0),
                        max_height_cells: Some(3.0),
                        image_id: Some(9),
                        move_cursor: Some(false),
                        ..ImageRenderOptions::default()
                    },
                )
                .map(|result| (result.sequence, result.rows, result.image_id));
                let seen = Rc::new(RefCell::new(Vec::new()));
                let captured = seen.clone();
                let prefix = raw.clone();
                let theme = Rc::new(ImageTheme {
                    fallback_color: Rc::new(move |value| {
                        captured.borrow_mut().push(value.clone());
                        prefix.clone() + value + &prefix
                    }),
                });
                let mut image = Image::new(
                    raw.clone(),
                    mime,
                    theme,
                    ImageOptions {
                        filename: Some(filename),
                        image_id: Some(9),
                        ..ImageOptions::default()
                    },
                    Some(dimensions),
                );
                let first = image.render(width);
                let cached = image.render(width);
                image.invalidate();
                let invalidated = image.render(width);
                let actual = serde_json::to_string(&(
                    kitty,
                    iterm,
                    link,
                    fallback,
                    (delete_kitty_image(9), delete_all_kitty_images()),
                    rendered,
                    (first, cached, invalidated, image.image_id()),
                    seen.borrow().clone(),
                ))
                .unwrap();
                assert_eq!(
                    parse_lossless(actual).unwrap(),
                    parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
                    "units={units:?} protocol={protocol_name} width={width}"
                );
            }
        }
    }
}

#[test]
fn native_base64_dimension_decode_does_not_rewrite_the_render_payload() {
    use pi_tui::components::image::{Image, ImageOptions, ImageTheme};
    use pi_tui::terminal_image::*;
    use pi_tui::tui::Component;
    use std::rc::Rc;
    let _lock = CAPS.lock().unwrap();
    let mut bytes = vec![0; 24];
    bytes[..4].copy_from_slice(&[0x89, 0x50, 0x4e, 0x47]);
    bytes[16..20].copy_from_slice(&18u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&36u32.to_be_bytes());
    let base64 = pi_js_compat::base64::buffer_to_base64(&bytes);
    let raw = JsString::from_utf16(base64.bytes().map(|byte| 0xd800 + u16::from(byte)).collect());
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let expected = oracle.request(&json!({"operation":"image_dimensions","units":raw.units()}));
    assert_eq!(expected["ok"], true, "{expected}");
    set_capabilities(TerminalCapabilities {
        images: Some(ImageProtocol::Kitty),
        true_color: true,
        hyperlinks: true,
    });
    set_cell_dimensions(CellDimensions {
        width_px: 9,
        height_px: 18,
    });
    let dimensions = get_image_dimensions(&raw, "image/png").map(|value| (value.width_px, value.height_px));
    let mut image = Image::new(
        raw,
        "image/png",
        Rc::new(ImageTheme {
            fallback_color: Rc::new(|value: &JsString| value.clone()),
        }),
        ImageOptions {
            image_id: Some(9),
            ..ImageOptions::default()
        },
        None,
    );
    let actual = serde_json::to_string(&(dimensions, image.render(10))).unwrap();
    assert_eq!(
        parse_lossless(actual).unwrap(),
        parse_lossless(expected["json"].as_str().unwrap()).unwrap()
    );
}
