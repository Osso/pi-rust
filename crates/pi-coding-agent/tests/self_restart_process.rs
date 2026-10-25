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
#[path = "support/self_restart_numeric.rs"]
mod numeric;
use crate::shared_oracle as oracle;
use numeric::{fixture_exit_code, number_flags};

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
    let returned_code = fixture_exit_code(request).unwrap_or(0.0);
    let record_number = request.get("resultCode").is_some();
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
                Ok(returned_code)
            };
            Box::pin(async move { result })
        })),
        exit: Some(Arc::new(move |code| {
            let mut call = object([("op", text("exit")), ("code", JsValue::Number(code))]);
            if record_number {
                call.as_object_mut()
                    .unwrap()
                    .insert("number".into(), number_flags(code));
            }
            exit_calls.lock().unwrap().push(call);
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
    let exit_code = fixture_exit_code(request);
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
                    let is_error = exit_op == "helper-error" || exit_op == "helper-nowait-error";
                    exit_calls.lock().unwrap().push(object([(
                        "op",
                        text(if is_error { "errorEvent" } else { "exitEvent" }),
                    )]));
                    if is_error { Err(exit_error) } else { Ok(exit_code) }
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
                .insert("result".into(), JsValue::Number(code));
            if request.get("resultCode").is_some() {
                wire.as_object_mut()
                    .unwrap()
                    .insert("number".into(), number_flags(code));
            }
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

const PRIVATE_FAKE_ERROR: &str = "PI_PRIVATE_RESTART_FAKE_ERROR";
#[test]
fn unobserved_fake_child_error_entry() {
    let Some(root) = std::env::var_os(PRIVATE_FAKE_ERROR) else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let request =
        pi_js_compat::js_json::parse_lossless(std::fs::read_to_string(root.join("input.json")).unwrap()).unwrap();
    let file = root.join("error.json");
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    pi_coding_agent::utils::uncaught::set_process_crash_handler(Some(Arc::new(move |payload| {
        let error = payload.downcast_ref::<CallbackError>().unwrap();
        std::fs::write(&file, pi_js_compat::js_json::stringify_lossless(&error_wire(error))).unwrap();
    })));
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    executor.block_on(async {
        let response = run_helper(&request).await;
        std::fs::write(
            root.join("returned.json"),
            pi_js_compat::js_json::stringify_lossless(&response),
        )
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        panic!("unobserved child error was not uncaught");
    });
}

#[test]
fn unobserved_child_error_preserves_native_or_wraps_raw_like_frozen() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let temp = tempfile::tempdir().unwrap();
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    for kind in ["native", "raw"] {
        let root = temp.path().join(kind);
        let native_root = root.join("native");
        let source_root = root.join("source");
        std::fs::create_dir_all(&native_root).unwrap();
        std::fs::create_dir_all(&source_root).unwrap();
        let mut request = fixture_request("helper-nowait-error", kind, 1);
        std::fs::write(
            native_root.join("input.json"),
            pi_js_compat::js_json::stringify_lossless(&request),
        )
        .unwrap();
        request.as_object_mut().unwrap().insert(
            "monitorFile".into(),
            JsValue::String(source_root.join("error.json").to_str().unwrap().into()),
        );
        let mut source = oracle::spawn_oracle_process(
            &lock,
            "self-restart-process-oracle.mts",
            "packages/coding-agent/src",
            &[],
            Stdio::piped(),
        );
        writeln!(
            source.stdin.take().unwrap(),
            "{}",
            pi_js_compat::js_json::stringify_lossless(&request)
        )
        .unwrap();
        let output = source.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(1), "source: {output:?}");
        let source_return = String::from_utf8(output.stdout).unwrap();
        let expected_return = pi_js_compat::js_json::parse_lossless(source_return.trim()).unwrap();
        let expected =
            pi_js_compat::js_json::parse_lossless(std::fs::read_to_string(source_root.join("error.json")).unwrap())
                .unwrap();
        assert_eq!(expected.get("kind"), Some(&text("error")));
        if kind == "raw" {
            assert_eq!(
                expected.get("value").unwrap().get("code"),
                Some(&text("ERR_UNHANDLED_ERROR"))
            );
        }
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "unobserved_fake_child_error_entry", "--nocapture"])
            .env(PRIVATE_FAKE_ERROR, &native_root)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "native: {output:?}");
        let actual =
            pi_js_compat::js_json::parse_lossless(std::fs::read_to_string(native_root.join("error.json")).unwrap())
                .unwrap();
        let actual_return =
            pi_js_compat::js_json::parse_lossless(std::fs::read_to_string(native_root.join("returned.json")).unwrap())
                .unwrap();
        assert_same_wire(&actual_return, &expected_return, &format!("no-wait return/{kind}"));
        assert_same_wire(&actual, &expected, &format!("unhandled error/{kind}"));
    }
}

