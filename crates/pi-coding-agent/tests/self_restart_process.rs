use pi_ai::request::CallbackError;
use pi_coding_agent::core::self_restart::{
    RestartEnvironment, SelfRestartDependencies, SelfRestartRequest, restart_current_process,
};
use pi_js_compat::{
    abort::AbortReason,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};
use serde_json::json;
use std::sync::{Arc, Mutex};
#[path = "support/oracle.rs"]
mod oracle;

fn object<const N: usize>(fields: [(&str, JsValue); N]) -> JsValue {
    JsValue::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn text(value: &str) -> JsValue {
    JsValue::String(value.into())
}
fn string_field(value: &JsValue, key: &str) -> JsString {
    value.get(key).unwrap().as_string().unwrap().clone()
}
fn error_wire(error: &CallbackError) -> JsValue {
    match error.thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => object([
            ("kind", text("error")),
            ("class", text(error.constructor_class().name())),
            ("value", error.value()),
            ("string", JsValue::String(error.string_value())),
        ]),
        AbortReason::Value(value) => object([("kind", text("value")), ("value", value)]),
    }
}
fn env_wire(env: &RestartEnvironment) -> JsValue {
    let pid = env.get("PI_SELF_RESTART_OLD_PID").unwrap();
    object([
        ("marker", JsValue::String(env["PI_SELF_RESTART_REQUEST"].clone())),
        ("session", JsValue::String(env["PI_SELF_RESTART_SESSION"].clone())),
        ("prompt", JsValue::String(env["PI_SELF_RESTART_PROMPT"].clone())),
        (
            "oldPid",
            JsValue::String(if pid == &JsString::from(std::process::id().to_string()) {
                "SELF".into()
            } else {
                pid.clone()
            }),
        ),
    ])
}
fn failure(request: &JsValue) -> CallbackError {
    let value = request.get("value").unwrap();
    if request.get("kind") == Some(&text("raw")) {
        return CallbackError::from_rejection(AbortReason::Value(value.clone()));
    }
    CallbackError::from_rejection(
        NativeNodeError::from_constructor(NativeErrorClass::RangeError, string_field(value, "message"))
            .with_property("code", value.get("code").unwrap().clone())
            .with_property("custom", value.get("custom").unwrap().clone())
            .into(),
    )
}
async fn run_restart(request: &JsValue) -> JsValue {
    let operation = string_field(request, "operation");
    let calls = Arc::new(Mutex::new(Vec::<JsValue>::new()));
    let error = failure(request);
    let execve = operation.starts_with("exec-").then(|| {
        let calls = Arc::clone(&calls);
        let operation = operation.clone();
        let error = error.clone();
        Arc::new(move |file: &JsString, args: &[JsString], env: &RestartEnvironment| {
            calls.lock().unwrap().push(object([
                ("op", text("exec")),
                (
                    "selfExecutable",
                    JsValue::Bool(file == &JsString::from(std::env::current_exe().unwrap().to_str().unwrap())),
                ),
                (
                    "args",
                    JsValue::Array(args.iter().cloned().map(JsValue::String).collect()),
                ),
                ("env", env_wire(env)),
            ]));
            if operation == "exec-throw" {
                Err(error.clone())
            } else {
                Ok(())
            }
        }) as pi_coding_agent::core::self_restart::RestartExecve
    });
    let spawn_calls = Arc::clone(&calls);
    let spawn_error = error.clone();
    let spawn_op = operation.clone();
    let exit_calls = Arc::clone(&calls);
    let exit_error = error.clone();
    let exit_op = operation.clone();
    let dependencies = SelfRestartDependencies {
        argv: Some(
            request
                .get("argv")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_string().unwrap().clone())
                .collect(),
        ),
        argv0: Some(string_field(request, "argv0")),
        exec_argv: Some(
            request
                .get("execArgv")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_string().unwrap().clone())
                .collect(),
        ),
        execve,
        spawn_self_restart: Some(Arc::new(move |input, dependencies| {
            spawn_calls.lock().unwrap().push(object([
                ("op", text("spawnRestart")),
                (
                    "request",
                    object([
                        ("sessionFile", JsValue::String(input.session_file)),
                        ("prompt", JsValue::String(input.prompt.unwrap())),
                        ("oldPidIsSelf", JsValue::Bool(input.old_pid == Some(std::process::id()))),
                    ]),
                ),
                (
                    "options",
                    object([
                        ("waitForExit", JsValue::Bool(dependencies.wait_for_exit.unwrap())),
                        ("hasArgv", JsValue::Bool(dependencies.argv.is_some())),
                        ("hasArgv0", JsValue::Bool(dependencies.argv0.is_some())),
                        ("hasExecArgv", JsValue::Bool(dependencies.exec_argv.is_some())),
                    ]),
                ),
            ]));
            let result = if spawn_op == "spawn-throw" {
                Err(spawn_error.clone())
            } else {
                Ok(17)
            };
            Box::pin(async move { result })
        })),
        exit: Some(Arc::new(move |code| {
            exit_calls.lock().unwrap().push(object([
                ("op", text("exit")),
                ("code", JsValue::Number(f64::from(code))),
            ]));
            if exit_op == "exit-throw" {
                Err(exit_error.clone())
            } else {
                Ok(())
            }
        })),
        ..Default::default()
    };
    let result = restart_current_process(
        SelfRestartRequest {
            session_file: string_field(request, "sessionFile"),
            prompt: Some(string_field(request, "prompt")),
            old_pid: Some(42),
        },
        &dependencies,
    )
    .await;
    let mut wire = object([("calls", JsValue::Array(calls.lock().unwrap().clone()))]);
    if let Err(error) = result {
        wire.as_object_mut().unwrap().insert("error".into(), error_wire(&error));
    }
    wire
}

