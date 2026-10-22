#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::{CreateDirOptions, FileContent, FileSystem, RemoveOptions, Shell, ShellExecOptions};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue, to_value};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "agent-core-node-env-oracle.ts",
        "packages/agent-core/src/harness/env/nodejs.ts",
    )
}
fn env(root: &str) -> Arc<NodeExecutionEnv> {
    Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.into(),
        shell_env: [("CORE_CONFIG".into(), Some("configured".into()))].into(),
        ..Default::default()
    }))
}
fn shell_result(
    result: Result<pi_agent_core::harness::ShellExecResult, pi_agent_core::harness::ExecutionError>,
) -> JsValue {
    match result {
        Ok(value) => {
            json!({"ok":true,"value":{"stdout":value.stdout,"stderr":value.stderr,"exitCode":value.exit_code}})
        }
        Err(error) => json!({"ok":false,"error":{"code":error.code.as_str(),"message":error.message}}),
    }
}
#[tokio::test]
async fn node_env_paths_match_frozen_posix_literals() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let paths = ["~", "~/child", "file:///tmp/a%20b", "/a/../b//", "nested/../x", ""];
    let joins = vec![
        vec!["a", "b/"],
        vec![".", ""],
        vec!["a", "..", ""],
        vec!["", ""],
        vec!["/a", "/b"],
        vec!["/", ".."],
    ];
    let expected = oracle().request(&json!({"kind":"paths","root":root,"paths":paths,"joins":joins}));
    let env = env(root);
    let mut actual = Vec::new();
    for path in paths {
        actual.push(json!({"ok":true,"value":env.absolute_path(&path.into(),None).await.unwrap()}));
    }
    for parts in joins {
        actual.push(json!({"ok":true,"value":env.join_path(&parts.into_iter().map(Into::into).collect::<Vec<_>>(),None).await.unwrap()}));
    }
    assert_eq!(actual, expected.as_array().unwrap().clone());
}
#[tokio::test]
async fn node_env_cleanup_is_noop_and_exec_waits_for_inherited_pipes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let mut oracle = oracle();
    for (kind, command) in [
        ("tail", "(sleep 0.25; printf late) & printf early"),
        ("cleanup", "printf ready; sleep 0.2; printf done"),
    ] {
        let expected = oracle.request(&json!({"kind":kind,"root":root}));
        let env = env(root);
        let (notify, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let options = ShellExecOptions {
            on_stdout: Some(Arc::new(move |_| {
                let _ = notify.send(());
                Ok(())
            })),
            ..Default::default()
        };
        let runner = env.clone();
        let task = tokio::spawn(async move { runner.exec(&command.into(), options).await });
        receiver.recv().await.unwrap();
        if kind == "cleanup" {
            env.cleanup().await;
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(shell_result(result), expected, "{kind}");
    }
}
#[tokio::test]
async fn node_env_exec_environment_and_spawn_failures_match_frozen() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let env = env(root);
    let mut oracle = oracle();
    for kind in ["env", "missing-cwd"] {
        let expected = oracle.request(&json!({"kind":kind,"root":root}));
        let options = if kind == "env" {
            ShellExecOptions {
                env: Some([("CORE_EXTRA".into(), "extra".into())].into()),
                ..Default::default()
            }
        } else {
            ShellExecOptions {
                cwd: Some("missing".into()),
                ..Default::default()
            }
        };
        let command = if kind == "env" {
            "printf '%s:%s' \"$CORE_CONFIG\" \"$CORE_EXTRA\""
        } else {
            "printf never"
        };
        assert_eq!(
            shell_result(env.exec(&command.into(), options).await),
            expected,
            "{kind}"
        );
    }
}
#[tokio::test]
async fn node_concrete_ignores_extra_abort_arguments() {
    let native = tempfile::tempdir().unwrap();
    let reference = tempfile::tempdir().unwrap();
    let env = env(native.path().to_str().unwrap());
    let expected = oracle().request(&json!({"kind":"ignored-abort","root":reference.path()}));
    let token = CancellationToken::new();
    token.cancel();
    let path = JsString::from("nested/file");
    let content = JsString::from("a");
    env.append_file(&path, FileContent::Text(&content), Some(&token))
        .await
        .unwrap();
    let exists = env.exists(&path, Some(&token)).await.unwrap();
    env.create_dir(&"extra".into(), CreateDirOptions::default())
        .await
        .unwrap();
    env.remove(
        &"extra".into(),
        RemoveOptions {
            recursive: Some(true),
            force: None,
        },
    )
    .await
    .unwrap();
    let text = env.read_text_file(&path, None).await.unwrap();
    let actual = json!([{"ok":true},{"ok":true,"value":exists},{"ok":true},{"ok":true},{"ok":true,"value":text}]);
    assert_eq!(actual, expected);
    // The native JSON authority must not change the external result shape.
    assert_eq!(to_value(&text).unwrap(), JsValue::String("a".into()));
}