const PRIVATE_VALIDATION: &str = "PI_PRIVATE_RESTART_VALIDATION";
#[test]
fn actual_default_validation_child() {
    let Some(input) = std::env::var_os(PRIVATE_VALIDATION) else {
        return;
    };
    let input = std::path::PathBuf::from(input);
    let root = input.parent().unwrap();
    let request = pi_js_compat::js_json::parse_lossless(std::fs::read_to_string(&input).unwrap()).unwrap();
    for (key, _) in pi_js_compat::process_env::vars() {
        pi_js_compat::process_env::remove(&key);
    }
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
        argv0: Some(string_field(&request, "argv0")),
        exec_argv: Some(vec![]),
        ..Default::default()
    };
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let error = executor.block_on(async {
        if request.get("operation") == Some(&text("real-exec")) {
            restart_current_process(restart_request(&request), &dependencies)
                .await
                .unwrap_err()
        } else {
            pi_coding_agent::core::self_restart::spawn_self_restart(&restart_request(&request), &dependencies)
                .await
                .unwrap_err()
        }
    });
    let wire = object([
        ("error", error_wire(&error)),
        ("executable", text(std::env::current_exe().unwrap().to_str().unwrap())),
    ]);
    std::fs::write(
        root.join("result.json"),
        pi_js_compat::js_json::stringify_lossless(&wire),
    )
    .unwrap();
}

fn normalize_executable_error(mut wire: JsValue) -> JsValue {
    let executable = string_field(&wire, "executable").to_utf8().unwrap();
    let error = wire
        .as_object_mut()
        .unwrap()
        .get_mut("error")
        .unwrap()
        .as_object_mut()
        .unwrap();
    for key in ["message", "syscall", "path"] {
        if let Some(value) = error.get_mut("value").unwrap().as_object_mut().unwrap().get_mut(key) {
            // Native boundary diagnostics inspect raw units as escape sequences;
            // these messages are scalar, never projected from raw callback text.
            *value = text(
                &value
                    .as_string()
                    .unwrap()
                    .to_utf8()
                    .unwrap()
                    .replace(&executable, "<SELF>"),
            );
        }
    }
    let string = error.get_mut("string").unwrap();
    *string = text(
        &string
            .as_string()
            .unwrap()
            .to_utf8()
            .unwrap()
            .replace(&executable, "<SELF>"),
    );
    wire.as_object_mut().unwrap().shift_remove("executable");
    wire
}

#[test]
fn native_validation_and_os_error_boundary_matches_frozen() {
    use std::process::{Command, Stdio};
    let temp = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("self-restart-process-oracle.mts", "packages/coding-agent/src");
    for operation in ["real-spawn", "real-exec"] {
        for case in ["argument", "environment", "too-big"] {
            let root = temp.path().join(format!("{operation}-{case}"));
            std::fs::create_dir(&root).unwrap();
            let mut request = fixture_request(operation, "native", 0);
            let fields = request.as_object_mut().unwrap();
            fields.insert("execArgv".into(), JsValue::Array(vec![]));
            let argument = if case == "argument" {
                JsValue::String(JsString::from_utf16(vec![0x78, 0xd800, 0, 0xdc00]))
            } else if case == "too-big" {
                text(&"X".repeat(200_000))
            } else {
                text("safe")
            };
            fields.insert(
                "argv".into(),
                JsValue::Array(vec![text("ignored"), text("script"), argument]),
            );
            if case == "environment" {
                fields.insert("sessionFile".into(), text(&"session-".repeat(30)));
                fields.insert(
                    "prompt".into(),
                    JsValue::String(JsString::from_utf16(vec![0xd800, 0, 0xdc00])),
                );
            }
            let expected = normalize_executable_error(source.request(&request));
            let expected_error = expected.get("error").unwrap().get("value").unwrap();
            assert_eq!(
                expected_error.get("code"),
                Some(&text(if case == "too-big" {
                    if operation == "real-exec" { "" } else { "E2BIG" }
                } else {
                    "ERR_INVALID_ARG_VALUE"
                })),
                "{operation}/{case}: {expected:?}"
            );
            let input = root.join("input.json");
            std::fs::write(&input, pi_js_compat::js_json::stringify_lossless(&request)).unwrap();
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "actual_default_validation_child", "--nocapture"])
                .env(PRIVATE_VALIDATION, &input)
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert!(output.status.success(), "{operation}/{case}: {output:?}");
            let actual = normalize_executable_error(
                pi_js_compat::js_json::parse_lossless(std::fs::read_to_string(root.join("result.json")).unwrap())
                    .unwrap(),
            );
            assert_eq!(actual, expected, "{operation}/{case}");
        }
    }
}

