#![cfg(target_os = "linux")]
#[path = "support/oracle.rs"]
mod oracle;
use pi_agent_core::agent::ToolError;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::bash_executor::{BashExecutorOptions, BashResult, execute_bash_with_operations};
use pi_coding_agent::core::tools::bash::{BashDataCallback, BashExecOptions, BashExecResult, BashOperations};
use pi_coding_agent::utils::{paths::node_dirname, uncaught};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue, encode_node_utf8};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

fn describe(text: &JsString) -> JsValue {
    json!({"length":text.len(),"digest":format!("{:x}",Sha256::digest(encode_node_utf8(text).as_bytes()))})
}
fn spill_names(root: &Path) -> HashSet<String> {
    if !root.exists() {
        return HashSet::new();
    }
    std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| valid_name(name))
        .collect()
}
fn valid_name(name: &str) -> bool {
    let Some(body) = name.strip_prefix("pi-bash-").and_then(|body| body.strip_suffix(".log")) else {
        return false;
    };
    body.len() == 16
        && body
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn normalize(value: &JsValue) -> JsValue {
    match value {
        JsValue::String(text) => {
            let regex = pi_js_compat::js_regex::JsRegex::new(r"pi-bash-[0-9a-f]{16}\.log").unwrap();
            JsValue::String(regex.replace_all(text, "<LOG>"))
        }
        JsValue::Array(items) => JsValue::Array(items.iter().map(normalize).collect()),
        JsValue::Object(items) => JsValue::Object(
            items
                .iter()
                .map(|(key, value)| (key.clone(), normalize(value)))
                .collect(),
        ),
        value => value.clone(),
    }
}
fn run_child(input: &JsValue, path: &Path, root: &Path, library: &Path) -> JsValue {
    std::fs::write(path, js_json::stringify_lossless(input)).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--ignored", "--exact", "bash_executor_spill_child", "--nocapture"])
        .current_dir(root)
        .env("PI_BASH_SPILL_CASE_PATH", path)
        .env("LD_PRELOAD", library)
        .env_remove("PI_BASH_SPILL_FAIL")
        .env("TMPDIR", root.join("default-spill"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(fault) = input["fault"].as_string() {
        if fault == "open" {
            command.env("TMPDIR", encode_node_utf8(input["env"]["TMPDIR"].as_string().unwrap()));
        } else {
            command.env("PI_BASH_SPILL_FAIL", fault.to_utf8().unwrap());
        }
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let timed_out = loop {
        if child.try_wait().unwrap().is_some() {
            break false;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            break true;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = child.wait_with_output().unwrap();
    if timed_out {
        return json!({"childTimedOut":true});
    }
    let stdout = String::from_utf8(output.stdout).unwrap();
    let record = stdout
        .lines()
        .find_map(|line| line.strip_prefix("BASH_SPILL:"))
        .unwrap_or_else(|| {
            panic!(
                "missing spill record: {stdout}; {}",
                String::from_utf8_lossy(&output.stderr)
            )
        });
    let record = js_json::parse_lossless(record).unwrap();
    let expected_status = if record.get("uncaught").is_some() || record.get("nonNative").is_some() {
        19
    } else {
        0
    };
    assert_eq!(output.status.code(), Some(expected_status), "{record:?}");
    record
}

// Frozen bash-executor.ts:56-162, real files/stream failures and reentrant callback,
// not a callback-count-only or source-shape test. Every file is hashed in full.
#[test]
fn executor_spill_lifetime_environment_and_reentrant_callbacks_match_frozen() {
    let temp = tempfile::tempdir().unwrap();
    let library = temp.path().join("spill-failure.so");
    let compiler = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/bash_spill_failure.c"))
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(
        compiler.status.success(),
        "{}",
        String::from_utf8_lossy(&compiler.stderr)
    );
    let tmp = temp.path().join("tmp");
    let temp_env = temp.path().join("temp");
    let tmpdir = temp.path().join("tmpdir");
    for root in [&tmp, &temp_env, &tmpdir] {
        std::fs::create_dir(root).unwrap();
    }
    let default_spill = temp.path().join("default-spill");
    std::fs::create_dir(&default_spill).unwrap();
    let large = vec![b'x'; 60_000];
    let cases = vec![
        json!({"chunks":[b"start".to_vec(),b"end".to_vec()],"reenter":true}),
        json!({"chunks":[large.clone()]}),
        json!({"chunks":["x\n".repeat(3000).into_bytes()]}),
        json!({"chunks":[vec![0;60_000]]}),
        json!({"chunks":[vec![b'a';60_000],vec![b'b';60_000],vec![b'c';60_000]]}),
        json!({"chunks":["é😀".repeat(10_000).into_bytes()]}),
        json!({"chunks":[large.clone()],"error":"operation-failed","expectFile":true}),
        json!({"chunks":[large.clone()],"env":{"TMPDIR":tmpdir.to_str().unwrap(),"TMP":tmp.to_str().unwrap(),"TEMP":temp_env.to_str().unwrap()}}),
        json!({"chunks":[large.clone()],"env":{"TMPDIR":"","TMP":tmp.to_str().unwrap(),"TEMP":temp_env.to_str().unwrap()}}),
        json!({"chunks":[large.clone()],"env":{"TEMP":temp_env.to_str().unwrap()}}),
        json!({"chunks":[large.clone()],"fault":"open","env":{"TMPDIR":temp.path().join("missing").to_str().unwrap()}}),
        json!({"chunks":[large.clone()],"fault":"write"}),
        json!({"chunks":[large.clone()],"fault":"close"}),
        json!({"chunks":[large.clone()],"fault":"close","error":"operation-failed"}),
        json!({"chunks":[b"small".to_vec()],"late":true}),
        json!({"chunks":[large.clone()],"late":true,"fault":"late"}),
        json!({"chunks":[large.clone()],"late":true,"fault":"late","error":"operation-failed"}),
        json!({"chunks":[large.clone()],"chunkError":JsString::from("chunk-")+JsString::from_utf16(vec![0xd800]),"expectFile":true}),
        json!({"chunks":[large],"fault":"write-close"}),
    ];
    let mut source = oracle::TsOracle::spawn_with_environment(
        "bash-executor-spill-oracle.ts",
        "packages/coding-agent/src/core/bash-executor.ts",
        &[
            ("LD_PRELOAD", library.as_os_str().to_owned()),
            ("TMPDIR", default_spill.as_os_str().to_owned()),
        ],
    );
    let expected = source.request(&json!({"id":28061,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    for index in [10, 11, 12, 13, 15, 16, 18] {
        assert!(
            expected["output"][index].get("uncaught").is_some(),
            "source fault case {index} must reach the real unhandled stream boundary: {:?}",
            expected["output"][index]
        );
    }
    let mut failures = Vec::new();
    for (index, input) in cases.iter().enumerate() {
        let actual = run_child(
            input,
            &temp.path().join(format!("case-{index}.json")),
            temp.path(),
            &library,
        );
        let actual = normalize(&actual);
        let expected = normalize(&expected["output"][index]);
        if actual != expected {
            failures.push(json!({"case":index,"actual":actual,"expected":expected}));
        }
    }
    assert!(failures.is_empty(), "{}", js_json::stringify_lossless(&json!(failures)));
}

struct Operations {
    chunks: Vec<Vec<u8>>,
    error: Option<JsString>,
    saved: Arc<Mutex<Option<BashDataCallback>>>,
}
impl BashOperations for Operations {
    fn exec<'a>(
        &'a self,
        _: &'a JsString,
        _: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            *self.saved.lock().unwrap() = Some(options.on_data.clone());
            for bytes in &self.chunks {
                (options.on_data)(bytes).map_err(|error| pi_agent_core::agent::ToolError::new(error.message))?;
            }
            if let Some(error) = &self.error {
                return Err(ToolError::new(error.clone()));
            }
            Ok(BashExecResult {
                exit_code: Some(0.0),
                detached: None,
            })
        })
    }
}
fn summarize(
    result: Result<BashResult, ToolError>,
    chunks: &[JsString],
    before: &HashSet<String>,
    temp_root: &Path,
) -> JsValue {
    let path = result
        .as_ref()
        .ok()
        .and_then(|value| value.full_output_path.as_ref())
        .map(|path| PathBuf::from(encode_node_utf8(path)))
        .or_else(|| {
            spill_names(temp_root)
                .difference(before)
                .next()
                .map(|name| temp_root.join(name))
        });
    let result = match result {
        Ok(value) => {
            let output = describe(&value.output);
            let mut value = js_json::to_value(&value).unwrap();
            value["output"] = output;
            json!({"ok":true,"value":value})
        }
        Err(error) => json!({"ok":false,"error":error.message}),
    };
    let mut record = json!({"result":result,"chunks":chunks.iter().map(describe).collect::<Vec<_>>()});
    if let Some(path) = path {
        use std::os::unix::fs::PermissionsExt;
        let bytes = std::fs::read(&path).unwrap();
        let name = path.file_name().unwrap().to_str().unwrap();
        record["fullFile"] = json!({"size":bytes.len(),"digest":format!("{:x}",Sha256::digest(&bytes)),"mode":std::fs::metadata(&path).unwrap().permissions().mode()&0o777,"directory":node_dirname(JsString::from(path.to_str().unwrap())),"randomName":valid_name(name)});
        std::fs::remove_file(path).unwrap();
    }
    record
}
#[test]
#[ignore = "isolated reentrant callback/process-hook/stream syscall fixture"]
fn bash_executor_spill_child() {
    let input =
        js_json::parse_lossless(std::fs::read_to_string(std::env::var("PI_BASH_SPILL_CASE_PATH").unwrap()).unwrap())
            .unwrap();
    if let Some(env) = input["env"].as_object() {
        for name in ["TMPDIR", "TMP", "TEMP"] {
            pi_js_compat::process_env::remove(name);
        }
        for (name, value) in env {
            pi_js_compat::process_env::set(&name.to_utf8().unwrap(), &value.as_string().unwrap().to_utf8().unwrap());
        }
    }
    let temp_root = std::env::temp_dir();
    let before = spill_names(&temp_root);
    let chunks = Arc::new(Mutex::new(Vec::new()));
    let captured = chunks.clone();
    uncaught::install_uncaught_handler();
    uncaught::set_process_crash_handler(Some(Arc::new(move |payload| {
        let chunks = captured.lock().unwrap().iter().map(describe).collect::<Vec<_>>();
        let record = match payload.downcast_ref::<pi_js_compat::node_error::NativeNodeError>() {
            Some(native) => {
                json!({"uncaught":native.value(),"constructor":native.constructor_class().name(),"string":native.string_value(),"chunks":chunks})
            }
            None => json!({"nonNative":true,"chunks":chunks}),
        };
        println!("BASH_SPILL:{}", js_json::stringify_lossless(&record));
        std::process::exit(19);
    })));
    let saved = Arc::new(Mutex::new(None::<BashDataCallback>));
    let callback_saved = Arc::downgrade(&saved);
    let callback_chunks = chunks.clone();
    let reenter = input["reenter"] == JsValue::Bool(true);
    let reentered = AtomicBool::new(false);
    let chunk_error = input["chunkError"].as_string().cloned();
    let on_chunk = Arc::new(move |text: &JsString| {
        callback_chunks.lock().unwrap().push(text.clone());
        if reenter && !reentered.swap(true, Ordering::SeqCst) {
            let callback = callback_saved.upgrade().unwrap().lock().unwrap().clone().unwrap();
            callback(b"nested")?;
        }
        if let Some(message) = &chunk_error {
            return Err(pi_js_compat::callback_error::CallbackError::new(message.clone()));
        }
        Ok(())
    });
    let operations = Arc::new(Operations {
        chunks: input["chunks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|chunk| {
                chunk
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|byte| byte.as_u64().unwrap() as u8)
                    .collect()
            })
            .collect(),
        error: input["error"].as_string().cloned(),
        saved,
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(execute_bash_with_operations(
        &"spill".into(),
        &"/cwd".into(),
        operations.clone(),
        &BashExecutorOptions {
            on_chunk: Some(on_chunk),
            signal: None,
        },
    ));
    if input["late"] == JsValue::Bool(true) {
        let callback = operations.saved.lock().unwrap().clone().unwrap();
        callback(b"late").unwrap();
    }
    runtime.block_on(async {
        tokio::time::sleep(Duration::from_millis(100)).await;
    });
    let record = summarize(result, &chunks.lock().unwrap(), &before, &temp_root);
    println!("BASH_SPILL:{}", js_json::stringify_lossless(&record));
}