const PRIVATE_SPAWN_FAILURE: &str = "PI_PRIVATE_RESTART_SPAWN_FAILURE";

#[test]
fn actual_default_spawn_failure_child() {
    use std::os::unix::fs::PermissionsExt;
    let Some(root) = std::env::var_os(PRIVATE_SPAWN_FAILURE) else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let executable = std::env::current_exe().unwrap();
    std::fs::write(
        root.join("image.json"),
        json!({"file":executable,"args":std::env::args().skip(1).collect::<Vec<_>>()} ).to_string(),
    )
    .unwrap();
    let error_file = root.join("error.json");
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    pi_coding_agent::utils::uncaught::set_process_crash_handler(Some(Arc::new(move |payload| {
        let native = if let Some(error) = payload.downcast_ref::<CallbackError>() {
            match error.thrown_value() {
                AbortReason::Error(error) | AbortReason::DomException(error) => Some(error),
                AbortReason::Value(_) => None,
            }
        } else {
            payload.downcast_ref::<NativeNodeError>().cloned()
        };
        let wire = match native {
            Some(error) => {
                json!({"error":error.value().to_serde().unwrap(),"string":error.string_value().to_utf8().unwrap()})
            }
            None => {
                json!({"unexpectedPayload":pi_coding_agent::utils::uncaught::format_uncaught(payload).to_utf8().unwrap()})
            }
        };
        std::fs::write(&error_file, wire.to_string()).unwrap();
    })));
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o600)).unwrap();
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    executor.block_on(async {
        let request = SelfRestartRequest {
            session_file: root.join("session.jsonl").to_str().unwrap().into(),
            prompt: None,
            old_pid: None,
        };
        let dependencies = SelfRestartDependencies {
            wait_for_exit: Some(false),
            ..Default::default()
        };
        match pi_coding_agent::core::self_restart::spawn_self_restart(&request, &dependencies).await {
            Ok(code) => std::fs::write(root.join("returned.json"), json!({"code":code}).to_string()).unwrap(),
            Err(error) => {
                std::fs::write(
                    root.join("caught.json"),
                    pi_js_compat::js_json::stringify_lossless(&error_wire(&error)),
                )
                .unwrap();
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        panic!("deferred default-spawn error was not uncaught");
    });
}

fn assert_uncaught_spawn_fields(root: &std::path::Path, stderr: &str) -> serde_json::Value {
    let image: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join("image.json")).unwrap()).unwrap();
    let reported: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join("error.json")).unwrap()).unwrap();
    let error = &reported["error"];
    let executable = image["file"].as_str().unwrap();
    assert_eq!(error["name"], "Error");
    assert_eq!(error["code"], "EACCES");
    assert_eq!(error["errno"], -13);
    assert_eq!(error["syscall"], format!("spawn {executable}"));
    assert_eq!(error["path"], executable);
    assert_eq!(error["spawnargs"], image["args"]);
    assert_eq!(error["message"], format!("spawn {executable} EACCES"));
    let label = format!("Error: spawn {executable} EACCES");
    assert_eq!(reported["string"], label);
    assert!(stderr.contains(&label), "{stderr}");
    let returned: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("returned.json")).unwrap()).unwrap();
    assert_eq!(returned["code"], 0);
    json!({"name":error["name"],"code":error["code"],"errno":error["errno"]})
}

