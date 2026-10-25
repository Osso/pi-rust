//! Separable Doom helpers only: the actual overlay is blocked on key-release opt-in.
use crate::shared_doom_engine as doom_engine;
#[path = "../examples/extensions/doom_overlay/doom_keys.rs"]
mod doom_keys;
use crate::shared_oracle as oracle;
#[path = "../examples/extensions/doom_overlay/wad_finder.rs"]
mod wad_finder;

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

static KEY_PROTOCOL: Mutex<()> = Mutex::new(());

#[test]
fn keys_match_pinned_raw_kitty_and_legacy_inputs() {
    let _protocol = KEY_PROTOCOL.lock().unwrap();
    let mut inputs: Vec<String> = (0..=127).map(|n| char::from(n).to_string()).collect();
    inputs.extend(
        [
            "", "12", "90", "9x", "10cheat", "00", "é", "İ", "Σ", "🚀", "𐀀", "e\u{301}", "\u{1b}[A", "\u{1b}[B",
            "\u{1b}[C", "\u{1b}[D", "\u{1b}OA", "\u{1b}[Z",
        ]
        .map(str::to_owned),
    );
    for code in [32, 49, 97, 99, 100, 102, 110, 115, 119, 121, 57352, 57353, 57354, 57355] {
        for modifier in [1, 2, 5, 6] {
            for event in [1, 2, 3] {
                inputs.push(format!("\u{1b}[{code};{modifier}:{event}u"));
            }
        }
    }
    let mut ts = oracle::TsOracle::spawn(
        "example-doom-helpers-oracle.ts",
        "packages/coding-agent/examples/extensions/doom-overlay/doom-keys.ts",
    );
    for kitty in [false, true] {
        pi_tui::keys::set_kitty_protocol_active(kitty);
        let expected = ts.request(&json!({"kind":"keys", "inputs":inputs, "kitty":kitty}));
        let mapped: Vec<_> = inputs
            .iter()
            .map(|input| doom_keys::map_key_to_doom(&JsString::from(input.as_str())))
            .collect();
        assert_eq!(json!(mapped), expected["mapped"], "kitty={kitty}");
        assert_eq!(json!(doom_keys::DOOM_KEYS), expected["constants"]);
        assert_eq!(
            doom_keys::map_key_to_doom(&JsString::from("12")),
            vec![49],
            "source lexicographic weapon branch accepts multi-character input"
        );
    }
    pi_tui::keys::set_kitty_protocol_active(false);
}

#[test]
fn lone_surrogates_survive_mapping_with_explicit_string_core_projection() {
    let _protocol = KEY_PROTOCOL.lock().unwrap();
    pi_tui::keys::set_kitty_protocol_active(false);
    let units = vec![
        vec![0xd800],
        vec![0xdfff],
        vec![0xfffd],
        vec![0xd800, 0x77],
        vec![0x30, 0xd800],
    ];
    let mut ts = oracle::TsOracle::spawn(
        "example-doom-helpers-oracle.ts",
        "packages/coding-agent/examples/extensions/doom-overlay/doom-keys.ts",
    );
    let expected = ts.request(&json!({"kind":"keys", "units":units, "kitty":false}));
    let mapped: Vec<_> = units
        .iter()
        .map(|units| doom_keys::map_key_to_doom(&JsString::from_utf16(units.clone())))
        .collect();
    assert_eq!(json!(mapped), expected["mapped"]);
    assert_eq!(mapped[0], vec![0xd800]);
    assert_eq!(mapped[1], vec![0xdfff]);
    // Existing pi-tui key APIs take UTF-8 String input. Their boundary projects
    // lone units to U+FFFD, while this helper retains raw units for charCodeAt.
    let projected = JsString::from_utf16(vec![0xd800]).to_utf8_lossy();
    assert_eq!(projected, "\u{fffd}");
    assert!(!pi_tui::keys::matches_key(&projected, "w"));
    assert_eq!(pi_tui::keys::parse_key(&projected), None);
    assert_eq!(mapped[2], vec![65533]);
}

#[test]
fn all_bmp_units_and_supplementary_cased_letters_match_pinned_lowercase_mapping() {
    let _protocol = KEY_PROTOCOL.lock().unwrap();
    pi_tui::keys::set_kitty_protocol_active(false);
    let mut units: Vec<Vec<u16>> = (0..=u16::MAX).map(|unit| vec![unit]).collect();
    units.extend(
        (0x10000..=0x10ffff)
            .filter_map(char::from_u32)
            .filter(|ch| ch.is_lowercase() || ch.is_uppercase())
            .map(|ch| {
                let mut buffer = [0; 2];
                ch.encode_utf16(&mut buffer).to_vec()
            }),
    );
    assert!(units.len() > 65536);
    let mut ts = oracle::TsOracle::spawn(
        "example-doom-helpers-oracle.ts",
        "packages/coding-agent/examples/extensions/doom-overlay/doom-keys.ts",
    );
    for (index, chunk) in units.chunks(2048).enumerate() {
        let expected = ts.request(&json!({"kind":"keys","units":chunk,"kitty":false}));
        let actual: Vec<_> = chunk
            .iter()
            .map(|units| doom_keys::map_key_to_doom(&JsString::from_utf16(units.clone())))
            .collect();
        assert_eq!(json!(actual), expected["mapped"], "Unicode corpus chunk {index}");
    }
}

