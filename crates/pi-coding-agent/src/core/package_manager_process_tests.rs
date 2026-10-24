//! Actual-source child-process boundaries for the private package-manager methods.

use super::*;
#[path = "../../tests/support/executable_script.rs"]
mod executable_script;
use crate::test_oracle as oracle;

const PROCESS_SCRIPT: &str = r#"#!/usr/bin/python3
import os,signal,sys,time
mode=sys.argv[1]
if mode=='bom': os.write(1,' \ufeffhello\ufeff \n'.encode())
elif mode=='nel': os.write(1,'\u0085hello\u0085'.encode())
elif mode=='invalid': os.write(1,b'\xff\xc0\xaf\xe0\x80\xa0')
elif mode=='split':
    os.write(1,b'\xf0\x9f')
    time.sleep(0.15)
    os.write(1,b'\x98\x80')
elif mode=='stderr-success': os.write(2,b' stderr-only \n')
elif mode=='stdout-failure':
    os.write(1,b'stdout\r\n')
    sys.exit(7)
elif mode=='stderr-failure':
    os.write(1,b'stdout')
    os.write(2,' stderr\u0085 '.encode())
    sys.exit(7)
elif mode=='signal': os.kill(os.getpid(),signal.SIGTERM)
elif mode=='wait': time.sleep(0.4)
elif mode=='descendant':
    if os.fork()==0:
        time.sleep(0.4)
        os.write(1,b'late')
        os._exit(0)
    os._exit(0)
"#;

fn normalize_result(result: Result<JsString, PackageManagerError>) -> JsValue {
    match result {
        Ok(stdout) => js_json!({"ok":true,"output":{"stdout":stdout}}),
        Err(error) => {
            let name = error
                .native_error()
                .map_or_else(|| JsString::from("Error"), |native| native.name.clone());
            let mut snapshot = js_json!({"name":name,"message":error.message()});
            if let Some(native) = error.native_error() {
                let value = native.value();
                for key in ["code", "errno", "syscall", "path", "spawnargs"] {
                    if let Some(property) = value.get(key) {
                        snapshot.as_object_mut().unwrap().insert(key.into(), property.clone());
                    }
                }
            }
            js_json!({"ok":false,"error":snapshot})
        }
    }
}

#[tokio::test]
async fn capture_and_sync_output_boundaries_match_frozen() {
    let directory = tempfile::tempdir().unwrap();
    let cwd: JsString = directory.path().to_str().unwrap().into();
    let agent = node_join(&cwd, "agent");
    let binary = directory.path().join("process");
    executable_script::write_executable_script(&binary, PROCESS_SCRIPT);
    let command: JsString = binary.to_str().unwrap().into();
    let settings = SettingsManager::in_memory(Settings::new(), Default::default());
    let manager = DefaultPackageManager::new(&cwd, &agent, Arc::new(SettingsManagerStore::new(settings))).unwrap();
    let mut source = oracle::TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let mut mismatches = Vec::new();
    let cases = [
        ("run_capture", "bom", None),
        ("run_capture", "nel", None),
        ("run_capture", "invalid", None),
        ("run_capture", "split", None),
        ("run_capture", "stderr-success", None),
        ("run_capture", "stdout-failure", None),
        ("run_capture", "stderr-failure", None),
        ("run_capture", "signal", None),
        ("run_capture", "wait", Some(80)),
        ("run_capture", "descendant", Some(80)),
        ("run_capture", "descendant", None),
        ("run_sync", "bom", None),
        ("run_sync", "nel", None),
        ("run_sync", "invalid", None),
        ("run_sync", "split", None),
        ("run_sync", "stderr-success", None),
        ("run_sync", "stdout-failure", None),
        ("run_sync", "stderr-failure", None),
    ];
    for (operation, mode, timeout_ms) in cases {
        let args: Vec<JsString> = vec![mode.into()];
        let mut options = js_json!({"cwd":cwd});
        if let Some(timeout) = timeout_ms {
            options["timeoutMs"] = js_json!(timeout);
        }
        let request = js_json!({"id":mode,"operation":operation,"command":command,"args":args,"options":options,
            "scenario":{"cwd":cwd,"agentDir":agent,"trusted":false}});
        let expected = source.request(&request);
        if mode == "descendant" && timeout_ms.is_some() {
            assert_eq!(
                expected["ok"],
                JsValue::Bool(false),
                "source must keep timeout active through descendant EOF: {expected:?}"
            );
        }
        let native = if operation == "run_sync" {
            manager.run_command_sync(&command, &args)
        } else {
            manager
                .run_command_capture(&command, &args, Some(&cwd), timeout_ms, &[])
                .await
        };
        let actual = normalize_result(native);
        let expected = if expected["ok"] == JsValue::Bool(true) {
            js_json!({"ok":true,"output":expected["output"]})
        } else {
            js_json!({"ok":false,"error":expected["error"]})
        };
        if actual != expected {
            mismatches.push(
                js_json!({"operation":operation,"mode":mode,"timeout":timeout_ms,"actual":actual,"expected":expected}),
            );
        }
    }
    assert!(
        mismatches.is_empty(),
        "{}",
        js_json::stringify_lossless(&js_json!(mismatches))
    );
}