#[tokio::test]
async fn post_exit_pipe_wait_keeps_timeout_abort_and_callback_live() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let env = env(root);
    let mut oracle = oracle();
    for mode in ["timeout", "abort", "callback"] {
        let expected = oracle.request(&json!({"kind":"post-exit","root":root,"mode":mode}));
        let abort = CancellationToken::new();
        let signal = abort.clone();
        let mode_string = mode.to_owned();
        let options = ShellExecOptions {
            timeout: (mode == "timeout").then_some(0.05),
            abort: (mode == "abort").then_some(abort),
            on_stdout: Some(Arc::new(move |chunk| {
                if mode_string == "abort" && chunk.contains("early") {
                    let signal = signal.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        signal.cancel();
                    });
                }
                if mode_string == "callback" && chunk.contains("late") {
                    return Err(JsString::from("late") + JsString::from_utf16(vec![0xd800]) + "failure");
                }
                Ok(())
            })),
            ..Default::default()
        };
        let actual = shell_result(
            env.exec(&"(sleep 0.25; printf late) & printf early".into(), options)
                .await,
        );
        assert_eq!(actual, expected, "{mode}");
    }
}

#[tokio::test]
async fn relative_env_paths_decode_raw_process_cwd_like_node() {
    const PROBE: &str = "PI_RAW_CWD_PROBE";
    if std::env::var_os(PROBE).is_some() {
        let env = env("base");
        let actual = env.absolute_path(&"child".into(), None).await.unwrap();
        let output = std::process::Command::new("node")
            .args([
                "--eval",
                "console.log(JSON.stringify({cwd:process.cwd(),path:require('node:path').resolve('base','child')}))",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let expected = pi_js_compat::js_json::parse_lossless(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
        assert_eq!(actual, expected["path"].as_string().unwrap().clone());
        assert_eq!(
            pi_js_compat::node_fs::decode_node_path(std::env::current_dir().unwrap().as_os_str()),
            expected["cwd"].as_string().unwrap().clone()
        );
        assert!(actual.units().contains(&0xfffd));
        return;
    }
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().unwrap();
    let raw = root
        .path()
        .join(std::ffi::OsString::from_vec(b"cwd-\xff-\xed\xa0\x80".to_vec()));
    std::fs::create_dir(&raw).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "relative_env_paths_decode_raw_process_cwd_like_node",
            "--nocapture",
        ])
        .env(PROBE, "1")
        .current_dir(&raw)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn file_error_value(error: pi_agent_core::harness::FileError) -> JsValue {
    let mut value = json!({"code":error.code.as_str(), "message":error.message});
    if let Some(path) = error.path {
        value["path"] = JsValue::String(path);
    }
    json!({"ok":false,"error":value})
}
fn file_info_value(info: pi_agent_core::harness::FileInfo) -> JsValue {
    json!({"name":info.name,"path":info.path,"kind":info.kind.as_str(),"size":info.size,"mtimeMs":info.mtime_ms})
}
async fn read_operation(env: &NodeExecutionEnv, method: &str, path: &JsString) -> JsValue {
    let result = match method {
        "fileInfo" => env.file_info(path).await.map(file_info_value),
        "exists" => env.exists(path, None).await.map(JsValue::Bool),
        "readTextFile" => env
            .read_text_file(path, None)
            .await
            .map(|text| JsValue::String(text.into())),
        "canonicalPath" => env.canonical_path(path).await.map(JsValue::String),
        "listDir" => env
            .list_dir(path, None)
            .await
            .map(|infos| JsValue::Array(infos.into_iter().map(file_info_value).collect())),
        _ => panic!("unknown fixture method"),
    };
    match result {
        Ok(value) => json!({"ok":true,"value":value}),
        Err(error) => file_error_value(error),
    }
}
#[tokio::test]
async fn node_filesystem_metadata_and_failure_matrix_matches_frozen() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("dir")).unwrap();
    let file = std::fs::File::create(root.path().join("dated")).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH - std::time::Duration::from_secs(1)))
        .unwrap();
    std::os::unix::fs::symlink("missing", root.path().join("broken")).unwrap();
    let fifo = root.path().join("fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let env = env(root.path().to_str().unwrap());
    let raw = JsString::from_utf16(vec![0xd800]);
    let operations: Vec<(&str, JsString)> = vec![
        ("fileInfo", "dir///".into()),
        ("fileInfo", "dated".into()),
        ("fileInfo", "fifo".into()),
        ("exists", "fifo".into()),
        ("exists", "broken".into()),
        ("exists", "missing".into()),
        ("fileInfo", raw),
        ("fileInfo", "null\0byte".into()),
        ("readTextFile", "dir".into()),
        ("fileInfo", "dated/child".into()),
        ("readTextFile", "missing".into()),
        ("canonicalPath", "missing".into()),
        ("listDir", "dated".into()),
    ];
    let expected = oracle().request(&json!({"kind":"fs","root":root.path(),"operations":operations}));
    let mut actual = Vec::new();
    for (method, path) in operations {
        actual.push(read_operation(&env, method, &path).await);
    }
    let actual = JsValue::Array(actual);
    assert!(
        actual == expected,
        "actual={}\nexpected={}",
        pi_js_compat::js_json::stringify_lossless(&actual),
        pi_js_compat::js_json::stringify_lossless(&expected)
    );
}

