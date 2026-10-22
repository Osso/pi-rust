use pi_js_compat::js_json::JsString;
use std::sync::Mutex;

use pi_tui::terminal_image::{
    CellDimensions, ImageCellSize, ImageDimensions, ImageProtocol, ImageRenderOptions, Iterm2Size,
    TerminalCapabilities, TerminalEnvironment, calculate_image_cell_size, calculate_image_rows,
    delete_all_kitty_images, delete_kitty_image, detect_capabilities_in, encode_iterm2, encode_kitty,
    get_cell_dimensions, get_gif_dimensions, get_image_dimensions, get_jpeg_dimensions, get_png_dimensions,
    get_webp_dimensions, hyperlink, image_fallback, is_image_line, render_image, reset_capabilities_cache,
    set_capabilities, set_cell_dimensions,
};

static CAPS_LOCK: Mutex<()> = Mutex::new(());
fn with_caps(caps: TerminalCapabilities, test: impl FnOnce()) {
    let _guard = CAPS_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    set_capabilities(caps);
    test();
    reset_capabilities_cache();
    set_cell_dimensions(CellDimensions {
        width_px: 9,
        height_px: 18,
    });
}

fn kitty_caps() -> TerminalCapabilities {
    TerminalCapabilities {
        images: Some(ImageProtocol::Kitty),
        true_color: true,
        hyperlinks: true,
    }
}

fn env() -> TerminalEnvironment {
    TerminalEnvironment::default()
}

#[test]
fn detects_iterm2_image_lines() {
    assert!(is_image_line(
        "\x1b]1337;File=size=100,100;inline=1:base64encodeddata==\x07"
    ));
    assert!(is_image_line(
        "Some text \x1b]1337;File=size=100,100;inline=1:base64data==\x07 more text"
    ));
    assert!(is_image_line(
        "Regular text ending with \x1b]1337;File=inline=1:base64data==\x07"
    ));
    assert!(is_image_line("\x1b]1337;File=:\x07"));
}

#[test]
fn detects_kitty_image_lines() {
    assert!(is_image_line(
        "\x1b_Ga=T,f=100,t=f,d=base64data...\x1b\\\x1b_Gm=i=1;\x1b\\"
    ));
    assert!(is_image_line("Output: \x1b_Ga=T,f=100;data...\x1b\\\x1b_Gm=i=1;\x1b\\"));
    assert!(is_image_line("  \x1b_Ga=T,f=100...\x1b\\\x1b_Gm=i=1;\x1b\\  "));
}

#[test]
fn rejects_non_image_lines() {
    assert!(!is_image_line(
        "This is just a regular text line without any escape sequences"
    ));
    assert!(!is_image_line("\x1b[31mRed text\x1b[0m and \x1b[32mgreen text\x1b[0m"));
    assert!(!is_image_line("\x1b[1A\x1b[2KLine cleared and moved up"));
    assert!(!is_image_line("Some text with ]1337;File but missing ESC at start"));
    assert!(!is_image_line("Some text with _G but missing ESC at start"));
    assert!(!is_image_line(""));
    assert!(!is_image_line("\n\n"));
    assert!(!is_image_line("/path/to/File_1337_backup/image.jpg"));
}

#[test]
fn detects_very_long_image_lines() {
    let long_line = format!(
        "Text prefix \x1b]1337;File=size=800,600;inline=1:{}{}",
        "A".repeat(300_000),
        " suffix"
    );
    assert!(is_image_line(&long_line));
}

#[test]
fn detects_unknown_terminal_as_conservative() {
    let caps = detect_capabilities_in(&env(), || false);
    assert_eq!(caps.images, None);
    assert!(!caps.true_color);
    assert!(!caps.hyperlinks);
}

#[test]
fn detects_tmux_hyperlink_forwarding() {
    let mut environment = env();
    environment.tmux = true;
    environment.term_program = Some("ghostty".to_string());
    let caps = detect_capabilities_in(&environment, || true);
    assert_eq!(caps.images, None);
    assert!(caps.hyperlinks);

    let caps = detect_capabilities_in(&environment, || false);
    assert_eq!(caps.images, None);
    assert!(!caps.hyperlinks);
}

