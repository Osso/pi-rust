//! Whole frozen clipboard.ts over actual private clipboard-tool processes.
use crate::shared_executable_script as executable_script;
#[path = "support/mode_harness.rs"]
mod mode_harness;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use pi_coding_agent::utils::clipboard::copy_to_clipboard;
use pi_coding_agent::utils::clipboard_image::ClipboardEnvironment;
use pi_coding_agent::utils::clipboard_native::{ClipboardError, ClipboardModule};
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless, stringify_lossless};
use serde_json::{Value, json};

struct NativeClipboard {
    root: PathBuf,
    fails: bool,
}

impl ClipboardModule for NativeClipboard {
    fn get_text(&self) -> Result<String, ClipboardError> {
        unreachable!("copy module never reads text")
    }

    fn set_text(&self, text: &str) -> Result<(), ClipboardError> {
        std::fs::write(self.root.join("native-input"), text).unwrap();
        if self.fails {
            Err(ClipboardError::new("native failed"))
        } else {
            Ok(())
        }
    }

    fn has_image(&self) -> Result<bool, ClipboardError> {
        unreachable!("copy module never reads images")
    }

    fn get_image_binary(&self) -> Result<Vec<u8>, ClipboardError> {
        unreachable!("copy module never reads images")
    }
}

async fn run_native_child(root: &Path) {
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    let crash_root = root.to_path_buf();
    pi_coding_agent::utils::uncaught::set_process_crash_handler(Some(std::sync::Arc::new(move |error| {
        if let Some(error) = error.downcast_ref::<pi_js_compat::node_error::NativeNodeError>() {
            std::fs::write(crash_root.join("uncaught.json"), stringify_lossless(&error.value())).unwrap();
        }
        std::process::exit(1);
    })));
    let config = parse_lossless(std::fs::read_to_string(root.join("config.json")).unwrap()).unwrap();
    let variables = config
        .get("env")
        .unwrap()
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.to_utf8().unwrap(), value.as_string().unwrap().clone()))
        .collect();
    let environment = ClipboardEnvironment::new(
        config.get("platform").unwrap().as_string().unwrap().to_utf8().unwrap(),
        variables,
    );
    let native = NativeClipboard {
        root: root.to_path_buf(),
        fails: config.get("nativeError").and_then(JsValue::as_bool) == Some(true),
    };
    let available = config.get("nativeAvailable").and_then(JsValue::as_bool) == Some(true);
    if config.get("closeStdout").and_then(JsValue::as_bool) == Some(true) {
        std::fs::write(root.join("copy-ready"), "ready").unwrap();
        while !root.join("copy-release").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    let result = copy_to_clipboard(
        config.get("text").unwrap().as_string().unwrap(),
        &environment,
        available.then_some(&native as &dyn ClipboardModule),
    )
    .await;
    let report = match result {
        Ok(()) => json!({"copied":true}),
        Err(error) => json!({"error":{"name":"Error","message":error.message().to_utf8().unwrap()}}),
    };
    std::fs::write(root.join("result.json"), serde_json::to_vec(&report).unwrap()).unwrap();
    if config.get("asyncError").and_then(JsValue::as_bool) == Some(true) {
        // A live fixture process lets the real deferred uncaught boundary run.
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if config.get("closeStdout").and_then(JsValue::as_bool) == Some(true) {
        // Avoid a later libtest status write becoming a different broken-pipe fault.
        std::process::exit(0);
    }
    if config.get("tools").unwrap().get("wl-copy").is_some()
        && config.get("asyncError").and_then(JsValue::as_bool) != Some(true)
    {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.join("wl-copy.finished").exists() {
            assert!(Instant::now() < deadline, "private unref stdin writer must drain");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

fn configure_command(command: &mut Command, root: &Path, bin: &Path, config: &Value) {
    for name in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_SESSION_TYPE",
        "XDG_CURRENT_DESKTOP",
        "TERMUX_VERSION",
        "SSH_CONNECTION",
        "SSH_CLIENT",
        "MOSH_CONNECTION",
        "WSL_DISTRO_NAME",
        "WSL_INTEROP",
    ] {
        command.env_remove(name);
    }
    for (key, value) in config["env"].as_object().unwrap() {
        command.env(key, value.as_str().unwrap());
    }
    command.env("PI_CLIPBOARD_TEXT_STATE", root).env("PATH", bin);
}

fn collect_output(mut command: Command, close_stdout: bool, root: &Path) -> Output {
    use std::process::Stdio;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    if close_stdout {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.join("copy-ready").exists() {
            assert!(Instant::now() < deadline, "copy must reach the closed-stdout boundary");
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(child.stdout.take());
        std::fs::write(root.join("copy-release"), "release").unwrap();
    }
    child.wait_with_output().unwrap()
}

fn tool_trace(root: &Path) -> Vec<Value> {
    std::fs::read_to_string(root.join("trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn await_tools(root: &Path, config: &Value) {
    if config["asyncError"] != true && config["tools"]["which"] == 0 && config["tools"].get("wl-copy").is_some() {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !root.join("wl-copy.finished").exists() {
            assert!(Instant::now() < deadline, "private wl-copy must finish");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn compare_processes(root: &Path, config: &Value, source: Output, native: Output) {
    let async_error = config["asyncError"] == true;
    assert_eq!(
        source.status.code(),
        Some(if async_error { 1 } else { 0 }),
        "source stderr: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    assert_eq!(
        native.status.code(),
        source.status.code(),
        "native stderr: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    let source_root = root.join("source");
    let native_root = root.join("native");
    await_tools(&source_root, config);
    await_tools(&native_root, config);
    let report = |root: &Path| parse_lossless(std::fs::read_to_string(root.join("result.json")).unwrap()).unwrap();
    if async_error {
        // Promise continuation vs next-tick error ordering is a scheduling gap;
        // compare the uncaught original Error fields, not result-file timing.
        assert_eq!(
            std::fs::read(source_root.join("uncaught.json")).unwrap(),
            std::fs::read(native_root.join("uncaught.json")).unwrap(),
            "original uncaught spawn Error"
        );
    } else {
        assert_eq!(report(&source_root), report(&native_root), "copy outcome");
    }
    assert_eq!(
        tool_trace(&source_root),
        tool_trace(&native_root),
        "actual tool argv/order"
    );
    for tool in config["tools"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain(std::iter::once("native"))
    {
        let filename = if tool == "native" {
            "native-input".to_owned()
        } else {
            format!("{tool}.input")
        };
        assert_eq!(
            std::fs::read(source_root.join(&filename)).ok(),
            std::fs::read(native_root.join(&filename)).ok(),
            "{tool} bytes"
        );
    }
    let sequence = |bytes: &[u8]| {
        bytes
            .split(|byte| *byte == 0x1b)
            .filter(|chunk| chunk.starts_with(b"]52;c;"))
            .map(<[u8]>::to_vec)
            .collect::<Vec<_>>()
    };
    // Native libtest's final status text follows BEL; compare just the OSC52 protocol.
    let sequence = |bytes: &[u8]| {
        sequence(bytes)
            .into_iter()
            .map(|chunk| chunk.into_iter().take_while(|byte| *byte != 7).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    assert_eq!(sequence(&source.stdout), sequence(&native.stdout), "OSC52 bytes");
}

fn cases() -> Vec<(&'static str, Value)> {
    let case = |platform: &str, env: Value, tools: Value, native: bool, fails: bool| json!({"platform":platform,"text":"copied text","env":env,"tools":tools,"nativeAvailable":native,"nativeError":fails});
    let x11 = json!({"DISPLAY":":fixture"});
    let wayland = json!({"DISPLAY":":fixture","WAYLAND_DISPLAY":"fixture","XDG_SESSION_TYPE":"wayland"});
    let mut cases = vec![
        (
            "nonzero-wl-copy",
            case(
                "linux",
                wayland.clone(),
                json!({"which":0,"wl-copy":27,"xclip":0}),
                false,
                false,
            ),
        ),
        (
            "successful-wl-copy",
            case(
                "linux",
                wayland.clone(),
                json!({"which":0,"wl-copy":0,"xclip":0}),
                false,
                false,
            ),
        ),
        (
            "remote-wl-copy",
            case(
                "linux",
                json!({"WAYLAND_DISPLAY":"fixture","SSH_CONNECTION":"remote"}),
                json!({"which":0,"wl-copy":0}),
                false,
                false,
            ),
        ),
        (
            "native-local",
            case("darwin", x11.clone(), json!({"pbcopy":0}), true, false),
        ),
        (
            "native-remote",
            case(
                "darwin",
                json!({"DISPLAY":":fixture","SSH_CONNECTION":"remote"}),
                json!({"pbcopy":0}),
                true,
                false,
            ),
        ),
        (
            "native-fail-pbcopy",
            case("darwin", x11.clone(), json!({"pbcopy":0}), true, true),
        ),
        (
            "pbcopy-fail-osc",
            case("darwin", x11.clone(), json!({"pbcopy":1}), true, true),
        ),
        (
            "linux-skip-native",
            case("linux", x11.clone(), json!({"xclip":0}), true, false),
        ),
        (
            "win32-command-selection-only",
            case("win32", json!({}), json!({"clip":0}), false, false),
        ),
        (
            "termux-first",
            case(
                "linux",
                json!({"TERMUX_VERSION":"fixture","DISPLAY":":fixture"}),
                json!({"termux-clipboard-set":0,"xclip":0}),
                false,
                false,
            ),
        ),
        (
            "termux-failure-x11",
            case(
                "linux",
                json!({"TERMUX_VERSION":"fixture","DISPLAY":":fixture"}),
                json!({"termux-clipboard-set":1,"xclip":0}),
                false,
                false,
            ),
        ),
        (
            "which-failure-x11",
            case("linux", wayland.clone(), json!({"which":1,"xclip":0}), false, false),
        ),
        (
            "which-failure-no-x11",
            case(
                "linux",
                json!({"WAYLAND_DISPLAY":"fixture"}),
                json!({"which":1}),
                false,
                false,
            ),
        ),
        (
            "x11-fallback-xsel",
            case("linux", x11.clone(), json!({"xclip":1,"xsel":0}), false, false),
        ),
        (
            "x11-all-fail-osc",
            case("linux", x11.clone(), json!({"xclip":1,"xsel":1}), false, false),
        ),
        ("no-display", case("linux", json!({}), json!({}), false, false)),
        (
            "remote-client",
            case(
                "linux",
                json!({"DISPLAY":":fixture","SSH_CLIENT":"0"}),
                json!({"xclip":0}),
                false,
                false,
            ),
        ),
        (
            "remote-mosh",
            case(
                "linux",
                json!({"DISPLAY":":fixture","MOSH_CONNECTION":" "}),
                json!({"xclip":0}),
                false,
                false,
            ),
        ),
        (
            "empty-remote-is-local",
            case(
                "linux",
                json!({"DISPLAY":":fixture","SSH_CLIENT":"","MOSH_CONNECTION":""}),
                json!({"xclip":0}),
                false,
                false,
            ),
        ),
    ];
    let mut early = case(
        "linux",
        wayland,
        json!({"which":0,"wl-copy":27,"xclip":0}),
        false,
        false,
    );
    early["earlyExit"] = json!(["wl-copy"]);
    cases.push(("early-wl-copy-epipe", early));
    let mut missing = case(
        "linux",
        json!({"DISPLAY":":fixture","WAYLAND_DISPLAY":"fixture"}),
        json!({"which":0,"xclip":0}),
        false,
        false,
    );
    missing["asyncError"] = json!(true);
    cases.push(("async-spawn-noent", missing));
    let mut denied = case(
        "linux",
        json!({"DISPLAY":":fixture","WAYLAND_DISPLAY":"fixture"}),
        json!({"which":0,"wl-copy":0,"xclip":0}),
        false,
        false,
    );
    denied["asyncError"] = json!(true);
    denied["nonExecutable"] = json!(["wl-copy"]);
    cases.push(("async-spawn-access", denied));
    let mut broken_stdout = case("linux", json!({}), json!({}), false, false);
    broken_stdout["asyncError"] = json!(true);
    broken_stdout["closeStdout"] = json!(true);
    cases.push(("osc-broken-pipe", broken_stdout));
    for (name, length) in [("osc-limit-exact", 75000), ("osc-limit-exceeded", 75001)] {
        let mut config = case("linux", json!({}), json!({}), false, false);
        config["text"] = json!("x".repeat(length));
        cases.push((name, config));
    }
    let mut remote_large = case(
        "darwin",
        json!({"DISPLAY":":fixture","SSH_CLIENT":"remote"}),
        json!({}),
        true,
        false,
    );
    remote_large["text"] = json!("x".repeat(75001));
    cases.push(("remote-success-survives-osc-cap", remote_large));
    let mut raw = case("linux", json!({}), json!({}), false, false);
    raw["rawText"] = json!(true);
    cases.push(("osc-raw-utf16", raw));
    let mut blocked = case("darwin", x11, json!({"pbcopy":0}), false, false);
    blocked["text"] = json!("x".repeat(2 * 1024 * 1024));
    blocked["delayRead"] = json!({"pbcopy":6});
    cases.push(("execsync-whole-call-timeout", blocked));
    cases
}

fn write_config(path: &Path, config: &Value) {
    let mut wire = JsValue::from(config.clone());
    if config["rawText"] == true {
        wire.as_object_mut().unwrap().insert(
            "text".into(),
            JsValue::String(JsString::from_utf16(vec![0x61, 0xd800, 0x62, 0xdc00, 0xd83e, 0xdd80])),
        );
    }
    std::fs::write(path, stringify_lossless(&wire)).unwrap();
}

#[tokio::test]
async fn clipboard_text_process_matrix() {
    run_matrix(None).await;
}

#[tokio::test]
async fn clipboard_text_async_spawn_errors() {
    run_matrix(Some("async-spawn-noent")).await;
    run_matrix(Some("async-spawn-access")).await;
}

#[tokio::test]
async fn clipboard_text_stdout_error() {
    run_matrix(Some("osc-broken-pipe")).await;
}

#[tokio::test]
async fn clipboard_text_execsync_timeout() {
    run_matrix(Some("execsync-whole-call-timeout")).await;
}

async fn run_matrix(selected: Option<&str>) {
    if let Some(root) = std::env::var_os("PI_CLIPBOARD_TEXT_CHILD") {
        run_native_child(Path::new(&root)).await;
        return;
    }
    let lock = mode_harness::read_source_lock();
    mode_harness::verify_source_head(&lock);
    // Frozen clipboard.ts:92-103 never awaits wl-copy's status. Nonzero exit
    // must not trigger X11 fallback after successful spawn/stdin/end/unref.
    for (name, config) in cases().into_iter().filter(|(name, _)| {
        selected.map_or(
            *name != "execsync-whole-call-timeout" && *name != "osc-broken-pipe" && !name.starts_with("async-spawn"),
            |selected| selected == *name,
        )
    }) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        for tool in config["tools"].as_object().unwrap().keys() {
            executable_script::write_executable_script(
                &bin.join(tool),
                include_str!("fixtures/clipboard_text_tool.py"),
            );
            if config["nonExecutable"]
                .as_array()
                .is_some_and(|tools| tools.iter().any(|name| name == tool))
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(bin.join(tool), std::fs::Permissions::from_mode(0o600)).unwrap();
            }
        }
        for runtime in ["source", "native"] {
            std::fs::create_dir(root.join(runtime)).unwrap();
            write_config(&root.join(runtime).join("config.json"), &config);
        }
        let oracle = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../oracle/clipboard-text-module-oracle.mts");
        let mut source = Command::new("/usr/bin/node");
        source
            .arg(lock.worktree.join("node_modules/tsx/dist/cli.mjs"))
            .arg(oracle)
            .arg(lock.worktree.join("packages/coding-agent/src"));
        configure_command(&mut source, &root.join("source"), &bin, &config);
        let source = collect_output(source, config["closeStdout"] == true, &root.join("source"));
        let mut native = Command::new(std::env::current_exe().unwrap());
        native
            .args(["clipboard_text_process_matrix", "--exact", "--nocapture"])
            .env("PI_CLIPBOARD_TEXT_CHILD", root.join("native"));
        configure_command(&mut native, &root.join("native"), &bin, &config);
        let native = collect_output(native, config["closeStdout"] == true, &root.join("native"));
        eprintln!("case {name}");
        compare_processes(root, &config, source, native);
    }
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
