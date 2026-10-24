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
                ("request", handoff_wire(input)),
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
    let result = restart_current_process(restart_request(request), &dependencies).await;
    let mut wire = object([("calls", JsValue::Array(calls.lock().unwrap().clone()))]);
    if let Err(error) = result {
        wire.as_object_mut().unwrap().insert("error".into(), error_wire(&error));
    }
    wire
}

fn fixture_request(operation: &str, kind: &str, variant: u8) -> JsValue {
    let mut request = JsValue::from(
        json!({"operation":operation,"kind":kind,"sessionFile":"session","prompt":"notice","oldPid":42,"argv":["ignored","/$bunfs/root/boot","/~BUN/root/boot","/%7EBUN/root/boot","script","arg"],"argv0":"pi","execArgv":["--import","loader"],"value":{"message":"failure","code":"E_RESTART","custom":{"x":17}}}),
    );
    let fields = request.as_object_mut().unwrap();
    if variant == 1 {
        let raw = JsValue::String(JsString::from_utf16(vec![0x78, 0xd800, 0, 0xdc00]));
        fields.insert("sessionFile".into(), raw.clone());
        fields.insert("prompt".into(), raw.clone());
        fields.insert("argv0".into(), raw.clone());
        fields
            .get_mut("argv")
            .unwrap()
            .as_array_mut()
            .unwrap()
            .push(raw.clone());
        fields
            .get_mut("execArgv")
            .unwrap()
            .as_array_mut()
            .unwrap()
            .push(raw.clone());
        let failure = fields.get_mut("value").unwrap().as_object_mut().unwrap();
        failure.insert("message".into(), raw.clone());
        failure.insert("custom".into(), object([("raw", raw)]));
    }
    if variant == 2 {
        fields.insert("prompt".into(), text(""));
        fields.insert("argv0".into(), text(""));
        fields.insert(
            "argv".into(),
            JsValue::Array(vec![text("ignored"), text("/$bunfs/root/only")]),
        );
        fields.insert("execArgv".into(), JsValue::Array(vec![]));
        fields.insert("oldPid".into(), JsValue::Number(0.0));
    }
    if variant == 3 {
        fields.shift_remove("prompt");
        fields.shift_remove("oldPid");
    }
    request
}

fn restart_request(request: &JsValue) -> SelfRestartRequest {
    SelfRestartRequest {
        session_file: string_field(request, "sessionFile"),
        prompt: request.get("prompt").map(|value| value.as_string().unwrap().clone()),
        old_pid: request
            .get("oldPid")
            .map(|value| u32::try_from(value.as_u64().unwrap()).unwrap()),
    }
}

fn handoff_wire(input: SelfRestartRequest) -> JsValue {
    let mut fields = pi_js_compat::js_json::JsObject::new();
    fields.insert("sessionFile".into(), JsValue::String(input.session_file));
    if let Some(prompt) = input.prompt {
        fields.insert("prompt".into(), JsValue::String(prompt));
    }
    fields.insert(
        "oldPidIsSelf".into(),
        JsValue::Bool(input.old_pid == Some(std::process::id())),
    );
    JsValue::Object(fields)
}

