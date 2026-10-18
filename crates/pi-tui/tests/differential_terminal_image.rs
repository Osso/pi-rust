#[path = "support/oracle.rs"]
mod oracle;

use std::sync::Mutex;

use pi_tui::terminal_image::{
    CellDimensions, ImageDimensions, ImageProtocol, ImageRenderOptions, Iterm2Options, Iterm2Size, KittyEncodeOptions,
    TerminalCapabilities, TerminalEnvironment, calculate_image_cell_size, crop_kitty_image_line,
    delete_all_kitty_images, delete_all_kitty_placements, delete_kitty_image, detect_capabilities_in, encode_iterm2,
    encode_kitty, get_gif_dimensions, get_image_dimensions, get_jpeg_dimensions, get_kitty_image_metadata,
    get_kitty_image_placement, get_png_dimensions, get_webp_dimensions, hyperlink, image_fallback, is_image_line,
    register_kitty_image_metadata, render_image, reset_capabilities_cache, set_capabilities, set_cell_dimensions,
};
use serde_json::{Value, json};

const SOURCE_DIRECTORY: &str = "packages/tui/src";
static CAPS_LOCK: Mutex<()> = Mutex::new(());

fn caps(images: Option<ImageProtocol>, true_color: bool, hyperlinks: bool) -> TerminalCapabilities {
    TerminalCapabilities {
        images,
        true_color,
        hyperlinks,
    }
}

fn environment(values: &[(&str, &str)]) -> TerminalEnvironment {
    let get = |key: &str| {
        values
            .iter()
            .find(|(candidate, _)| *candidate == key)
            .map(|(_, value)| value.to_lowercase())
    };
    let has = |key: &str| values.iter().any(|(candidate, _)| *candidate == key);
    TerminalEnvironment {
        term_program: get("TERM_PROGRAM"),
        terminal_emulator: get("TERMINAL_EMULATOR"),
        term: get("TERM"),
        color_term: get("COLORTERM"),
        tmux: has("TMUX"),
        kitty_window_id: has("KITTY_WINDOW_ID"),
        ghostty_resources_dir: has("GHOSTTY_RESOURCES_DIR"),
        wezterm_pane: has("WEZTERM_PANE"),
        warp_session_id: has("WARP_SESSION_ID"),
        warp_terminal_session_uuid: has("WARP_TERMINAL_SESSION_UUID"),
        iterm_session_id: has("ITERM_SESSION_ID"),
        wt_session: has("WT_SESSION"),
        is_windows_console: false,
    }
}

fn caps_json(value: TerminalCapabilities) -> Value {
    json!({
        "images": match value.images {
            Some(ImageProtocol::Kitty) => Some("kitty"),
            Some(ImageProtocol::Iterm2) => Some("iterm2"),
            None => None,
        },
        "trueColor": value.true_color,
        "hyperlinks": value.hyperlinks,
    })
}

