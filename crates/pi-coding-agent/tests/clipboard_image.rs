#[path = "support/clipboard_harness.rs"]
mod harness;

use pi_coding_agent::utils::clipboard_image::{
    ClipboardImage, extension_for_image_mime_type, is_wayland_session, read_clipboard_image,
};
use pi_coding_agent::utils::clipboard_native::{ClipboardError, ClipboardModule};

use harness::{Scenario, assert_child_passed, child_environment, octal_printf, run_scenario};

struct StaticClipboard {
    text: Result<String, ClipboardError>,
    image: Result<Vec<u8>, ClipboardError>,
}

impl ClipboardModule for StaticClipboard {
    fn get_text(&self) -> Result<String, ClipboardError> {
        self.text.clone()
    }

    fn set_text(&self, _text: &str) -> Result<(), ClipboardError> {
        Ok(())
    }

    fn has_image(&self) -> Result<bool, ClipboardError> {
        Ok(self.image.is_ok())
    }

    fn get_image_binary(&self) -> Result<Vec<u8>, ClipboardError> {
        self.image.clone()
    }
}

fn native_image(bytes: Vec<u8>) -> StaticClipboard {
    StaticClipboard {
        text: Ok(String::new()),
        image: Ok(bytes),
    }
}

fn native_without_image() -> StaticClipboard {
    StaticClipboard {
        text: Ok(String::new()),
        image: Err(ClipboardError::new("no image")),
    }
}

#[test]
fn maps_image_mime_types_to_extensions() {
    assert_eq!(extension_for_image_mime_type("image/png"), Some("png"));
    assert_eq!(extension_for_image_mime_type("image/jpeg"), Some("jpg"));
    assert_eq!(extension_for_image_mime_type("IMAGE/PNG; charset=binary"), Some("png"));
    assert_eq!(extension_for_image_mime_type("image/webp"), Some("webp"));
    assert_eq!(extension_for_image_mime_type("image/gif"), Some("gif"));
    assert_eq!(extension_for_image_mime_type("image/bmp"), None);
    assert_eq!(extension_for_image_mime_type(""), None);
}

#[test]
fn detects_wayland_sessions_from_display_or_session_type() {
    let wayland_display = harness::environment(&[("WAYLAND_DISPLAY", "wayland-0")]);
    assert!(is_wayland_session(&wayland_display));

    let session_type = harness::environment(&[("XDG_SESSION_TYPE", "wayland")]);
    assert!(is_wayland_session(&session_type));

    let empty_display = harness::environment(&[("WAYLAND_DISPLAY", "")]);
    assert!(!is_wayland_session(&empty_display));

    assert!(!is_wayland_session(&harness::environment(&[])));
}

#[tokio::test]
async fn wayland_reads_wl_paste_and_never_touches_the_native_clipboard() {
    let scenario_name = "wayland_reads_wl_paste_and_never_touches_the_native_clipboard";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [(
            "wl-paste",
            "printf '%s\\n' \"$*\" >> \"$PI_CLIPBOARD_FAKE_STATE/wl-paste.log\"\ncase \"$*\" in\n  \"--list-types\") printf 'text/plain\\nimage/png\\n' ;;\n  \"--type image/png --no-newline\") printf '\\001\\002\\003' ;;\n  *) exit 1 ;;\nesac\n",
        )];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(scenario_name, &scenario, "linux", &[("WAYLAND_DISPLAY", "1")]);
        assert_child_passed(&output);
        let log = std::fs::read_to_string(scenario.state.join("wl-paste.log")).unwrap();
        assert_eq!(log, "--list-types\n--type image/png --no-newline\n");
        return;
    };

    // has_image would panic inside this fake, mirroring the TypeScript guard.
    struct PanickingClipboard;
    impl ClipboardModule for PanickingClipboard {
        fn get_text(&self) -> Result<String, ClipboardError> {
            panic!("native clipboard must not be used on Wayland");
        }
        fn set_text(&self, _text: &str) -> Result<(), ClipboardError> {
            panic!("native clipboard must not be used on Wayland");
        }
        fn has_image(&self) -> Result<bool, ClipboardError> {
            panic!("native clipboard must not be used on Wayland");
        }
        fn get_image_binary(&self) -> Result<Vec<u8>, ClipboardError> {
            panic!("native clipboard must not be used on Wayland");
        }
    }

    let image = read_clipboard_image(&child.environment, Some(&PanickingClipboard))
        .await
        .expect("clipboard read")
        .expect("clipboard image");
    assert_eq!(image.mime_type, "image/png");
    assert_eq!(image.bytes, vec![1, 2, 3]);
}