async fn run_helper(request: &JsValue) -> JsValue {
    use pi_coding_agent::core::self_restart::{
        RestartChildProcess, RestartSpawn, RestartSpawnOptions, RestartUnref, spawn_self_restart,
    };
    let calls = Arc::new(Mutex::new(Vec::<JsValue>::new()));
    let operation = string_field(request, "operation");
    let error = failure(request);
    let spawn_calls = Arc::clone(&calls);
    let spawn_op = operation.clone();
    let spawn: RestartSpawn = Arc::new(
        move |file: &JsString, args: &[JsString], options: RestartSpawnOptions| {
            spawn_calls.lock().unwrap().push(object([
                ("op", text("spawn")),
                (
                    "selfExecutable",
                    JsValue::Bool(file == &JsString::from(std::env::current_exe().unwrap().to_str().unwrap())),
                ),
                (
                    "args",
                    JsValue::Array(args.iter().cloned().map(JsValue::String).collect()),
                ),
                (
                    "cwdMatches",
                    JsValue::Bool(options.cwd == pi_js_compat::process_cwd::cwd().unwrap()),
                ),
                ("env", env_wire(&options.env)),
                ("stdio", text(options.stdio)),
            ]));
            if spawn_op == "helper-spawn-throw" {
                return Err(error.clone());
            }
            let unref_calls = Arc::clone(&spawn_calls);
            let unref_op = spawn_op.clone();
            let unref_error = error.clone();
            let unref = (spawn_op != "helper-nowait-no-unref").then(|| {
                Arc::new(move || {
                    unref_calls.lock().unwrap().push(object([("op", text("unref"))]));
                    if unref_op == "helper-nowait-unref-throw" {
                        Err(unref_error.clone())
                    } else {
                        Ok(())
                    }
                }) as RestartUnref
            });
            let exit_calls = Arc::clone(&spawn_calls);
            let exit_op = spawn_op.clone();
            let exit_error = error.clone();
            Ok(RestartChildProcess {
                unref,
                exit: Box::pin(async move {
                    let is_error = exit_op == "helper-error";
                    exit_calls.lock().unwrap().push(object([(
                        "op",
                        text(if is_error { "errorEvent" } else { "exitEvent" }),
                    )]));
                    if is_error {
                        Err(exit_error)
                    } else {
                        Ok((exit_op != "helper-null").then_some(17))
                    }
                }),
            })
        },
    );
    let dependencies = SelfRestartDependencies {
        argv: Some(
            request
                .get("argv")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_string().unwrap().clone())
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
                .map(|value| value.as_string().unwrap().clone())
                .collect(),
        ),
        spawn: Some(spawn),
        wait_for_exit: operation.starts_with("helper-nowait").then_some(false),
        ..Default::default()
    };
    let result = spawn_self_restart(&restart_request(request), &dependencies).await;
    let mut wire = object([("calls", JsValue::Array(calls.lock().unwrap().clone()))]);
    match result {
        Ok(code) => {
            wire.as_object_mut()
                .unwrap()
                .insert("result".into(), JsValue::Number(f64::from(code)));
        }
        Err(error) => {
            wire.as_object_mut().unwrap().insert("error".into(), error_wire(&error));
        }
    }
    wire
}

fn assert_same_wire(actual: &JsValue, expected: &JsValue, label: &str) {
    assert_eq!(
        pi_js_compat::js_json::stringify_lossless(actual),
        pi_js_compat::js_json::stringify_lossless(expected),
        "{label}"
    );
}

#[tokio::test]
async fn child_dependencies_and_rejections_match_actual_frozen_module() {
    let mut source = oracle::TsOracle::spawn("self-restart-process-oracle.mts", "packages/coding-agent/src");
    for operation in [
        "helper-exit",
        "helper-null",
        "helper-error",
        "helper-spawn-throw",
        "helper-nowait",
        "helper-nowait-no-unref",
        "helper-nowait-unref-throw",
    ] {
        for kind in ["native", "raw"] {
            for variant in 0..4 {
                let request = fixture_request(operation, kind, variant);
                let expected = source.request(&request);
                assert_same_wire(
                    &run_helper(&request).await,
                    &expected,
                    &format!("{operation}/{kind}/{variant}"),
                );
            }
        }
    }
}

