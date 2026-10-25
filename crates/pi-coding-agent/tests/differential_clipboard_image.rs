use crate::shared_oracle as oracle;

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use pi_coding_agent::utils::clipboard_image::{ClipboardEnvironment, read_clipboard_image};
use pi_coding_agent::utils::clipboard_native::{ClipboardError, ClipboardModule};
use serde_json::{Value, json};

struct MatrixClipboard<'a> {
    state: &'a Path,
    config: &'a Value,
}

impl MatrixClipboard<'_> {
    fn record(&self, program: &str) {
        use std::io::Write;
        let mut trace = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.state.join("trace.jsonl"))
            .unwrap();
        writeln!(trace, "{}", json!({"program":program,"args":[]})).unwrap();
    }
}

impl ClipboardModule for MatrixClipboard<'_> {
    fn get_text(&self) -> Result<String, ClipboardError> {
        unreachable!("image module does not read text")
    }
    fn set_text(&self, _: &str) -> Result<(), ClipboardError> {
        unreachable!("image module does not write text")
    }
    fn has_image(&self) -> Result<bool, ClipboardError> {
        self.record("native.hasImage");
        if let Some(error) = self.config["nativeHasError"].as_str() {
            return Err(ClipboardError::new(error));
        }
        Ok(self.config["nativeHas"].as_bool().unwrap_or(false))
    }
    fn get_image_binary(&self) -> Result<Vec<u8>, ClipboardError> {
        self.record("native.getImageBinary");
        if let Some(error) = self.config["nativeGetError"].as_str() {
            return Err(ClipboardError::new(error));
        }
        Ok(decode_hex(self.config["nativeHex"].as_str().unwrap_or("")))
    }
}

fn decode_hex(text: &str) -> Vec<u8> {
    let (pairs, remainder) = text.as_bytes().as_chunks::<2>();
    assert!(remainder.is_empty(), "hex fixtures contain complete bytes");
    pairs
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn cases() -> Vec<(&'static str, Value, Value)> {
    let mut cases = vec![
        (
            "headless-wl",
            json!({}),
            json!({"wlTypes":{"text":"image/png\n"},"wlData":{"hex":"010203"}}),
        ),
        (
            "x11-no-extra-fallback",
            json!({"DISPLAY":":fixture"}),
            json!({"xTypes":{"text":"image/png\n"},"xData:image/png":{"hex":"04"}}),
        ),
        (
            "wayland-preference",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"text":"image/webp\r\nIMAGE/PNG; charset=binary\n"},"wlData":{"hex":"ff00"}}),
        ),
        (
            "wayland-xclip",
            json!({"XDG_SESSION_TYPE":"wayland"}),
            json!({"xTypes":{"text":"image/webp\n"},"xData:image/png":{"hex":"0506"}}),
        ),
        (
            "wsl-filename",
            json!({"WSL_DISTRO_NAME":"fixture","WAYLAND_DISPLAY":"fixture"}),
            json!({"winPath":{"text":" C:\\Users\\O'Hare\\clip.png\r\n"},"powershell":{"text":"ok\n"},"fileHex":"0809"}),
        ),
        (
            "wsl-empty",
            json!({"WSLENV":"fixture","WAYLAND_DISPLAY":"fixture"}),
            json!({"winPath":{"text":"C:\\clip.png"},"powershell":{"text":"empty\n"},"fileHex":"0809"}),
        ),
        (
            "list-stderr-overflow",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"text":"image/png\n","stderrCount":50*1024*1024+65536},"wlData":{"hex":"0708"}}),
        ),
        (
            "list-timeout-signal",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"sleepMs":1500,"text":"image/png\n"},"wlData":{"hex":"0708"}}),
        ),
        (
            "native-has-error",
            json!({"DISPLAY":":fixture"}),
            json!({"nativeHasError":"native has image failed"}),
        ),
        (
            "native-get-error",
            json!({"DISPLAY":":fixture"}),
            json!({"nativeHas":true,"nativeGetError":"native get failed"}),
        ),
        (
            "native-empty",
            json!({"DISPLAY":":fixture"}),
            json!({"nativeHas":true,"nativeHex":""}),
        ),
        (
            "termux",
            json!({"TERMUX_VERSION":"fixture","WAYLAND_DISPLAY":"fixture"}),
            json!({"nativeHas":true,"nativeHex":"01","wlTypes":{"text":"image/png\n"}}),
        ),
    ];
    cases.extend(failure_cases());
    cases.extend(conversion_cases());
    cases
}