#[derive(Clone)]
struct SpawnCase {
    label: &'static str,
    command: JsString,
    args: Vec<JsString>,
    cwd: JsString,
    env: Vec<(String, JsString)>,
}

fn spawn_validation_cases(command: &JsString, cwd: &JsString) -> Vec<SpawnCase> {
    let base = SpawnCase {
        label: "valid",
        command: command.clone(),
        args: Vec::new(),
        cwd: cwd.clone(),
        env: Vec::new(),
    };
    let long_argument = JsString::from("x".repeat(115)) + JsString::from_utf16(vec![0xd800, 0]) + "tail";
    vec![
        base.clone(),
        SpawnCase {
            label: "empty-file",
            command: "".into(),
            ..base.clone()
        },
        SpawnCase {
            label: "nul-file",
            command: "bad\0file".into(),
            args: vec!["bad\0arg".into()],
            ..base.clone()
        },
        SpawnCase {
            label: "nul-arg",
            args: vec!["bad\0argument".into()],
            ..base.clone()
        },
        SpawnCase {
            label: "long-arg",
            args: vec![long_argument],
            ..base.clone()
        },
        SpawnCase {
            label: "nul-cwd",
            cwd: "bad\0cwd".into(),
            env: vec![("TEST".into(), "bad\0env".into())],
            ..base.clone()
        },
        SpawnCase {
            label: "missing-cwd",
            cwd: node_join(cwd, "missing-cwd"),
            ..base.clone()
        },
        SpawnCase {
            label: "nul-env-value",
            env: vec![("TEST".into(), "bad\0env".into())],
            ..base.clone()
        },
        SpawnCase {
            label: "nul-env-key",
            env: vec![("BAD\0KEY".into(), "value".into())],
            ..base.clone()
        },
        SpawnCase {
            label: "missing-file",
            command: node_join(cwd, "missing-file"),
            args: vec!["argument".into()],
            ..base.clone()
        },
        SpawnCase {
            label: "not-executable",
            command: node_join(cwd, "not-executable"),
            ..base.clone()
        },
        SpawnCase {
            label: "not-directory",
            command: node_join(cwd, "not-executable/child"),
            ..base.clone()
        },
        SpawnCase {
            label: "raw-missing-file",
            command: node_join(cwd, &(JsString::from("missing-") + JsString::from_utf16(vec![0xd800]))),
            ..base.clone()
        },
        SpawnCase {
            label: "raw-existing-file",
            command: node_join(cwd, &(JsString::from("existing-") + JsString::from_utf16(vec![0xd800]))),
            ..base
        },
    ]
}