#[derive(Clone)]
struct Module {
    trace: Arc<Mutex<Vec<Value>>>,
    next: u32,
    fail: bool,
}
impl doom_engine::DoomModule for Module {
    fn create_path(&mut self, parent: &str, path: &str) -> Result<(), String> {
        self.trace
            .lock()
            .unwrap()
            .push(json!(["path", parent, path, true, true]));
        Ok(())
    }
    fn create_data_file(&mut self, parent: &str, name: &str, data: &[u8]) -> Result<(), String> {
        self.trace
            .lock()
            .unwrap()
            .push(json!(["file", parent, name, data, true, false]));
        Ok(())
    }
    fn malloc(&mut self, size: u32) -> Result<u32, String> {
        let ptr = self.next;
        self.next += size;
        self.trace.lock().unwrap().push(json!(["malloc", size, ptr]));
        Ok(ptr)
    }
    fn set_value(&mut self, ptr: u32, value: u32, kind: &str) -> Result<(), String> {
        self.trace.lock().unwrap().push(json!(["set", ptr, value, kind]));
        Ok(())
    }
    fn create(&mut self, argc: u32, argv: u32) -> Result<(), String> {
        self.trace.lock().unwrap().push(json!(["create", argc, argv]));
        if self.fail {
            Err("fixture create failed".into())
        } else {
            Ok(())
        }
    }
    fn free(&mut self, ptr: u32) -> Result<(), String> {
        self.trace.lock().unwrap().push(json!(["free", ptr]));
        Ok(())
    }
    fn frame_buffer(&self) -> Result<u32, String> {
        Ok(1024)
    }
    fn width(&self) -> Result<usize, String> {
        Ok(2)
    }
    fn height(&self) -> Result<usize, String> {
        Ok(1)
    }
    fn get_i32(&self, ptr: u32) -> Result<i32, String> {
        Ok(if ptr == 1024 { 0x12345678 } else { 0xffabcdefu32 as i32 })
    }
    fn tick(&mut self) -> Result<(), String> {
        self.trace.lock().unwrap().push(json!(["tick"]));
        Ok(())
    }
    fn push_key(&mut self, pressed: bool, key: u32) -> Result<(), String> {
        self.trace.lock().unwrap().push(json!(["key", u32::from(pressed), key]));
        Ok(())
    }
}

#[test]
fn engine_wrapper_matches_pinned_memory_frame_and_input_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let wad = temp.path().join("fixture.wad");
    std::fs::write(&wad, b"IWAD-fixture").unwrap();
    let mut ts = oracle::TsOracle::spawn(
        "example-doom-helpers-oracle.ts",
        "packages/coding-agent/examples/extensions/doom-overlay/doom-engine.ts",
    );
    for fail in [false, true] {
        let expected = ts.request(&json!({"kind":"engine", "wad":wad, "fail":fail}));
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut engine = doom_engine::DoomEngine::new(wad.clone());
        assert!(!engine.is_initialized());
        assert_eq!((engine.width(), engine.height()), (640, 400));
        assert_eq!(engine.get_frame_rgba().unwrap(), vec![0; 640 * 400 * 4]);
        engine.tick().unwrap();
        engine.push_key(true, 173).unwrap();
        assert!(trace.lock().unwrap().is_empty());
        let module = Module {
            trace: trace.clone(),
            next: 64,
            fail,
        };
        let error = engine
            .init_with_module(Box::new(module))
            .err()
            .map(|error| error.message());
        engine.tick().unwrap();
        engine.push_key(true, 173).unwrap();
        engine.push_key(false, 173).unwrap();
        let actual = json!({"trace":trace.lock().unwrap().clone(), "error":error, "initialized":engine.is_initialized(), "width":engine.width(), "height":engine.height(), "frame":engine.get_frame_rgba().unwrap()});
        assert_eq!(actual, expected);
        if !fail {
            assert_eq!(
                engine.get_frame_rgba().unwrap(),
                vec![0x34, 0x56, 0x78, 255, 0xab, 0xcd, 0xef, 255]
            );
        }
    }
}