fn normalize_root(value: JsValue, root: &str) -> JsValue {
    match value {
        JsValue::String(text) => JsValue::String(text.replace(root, "<root>")),
        JsValue::Array(items) => JsValue::Array(items.into_iter().map(|item| normalize_root(item, root)).collect()),
        JsValue::Object(items) => JsValue::Object(
            items
                .into_iter()
                .map(|(key, value)| (key, normalize_root(value, root)))
                .collect(),
        ),
        other => other,
    }
}
async fn mutation_operation(env: &NodeExecutionEnv, args: &[JsValue]) -> JsValue {
    let path = args[1].as_string().unwrap();
    let content = JsString::from_utf16(vec![0xd800, 0x61]);
    let result = match args[0].as_string().unwrap().to_utf8().unwrap().as_str() {
        "writeFile" => env.write_file(path, FileContent::Text(&content), None).await,
        "appendFile" => env.append_file(path, FileContent::Text(&content), None).await,
        "createDir" => {
            env.create_dir(
                path,
                CreateDirOptions {
                    recursive: args.get(2).and_then(|value| value["recursive"].as_bool()),
                },
            )
            .await
        }
        "remove" => {
            env.remove(
                path,
                RemoveOptions {
                    recursive: args.get(2).and_then(|value| value["recursive"].as_bool()),
                    force: args.get(2).and_then(|value| value["force"].as_bool()),
                },
            )
            .await
        }
        other => panic!("unknown operation {other}"),
    };
    match result {
        Ok(()) => json!({"ok":true}),
        Err(error) => file_error_value(error),
    }
}
#[tokio::test]
async fn node_filesystem_mutation_failures_and_state_match_frozen() {
    let native = tempfile::tempdir().unwrap();
    let reference = tempfile::tempdir().unwrap();
    for root in [native.path(), reference.path()] {
        std::fs::write(root.join("file"), "data").unwrap();
        std::fs::create_dir(root.join("dir")).unwrap();
        std::os::unix::fs::symlink("missing", root.join("link")).unwrap();
    }
    let content = JsString::from_utf16(vec![0xd800, 0x61]);
    let operations = json!([
        ["writeFile","file/child",content],["appendFile","file/child",content],
        ["writeFile","dir",content],["appendFile","dir",content],
        ["createDir","file"],["createDir","parent/child",{"recursive":false}],
        ["remove","missing"],["remove","dir"],["remove","link"],
        ["writeFile","nested/new",content],["appendFile","nested/new",content],
        ["remove","missing",{"force":true}], ["remove","dir",{"recursive":true}]
    ]);
    let expected = oracle().request(&json!({"kind":"fs","root":reference.path(),"operations":operations}));
    let env = env(native.path().to_str().unwrap());
    let mut actual = Vec::new();
    for args in operations.as_array().unwrap() {
        actual.push(mutation_operation(&env, args.as_array().unwrap()).await);
    }
    let actual = normalize_root(JsValue::Array(actual), native.path().to_str().unwrap());
    let expected = normalize_root(expected, reference.path().to_str().unwrap());
    assert!(
        actual == expected,
        "actual={}\nexpected={}",
        pi_js_compat::js_json::stringify_lossless(&actual),
        pi_js_compat::js_json::stringify_lossless(&expected)
    );
    assert_eq!(
        std::fs::read(native.path().join("nested/new")).unwrap(),
        std::fs::read(reference.path().join("nested/new")).unwrap()
    );
    for root in [native.path(), reference.path()] {
        assert!(!root.join("dir").exists());
        assert!(!root.join("link").is_symlink());
    }
}