fn failure_cases() -> Vec<(&'static str, Value, Value)> {
    vec![
        (
            "wl-empty-types",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"text":" \r\ntext/plain\n"}}),
        ),
        (
            "wl-empty-data",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"text":"image/png\n"},"wlData":{"text":""}}),
        ),
        (
            "wl-feff-types",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"text":"\u{feff}IMAGE/PNG\n"},"wlData":{"hex":"07"}}),
        ),
        (
            "wl-nel-types",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"text":"\u{85}image/png\n"},"wlData":{"hex":"07"}}),
        ),
        (
            "wl-invalid-utf8-types",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"hex":"ff696d6167652f706e670a"},"wlData":{"hex":"07"}}),
        ),
        (
            "wl-nul-mime",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"text":"image/png\0\n"},"wlData":{"hex":"07"}}),
        ),
        (
            "wl-read-stdout-overflow",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"text":"image/png\n"},"wlData":{"stdoutCount":50*1024*1024+65536}}),
        ),
        (
            "xclip-duplicate-preferred",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"xTypes":{"text":"image/png\n"},"xData:image/png":{"text":""},"xData:image/jpeg":{"hex":"aabb"}}),
        ),
        ("native-absent", json!({}), json!({"nativeAvailable":false})),
        (
            "darwin-native",
            json!({}),
            json!({"platform":"darwin","nativeHas":true,"nativeHex":"0304"}),
        ),
        (
            "win32-native",
            json!({}),
            json!({"platform":"win32","nativeHas":true,"nativeHex":"0304"}),
        ),
        (
            "empty-termux",
            json!({"TERMUX_VERSION":"","DISPLAY":":fixture"}),
            json!({"nativeHas":true,"nativeHex":"0304"}),
        ),
        (
            "powershell-wslpath-failure",
            json!({"WSLENV":"fixture","WAYLAND_DISPLAY":"fixture"}),
            json!({"winPath":{"status":7,"text":"C:\\clip.png"}}),
        ),
        (
            "powershell-wslpath-empty",
            json!({"WSLENV":"fixture","WAYLAND_DISPLAY":"fixture"}),
            json!({"winPath":{"text":" \r\n"}}),
        ),
        (
            "powershell-failure",
            json!({"WSLENV":"fixture","WAYLAND_DISPLAY":"fixture"}),
            json!({"winPath":{"text":"C:\\clip.png"},"powershell":{"status":2,"text":"ok"}}),
        ),
        (
            "powershell-read-missing",
            json!({"WSLENV":"fixture","WAYLAND_DISPLAY":"fixture"}),
            json!({"winPath":{"text":"C:\\clip.png"},"powershell":{"text":"ok"}}),
        ),
        (
            "powershell-read-empty",
            json!({"WSLENV":"fixture","WAYLAND_DISPLAY":"fixture"}),
            json!({"winPath":{"text":"C:\\clip.png"},"powershell":{"text":"ok"},"fileHex":""}),
        ),
        (
            "powershell-js-trim",
            json!({"WSLENV":"fixture","WAYLAND_DISPLAY":"fixture"}),
            json!({"winPath":{"text":"\u{feff}C:\\clip.png"},"powershell":{"text":"\u{feff}ok\r\n"},"fileHex":"0102"}),
        ),
        (
            "powershell-nul-caught",
            json!({"WSLENV":"fixture","WAYLAND_DISPLAY":"fixture"}),
            json!({"winPath":{"text":"C:\\clip\0.png"},"powershell":{"text":"ok"},"fileHex":"0102"}),
        ),
    ]
}