#[test]
fn environment_order_boundary_matches_actual_frozen_builder() {
    use pi_coding_agent::core::self_restart::{
        ENV_SELF_RESTART_OLD_PID, ENV_SELF_RESTART_PROMPT, ENV_SELF_RESTART_REQUEST, build_self_restart_env,
    };
    let mut source = oracle::TsOracle::spawn("self-restart-process-oracle.mts", "packages/coding-agent/src");
    let request = fixture_request("environment-order", "native", 1);
    let expected = source.request(&request);
    let base = RestartEnvironment::from([
        ("PI_RESTART_ENV_Z".into(), "z".into()),
        (ENV_SELF_RESTART_PROMPT.into(), "stale".into()),
        ("PI_RESTART_ENV_A".into(), "a".into()),
        (ENV_SELF_RESTART_REQUEST.into(), "stale".into()),
        ("10".into(), "ten".into()),
        ("2".into(), "two".into()),
    ]);
    let input = SelfRestartRequest {
        old_pid: Some(777),
        ..restart_request(&request)
    };
    let env = build_self_restart_env(&input, &base, 777);
    let actual = JsValue::Array(
        env.into_iter()
            .map(|(key, value)| {
                JsValue::Array(vec![
                    JsValue::String(key.clone()),
                    if key == ENV_SELF_RESTART_OLD_PID {
                        text("SELF")
                    } else {
                        JsValue::String(value)
                    },
                ])
            })
            .collect(),
    );
    assert_same_wire(&actual, &expected, "environment enumeration/overwrite/raw values");
}