#[tokio::test]
async fn spawn_validation_and_exec_error_fields_match_frozen() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let cwd: JsString = directory.path().to_str().unwrap().into();
    let agent = node_join(&cwd, "agent");
    let binary = directory.path().join("valid");
    executable_script::write_executable_script(&binary, "#!/usr/bin/python3\n");
    executable_script::write_executable_script(&directory.path().join("existing-�"), "#!/usr/bin/python3\n");
    let not_executable = directory.path().join("not-executable");
    std::fs::write(&not_executable, b"not executable").unwrap();
    std::fs::set_permissions(&not_executable, std::fs::Permissions::from_mode(0o644)).unwrap();
    let settings = SettingsManager::in_memory(Settings::new(), Default::default());
    let manager = DefaultPackageManager::new(&cwd, &agent, Arc::new(SettingsManagerStore::new(settings))).unwrap();
    let mut source = oracle::TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let mut mismatches = Vec::new();
    for operation in ["run_command", "run_capture", "run_sync"] {
        for case in spawn_validation_cases(&binary.to_str().unwrap().into(), &cwd) {
            let environment = JsValue::Object(
                case.env
                    .iter()
                    .map(|(key, value)| (JsString::from(key.as_str()), JsValue::String(value.clone())))
                    .collect(),
            );
            let request = js_json!({"id":case.label,"operation":operation,"command":case.command,"args":case.args,
                "options":{"cwd":case.cwd,"env":environment},"scenario":{"cwd":cwd,"agentDir":agent,"trusted":false}});
            let expected = source.request(&request);
            let native = match operation {
                "run_command" => manager
                    .run_command(&case.command, &case.args, Some(&case.cwd))
                    .await
                    .map(|()| JsString::default()),
                "run_capture" => {
                    manager
                        .run_command_capture(&case.command, &case.args, Some(&case.cwd), None, &case.env)
                        .await
                }
                "run_sync" => manager.run_command_sync(&case.command, &case.args),
                _ => unreachable!(),
            };
            let actual = normalize_result(native);
            let expected = if expected["ok"] == JsValue::Bool(true) {
                js_json!({"ok":true,"output":expected["output"]})
            } else {
                js_json!({"ok":false,"error":expected["error"]})
            };
            if actual != expected {
                mismatches
                    .push(js_json!({"operation":operation,"case":case.label,"actual":actual,"expected":expected}));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "{}",
        js_json::stringify_lossless(&js_json!(mismatches))
    );
}

fn process_test_manager(directory: &Path) -> DefaultPackageManager {
    let cwd = pi_js_compat::node_fs::decode_node_path(directory.as_os_str());
    let settings = SettingsManager::in_memory(Settings::new(), Default::default());
    DefaultPackageManager::new(
        &cwd,
        &node_join(&cwd, "agent"),
        Arc::new(SettingsManagerStore::new(settings)),
    )
    .unwrap()
}

#[test]
fn environment_probe_child() {
    if std::env::var_os("PI_PACKAGE_ENV_CHILD").is_none() {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("env-probe");
    let result_path = directory.path().join("result");
    executable_script::write_executable_script(
        &binary,
        r#"#!/usr/bin/python3
import os,json,sys
keys=['PI_PACKAGE_ENV_MARKER','PI_PACKAGE_ENV_NEW','PI_PACKAGE_ENV_EMPTY']
data=json.dumps({k:os.environ.get(k) for k in keys},ensure_ascii=True,separators=(',',':'))
if len(sys.argv)>1: open(sys.argv[1],'w').write(data)
else: os.write(1,data.encode())
"#,
    );
    let manager = process_test_manager(directory.path());
    let cwd = pi_js_compat::node_fs::decode_node_path(directory.path().as_os_str());
    let command = pi_js_compat::node_fs::decode_node_path(binary.as_os_str());
    let result_file = pi_js_compat::node_fs::decode_node_path(result_path.as_os_str());
    let mut source = oracle::TsOracle::spawn(
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let environments = [
        js_json!({"PI_PACKAGE_ENV_MARKER":"overlay","PI_PACKAGE_ENV_NEW":"new","PI_PACKAGE_ENV_EMPTY":""}),
        js_json!({}),
    ];
    let mut mismatches = Vec::new();
    for environment in environments {
        for (name, _) in pi_js_compat::process_env::vars() {
            pi_js_compat::process_env::remove(&name);
        }
        for (name, value) in environment.as_object().unwrap() {
            pi_js_compat::process_env::set(&name.to_utf8().unwrap(), &value.as_string().unwrap().to_utf8().unwrap());
        }
        for operation in ["run_command", "run_capture", "run_sync"] {
            let args = if operation == "run_command" {
                vec![result_file.clone()]
            } else {
                Vec::new()
            };
            let overrides = if operation == "run_capture" {
                vec![(
                    "PI_PACKAGE_ENV_NEW".to_owned(),
                    JsString::from_utf16(vec![0xd800, 0x78]),
                )]
            } else {
                Vec::new()
            };
            let options_env = JsValue::Object(
                overrides
                    .iter()
                    .map(|(key, value)| (key.as_str().into(), JsValue::String(value.clone())))
                    .collect(),
            );
            let request = js_json!({"id":operation,"operation":operation,"command":command,"args":args,
                "options":{"env":options_env},"processEnv":environment,
                "scenario":{"cwd":cwd,"agentDir":node_join(&cwd,"agent")}});
            let mut expected = source.request(&request);
            if operation == "run_command" {
                expected["output"]["stdout"] = JsValue::String(read_text_file(&result_file).unwrap());
            }
            let result = match operation {
                "run_command" => runtime
                    .block_on(manager.run_command(&command, &args, None))
                    .map(|()| read_text_file(&result_file).unwrap()),
                "run_capture" => runtime.block_on(manager.run_command_capture(&command, &args, None, None, &overrides)),
                _ => manager.run_command_sync(&command, &args),
            };
            let actual = normalize_result(result);
            let expected = js_json!({"ok":expected["ok"],"output":expected["output"]});
            if actual != expected {
                mismatches
                    .push(js_json!({"operation":operation,"env":environment,"actual":actual,"expected":expected}));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "{}",
        js_json::stringify_lossless(&js_json!(mismatches))
    );
}

#[test]
fn environment_forwarding_and_linux_empty_restore_match_frozen() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "core::package_manager::process_tests::environment_probe_child",
            "--nocapture",
        ])
        .env("PI_PACKAGE_ENV_CHILD", "1")
        .env("PI_PACKAGE_ENV_MARKER", "initial")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn inherited_stdio_probe_child() {
    let Some(payload) = std::env::var_os("PI_PACKAGE_NATIVE_STDIO_PROBE") else {
        return;
    };
    let request = js_json::parse_lossless(&payload.into_string().unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let manager = process_test_manager(directory.path());
    if request["takeOverStdout"] == JsValue::Bool(true) {
        crate::core::output_guard::take_over_stdout();
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let args: Vec<JsString> = request["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_string().unwrap().clone())
        .collect();
    runtime
        .block_on(manager.run_command(request["command"].as_string().unwrap(), &args, None))
        .unwrap();
}

#[test]
fn inherited_stdio_matches_frozen() {
    use std::io::Write;
    use std::process::Stdio;
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("stdio-probe");
    executable_script::write_executable_script(
        &binary,
        r#"#!/usr/bin/python3
import os
value=os.read(0,1024)
os.write(1,b'out<'+value+b'>')
os.write(2,b'err')
"#,
    );
    let cwd = pi_js_compat::node_fs::decode_node_path(directory.path().as_os_str());
    let mut mismatches = Vec::new();
    for takeover in [false, true] {
        let request = js_json!({"id":0,"operation":"run_command","command":pi_js_compat::node_fs::decode_node_path(binary.as_os_str()),
            "args":[],"takeOverStdout":takeover,"scenario":{"cwd":cwd,"agentDir":node_join(&cwd,"agent")}});
        let payload = js_json::stringify_lossless(&request);
        let source_lock = oracle::read_source_lock();
        oracle::verify_source_head(&source_lock);
        let mut source = oracle::spawn_oracle_process(
            &source_lock,
            "package-manager-oracle.ts",
            "packages/coding-agent/src/core/package-manager.ts",
            &[("PI_PACKAGE_STDIO_PROBE", payload.clone().into())],
            Stdio::piped(),
        );
        source.stdin.take().unwrap().write_all(b"input").unwrap();
        let expected = source.wait_with_output().unwrap();
        assert!(expected.status.success(), "source stderr: {:?}", expected.stderr);
        let mut native = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "core::package_manager::process_tests::inherited_stdio_probe_child",
                "--nocapture",
            ])
            .env("PI_PACKAGE_NATIVE_STDIO_PROBE", payload)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        native.stdin.take().unwrap().write_all(b"input").unwrap();
        let actual = native.wait_with_output().unwrap();
        assert!(actual.status.success(), "native stderr: {:?}", actual.stderr);
        let expected_stdout = if takeover {
            b"".as_slice()
        } else {
            b"out<input>".as_slice()
        };
        let expected_stderr = if takeover {
            b"out<>err".as_slice()
        } else {
            b"err".as_slice()
        };
        assert_eq!(expected.stdout, expected_stdout, "frozen stdout, takeover={takeover}");
        assert_eq!(expected.stderr, expected_stderr, "frozen stderr, takeover={takeover}");
        // The Rust unit-test runner owns its own stdout header/trailer, unlike tsx.
        let native_stdout = String::from_utf8(actual.stdout).unwrap();
        let child_stdout = native_stdout
            .split_once("out<")
            .map(|(_, rest)| format!("out<{}>", rest.split_once('>').unwrap().0))
            .unwrap_or_default();
        if child_stdout.as_bytes() != expected_stdout || actual.stderr != expected_stderr {
            mismatches.push((takeover, child_stdout, actual.stderr));
        }
    }
    assert!(mismatches.is_empty(), "{mismatches:?}");
}

#[test]
fn takeover_outputs_share_inherited_stderr_offset() {
    use std::io::Write;
    use std::process::Stdio;
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("fd-offset");
    executable_script::write_executable_script(
        &binary,
        "#!/usr/bin/python3\nimport os\nos.write(1,b'OUT')\nos.write(2,b'ERR')\n",
    );
    let cwd = pi_js_compat::node_fs::decode_node_path(directory.path().as_os_str());
    let request = js_json!({"id":0,"operation":"run_command","command":pi_js_compat::node_fs::decode_node_path(binary.as_os_str()),
        "args":[],"takeOverStdout":true,"scenario":{"cwd":cwd,"agentDir":node_join(&cwd,"agent")}});
    let payload = js_json::stringify_lossless(&request);
    let source_path = directory.path().join("source-stderr");
    let native_path = directory.path().join("native-stderr");
    let mut source_file = std::fs::File::create(&source_path).unwrap();
    let mut native_file = std::fs::File::create(&native_path).unwrap();
    source_file.write_all(b"prefix:").unwrap();
    native_file.write_all(b"prefix:").unwrap();
    let source_lock = oracle::read_source_lock();
    oracle::verify_source_head(&source_lock);
    let source = oracle::spawn_oracle_process(
        &source_lock,
        "package-manager-oracle.ts",
        "packages/coding-agent/src/core/package-manager.ts",
        &[("PI_PACKAGE_STDIO_PROBE", payload.clone().into())],
        Stdio::from(source_file),
    );
    let expected = source.wait_with_output().unwrap();
    assert!(expected.status.success());
    assert!(expected.stdout.is_empty());
    let actual = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "core::package_manager::process_tests::inherited_stdio_probe_child",
            "--nocapture",
        ])
        .env("PI_PACKAGE_NATIVE_STDIO_PROBE", payload)
        .stdin(Stdio::null())
        .stderr(Stdio::from(native_file))
        .output()
        .unwrap();
    assert!(actual.status.success());
    let expected = std::fs::read(source_path).unwrap();
    assert_eq!(expected, b"prefix:OUTERR");
    assert_eq!(std::fs::read(native_path).unwrap(), expected);
}