#[test]
fn terminal_image_matches_the_pinned_typescript_source() {
    let _guard = CAPS_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    let mut oracle = oracle::TsOracle::spawn("tui-leaves-oracle.ts", SOURCE_DIRECTORY);

    let lines = [
        "\x1b]1337;File=size=100,100;inline=1:base64encodeddata==\x07",
        "Some text \x1b]1337;File=size=100,100;inline=1:base64data==\x07 more text",
        "\x1b_Ga=T,f=100,t=f,d=base64data...\x1b\\\x1b_Gm=i=1;\x1b\\",
        "  \x1b_Ga=T,f=100...\x1b\\\x1b_Gm=i=1;\x1b\\  ",
        "This is just a regular text line",
        "\x1b[31mRed text\x1b[0m",
        "",
        "\n\n",
        "/path/to/File_1337_backup/image.jpg",
        "Some text with ]1337;File but missing ESC at start",
        "Some text with _G but missing ESC at start",
    ];
    let response = oracle.request(&json!({
        "id": "image-lines",
        "operation": "terminal_image_lines",
        "cases": lines,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let results = response["output"]["results"].as_array().unwrap();
    for (line, expected) in lines.iter().zip(results) {
        assert_eq!(is_image_line(line), expected.as_bool().unwrap(), "{line:?}");
    }

    let long_data = "A".repeat(8192);
    let encode_cases = vec![
        json!({ "kind": "kitty", "data": "AAAA", "options": {} }),
        json!({ "kind": "kitty", "data": "AAAA", "options": { "columns": 2, "rows": 2, "moveCursor": false } }),
        json!({ "kind": "kitty", "data": "AAAA", "options": { "columns": 3, "rows": 3, "imageId": 42 } }),
        json!({ "kind": "kitty", "data": long_data, "options": { "columns": 3, "rows": 3, "imageId": 42, "moveCursor": false } }),
        json!({ "kind": "iterm2", "data": "AAAA", "options": { "width": 2, "height": "auto" } }),
        json!({ "kind": "iterm2", "data": "AAAA", "options": { "inline": false, "name": "shot.png", "preserveAspectRatio": false } }),
        json!({ "kind": "iterm2", "data": "AAAA", "options": { "width": "50%", "height": 10 } }),
    ];
    let response = oracle.request(&json!({
        "id": "image-encode",
        "operation": "terminal_image_encode",
        "cases": encode_cases,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let results = response["output"]["results"].as_array().unwrap();
    let expected_encoded = [
        encode_kitty("AAAA", &KittyEncodeOptions::default()),
        encode_kitty(
            "AAAA",
            &KittyEncodeOptions {
                columns: Some(2),
                rows: Some(2),
                move_cursor: Some(false),
                ..Default::default()
            },
        ),
        encode_kitty(
            "AAAA",
            &KittyEncodeOptions {
                columns: Some(3),
                rows: Some(3),
                image_id: Some(42),
                ..Default::default()
            },
        ),
        encode_kitty(
            &long_data,
            &KittyEncodeOptions {
                columns: Some(3),
                rows: Some(3),
                image_id: Some(42),
                move_cursor: Some(false),
            },
        ),
        encode_iterm2(
            "AAAA",
            &Iterm2Options {
                width: Some(Iterm2Size::Number(2.0)),
                height: Some(Iterm2Size::Text("auto".to_string())),
                ..Default::default()
            },
        ),
        encode_iterm2(
            "AAAA",
            &Iterm2Options {
                inline: Some(false),
                name: Some("shot.png".to_string()),
                preserve_aspect_ratio: Some(false),
                ..Default::default()
            },
        ),
        encode_iterm2(
            "AAAA",
            &Iterm2Options {
                width: Some(Iterm2Size::Text("50%".to_string())),
                height: Some(Iterm2Size::Number(10.0)),
                ..Default::default()
            },
        ),
    ];
    for (expected, actual) in results.iter().zip(expected_encoded) {
        assert_eq!(actual, expected.as_str().unwrap());
    }

    let png = {
        let mut bytes = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&640u32.to_be_bytes());
        bytes.extend_from_slice(&480u32.to_be_bytes());
        pi_js_compat::base64::btoa(&bytes)
    };
    let jpeg = pi_js_compat::base64::btoa(&[0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x64, 0x00, 0xc8, 0x00]);
    let jpeg_short = pi_js_compat::base64::btoa(&[0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x64, 0x00, 0xc8]);
    let gif = {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend_from_slice(&320u16.to_le_bytes());
        bytes.extend_from_slice(&240u16.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        pi_js_compat::base64::btoa(&bytes)
    };
    let vp8x = {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8X");
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(&[0x3f, 0x02, 0x00]);
        bytes.extend_from_slice(&[0x8f, 0x01, 0x00]);
        pi_js_compat::base64::btoa(&bytes)
    };
    let vp8l = {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8L");
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.push(0x2f);
        bytes.extend_from_slice(&((639u32) | ((479u32) << 14)).to_le_bytes());
        bytes.extend_from_slice(&[0; 5]);
        pi_js_compat::base64::btoa(&bytes)
    };
    let vp8 = {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(b"WEBP");
        bytes.extend_from_slice(b"VP8 ");
        bytes.extend_from_slice(&[0; 10]);
        bytes.extend_from_slice(&300u16.to_le_bytes());
        bytes.extend_from_slice(&200u16.to_le_bytes());
        pi_js_compat::base64::btoa(&bytes)
    };
    let dimension_cases = vec![
        json!({ "data": png, "mimeType": "image/png" }),
        json!({ "data": "AAAA", "mimeType": "image/png" }),
        json!({ "data": jpeg, "mimeType": "image/jpeg" }),
        json!({ "data": jpeg_short, "mimeType": "image/jpeg" }),
        json!({ "data": gif, "mimeType": "image/gif" }),
        json!({ "data": vp8x, "mimeType": "image/webp" }),
        json!({ "data": vp8l, "mimeType": "image/webp" }),
        json!({ "data": vp8, "mimeType": "image/webp" }),
        json!({ "data": "AAAA", "mimeType": "image/webp" }),
        json!({ "data": png, "mimeType": "image/unknown" }),
    ];
    let response = oracle.request(&json!({
        "id": "image-dimensions",
        "operation": "terminal_image_dimensions",
        "cases": dimension_cases,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let results = response["output"]["results"].as_array().unwrap();
    let rust_dimensions: Vec<Value> = [
        (&png, "image/png"),
        (&"AAAA".to_string(), "image/png"),
        (&jpeg, "image/jpeg"),
        (&jpeg_short, "image/jpeg"),
        (&gif, "image/gif"),
        (&vp8x, "image/webp"),
        (&vp8l, "image/webp"),
        (&vp8, "image/webp"),
        (&"AAAA".to_string(), "image/webp"),
        (&png, "image/unknown"),
    ]
    .iter()
    .map(|(data, mime)| {
        let encode = |value: Option<ImageDimensions>| match value {
            Some(dimensions) => json!({ "widthPx": dimensions.width_px, "heightPx": dimensions.height_px }),
            None => Value::Null,
        };
        json!({
            "png": encode(get_png_dimensions(data)),
            "jpeg": encode(get_jpeg_dimensions(data)),
            "gif": encode(get_gif_dimensions(data)),
            "webp": encode(get_webp_dimensions(data)),
            "dispatched": encode(get_image_dimensions(data, mime)),
        })
    })
    .collect();
    for (expected, actual) in results.iter().zip(rust_dimensions) {
        assert_eq!(actual, *expected);
    }

    let calc_cases = [
        (
            ImageDimensions {
                width_px: 100,
                height_px: 100,
            },
            3.0,
            None,
            CellDimensions {
                width_px: 10,
                height_px: 10,
            },
        ),
        (
            ImageDimensions {
                width_px: 10,
                height_px: 100,
            },
            10.0,
            Some(5.0),
            CellDimensions {
                width_px: 10,
                height_px: 10,
            },
        ),
        (
            ImageDimensions {
                width_px: 20,
                height_px: 20,
            },
            2.0,
            None,
            CellDimensions {
                width_px: 10,
                height_px: 10,
            },
        ),
        (
            ImageDimensions {
                width_px: 1000,
                height_px: 500,
            },
            7.5,
            None,
            CellDimensions {
                width_px: 9,
                height_px: 18,
            },
        ),
        (
            ImageDimensions {
                width_px: 1,
                height_px: 1,
            },
            0.0,
            Some(0.0),
            CellDimensions {
                width_px: 9,
                height_px: 18,
            },
        ),
    ];
    let calc_request: Vec<Value> = calc_cases
        .iter()
        .map(|(image, max_width, max_height, cell)| {
            let mut case = json!({
                "image": { "widthPx": image.width_px, "heightPx": image.height_px },
                "maxWidth": max_width,
                "cell": { "widthPx": cell.width_px, "heightPx": cell.height_px },
            });
            if let Some(max_height) = max_height {
                case["maxHeight"] = json!(max_height);
            }
            case
        })
        .collect();
    let response = oracle.request(&json!({
        "id": "image-calc",
        "operation": "terminal_image_calc",
        "cases": calc_request,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let results = response["output"]["results"].as_array().unwrap();
    for ((image, max_width, max_height, cell), expected) in calc_cases.iter().zip(results) {
        let actual = calculate_image_cell_size(*image, *max_width, *max_height, *cell);
        assert_eq!(actual.columns, expected["columns"].as_u64().unwrap() as u32);
        assert_eq!(actual.rows, expected["rows"].as_u64().unwrap() as u32);
    }

    let detect_cases: &[(&[(&str, &str)], bool)] = &[
        (&[], false),
        (&[("TMUX", "x"), ("TERM_PROGRAM", "ghostty")], true),
        (&[("TMUX", "x"), ("TERM_PROGRAM", "ghostty")], false),
        (&[("TERM", "tmux-256color"), ("TERM_PROGRAM", "iterm.app")], true),
        (&[("TERM", "screen-256color")], false),
        (&[("TERM_PROGRAM", "ghostty")], false),
        (
            &[("TERM_PROGRAM", "ghostty"), ("CMUX_WORKSPACE_ID", "workspace")],
            false,
        ),
        (&[("KITTY_WINDOW_ID", "1")], false),
        (&[("WEZTERM_PANE", "0")], false),
        (&[("TERM_PROGRAM", "WarpTerminal")], false),
        (&[("WARP_SESSION_ID", "id")], false),
        (&[("WARP_TERMINAL_SESSION_UUID", "uuid")], false),
        (
            &[
                ("TERM_PROGRAM", "WarpTerminal"),
                ("TMUX", "x"),
                ("TERM", "tmux-256color"),
            ],
            true,
        ),
        (&[("TERM_PROGRAM", "iterm.app")], false),
        (&[("TERM_PROGRAM", "vscode")], false),
        (&[("WT_SESSION", "session"), ("TERM", "xterm-256color")], false),
        (
            &[("TERMINAL_EMULATOR", "JetBrains-JediTerm"), ("TERM", "xterm-256color")],
            false,
        ),
        (
            &[("WT_SESSION", "session"), ("TMUX", "x"), ("TERM", "tmux-256color")],
            false,
        ),
        (
            &[("COLORTERM", "truecolor"), ("TMUX", "x"), ("TERM", "tmux-256color")],
            false,
        ),
        (&[("COLORTERM", "24bit")], false),
        (&[("TERM_PROGRAM", "alacritty")], false),
        (&[("GHOSTTY_RESOURCES_DIR", "x")], false),
        (&[("ITERM_SESSION_ID", "x")], false),
    ];
    let detect_request: Vec<Value> = detect_cases
        .iter()
        .map(|(values, tmux)| {
            let env: serde_json::Map<String, Value> = values
                .iter()
                .map(|(key, value)| ((*key).to_string(), json!(value)))
                .collect();
            json!({ "env": env, "tmux": tmux })
        })
        .collect();
    let response = oracle.request(&json!({
        "id": "image-detect",
        "operation": "terminal_image_detect",
        "cases": detect_request,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let results = response["output"]["results"].as_array().unwrap();
    for ((values, tmux), expected) in detect_cases.iter().zip(results) {
        let actual = caps_json(detect_capabilities_in(&environment(values), || *tmux));
        assert_eq!(actual, *expected, "detect mismatch for {values:?}");
    }

    let response = oracle.request(&json!({
        "id": "image-placement",
        "operation": "terminal_image_placement",
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected = &response["output"]["results"];
    register_kitty_image_metadata(pi_tui::terminal_image::KittyImageMetadata {
        image_id: 42,
        columns: 3,
        rows: 3,
        width_px: 100,
        height_px: 100,
    });
    let transmission = encode_kitty(
        &"A".repeat(8192),
        &KittyEncodeOptions {
            columns: Some(3),
            rows: Some(3),
            image_id: Some(42),
            move_cursor: Some(false),
        },
    );
    let cropped = crop_kitty_image_line(&transmission, 2, 1);
    assert_eq!(cropped, expected["cropped"].as_str().unwrap());
    assert_eq!(
        get_kitty_image_metadata(&transmission).map(|metadata| json!({
            "imageId": metadata.image_id,
            "columns": metadata.columns,
            "rows": metadata.rows,
            "widthPx": metadata.width_px,
            "heightPx": metadata.height_px,
        })),
        Some(expected["metadata"].clone())
    );
    let line = format!("left {cropped} right");
    let placement = get_kitty_image_placement(&line).expect("placement");
    assert_eq!(
        placement.image_id,
        expected["placement"]["imageId"].as_u64().unwrap() as u32
    );
    assert_eq!(
        placement.transmission_bytes,
        expected["placement"]["transmissionBytes"].as_u64().unwrap() as usize
    );
    assert_eq!(
        placement.estimated_decoded_bytes,
        expected["placement"]["estimatedDecodedBytes"].as_u64().unwrap()
    );
    assert_eq!(placement.sequence, expected["placement"]["sequence"].as_str().unwrap());
    assert_eq!(
        placement.replacement_line,
        expected["placement"]["replacementLine"].as_str().unwrap()
    );

    let render_cases = [
        (
            caps(Some(ImageProtocol::Kitty), true, true),
            CellDimensions {
                width_px: 10,
                height_px: 10,
            },
            ImageDimensions {
                width_px: 20,
                height_px: 20,
            },
            ImageRenderOptions {
                max_width_cells: Some(2.0),
                ..Default::default()
            },
        ),
        (
            caps(Some(ImageProtocol::Kitty), true, true),
            CellDimensions {
                width_px: 10,
                height_px: 10,
            },
            ImageDimensions {
                width_px: 20,
                height_px: 20,
            },
            ImageRenderOptions {
                max_width_cells: Some(2.0),
                move_cursor: Some(false),
                ..Default::default()
            },
        ),
        (
            caps(Some(ImageProtocol::Kitty), true, true),
            CellDimensions {
                width_px: 10,
                height_px: 10,
            },
            ImageDimensions {
                width_px: 10,
                height_px: 100,
            },
            ImageRenderOptions {
                max_width_cells: Some(10.0),
                max_height_cells: Some(5.0),
                ..Default::default()
            },
        ),
        (
            caps(Some(ImageProtocol::Iterm2), true, true),
            CellDimensions {
                width_px: 9,
                height_px: 18,
            },
            ImageDimensions {
                width_px: 100,
                height_px: 50,
            },
            ImageRenderOptions {
                max_width_cells: Some(20.0),
                preserve_aspect_ratio: Some(false),
                ..Default::default()
            },
        ),
        (
            caps(None, false, false),
            CellDimensions {
                width_px: 9,
                height_px: 18,
            },
            ImageDimensions {
                width_px: 100,
                height_px: 50,
            },
            ImageRenderOptions::default(),
        ),
    ];
    let render_request: Vec<Value> = render_cases
        .iter()
        .map(|(caps, cell, image, options)| {
            let mut options_json = serde_json::Map::new();
            if let Some(value) = options.max_width_cells {
                options_json.insert("maxWidthCells".to_string(), json!(value));
            }
            if let Some(value) = options.max_height_cells {
                options_json.insert("maxHeightCells".to_string(), json!(value));
            }
            if let Some(value) = options.preserve_aspect_ratio {
                options_json.insert("preserveAspectRatio".to_string(), json!(value));
            }
            if let Some(value) = options.image_id {
                options_json.insert("imageId".to_string(), json!(value));
            }
            if let Some(value) = options.move_cursor {
                options_json.insert("moveCursor".to_string(), json!(value));
            }
            json!({
                "caps": caps_json(*caps),
                "cell": { "widthPx": cell.width_px, "heightPx": cell.height_px },
                "data": "AAAA",
                "image": { "widthPx": image.width_px, "heightPx": image.height_px },
                "options": options_json,
            })
        })
        .collect();
    let response = oracle.request(&json!({
        "id": "image-render",
        "operation": "terminal_image_render",
        "cases": render_request,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let results = response["output"]["results"].as_array().unwrap();
    for ((caps, cell, image, options), expected) in render_cases.iter().zip(results) {
        set_capabilities(*caps);
        set_cell_dimensions(*cell);
        let actual = render_image("AAAA", *image, options);
        reset_capabilities_cache();
        set_cell_dimensions(CellDimensions {
            width_px: 9,
            height_px: 18,
        });
        match (actual, expected) {
            (None, Value::Null) => {}
            (Some(actual), expected) => {
                assert_eq!(actual.sequence, expected["sequence"].as_str().unwrap());
                assert_eq!(actual.columns, expected["columns"].as_u64().unwrap() as u32);
                assert_eq!(actual.rows, expected["rows"].as_u64().unwrap() as u32);
                assert_eq!(
                    actual.image_id.map(Value::from).unwrap_or(Value::Null),
                    expected["imageId"]
                );
            }
            (None, _) => panic!("expected a rendered image"),
        }
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let home_path = format!("{home}/.pi/agent/shot.png");
    let fallback_cases = [
        (
            caps(None, false, false),
            "image/png",
            Some(ImageDimensions {
                width_px: 1280,
                height_px: 720,
            }),
            Some(home_path.clone()),
        ),
        (
            caps(None, false, true),
            "image/png",
            Some(ImageDimensions {
                width_px: 10,
                height_px: 10,
            }),
            Some(home_path.clone()),
        ),
        (
            caps(None, false, true),
            "image/png",
            Some(ImageDimensions {
                width_px: 1,
                height_px: 1,
            }),
            Some("clankolas.png".to_string()),
        ),
        (
            caps(None, false, false),
            "image/png",
            Some(ImageDimensions {
                width_px: 8,
                height_px: 6,
            }),
            None,
        ),
    ];
    let fallback_request: Vec<Value> = fallback_cases
        .iter()
        .map(|(caps, mime, dimensions, filename)| {
            json!({
                "caps": caps_json(*caps),
                "mimeType": mime,
                "dimensions": dimensions.map(|d| json!({ "widthPx": d.width_px, "heightPx": d.height_px })),
                "filename": filename,
            })
        })
        .collect();
    let response = oracle.request(&json!({
        "id": "image-fallback",
        "operation": "terminal_image_fallback",
        "cases": fallback_request,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let results = response["output"]["results"].as_array().unwrap();
    for ((caps, mime, dimensions, filename), expected) in fallback_cases.iter().zip(results) {
        set_capabilities(*caps);
        let actual = image_fallback(mime, *dimensions, filename.as_deref());
        reset_capabilities_cache();
        assert_eq!(actual, expected.as_str().unwrap());
    }

    let hyperlink_cases = [
        ("click me", "https://example.com"),
        ("", "https://example.com"),
        ("\x1b[4m\x1b[34mclick me\x1b[0m", "https://example.com"),
        ("README.md", "file:///home/user/README.md"),
    ];
    let response = oracle.request(&json!({
        "id": "image-hyperlink",
        "operation": "terminal_image_hyperlink",
        "cases": hyperlink_cases.iter().map(|(text, url)| json!({ "text": text, "url": url })).collect::<Vec<_>>(),
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let results = response["output"]["results"].as_array().unwrap();
    for ((text, url), expected) in hyperlink_cases.iter().zip(results) {
        assert_eq!(hyperlink(text, url), expected.as_str().unwrap());
    }

    let response = oracle.request(&json!({
        "id": "image-delete",
        "operation": "terminal_image_delete",
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected = &response["output"]["results"];
    assert_eq!(delete_kitty_image(42), expected["one"].as_str().unwrap());
    assert_eq!(delete_all_kitty_images(), expected["all"].as_str().unwrap());
    assert_eq!(delete_all_kitty_placements(), expected["placements"].as_str().unwrap());
}