#[tokio::test]
async fn wayland_falls_back_to_xclip_when_wl_paste_is_missing() {
    let scenario_name = "wayland_falls_back_to_xclip_when_wl_paste_is_missing";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [(
            "xclip",
            "printf '%s\\n' \"$*\" >> \"$PI_CLIPBOARD_FAKE_STATE/xclip.log\"\ncase \"$*\" in\n  \"-selection clipboard -t TARGETS -o\") printf 'image/png\\n' ;;\n  \"-selection clipboard -t image/png -o\") printf '\\011\\010' ;;\n  *) exit 1 ;;\nesac\n",
        )];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(scenario_name, &scenario, "linux", &[("XDG_SESSION_TYPE", "wayland")]);
        assert_child_passed(&output);
        return;
    };

    let image = read_clipboard_image(&child.environment, None)
        .await
        .expect("clipboard read")
        .expect("clipboard image");
    assert_eq!(image.mime_type, "image/png");
    assert_eq!(image.bytes, vec![9, 8]);
}

#[tokio::test]
async fn wsl_uses_powershell_with_the_quoted_windows_path() {
    let scenario_name = "wsl_uses_powershell_with_the_quoted_windows_path";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [
            (
                "wslpath",
                "printf '%s\\n' \"$*\" >> \"$PI_CLIPBOARD_FAKE_STATE/wslpath.log\"\nprintf '%s' \"$2\" > \"$PI_CLIPBOARD_FAKE_STATE/tmp_path\"\nprintf \"C:\\\\Users\\\\O'Hare\\\\clip.png\\n\"\n",
            ),
            (
                "powershell.exe",
                "printf '%s\\n' \"$*\" >> \"$PI_CLIPBOARD_FAKE_STATE/powershell.log\"\nprintf 'env:%s\\n' \"${PI_WSL_CLIPBOARD_IMAGE_PATH-}\" >> \"$PI_CLIPBOARD_FAKE_STATE/powershell.log\"\nread -r path < \"$PI_CLIPBOARD_FAKE_STATE/tmp_path\"\nprintf '\\004\\005\\006' > \"$path\"\nprintf 'ok\\n'\n",
            ),
        ];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(scenario_name, &scenario, "linux", &[("WSL_DISTRO_NAME", "Ubuntu")]);
        assert_child_passed(&output);

        let powershell = std::fs::read_to_string(scenario.state.join("powershell.log")).unwrap();
        assert!(
            powershell.contains("$path = 'C:\\Users\\O''Hare\\clip.png'"),
            "{powershell}"
        );
        assert!(powershell.contains("env:\n"), "{powershell}");
        return;
    };

    let image = read_clipboard_image(&child.environment, None)
        .await
        .expect("clipboard read")
        .expect("clipboard image");
    assert_eq!(image.mime_type, "image/png");
    assert_eq!(image.bytes, vec![4, 5, 6]);
}

#[tokio::test]
async fn non_wayland_reads_the_native_clipboard_without_spawning_tools() {
    let scenario_name = "non_wayland_reads_the_native_clipboard_without_spawning_tools";
    let Some(child) = child_environment(scenario_name) else {
        let scenario = Scenario::new(&[]);
        let output = run_scenario(scenario_name, &scenario, "linux", &[("DISPLAY", ":fixture")]);
        assert_child_passed(&output);
        return;
    };

    let image = read_clipboard_image(&child.environment, Some(&native_image(vec![7])))
        .await
        .expect("clipboard read")
        .expect("clipboard image");
    assert_eq!(image.mime_type, "image/png");
    assert_eq!(image.bytes, vec![7]);
}

#[tokio::test]
async fn non_wayland_does_not_fall_back_to_xclip_when_the_native_clipboard_has_no_image() {
    let scenario_name = "non_wayland_does_not_fall_back_to_xclip_when_the_native_clipboard_has_no_image";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [(
            "xclip",
            "case \"$*\" in\n  \"-selection clipboard -t TARGETS -o\") printf 'image/png\\n' ;;\n  \"-selection clipboard -t image/png -o\") printf '\\010\\011' ;;\n  *) exit 1 ;;\nesac\n",
        )];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(scenario_name, &scenario, "linux", &[("DISPLAY", ":fixture")]);
        assert_child_passed(&output);
        return;
    };

    assert!(
        read_clipboard_image(&child.environment, Some(&native_without_image()))
            .await
            .expect("clipboard read")
            .is_none()
    );
    assert!(child.invocations("xclip").is_empty());
}