const PRIVATE_STDIO: &str = "PI_PRIVATE_RESTART_STDIO";
fn stdio_snapshot(root: &std::path::Path) -> serde_json::Value {
    serde_json::Value::Array((0..3).map(|fd| {
        let target=match std::fs::read_link(format!("/proc/self/fd/{fd}")) {Ok(path)=>path,Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return json!({"open":false}),Err(error)=>panic!("read descriptor: {error}")};
        let info=std::fs::read_to_string(format!("/proc/self/fdinfo/{fd}")).unwrap();
        let flags=info.lines().find_map(|line|line.strip_prefix("flags:")).unwrap().trim();
        let flags=u32::from_str_radix(flags,8).unwrap();
        json!({"open":true,"targetMatches":target==root.join(format!("stdio-{fd}")),"closeOnExec":flags & 0o2000000 != 0})
    }).collect())
}
fn replace_stdio(root: &std::path::Path) {
    use std::os::fd::AsRawFd;
    std::fs::write(root.join("stdio-0"), "").unwrap();
    for fd in 0..3 {
        nix::unistd::close(fd).unwrap();
        let path = root.join(format!("stdio-{fd}"));
        let file = if fd == 0 {
            std::fs::File::open(path).unwrap()
        } else {
            std::fs::File::create(path).unwrap()
        };
        assert_eq!(file.as_raw_fd(), fd);
        // The private process's standard stream now owns this descriptor.
        std::mem::forget(file);
    }
}
#[test]
fn actual_default_stdio_child() {
    use std::io::Write;
    let Some(root) = std::env::var_os(PRIVATE_STDIO) else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    if std::env::var("PI_SELF_RESTART_REQUEST").as_deref() == Ok("1") {
        std::io::stdout().write_all(b"stdout preserved\n").unwrap();
        std::io::stdout().flush().unwrap();
        std::io::stderr().write_all(b"stderr preserved\n").unwrap();
        std::io::stderr().flush().unwrap();
        std::fs::write(
            root.join("fd-after.json"),
            json!({"phase":"exec","pid":std::process::id(),"stdio":stdio_snapshot(&root)}).to_string(),
        )
        .unwrap();
        std::process::exit(0);
    }
    let mode = std::env::var("PI_PRIVATE_RESTART_STDIO_MODE").unwrap();
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    executor.block_on(async { tokio::time::sleep(std::time::Duration::from_millis(1)).await });
    replace_stdio(&root);
    if mode == "closed" {
        nix::unistd::close(1).unwrap();
    }
    std::fs::write(
        root.join("fd-before.json"),
        json!({"pid":std::process::id(),"stdio":stdio_snapshot(&root)}).to_string(),
    )
    .unwrap();
    let executable = std::env::current_exe().unwrap();
    let request = SelfRestartRequest {
        session_file: root.join("session.jsonl").to_str().unwrap().into(),
        prompt: Some(json!({"proof":"fd-exec","root":root}).to_string().into()),
        old_pid: None,
    };
    let dependencies = if mode == "failure" {
        SelfRestartDependencies {
            argv: Some(vec!["ignored".into(), "X".repeat(200000).into()]),
            exec_argv: Some(vec![]),
            ..Default::default()
        }
    } else {
        SelfRestartDependencies::default()
    };
    let error = executor
        .block_on(restart_current_process(request, &dependencies))
        .unwrap_err();
    let native = match error.thrown_value() {
        AbortReason::Error(error) => error,
        other => panic!("unexpected rejection {other:?}"),
    };
    std::fs::write(root.join("fd-after.json"),json!({"phase":"caught","stdio":stdio_snapshot(&root),"error":native.value().to_serde().unwrap(),"string":native.string_value().to_utf8().unwrap(),"executable":executable}).to_string()).unwrap();
    // In closed-stdout mode the test harness cannot print its usual summary.
    std::process::exit(0);
}
fn normalize_fd_report(mut report: serde_json::Value) -> serde_json::Value {
    if let Some(executable) = report["executable"].as_str().map(str::to_owned) {
        for key in ["message", "path"] {
            if let Some(value) = report["error"][key].as_str() {
                report["error"][key] = json!(value.replace(&executable, "<SELF>"));
            }
        }
        report["string"] = json!(report["string"].as_str().unwrap().replace(&executable, "<SELF>"));
    }
    if report["phase"] == "exec" {
        for stream in report["stdio"].as_array_mut().unwrap() {
            stream.as_object_mut().unwrap().remove("closeOnExec");
        }
    }
    report.as_object_mut().unwrap().remove("executable");
    report.as_object_mut().unwrap().remove("pid");
    report
}
#[test]
fn stdio_cloexec_success_failure_and_fcntl_boundary_matches_frozen() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let temp = tempfile::tempdir().unwrap();
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    for mode in ["success", "failure", "closed"] {
        let source_root = temp.path().join(format!("source-{mode}"));
        let native_root = temp.path().join(format!("native-{mode}"));
        std::fs::create_dir(&source_root).unwrap();
        std::fs::create_dir(&native_root).unwrap();
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
            json!({"root":source_root,"operation":"fd-exec","mode":mode})
        )
        .unwrap();
        let output = source.wait_with_output().unwrap();
        assert!(output.status.success(), "source/{mode}: {output:?}");
        let before: serde_json::Value =
            serde_json::from_slice(&std::fs::read(source_root.join("fd-before.json")).unwrap()).unwrap();
        let after: serde_json::Value =
            serde_json::from_slice(&std::fs::read(source_root.join("fd-after.json")).unwrap()).unwrap();
        for fd in [0, 2] {
            assert_eq!(before["stdio"][fd]["closeOnExec"], true);
        }
        if mode == "success" {
            assert_eq!(after["pid"], before["pid"]);
            assert_eq!(after["phase"], "exec");
            for fd in 0..3 {
                assert_eq!(after["stdio"][fd]["closeOnExec"], true, "Node bootstrap re-marks stdio");
            }
            for fd in [1, 2] {
                assert!(
                    std::fs::read_to_string(source_root.join(format!("stdio-{fd}")))
                        .unwrap()
                        .contains(if fd == 1 {
                            "stdout preserved"
                        } else {
                            "stderr preserved"
                        })
                );
            }
        } else {
            assert_eq!(after["phase"], "caught");
            assert_eq!(after["stdio"], before["stdio"]);
            assert_eq!(
                after["error"]["syscall"],
                if mode == "closed" { "fcntl" } else { "execve" }
            );
        }
        let expected = normalize_fd_report(after);
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "actual_default_stdio_child", "--nocapture"])
            .env(PRIVATE_STDIO, &native_root)
            .env("PI_PRIVATE_RESTART_STDIO_MODE", mode)
            .env_remove("PI_SELF_RESTART_REQUEST")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "native/{mode}: {output:?}");
        let actual_before: serde_json::Value =
            serde_json::from_slice(&std::fs::read(native_root.join("fd-before.json")).unwrap()).unwrap();
        let actual_after: serde_json::Value =
            serde_json::from_slice(&std::fs::read(native_root.join("fd-after.json")).unwrap()).unwrap();
        assert_eq!(actual_before["stdio"], before["stdio"]);
        if mode == "success" {
            assert_eq!(actual_after["pid"], pid);
            for fd in 0..3 {
                assert_eq!(
                    actual_after["stdio"][fd]["closeOnExec"], false,
                    "native image keeps the cleared kernel flags; approved bootstrap gap"
                );
            }
            for fd in [1, 2] {
                assert!(
                    std::fs::read_to_string(native_root.join(format!("stdio-{fd}")))
                        .unwrap()
                        .contains(if fd == 1 {
                            "stdout preserved"
                        } else {
                            "stderr preserved"
                        })
                );
            }
        }
        assert_eq!(normalize_fd_report(actual_after), expected, "{mode}");
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
    assert_eq!(code, 0.0);
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
            Ok(code) => std::fs::write(
                root.join("returned.json"),
                pi_js_compat::js_json::stringify_lossless(&object([("code", JsValue::Number(code))])),
            )
            .unwrap(),
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

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