#[test]
fn wad_read_failures_match_node_error_messages_before_any_module_calls() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("file.wad");
    std::fs::write(&file, b"IWAD").unwrap();
    let cycle = temp.path().join("cycle.wad");
    std::os::unix::fs::symlink(&cycle, &cycle).unwrap();
    let mut ts = oracle::TsOracle::spawn(
        "example-doom-helpers-oracle.ts",
        "packages/coding-agent/examples/extensions/doom-overlay/doom-engine.ts",
    );
    for path in [
        temp.path().join("missing '🚀.wad"),
        temp.path().to_owned(),
        file.join("child"),
        cycle,
    ] {
        let expected = ts.request(&json!({"kind":"engine-read", "wad":path}));
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut engine = doom_engine::DoomEngine::new(path);
        let error = engine
            .init_with_module(Box::new(Module {
                trace: trace.clone(),
                next: 64,
                fail: false,
            }))
            .unwrap_err();
        assert_eq!(error.message(), expected["error"]["message"].as_str().unwrap());
        let doom_engine::DoomInitError::Wad(identity) = error else {
            panic!("WAD read must retain Node error identity")
        };
        assert_eq!(
            json!({"name":identity.name,"message":identity.message,"errno":identity.errno,"code":identity.code,"syscall":identity.syscall,"path":identity.path}),
            expected["error"]
        );
        assert_eq!(json!(trace.lock().unwrap().clone()), expected["trace"]);
        assert!(!engine.is_initialized());
        assert_eq!((engine.width(), engine.height()), (640, 400));
    }
}

struct Body(Option<Result<Vec<u8>, CallbackError>>);
impl HttpBody for Body {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = self.0.take();
        Box::pin(async move { chunk })
    }
}
struct Download {
    status: u16,
    bytes: Vec<u8>,
    fail: bool,
    body_fail: bool,
    calls: Arc<Mutex<Vec<Value>>>,
}
impl Fetch for Download {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.calls.lock().unwrap().push(json!([request.method, request.url]));
        Box::pin(async move {
            if self.fail {
                return Err(CallbackError::new("fixture network failed"));
            }
            Ok(HttpResponse {
                status: self.status,
                status_text: String::new(),
                headers: vec![],
                body: Box::new(Body(Some(if self.body_fail {
                    Err(CallbackError::new("fixture body failed"))
                } else {
                    Ok(self.bytes.clone())
                }))),
            })
        })
    }
}

#[tokio::test]
async fn absent_home_directory_wads_and_concatenated_gzip_match_pinned() {
    let mut ts = oracle::TsOracle::spawn(
        "example-doom-helpers-oracle.ts",
        "packages/coding-agent/examples/extensions/doom-overlay/wad-finder.ts",
    );
    for case in [
        "absent-home",
        "empty-home",
        "cwd-directory",
        "bundled-directory",
        "concatenated-gzip",
        "zero-padded-gzip",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("cwd");
        let bundled = temp.path().join("module");
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::create_dir_all(&bundled).unwrap();
        let finder = wad_finder::WadFinder::new(&bundled, &cwd, if case == "empty-home" { Some("") } else { None });
        if matches!(case, "absent-home" | "empty-home") {
            std::fs::write(cwd.join("chosen.wad"), b"IWAD").unwrap();
            let mut request = json!({"kind":"wad-find","bundled":bundled,"cwd":cwd,"custom":"~./chosen.wad"});
            if case == "empty-home" {
                request["home"] = json!("");
            }
            assert_eq!(
                json!(finder.find_wad_file(Some("~./chosen.wad"))),
                ts.request(&request),
                "{case}"
            );
            request["custom"] = json!("~/definitely-missing-pi-rust-doom-q.wad");
            assert_eq!(
                json!(finder.find_wad_file(request["custom"].as_str())),
                ts.request(&request),
                "{case} absolute tilde expansion"
            );
            request["custom"] = Value::Null;
            assert_eq!(
                json!(finder.find_wad_file(None)),
                ts.request(&request),
                "{case} defaults"
            );
            continue;
        }
        if case == "cwd-directory" {
            std::fs::create_dir(cwd.join("doom1.wad")).unwrap();
        }
        if case == "bundled-directory" {
            std::fs::create_dir(bundled.join("doom1.wad")).unwrap();
        }
        let first = ts.request(&json!({"kind":"gzip","bytes":b"IWAD-first".to_vec()}));
        let second = ts.request(&json!({"kind":"gzip","bytes":b"second-member".to_vec()}));
        let mut bytes: Vec<u8> = serde_json::from_value(first).unwrap();
        if case == "zero-padded-gzip" {
            bytes.extend([0; 10]);
        }
        bytes.extend(serde_json::from_value::<Vec<u8>>(second).unwrap());
        let expected = ts.request(&json!({"kind":"wad-ensure","bundled":bundled,"cwd":cwd,"bytes":bytes,"status":200}));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let fetch = Download {
            status: 200,
            bytes,
            fail: false,
            body_fail: false,
            calls: calls.clone(),
        };
        let result = finder.ensure_wad_file(&fetch).await;
        assert_eq!(
            json!({"path":result,"calls":calls.lock().unwrap().clone(),"bytes":std::fs::read(bundled.join("doom1.wad")).ok()}),
            expected,
            "{case}"
        );
        if case == "concatenated-gzip" {
            assert_eq!(std::fs::read(bundled.join("doom1.wad")).unwrap(), fetch.bytes);
        }
    }
}