#[test]
fn detects_tmux_from_term_name() {
    let mut environment = env();
    environment.term = Some("tmux-256color".to_string());
    environment.term_program = Some("iterm.app".to_string());
    assert!(detect_capabilities_in(&environment, || true).hyperlinks);
    assert!(!detect_capabilities_in(&environment, || false).hyperlinks);
}

#[test]
fn detects_screen_without_hyperlinks() {
    let mut environment = env();
    environment.term = Some("screen-256color".to_string());
    let caps = detect_capabilities_in(&environment, || true);
    assert_eq!(caps.images, None);
    assert!(!caps.hyperlinks);
}

#[test]
fn detects_known_terminals() {
    let cases = [
        ("TERM_PROGRAM", "ghostty", Some(ImageProtocol::Kitty), true, true),
        ("TERM_PROGRAM", "kitty", Some(ImageProtocol::Kitty), true, true),
        ("TERM_PROGRAM", "wezterm", Some(ImageProtocol::Kitty), true, true),
        ("TERM_PROGRAM", "warpterminal", Some(ImageProtocol::Kitty), true, true),
        ("TERM_PROGRAM", "iterm.app", Some(ImageProtocol::Iterm2), true, true),
        ("TERM_PROGRAM", "vscode", None, true, true),
        ("TERM_PROGRAM", "alacritty", None, true, true),
    ];
    for (field, value, images, true_color, hyperlinks) in cases {
        let mut environment = env();
        if field == "TERM_PROGRAM" {
            environment.term_program = Some(value.to_string());
        }
        let caps = detect_capabilities_in(&environment, || false);
        assert_eq!(caps.images, images, "{value}");
        assert_eq!(caps.true_color, true_color, "{value}");
        assert_eq!(caps.hyperlinks, hyperlinks, "{value}");
    }
}

#[test]
fn detects_terminal_environment_flags() {
    let mut environment = env();
    environment.kitty_window_id = true;
    assert_eq!(
        detect_capabilities_in(&environment, || false).images,
        Some(ImageProtocol::Kitty)
    );

    let mut environment = env();
    environment.ghostty_resources_dir = true;
    assert_eq!(
        detect_capabilities_in(&environment, || false).images,
        Some(ImageProtocol::Kitty)
    );

    let mut environment = env();
    environment.wezterm_pane = true;
    assert_eq!(
        detect_capabilities_in(&environment, || false).images,
        Some(ImageProtocol::Kitty)
    );

    let mut environment = env();
    environment.warp_session_id = true;
    assert_eq!(
        detect_capabilities_in(&environment, || false).images,
        Some(ImageProtocol::Kitty)
    );

    let mut environment = env();
    environment.iterm_session_id = true;
    assert_eq!(
        detect_capabilities_in(&environment, || false).images,
        Some(ImageProtocol::Iterm2)
    );

    let mut environment = env();
    environment.wt_session = true;
    environment.term = Some("xterm-256color".to_string());
    let caps = detect_capabilities_in(&environment, || false);
    assert_eq!(caps.images, None);
    assert!(caps.true_color);
    assert!(caps.hyperlinks);

    let mut environment = env();
    environment.terminal_emulator = Some("jetbrains-jediterm".to_string());
    environment.term = Some("xterm-256color".to_string());
    let caps = detect_capabilities_in(&environment, || false);
    assert_eq!(caps.images, None);
    assert!(caps.true_color);
    assert!(!caps.hyperlinks);
}

#[test]
fn detects_truecolor_hints() {
    let mut environment = env();
    environment.color_term = Some("truecolor".to_string());
    assert!(detect_capabilities_in(&environment, || false).true_color);

    let mut environment = env();
    environment.color_term = Some("24bit".to_string());
    assert!(detect_capabilities_in(&environment, || false).true_color);
}

#[test]
fn encodes_iterm2_with_frozen_options_without_size_metadata() {
    assert_eq!(
        encode_iterm2(
            "AAAA",
            &pi_tui::terminal_image::Iterm2Options {
                width: Some(Iterm2Size::Number(2.0)),
                height: Some(Iterm2Size::Text("auto".into())),
                ..Default::default()
            }
        ),
        "\x1b]1337;File=inline=1;width=2;height=auto:AAAA\x07"
    );
}

#[test]
fn encodes_kitty_with_cursor_movement_controls() {
    let sequence = encode_kitty(
        "AAAA",
        &pi_tui::terminal_image::KittyEncodeOptions {
            columns: Some(2),
            rows: Some(2),
            move_cursor: Some(false),
            ..Default::default()
        },
    );
    assert!(sequence.starts_with("\x1b_Ga=T,f=100,q=2,C=1,c=2,r=2;"));
}