#[tokio::test]
async fn temporary_files_and_raw_directory_prefix_match_node_contract() {
    let root = tempfile::tempdir().unwrap();
    let env = env(root.path().to_str().unwrap());
    let expected = oracle().request(&json!({"kind":"temp","root":root.path()}));
    let file = env
        .create_temp_file(pi_agent_core::harness::CreateTempFileOptions {
            prefix: Some("pre".into()),
            suffix: Some(".end".into()),
            abort: None,
        })
        .await
        .unwrap();
    let name = file.split('/').last().unwrap();
    let uuid = name.slice(3, name.len() - 4).to_utf8().unwrap();
    let valid = uuid.len() == 36
        && uuid.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            14 => byte == b'4',
            19 => b"89ab".contains(&byte),
            _ => byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase(),
        });
    let directory = std::path::PathBuf::from(pi_js_compat::js_json::encode_node_utf8(&file))
        .parent()
        .unwrap()
        .to_path_buf();
    let parent = directory.file_name().unwrap().to_str().unwrap();
    let raw = env
        .create_temp_dir(&(JsString::from("raw") + JsString::from_utf16(vec![0xd800]) + "-"))
        .await
        .unwrap();
    let text = std::fs::read_to_string(pi_js_compat::js_json::encode_node_utf8(&file)).unwrap();
    let raw_name = raw.split('/').last().unwrap();
    let actual = json!({"uuid":valid,"prefix":name.starts_with("pre"),"suffix":name.ends_with(".end"),"text":text,"tempDirectory":parent.starts_with("tmp-")&&parent.len()==10&&parent[4..].bytes().all(|byte|byte.is_ascii_alphanumeric()),"rawDirectoryPrefix":raw_name.slice(0,raw_name.len()-6)});
    std::fs::remove_dir_all(directory).unwrap();
    std::fs::remove_dir_all(pi_js_compat::js_json::encode_node_utf8(&raw)).unwrap();
    assert!(
        actual == expected,
        "actual={}\nexpected={}",
        pi_js_compat::js_json::stringify_lossless(&actual),
        pi_js_compat::js_json::stringify_lossless(&expected)
    );
}

#[tokio::test]
async fn shell_options_retain_native_units_until_process_boundary() {
    let root = tempfile::tempdir().unwrap();
    let cwd = JsString::from(root.path().to_str().unwrap());
    let raw = JsString::from_utf16(vec![0xd800]);
    let shell = cwd.clone() + "/" + raw.clone();
    std::os::unix::fs::symlink("/bin/bash", pi_js_compat::js_json::encode_node_utf8(&shell)).unwrap();
    let native = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: cwd.clone(),
        shell_path: Some(shell.clone()),
        shell_env: [("RAW".into(), Some(raw.clone())), ("OMIT".into(), None)].into(),
    });
    let command = JsString::from("printf '%s:%s' \"$RAW\" \"$EXTRA\"");
    let options = ShellExecOptions {
        env: Some([("EXTRA".into(), raw.clone())].into()),
        ..Default::default()
    };
    let expected=oracle().request(&json!({"kind":"shell-boundary","root":cwd,"shellPath":shell,"shellEnv":{"RAW":raw},"undefinedEnv":["OMIT"],"extraEnv":{"EXTRA":raw},"command":command}));
    assert_eq!(shell_result(native.exec(&command, options).await), expected);
    let missing = cwd.clone() + "/missing" + raw;
    let native = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: cwd.clone(),
        shell_path: Some(missing.clone()),
        ..Default::default()
    });
    let expected = oracle().request(&json!({"kind":"shell-boundary","root":cwd,"shellPath":missing,"command":"true"}));
    assert_eq!(
        shell_result(native.exec(&"true".into(), Default::default()).await),
        expected
    );
}