#[tokio::test]
async fn wad_search_order_and_download_failures_match_pinned_without_network() {
    let mut ts = oracle::TsOracle::spawn(
        "example-doom-helpers-oracle.ts",
        "packages/coding-agent/examples/extensions/doom-overlay/wad-finder.ts",
    );
    for case in [
        "none",
        "custom",
        "custom-missing",
        "tilde",
        "tilde-user",
        "empty",
        "bundled",
        "cwd-lower",
        "cwd-upper",
        "home",
        "dotdoom",
        "download",
        "http",
        "network",
        "body",
        "gzip",
        "invalid",
        "ascii-mask",
        "write",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let cwd = root.join("cwd");
        let home = root.join("home");
        let bundled = root.join("module");
        for dir in [&cwd, &home, &bundled] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::create_dir_all(home.join(".doom")).unwrap();
        let mut custom = None;
        let paths = match case {
            "custom" => {
                custom = Some("./other/../chosen.wad");
                vec![cwd.join("chosen.wad"), bundled.join("doom1.wad")]
            }
            "custom-missing" => {
                custom = Some("missing.wad");
                vec![bundled.join("doom1.wad")]
            }
            "tilde" => {
                custom = Some("~/chosen.wad");
                vec![home.join("chosen.wad")]
            }
            "tilde-user" => {
                custom = Some("~user.wad");
                vec![root.join("homeuser.wad")]
            }
            "bundled" | "empty" => {
                if case == "empty" {
                    custom = Some("");
                }
                vec![bundled.join("doom1.wad"), cwd.join("doom1.wad")]
            }
            "cwd-lower" => vec![cwd.join("doom1.wad"), cwd.join("DOOM1.WAD")],
            "cwd-upper" => vec![cwd.join("DOOM1.WAD"), home.join("doom1.wad")],
            "home" => vec![home.join("doom1.wad"), home.join(".doom/doom1.wad")],
            "dotdoom" => vec![home.join(".doom/doom1.wad")],
            _ => vec![],
        };
        for path in paths {
            std::fs::write(path, b"existing").unwrap();
        }
        let finder = wad_finder::WadFinder::new(&bundled, &cwd, Some(home.to_str().unwrap()));
        let found = finder.find_wad_file(custom);
        let search = ts.request(&json!({"kind":"wad-find","bundled":bundled,"cwd":cwd,"home":home,"custom":custom}));
        assert_eq!(json!(found), search, "search {case}");
        if matches!(case, "custom" | "custom-missing" | "tilde" | "tilde-user") {
            continue;
        }
        let download = matches!(
            case,
            "download" | "http" | "network" | "body" | "gzip" | "invalid" | "ascii-mask" | "write"
        );
        if !download && case == "none" {
            continue;
        }
        let payload: Vec<u8> = match case {
            "invalid" => b"PWADfixture".to_vec(),
            "ascii-mask" => vec![0xc9, 0xd7, 0xc1, 0xc4, 1, 2],
            _ => b"IWADfixture".to_vec(),
        };
        let encoded = ts.request(&json!({"kind":"gzip","bytes":payload}));
        let bytes: Vec<u8> = if case == "gzip" {
            vec![1, 2, 3]
        } else {
            serde_json::from_value(encoded).unwrap()
        };
        if case == "write" {
            std::fs::remove_dir(&bundled).unwrap();
        }
        let request = json!({"kind":"wad-ensure","bundled":bundled,"cwd":cwd,"home":home,"bytes":bytes,"status":if case=="http" {404} else {200},"fail":case=="network","bodyFail":case=="body"});
        // Oracle restores its writes; native then sees identical input state.
        let expected = ts.request(&request);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let fetch = Download {
            status: if case == "http" { 404 } else { 200 },
            bytes,
            fail: case == "network",
            body_fail: case == "body",
            calls: calls.clone(),
        };
        let result = finder.ensure_wad_file(&fetch).await;
        assert_eq!(
            json!({"path":result,"calls":calls.lock().unwrap().clone(),"bytes":std::fs::read(bundled.join("doom1.wad")).ok()}),
            expected,
            "ensure {case}"
        );
    }
}

// Shared support is compiled once per integration target.
#[path = "../examples/extensions/doom_overlay/doom_engine.rs"]
pub mod shared_doom_engine;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