fn conversion_cases() -> Vec<(&'static str, Value, Value)> {
    let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(3, 2, |x, y| {
        image::Rgb([x as u8 * 40, y as u8 * 80, 7])
    }));
    let mut bmp = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut bmp), image::ImageOutputFormat::Bmp)
        .unwrap();
    let mut jpeg = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut jpeg),
            image::ImageOutputFormat::Jpeg(100),
        )
        .unwrap();
    let exif = b"Exif\0\0MM\0*\0\0\0\x08\0\x01\x01\x12\0\x03\0\0\0\x01\0\x06\0\0\0\0\0\0";
    let mut segment = vec![0xff, 0xe1];
    segment.extend_from_slice(&(exif.len() as u16 + 2).to_be_bytes());
    segment.extend_from_slice(exif);
    jpeg.splice(2..2, segment);
    let hex = |bytes: &[u8]| bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    vec![
        (
            "convert-bmp",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"text":"image/bmp\n"},"wlData":{"hex":hex(&bmp)}}),
        ),
        (
            "convert-jpeg-exif-under-unsupported-mime",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"text":"image/bmp\n"},"wlData":{"hex":hex(&jpeg)}}),
        ),
        (
            "convert-invalid",
            json!({"WAYLAND_DISPLAY":"fixture"}),
            json!({"wlTypes":{"text":"image/bmp\n"},"wlData":{"text":"not an image"}}),
        ),
    ]
}

fn trace(state: &Path) -> (Vec<Value>, Vec<String>) {
    let mut errors = Vec::new();
    let text = std::fs::read_to_string(state.join("trace.jsonl")).unwrap_or_default();
    let pattern =
        regex::Regex::new(r"^pi-wsl-clip-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\.png$")
            .unwrap();
    let events = text
        .lines()
        .map(|line| {
            let mut event: Value = serde_json::from_str(line).unwrap();
            if event["program"] == "wslpath" {
                let path = Path::new(event["args"][1].as_str().unwrap());
                let filename = path.file_name().unwrap().to_str().unwrap();
                if !pattern.is_match(filename) {
                    errors.push(format!("UUIDv4 filename required: {filename}"));
                }
                event["args"][1] = json!(path.parent().unwrap().join("pi-wsl-clip-<uuid>.png").to_str().unwrap());
            }
            event
        })
        .collect();
    (events, errors)
}