#[tokio::test]
async fn xclip_prefers_the_listed_image_type_then_falls_back_to_supported_types() {
    let scenario_name = "xclip_prefers_the_listed_image_type_then_falls_back_to_supported_types";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [(
            "xclip",
            "printf '%s\\n' \"$*\" >> \"$PI_CLIPBOARD_FAKE_STATE/xclip.log\"\ncase \"$*\" in\n  \"-selection clipboard -t TARGETS -o\") printf 'image/webp\\n' ;;\n  \"-selection clipboard -t image/webp -o\") exit 1 ;;\n  \"-selection clipboard -t image/png -o\") printf '\\001\\001' ;;\n  *) exit 1 ;;\nesac\n",
        )];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(scenario_name, &scenario, "linux", &[("WAYLAND_DISPLAY", "fixture")]);
        assert_child_passed(&output);
        let log = std::fs::read_to_string(scenario.state.join("xclip.log")).unwrap();
        assert!(log.contains("-selection clipboard -t image/webp -o"), "{log}");
        assert!(log.contains("-selection clipboard -t image/png -o"), "{log}");
        return;
    };

    let image = read_clipboard_image(&child.environment, Some(&native_without_image()))
        .await
        .expect("clipboard read")
        .expect("clipboard image");
    assert_eq!(image.mime_type, "image/png");
}

#[tokio::test]
async fn converts_unsupported_clipboard_formats_to_png() {
    let scenario_name = "converts_unsupported_clipboard_formats_to_png";
    let bmp = harness::bmp_1x1_red_24bpp();
    let Some(child) = child_environment(scenario_name) else {
        let payload = octal_printf(&bmp);
        let script = format!(
            "case \"$*\" in\n  \"--list-types\") printf 'image/bmp\\n' ;;\n  \"--type image/bmp --no-newline\") printf '{payload}' ;;\n  *) exit 1 ;;\nesac\n"
        );
        let scenario = Scenario::new(&[("wl-paste", script.as_str())]);
        let output = run_scenario(scenario_name, &scenario, "linux", &[("WAYLAND_DISPLAY", "wayland-0")]);
        assert_child_passed(&output);
        return;
    };

    let image = read_clipboard_image(&child.environment, None)
        .await
        .expect("clipboard read")
        .expect("clipboard image");
    assert_eq!(image.mime_type, "image/png");
    assert_eq!(&image.bytes[..4], &[0x89, 0x50, 0x4e, 0x47]);
}

#[tokio::test]
async fn returns_none_when_no_clipboard_source_has_an_image() {
    let scenario_name = "returns_none_when_no_clipboard_source_has_an_image";
    let Some(child) = child_environment(scenario_name) else {
        let scenario = Scenario::new(&[]);
        let output = run_scenario(scenario_name, &scenario, "linux", &[("WAYLAND_DISPLAY", "wayland-0")]);
        assert_child_passed(&output);
        return;
    };

    assert!(
        read_clipboard_image(&child.environment, None)
            .await
            .expect("clipboard read")
            .is_none()
    );
}

#[tokio::test]
async fn termux_never_reads_the_clipboard() {
    let scenario_name = "termux_never_reads_the_clipboard";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [(
            "wl-paste",
            "printf 'called\\n' >> \"$PI_CLIPBOARD_FAKE_STATE/wl-paste.log\"\nprintf 'image/png\\n'\n",
        )];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(
            scenario_name,
            &scenario,
            "linux",
            &[("TERMUX_VERSION", "0.118"), ("WAYLAND_DISPLAY", "wayland-0")],
        );
        assert_child_passed(&output);
        assert!(!scenario.state.join("wl-paste.log").exists());
        return;
    };

    assert!(
        read_clipboard_image(&child.environment, Some(&native_image(vec![1])))
            .await
            .expect("clipboard read")
            .is_none()
    );
}

#[test]
fn clipboard_image_is_a_plain_value() {
    let image = ClipboardImage {
        bytes: vec![1, 2],
        mime_type: "image/png".into(),
    };
    assert_eq!(image.bytes, vec![1, 2]);
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