#[test]
fn no_wait_default_spawn_error_is_uncaught_like_frozen() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let temp = tempfile::tempdir().unwrap();
    let source_root = temp.path().join("source");
    let native_root = temp.path().join("native");
    std::fs::create_dir(&source_root).unwrap();
    std::fs::create_dir(&native_root).unwrap();
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    let mut source = oracle::spawn_oracle_process(
        &lock,
        "self-restart-default-oracle.mts",
        "packages/coding-agent/src",
        &[("PI_SELF_RESTART_REQUEST", "0".into())],
        Stdio::piped(),
    );
    writeln!(
        source.stdin.take().unwrap(),
        "{}",
        json!({"root":source_root,"operation":"spawn-default-failure"})
    )
    .unwrap();
    let source_output = source.wait_with_output().unwrap();
    assert!(source_output.status.success(), "source fixture: {source_output:?}");
    let outcome: serde_json::Value =
        serde_json::from_slice(&std::fs::read(source_root.join("outcome.json")).unwrap()).unwrap();
    assert_eq!(outcome["code"], 1, "source must exit uncaught");
    let expected = assert_uncaught_spawn_fields(&source_root, outcome["stderr"].as_str().unwrap());
    let executable = native_root.join("native-copy");
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let output = Command::new(&executable)
        .current_dir(&native_root)
        .args(["--exact", "actual_default_spawn_failure_child", "--nocapture"])
        .env(PRIVATE_SPAWN_FAILURE, &native_root)
        .env_remove("PI_SELF_RESTART_REQUEST")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "no-wait launch failure must not become a caught return: {output:?}"
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(assert_uncaught_spawn_fields(&native_root, &stderr), expected);
}

#[tokio::test]
async fn restart_dependencies_and_rejections_match_actual_frozen_module() {
    let mut source = oracle::TsOracle::spawn("self-restart-process-oracle.mts", "packages/coding-agent/src");
    for operation in ["exec-return", "exec-throw", "spawn-return", "spawn-throw", "exit-throw"] {
        for kind in ["native", "raw"] {
            let request=pi_js_compat::js_json::parse_lossless(&json!({"operation":operation,"kind":kind,"sessionFile":"session","prompt":"notice","oldPid":42,"argv":["ignored","/$bunfs/root/boot","/~BUN/root/boot","/%7EBUN/root/boot","script","arg"],"argv0":"pi","execArgv":["--import","loader"],"value":{"message":"failure","code":"E_RESTART","custom":{"x":17}}}).to_string()).unwrap();
            let expected = source.request(&request);
            assert_eq!(run_restart(&request).await, expected, "{operation}/{kind}");
        }
    }
}
