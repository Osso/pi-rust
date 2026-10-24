//! Actual-source child-process boundaries for the private package-manager methods.

use super::*;
#[path = "../../tests/support/executable_script.rs"]
mod executable_script;
#[path = "../../tests/support/oracle.rs"]
mod oracle;

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
            js_json!({"ok":false,"error":{"name":name,"message":error.message()}})
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
