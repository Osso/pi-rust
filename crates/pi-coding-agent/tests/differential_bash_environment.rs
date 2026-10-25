use crate::shared_oracle as oracle;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
fn source_contract(cwd: &str, raw: &JsString) -> JsValue {
    let mut source = oracle::TsOracle::spawn(
        "bash-environment-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected =
        source.request(&json!({"id":1,"cwd":cwd,"raw":raw,"cases":["custom","default","empty","undefined"]}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(
        expected["output"][0],
        json!({"own":true,"value":"undefined","kept":raw})
    );
    for (index, text) in ["inherited:absent", "absent:absent", "absent:kept"]
        .into_iter()
        .enumerate()
    {
        assert_eq!(expected["output"][index + 1]["bytes"], json!(text.as_bytes().to_vec()));
    }
    expected["output"].clone()
}
#[test]
fn frozen_undefined_env_property_is_retained_until_spawn() {
    let root = tempfile::tempdir().unwrap();
    source_contract(root.path().to_str().unwrap(), &JsString::from_utf16(vec![0xd800]));
}

use indexmap::IndexMap;
use pi_agent_core::agent::{AgentTool, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::bash::{
    BashExecOptions, BashExecResult, BashOperations, BashTool, BashToolOptions, create_local_bash_operations,
};
use std::sync::{Arc, Mutex};
struct Operations(Arc<Mutex<Option<JsValue>>>);
impl BashOperations for Operations {
    fn exec<'a>(
        &'a self,
        _: &'a JsString,
        _: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async move {
            let env = options.env.unwrap();
            let kept = env.get(&JsString::from("BASH_KEEP")).unwrap().as_ref().unwrap();
            *self.0.lock().unwrap() = Some(
                json!({"own":env.contains_key(&JsString::from("BASH_REMOVE")),"value":if env.get(&JsString::from("BASH_REMOVE")).unwrap().is_none(){"undefined"}else{"defined"},"kept":kept}),
            );
            Ok(BashExecResult {
                exit_code: Some(0.0),
                detached: None,
            })
        })
    }
}
#[tokio::test]
async fn native_undefined_env_key_and_absent_empty_local_environments_match_frozen() {
    let root = tempfile::tempdir().unwrap();
    let cwd = JsString::from(root.path().to_str().unwrap());
    let raw = JsString::from_utf16(vec![0xd800]);
    let expected = source_contract(root.path().to_str().unwrap(), &raw);
    let captured = Arc::new(Mutex::new(None));
    let tool = BashTool::with_options(
        cwd.clone(),
        BashToolOptions {
            operations: Some(Arc::new(Operations(captured.clone()))),
            spawn_hook: Some(Arc::new(move |mut context| {
                context.env.insert("BASH_REMOVE".into(), None);
                context.env.insert("BASH_KEEP".into(), Some(raw.clone()));
                Ok(context)
            })),
            ..Default::default()
        },
    );
    tool.execute(
        "env".into(),
        json!({"command":"probe"}),
        None,
        Arc::new(|_| Ok(())),
        None,
    )
    .await
    .unwrap();
    assert_eq!(captured.lock().unwrap().as_ref().unwrap(), &expected[0]);
    pi_js_compat::process_env::set("BASH_REMOVE", "inherited");
    for (index, env) in [
        None,
        Some(IndexMap::new()),
        Some(IndexMap::from([
            ("BASH_REMOVE".into(), None),
            ("BASH_KEEP".into(), Some("kept".into())),
        ])),
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let sink = bytes.clone();
        let result = create_local_bash_operations(None)
            .exec(
                &"printf '%s:%s' \"${BASH_REMOVE-absent}\" \"${BASH_KEEP-absent}\"".into(),
                &cwd,
                BashExecOptions {
                    on_data: Arc::new(move |data| {
                        sink.lock().unwrap().extend_from_slice(data);
                        Ok(())
                    }),
                    signal: None,
                    timeout: None,
                    env,
                    detach: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            json!({"result":{"exitCode":result.exit_code},"bytes":bytes.lock().unwrap().clone()}),
            expected[index + 1]
        );
    }
    pi_js_compat::process_env::clear();
}

#[tokio::test]
async fn environment_value_validation_retains_native_property_labels() {
    let root = tempfile::tempdir().unwrap();
    let cwd = JsString::from(root.path().to_str().unwrap());
    let mut source = oracle::TsOracle::spawn(
        "bash-environment-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let mut failures = Vec::new();
    for key in [
        JsString::from("BASH_VALUE"),
        JsString::from("BASH_") + JsString::from_utf16(vec![0xd800]),
        JsString::from("BASH_\0KEY"),
    ] {
        let expected = source.request(&json!({"id":2,"operation":"invalidEnv","cwd":cwd,"key":key}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        assert_eq!(expected["output"]["ok"], json!(false), "{expected:?}");
        let result = create_local_bash_operations(None)
            .exec(
                &"true".into(),
                &cwd,
                BashExecOptions {
                    on_data: Arc::new(|_| Ok(())),
                    signal: None,
                    timeout: None,
                    env: Some([(key.clone(), Some(JsString::from("bad\0value")))].into()),
                    detach: None,
                },
            )
            .await;
        let actual = match result {
            Ok(_) => json!({"ok":true}),
            Err(error) => json!({"ok":false,"error":error.message}),
        };
        if actual != expected["output"] {
            failures.push(json!({"key":key,"actual":actual,"expected":expected["output"]}));
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        pi_js_compat::js_json::stringify_lossless(&json!(failures))
    );
}

use crate::shared_executable_script as executable_script;
#[tokio::test]
async fn native_child_envp_is_sorted_and_frozen_own_key_order_is_preserved_as_evidence() {
    let root = tempfile::tempdir().unwrap();
    let shell = root.path().join("env-probe");
    executable_script::write_executable_script(
        &shell,
        "#!/usr/bin/python3\nimport os\nos.write(1, open('/proc/self/environ', 'rb').read())\n",
    );
    let cwd = JsString::from(root.path().to_str().unwrap());
    let shell = JsString::from(shell.to_str().unwrap());
    let mut source = oracle::TsOracle::spawn(
        "bash-environment-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected = source.request(&json!({"id":3,"operation":"envOrder","cwd":cwd,"shell":shell}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let sink = bytes.clone();
    let env = [("Z_FIRST", "one"), ("A_SECOND", "two"), ("10", "ten"), ("2", "two")]
        .into_iter()
        .map(|(key, value)| (key.into(), Some(value.into())))
        .collect();
    create_local_bash_operations(Some(&shell))
        .exec(
            &"probe".into(),
            &cwd,
            BashExecOptions {
                on_data: Arc::new(move |data| {
                    sink.lock().unwrap().extend_from_slice(data);
                    Ok(())
                }),
                signal: None,
                timeout: None,
                env: Some(env),
                detach: None,
            },
        )
        .await
        .unwrap();
    let direct = std::process::Command::new("/usr/bin/python3")
        .args(["-c", "import os; os.write(1, open('/proc/self/environ','rb').read())"])
        .env_clear()
        .env("Z_FIRST", "one")
        .env("A_SECOND", "two")
        .env("10", "ten")
        .env("2", "two")
        .output()
        .unwrap();
    assert!(direct.status.success());
    assert_eq!(
        direct.stdout,
        *bytes.lock().unwrap(),
        "stdlib explicit .env insertion must show the same sort without our IndexMap"
    );
    let frozen_bytes: Vec<u8> = pi_js_compat::js_json::from_value(&expected["output"]).unwrap();
    let decode_entries = |bytes: &[u8]| {
        bytes
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
            .map(|entry| String::from_utf8(entry.to_vec()).expect("fixture environment is UTF8"))
            .collect::<Vec<_>>()
    };
    let select_fixture_entries = |entries: &[String]| {
        entries
            .iter()
            .filter(|entry| ["2=two", "10=ten", "Z_FIRST=one", "A_SECOND=two"].contains(&entry.as_str()))
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut frozen_entries = decode_entries(&frozen_bytes);
    let mut native_entries = decode_entries(&bytes.lock().unwrap());
    assert_eq!(
        select_fixture_entries(&frozen_entries),
        ["2=two", "10=ten", "Z_FIRST=one", "A_SECOND=two"]
    );
    // Accepted runtime gap, user decision 2026-10-24: keep std Command's native sort.
    assert_eq!(
        select_fixture_entries(&native_entries),
        ["10=ten", "2=two", "A_SECOND=two", "Z_FIRST=one"]
    );
    frozen_entries.sort();
    native_entries.sort();
    assert_eq!(
        native_entries, frozen_entries,
        "all names and values must remain identical"
    );
}

fn source_order_failures(cwd: &JsString) -> Vec<JsValue> {
    let mut source = oracle::TsOracle::spawn(
        "bash-environment-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    [(vec!["Z_FIRST", "A_SECOND"], "Z_FIRST"), (vec!["10", "2"], "2")]
        .into_iter()
        .map(|(keys, first)| {
            let expected = source.request(&json!({"id":4,"operation":"invalidEnv","cwd":cwd,"keys":keys}));
            assert_eq!(expected["ok"], json!(true), "{expected:?}");
            let prefix = pi_js_compat::js_format!("The property 'options.env['{}']'", first);
            assert!(
                expected["output"]["error"].as_string().unwrap().starts_with(&prefix),
                "{expected:?}"
            );
            expected["output"].clone()
        })
        .collect()
}
#[test]
fn frozen_environment_validation_uses_js_own_key_order() {
    let root = tempfile::tempdir().unwrap();
    source_order_failures(&JsString::from(root.path().to_str().unwrap()));
}

#[tokio::test]
async fn ordered_native_env_validation_matches_js_own_keys() {
    let root = tempfile::tempdir().unwrap();
    let cwd = JsString::from(root.path().to_str().unwrap());
    let expected = source_order_failures(&cwd);
    for (index, keys) in [["Z_FIRST", "A_SECOND"], ["10", "2"]].into_iter().enumerate() {
        let env: indexmap::IndexMap<JsString, Option<JsString>> = keys
            .into_iter()
            .map(|name| (name.into(), Some("bad\0value".into())))
            .collect();
        let result = create_local_bash_operations(None)
            .exec(
                &"true".into(),
                &cwd,
                BashExecOptions {
                    on_data: Arc::new(|_| Ok(())),
                    signal: None,
                    timeout: None,
                    env: Some(env),
                    detach: None,
                },
            )
            .await;
        let actual = match result {
            Ok(_) => json!({"ok":true}),
            Err(error) => json!({"ok":false,"error":error.message}),
        };
        assert_eq!(actual, expected[index]);
    }
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