#[test]
fn emits_kitty_delete_commands() {
    assert_eq!(delete_kitty_image(42), "\x1b_Ga=d,d=I,i=42,q=2\x1b\\");
    assert_eq!(delete_all_kitty_images(), "\x1b_Ga=d,d=A,q=2\x1b\\");
}

#[test]
fn renders_kitty_images_with_default_cursor_movement() {
    with_caps(kitty_caps(), || {
        set_cell_dimensions(CellDimensions {
            width_px: 10,
            height_px: 10,
        });
        let result = render_image(
            "AAAA",
            ImageDimensions {
                width_px: 20,
                height_px: 20,
            },
            &ImageRenderOptions {
                max_width_cells: Some(2.0),
                ..Default::default()
            },
        )
        .expect("image");
        assert!(!result.sequence.contains(",C=1,"));
        assert_eq!(result.rows, 2);
    });
}

#[test]
fn renders_kitty_images_without_cursor_movement_when_requested() {
    with_caps(kitty_caps(), || {
        set_cell_dimensions(CellDimensions {
            width_px: 10,
            height_px: 10,
        });
        let result = render_image(
            "AAAA",
            ImageDimensions {
                width_px: 20,
                height_px: 20,
            },
            &ImageRenderOptions {
                max_width_cells: Some(2.0),
                move_cursor: Some(false),
                ..Default::default()
            },
        )
        .expect("image");
        assert!(result.sequence.contains(",C=1,"));
        assert_eq!(result.rows, 2);
    });
}

#[test]
fn honors_max_height_by_reducing_rendered_width() {
    with_caps(kitty_caps(), || {
        set_cell_dimensions(CellDimensions {
            width_px: 10,
            height_px: 10,
        });
        let result = render_image(
            "AAAA",
            ImageDimensions {
                width_px: 10,
                height_px: 100,
            },
            &ImageRenderOptions {
                max_width_cells: Some(10.0),
                max_height_cells: Some(5.0),
                ..Default::default()
            },
        )
        .expect("image");
        assert_eq!(result.rows, 5);
        assert!(result.sequence.contains(",c=1,r=5"));
    });
}

#[test]
fn calculates_cell_sizes() {
    let cell = CellDimensions {
        width_px: 10,
        height_px: 10,
    };
    assert_eq!(
        calculate_image_cell_size(
            ImageDimensions {
                width_px: 100,
                height_px: 100
            },
            3.0,
            None,
            cell
        ),
        ImageCellSize { columns: 3, rows: 3 }
    );
    assert_eq!(
        calculate_image_cell_size(
            ImageDimensions {
                width_px: 10,
                height_px: 100
            },
            10.0,
            Some(5.0),
            cell
        ),
        ImageCellSize { columns: 1, rows: 5 }
    );
    assert_eq!(
        calculate_image_cell_size(
            ImageDimensions {
                width_px: 20,
                height_px: 20
            },
            2.0,
            None,
            cell
        ),
        ImageCellSize { columns: 2, rows: 2 }
    );
    assert_eq!(
        calculate_image_rows(
            ImageDimensions {
                width_px: 100,
                height_px: 100
            },
            3.0,
            cell
        ),
        3
    );
}

#[test]
fn reads_png_dimensions() {
    let mut bytes = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&640u32.to_be_bytes());
    bytes.extend_from_slice(&480u32.to_be_bytes());
    let encoded = pi_js_compat::base64::btoa(&bytes);
    assert_eq!(
        get_png_dimensions(&encoded),
        Some(ImageDimensions {
            width_px: 640,
            height_px: 480
        })
    );
    assert_eq!(get_png_dimensions("AAAA"), None);
}

#[test]
fn reads_jpeg_dimensions() {
    let bytes = [0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x64, 0x00, 0xc8, 0x00];
    let encoded = pi_js_compat::base64::btoa(&bytes);
    assert_eq!(
        get_jpeg_dimensions(&encoded),
        Some(ImageDimensions {
            width_px: 200,
            height_px: 100
        })
    );
    assert_eq!(get_jpeg_dimensions("AAAA"), None);
}

