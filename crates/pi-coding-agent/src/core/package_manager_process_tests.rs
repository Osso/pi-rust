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