const PRIVATE_SPAWN_SUCCESS: &str = "PI_PRIVATE_RESTART_SPAWN_SUCCESS";
#[test]
fn actual_default_spawn_success_child() {
    let Some(root) = std::env::var_os(PRIVATE_SPAWN_SUCCESS) else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    if std::env::var("PI_SELF_RESTART_REQUEST").as_deref() == Ok("1") {
        let proof: serde_json::Value = serde_json::from_str(&std::env::var("PI_SELF_RESTART_PROMPT").unwrap()).unwrap();
        let before: serde_json::Value =
            serde_json::from_slice(&std::fs::read(proof["before"].as_str().unwrap()).unwrap()).unwrap();
        let session = std::env::var("PI_SELF_RESTART_SESSION").unwrap();
        let value = json!({"pid":std::process::id(),"samePid":before["pid"]==json!(std::process::id()),"oldPidMatches":std::env::var("PI_SELF_RESTART_OLD_PID").unwrap()==before["pid"].as_u64().unwrap().to_string(),"cwdRetained":json!(std::env::current_dir().unwrap())==before["cwd"],"sessionExists":std::path::Path::new(&session).exists(),"marker":std::env::var("PI_SELF_RESTART_REQUEST").unwrap(),"removedEnvironment":std::env::var_os("PI_RESTART_REMOVE_ME").is_none()});
        std::fs::write(proof["after"].as_str().unwrap(), value.to_string()).unwrap();
        return;
    }
    assert_eq!(std::env::var("PI_RESTART_REMOVE_ME").unwrap(), "remove");
    pi_js_compat::process_env::remove("PI_RESTART_REMOVE_ME");
    let session = root.join("session.jsonl");
    std::fs::write(&session, "fixture").unwrap();
    let proof = json!({"proof":"spawn-default","before":root.join("before.json"),"after":root.join("after.json")});
    std::fs::write(
        root.join("before.json"),
        json!({"pid":std::process::id(),"cwd":std::env::current_dir().unwrap()}).to_string(),
    )
    .unwrap();
    let request = SelfRestartRequest {
        session_file: session.to_str().unwrap().into(),
        prompt: Some(proof.to_string().into()),
        old_pid: None,
    };
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let code = executor
        .block_on(pi_coding_agent::core::self_restart::spawn_self_restart(
            &request,
            &SelfRestartDependencies::default(),
        ))
        .unwrap();
    assert_eq!(code, 0);
}

#[test]
fn successful_default_spawn_matches_frozen_without_injected_hooks() {
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
        &[
            ("PI_SELF_RESTART_REQUEST", "0".into()),
            ("PI_RESTART_REMOVE_ME", "remove".into()),
        ],
        Stdio::piped(),
    );
    writeln!(
        source.stdin.take().unwrap(),
        "{}",
        json!({"root":source_root,"operation":"spawn-default"})
    )
    .unwrap();
    let output = source.wait_with_output().unwrap();
    assert!(output.status.success(), "source: {output:?}");
    let mut expected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(source_root.join("after.json")).unwrap()).unwrap();
    assert_eq!(expected["samePid"], false);
    assert_eq!(expected["removedEnvironment"], true);
    expected.as_object_mut().unwrap().remove("pid");
    let child = Command::new(std::env::current_exe().unwrap())
        .current_dir(&native_root)
        .args(["--exact", "actual_default_spawn_success_child", "--nocapture"])
        .env(PRIVATE_SPAWN_SUCCESS, &native_root)
        .env("PI_RESTART_REMOVE_ME", "remove")
        .env_remove("PI_SELF_RESTART_REQUEST")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let parent_pid = child.id();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "native: {output:?}");
    let mut actual: serde_json::Value =
        serde_json::from_slice(&std::fs::read(native_root.join("after.json")).unwrap()).unwrap();
    assert_ne!(actual["pid"], parent_pid, "actual spawn creates another kernel PID");
    actual.as_object_mut().unwrap().remove("pid");
    assert_eq!(actual, expected);
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
            for variant in 0..4 {
                let request = fixture_request(operation, kind, variant);
                let expected = source.request(&request);
                assert_same_wire(
                    &run_restart(&request).await,
                    &expected,
                    &format!("{operation}/{kind}/{variant}"),
                );
            }
        }
    }
}