#[test]
fn reads_gif_dimensions() {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend_from_slice(&320u16.to_le_bytes());
    bytes.extend_from_slice(&240u16.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    let encoded = pi_js_compat::base64::btoa(&bytes);
    assert_eq!(
        get_gif_dimensions(&encoded),
        Some(ImageDimensions {
            width_px: 320,
            height_px: 240
        })
    );
    assert_eq!(get_gif_dimensions("AAAA"), None);
}

#[test]
fn reads_webp_dimensions() {
    let mut vp8x = b"RIFF".to_vec();
    vp8x.extend_from_slice(&[0, 0, 0, 0]);
    vp8x.extend_from_slice(b"WEBP");
    vp8x.extend_from_slice(b"VP8X");
    vp8x.extend_from_slice(&[0, 0, 0, 0]);
    vp8x.extend_from_slice(&[0, 0, 0, 0]);
    vp8x.extend_from_slice(&[0x3f, 0x02, 0x00]);
    vp8x.extend_from_slice(&[0x8f, 0x01, 0x00]);
    let encoded = pi_js_compat::base64::btoa(&vp8x);
    assert_eq!(
        get_webp_dimensions(&encoded),
        Some(ImageDimensions {
            width_px: 576,
            height_px: 400
        })
    );

    let mut vp8l = b"RIFF".to_vec();
    vp8l.extend_from_slice(&[0, 0, 0, 0]);
    vp8l.extend_from_slice(b"WEBP");
    vp8l.extend_from_slice(b"VP8L");
    vp8l.extend_from_slice(&[0, 0, 0, 0]);
    vp8l.push(0x2f);
    let bits: u32 = (639) | ((479) << 14);
    vp8l.extend_from_slice(&bits.to_le_bytes());
    vp8l.extend_from_slice(&[0; 5]);
    let encoded = pi_js_compat::base64::btoa(&vp8l);
    assert_eq!(
        get_webp_dimensions(&encoded),
        Some(ImageDimensions {
            width_px: 640,
            height_px: 480
        })
    );

    let mut vp8 = b"RIFF".to_vec();
    vp8.extend_from_slice(&[0, 0, 0, 0]);
    vp8.extend_from_slice(b"WEBP");
    vp8.extend_from_slice(b"VP8 ");
    vp8.extend_from_slice(&[0; 10]);
    vp8.extend_from_slice(&300u16.to_le_bytes());
    vp8.extend_from_slice(&200u16.to_le_bytes());
    let encoded = pi_js_compat::base64::btoa(&vp8);
    assert_eq!(
        get_webp_dimensions(&encoded),
        Some(ImageDimensions {
            width_px: 300,
            height_px: 200
        })
    );

    assert_eq!(get_webp_dimensions("AAAA"), None);
}

#[test]
fn dispatches_image_dimensions_by_mime_type() {
    let bytes = [0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x64, 0x00, 0xc8, 0x00];
    let encoded = pi_js_compat::base64::btoa(&bytes);
    assert_eq!(
        get_image_dimensions(&encoded, "image/jpeg"),
        Some(ImageDimensions {
            width_px: 200,
            height_px: 100
        })
    );
    assert_eq!(get_image_dimensions(&encoded, "image/png"), None);
    assert_eq!(get_image_dimensions(&encoded, "image/unknown"), None);
}

#[test]
fn wraps_text_in_osc8_hyperlinks() {
    assert_eq!(
        hyperlink("click me", "https://example.com"),
        "\x1b]8;;https://example.com\x1b\\click me\x1b]8;;\x1b\\"
    );
    assert_eq!(
        hyperlink("", "https://example.com"),
        "\x1b]8;;https://example.com\x1b\\\x1b]8;;\x1b\\"
    );
    let styled = "\x1b[4m\x1b[34mclick me\x1b[0m";
    let result = hyperlink(styled, "https://example.com");
    assert!(result.starts_with("\x1b]8;;https://example.com\x1b\\"));
    assert!(result.contains(styled));
    assert!(result.ends_with("\x1b]8;;\x1b\\"));
}

#[test]
fn builds_image_fallbacks() {
    with_caps(
        TerminalCapabilities {
            images: None,
            true_color: false,
            hyperlinks: false,
        },
        || {
            let home = std::env::var("HOME").unwrap_or_default();
            let path = format!("{home}/.pi/agent/shot.png");
            assert_eq!(
                image_fallback(
                    "image/png",
                    Some(ImageDimensions {
                        width_px: 1280,
                        height_px: 720
                    }),
                    Some(&JsString::from(&path))
                ),
                format!("[Image: {path} [image/png] 1280x720]")
            );
            assert_eq!(
                image_fallback(
                    "image/png",
                    Some(ImageDimensions {
                        width_px: 8,
                        height_px: 6
                    }),
                    None
                ),
                "[Image: [image/png] 8x6]"
            );
            assert_eq!(
                image_fallback(
                    "image/png",
                    Some(ImageDimensions {
                        width_px: 1,
                        height_px: 1
                    }),
                    Some(&JsString::from("clankolas.png"))
                ),
                "[Image: clankolas.png [image/png] 1x1]"
            );
        },
    );
}

#[test]
fn fallback_keeps_literal_absolute_filename_even_with_hyperlinks() {
    with_caps(
        TerminalCapabilities {
            images: None,
            true_color: false,
            hyperlinks: true,
        },
        || {
            let home = std::env::var("HOME").unwrap_or_default();
            let path = format!("{home}/.pi/agent/shot.png");
            let result = image_fallback(
                "image/png",
                Some(ImageDimensions {
                    width_px: 10,
                    height_px: 10,
                }),
                Some(&JsString::from(&path)),
            );
            assert_eq!(result, format!("[Image: {path} [image/png] 10x10]"));
        },
    );
}

#[test]
fn allocate_image_id_stays_in_range() {
    for _ in 0..64 {
        let id = pi_tui::terminal_image::allocate_image_id();
        assert!((1..=0xffff_fffe).contains(&id));
    }
}

#[test]
fn default_cell_dimensions_match_the_source() {
    let _guard = CAPS_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
    assert_eq!(
        get_cell_dimensions(),
        CellDimensions {
            width_px: 9,
            height_px: 18
        }
    );
}

#[test]
fn node_ascii_dimension_headers_mask_the_high_bit() {
    let mut gif = b"GIF89a\x0a\x00\x14\x00".to_vec();
    for byte in &mut gif[..6] {
        *byte |= 0x80;
    }
    assert_eq!(
        get_gif_dimensions(&pi_js_compat::base64::btoa(&gif)),
        Some(ImageDimensions {
            width_px: 10,
            height_px: 20
        })
    );
}

#[test]
fn node_ascii_webp_header_masks_the_high_bit() {
    let mut webp = vec![0; 30];
    webp[..4].copy_from_slice(b"RIFF");
    webp[8..12].copy_from_slice(b"WEBP");
    webp[12..16].copy_from_slice(b"VP8X");
    for index in (0..4).chain(8..16) {
        webp[index] |= 0x80;
    }
    assert_eq!(
        get_webp_dimensions(&pi_js_compat::base64::btoa(&webp)),
        Some(ImageDimensions {
            width_px: 1,
            height_px: 1
        })
    );
}

#[test]
fn iterm2_number_options_use_javascript_number_strings() {
    assert_eq!(
        encode_iterm2(
            "AAAA",
            &pi_tui::terminal_image::Iterm2Options {
                width: Some(Iterm2Size::Number(1e21)),
                height: Some(Iterm2Size::Number(-0.0)),
                ..Default::default()
            }
        ),
        "\x1b]1337;File=inline=1;width=1e+21;height=0:AAAA\x07"
    );
}

#[test]
fn tmux_probe_requires_a_successful_exit() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let tmux = directory.path().join("tmux");
    for (exit, expected) in [(1, "false"), (0, "true")] {
        std::fs::write(
            &tmux,
            format!("#!/usr/bin/python3\nprint('hyperlinks')\nraise SystemExit({exit})\n"),
        )
        .unwrap();
        std::fs::set_permissions(&tmux, std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tmux_probe_child", "--ignored"])
            .env("PATH", directory.path())
            .env("TMUX", "fixture")
            .env("PI_EXPECT_HYPERLINKS", expected)
            .output()
            .unwrap();
        assert!(output.status.success(), "tmux exit {exit}: {output:?}");
    }
}

#[test]
#[ignore = "child probe entry point"]
fn tmux_probe_child() {
    let expected = std::env::var("PI_EXPECT_HYPERLINKS").unwrap() == "true";
    assert_eq!(pi_tui::detect_capabilities().hyperlinks, expected);
}