async fn run_native_child(root: &Path) {
    let state = root.join("native");
    let config: Value = serde_json::from_slice(&std::fs::read(state.join("config.json")).unwrap()).unwrap();
    let request: Value = serde_json::from_slice(&std::fs::read(root.join("request.json")).unwrap()).unwrap();
    let variables: BTreeMap<String, pi_js_compat::js_json::JsString> =
        serde_json::from_value(request["env"].clone()).unwrap();
    let environment = ClipboardEnvironment::new(request["platform"].as_str().unwrap(), variables);
    let native = MatrixClipboard {
        state: &state,
        config: &config,
    };
    let native = if config["nativeAvailable"] == false {
        None
    } else {
        Some(&native as &dyn ClipboardModule)
    };
    let result = read_clipboard_image(&environment, native).await;
    let output = match result {
        Ok(image) => json!({"image":image.map(|image| json!({"bytes":image.bytes,"mimeType":image.mime_type}))}),
        Err(error) => json!({"error":error.thrown_value().value().to_serde().unwrap()}),
    };
    std::fs::write(root.join("native-result.json"), serde_json::to_vec(&output).unwrap()).unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn frozen_clipboard_image_module_matrix() {
    const CHILD: &str = "PI_CLIPBOARD_MATRIX_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        run_native_child(Path::new(&root)).await;
        return;
    }
    let mut failures = Vec::new();
    for (name, environment, config) in cases() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let bin = root.join("bin");
        let image_root = root.join("images");
        std::fs::create_dir(&bin).unwrap();
        std::fs::create_dir(&image_root).unwrap();
        for state in ["source", "native"] {
            std::fs::create_dir(root.join(state)).unwrap();
            std::fs::write(
                root.join(state).join("config.json"),
                serde_json::to_vec(&config).unwrap(),
            )
            .unwrap();
        }
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clipboard_image_tool.py");
        for tool in ["wl-paste", "xclip", "wslpath", "powershell.exe"] {
            let path = bin.join(tool);
            std::fs::copy(&fixture, &path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let request = json!({"bin":bin,"temporary":image_root,"env":environment,"platform":config["platform"].as_str().unwrap_or("linux")});
        std::fs::write(root.join("request.json"), serde_json::to_vec(&request).unwrap()).unwrap();
        let expected = {
            let mut source = oracle::TsOracle::spawn_with_environment(
                "clipboard-image-module-oracle.mts",
                "packages/coding-agent/src",
                &[("PI_CLIPBOARD_MATRIX_STATE", root.join("source").into_os_string())],
            );
            source.request(&request)
        };
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "frozen_clipboard_image_module_matrix", "--nocapture"])
            .env(CHILD, root)
            .env("PI_CLIPBOARD_MATRIX_STATE", root.join("native"))
            .env("PATH", &bin)
            .env("TMPDIR", &image_root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: Value = serde_json::from_slice(&std::fs::read(root.join("native-result.json")).unwrap()).unwrap();
        if actual != expected {
            failures.push(format!("{name}: output actual={actual}, expected={expected}"));
        }
        let (source_trace, source_errors) = trace(&root.join("source"));
        assert!(source_errors.is_empty(), "source: {source_errors:?}");
        // Validate the random filename before its sole nonce normalization.
        let (native_trace, native_errors) = trace(&root.join("native"));
        failures.extend(native_errors.into_iter().map(|error| format!("{name}: {error}")));
        if native_trace != source_trace {
            failures.push(format!(
                "{name}: trace actual={native_trace:?}, expected={source_trace:?}"
            ));
        }
        let signals = |state: &str| std::fs::read_to_string(root.join(state).join("signals")).unwrap_or_default();
        if signals("native") != signals("source") {
            failures.push(format!(
                "{name}: kill signal actual={:?}, expected={:?}",
                signals("native"),
                signals("source")
            ));
        }
        assert_eq!(
            std::fs::read_dir(&image_root).unwrap().count(),
            0,
            "{name}: temporary images removed"
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn clipboard_image_helpers_match_frozen_js_whitespace_and_truthiness() {
    use pi_coding_agent::utils::clipboard_image::{extension_for_image_mime_type, is_wayland_session};
    use pi_js_compat::js_json::{JsObject, JsString, JsValue, to_value};
    let state = tempfile::tempdir().unwrap();
    std::fs::write(state.path().join("config.json"), "{}").unwrap();
    let mut source = oracle::TsOracle::spawn_with_environment(
        "clipboard-image-module-oracle.mts",
        "packages/coding-agent/src",
        &[("PI_CLIPBOARD_MATRIX_STATE", state.path().as_os_str().to_owned())],
    );
    let mut mimes: Vec<JsString> = [
        "image/png",
        "\u{feff}IMAGE/PNG; charset=binary",
        "image/png\u{85}",
        "\u{85}image/png",
        "image/jpeg; x=y",
        "image/gif",
        "image/webp",
        "image/bmp",
        "",
        ";image/png",
    ]
    .into_iter()
    .map(JsString::from)
    .collect();
    let raw = JsString::from_utf16(vec![0xd800]);
    mimes.extend([
        raw.clone(),
        JsString::from("image/png") + &raw,
        JsString::from("image/png; x=") + &raw,
    ]);
    let mut environments: Vec<BTreeMap<String, JsString>> = [
        json!({}),
        json!({"WAYLAND_DISPLAY":""}),
        json!({"WAYLAND_DISPLAY":"0"}),
        json!({"XDG_SESSION_TYPE":"wayland"}),
        json!({"XDG_SESSION_TYPE":"Wayland"}),
    ]
    .into_iter()
    .map(|env| serde_json::from_value(env).unwrap())
    .collect();
    environments.push(BTreeMap::from([("WAYLAND_DISPLAY".into(), raw.clone())]));
    environments.push(BTreeMap::from([(
        "XDG_SESSION_TYPE".into(),
        JsString::from("wayland") + &raw,
    )]));
    let request = JsValue::Object(JsObject::from([
        ("operation".into(), JsValue::String("helpers".into())),
        ("bin".into(), JsValue::String("".into())),
        (
            "temporary".into(),
            JsValue::String(state.path().to_str().unwrap().into()),
        ),
        ("mimeTypes".into(), to_value(&mimes).unwrap()),
        ("environments".into(), to_value(&environments).unwrap()),
    ]));
    let expected = source.request(&request);
    let extensions: Vec<_> = mimes.iter().map(extension_for_image_mime_type).collect();
    let wayland: Vec<_> = environments
        .into_iter()
        .map(|env| is_wayland_session(&ClipboardEnvironment::new("linux", env)))
        .collect();
    let actual = JsValue::from(json!({"extensions":extensions,"wayland":wayland}));
    assert_eq!(actual, expected);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
