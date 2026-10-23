#[path = "support/operation.rs"]
mod operation;
use pi_agent_core::harness::types::{NodeRejection, OperationFailure};
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
    result: Result<pi_agent_core::harness::ShellExecResult, OperationFailure<pi_agent_core::harness::ExecutionError>>,
) -> JsValue {
    match result {
        Ok(value) => {
            json!({"ok":true,"value":{"stdout":value.stdout,"stderr":value.stderr,"exitCode":value.exit_code}})
        }
        Err(OperationFailure::Returned(error)) => {
            json!({"ok":false,"error":{"code":error.code.as_str(),"message":error.message}})
        }
        Err(OperationFailure::Rejected(error)) => json!({"rejected":rejection_value(error)}),
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

fn file_error_value(error: impl Into<OperationFailure<pi_agent_core::harness::FileError>>) -> JsValue {
    let error = match error.into() {
        OperationFailure::Returned(error) => error,
        OperationFailure::Rejected(error) => return json!({"rejected":rejection_value(error)}),
    };
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

#[tokio::test]
async fn shell_spawn_argument_validation_and_envpair_encoding_match_node() {
    let root = tempfile::tempdir().unwrap();
    let native = env(root.path().to_str().unwrap());
    let mut oracle = oracle();
    let cases: Vec<(&str, Option<JsString>, Option<indexmap::IndexMap<String, JsString>>)> = vec![
        ("true\0bad", None, None),
        ("true", Some("bad\0cwd".into()), None),
        ("true", None, Some([("EXTRA".into(), "bad\0value".into())].into())),
        ("true", None, Some([("BAD\0KEY".into(), "ok".into())].into())),
        ("printenv a", None, Some([("a=b".into(), "payload".into())].into())),
        ("printenv ''", None, Some([("".into(), "payload".into())].into())),
        (
            "true",
            None,
            Some(
                [
                    ("EXTRA".into(), "bad\0value".into()),
                    ("0".into(), "bad\0number".into()),
                ]
                .into(),
            ),
        ),
        ("true\0arg", Some("bad\0cwd".into()), None),
    ];
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    for (command, cwd, extra) in cases {
        expected.push(oracle.request(
            &json!({"kind":"shell-boundary","root":root.path(),"command":command,"cwd":cwd,"extraEnv":extra}),
        ));
        actual.push(shell_result(
            native
                .exec(
                    &command.into(),
                    ShellExecOptions {
                        cwd,
                        env: extra,
                        ..Default::default()
                    },
                )
                .await,
        ));
    }
    let actual = JsValue::Array(actual);
    let expected = JsValue::Array(expected);
    assert!(
        actual == expected,
        "actual={}\nexpected={}",
        pi_js_compat::js_json::stringify_lossless(&actual),
        pi_js_compat::js_json::stringify_lossless(&expected)
    );
}

#[tokio::test]
async fn read_file_abort_after_io_started_matches_node() {
    use std::io::Write;
    for method in ["readTextFile", "readBinaryFile"] {
        let root = tempfile::tempdir().unwrap();
        let reference = tempfile::tempdir().unwrap();
        let expected = oracle().request(&json!({"kind":"read-abort","root":reference.path(),"method":method}));
        let path = root.path().join("read.fifo");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let mut writer = std::fs::OpenOptions::new().read(true).write(true).open(path).unwrap();
        writer.write_all(b"prefix").unwrap();
        let native = env(root.path().to_str().unwrap());
        let token = CancellationToken::new();
        let signal = token.clone();
        let task = tokio::spawn(async move {
            if method == "readTextFile" {
                native
                    .read_text_file(&"read.fifo".into(), Some(&signal))
                    .await
                    .map(|text| json!(text))
            } else {
                native
                    .read_binary_file(&"read.fifo".into(), Some(&signal))
                    .await
                    .map(|bytes| json!(bytes))
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        token.cancel();
        drop(writer);
        let result = task.await.unwrap();
        let actual = match result {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(error) => file_error_value(error),
        };
        let actual = normalize_root(actual, root.path().to_str().unwrap());
        let expected = normalize_root(expected, reference.path().to_str().unwrap());
        assert!(
            actual == expected,
            "{method}: actual={}\nexpected={}",
            pi_js_compat::js_json::stringify_lossless(&actual),
            pi_js_compat::js_json::stringify_lossless(&expected)
        );
    }
}

#[tokio::test]
async fn write_file_abort_after_blocked_open_preserves_frozen_written_bytes_and_error() {
    let native = tempfile::tempdir().unwrap();
    let reference = tempfile::tempdir().unwrap();
    let expected = oracle().request(&json!({"kind":"write-abort","root":reference.path()}));
    let path = native.path().join("write.fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let env = env(native.path().to_str().unwrap());
    let token = CancellationToken::new();
    let signal = token.clone();
    let task = tokio::spawn(async move {
        env.write_file(&"write.fifo".into(), FileContent::Text(&"hello".into()), Some(&signal))
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    token.cancel();
    let text = tokio::task::spawn_blocking(move || std::fs::read_to_string(path).unwrap())
        .await
        .unwrap();
    let result = task.await.unwrap();
    let write = match result {
        Ok(()) => json!({"ok":true}),
        Err(error) => file_error_value(error),
    };
    let actual = normalize_root(json!({"write":write,"text":text}), native.path().to_str().unwrap());
    let expected = normalize_root(expected, reference.path().to_str().unwrap());
    assert!(
        actual == expected,
        "actual={}\nexpected={}",
        pi_js_compat::js_json::stringify_lossless(&actual),
        pi_js_compat::js_json::stringify_lossless(&expected)
    );
}

fn normalize_temp_file_error(mut value: JsValue) -> JsValue {
    let path = value["error"]["path"].as_string().unwrap().clone();
    let parts = path.split('/').collect::<Vec<_>>();
    let directory = JsString::join(&parts[..parts.len() - 2], "/");
    let name = parts.last().unwrap();
    fn replace(value: &mut JsValue, directory: &JsString, name: &JsString) {
        match value {
            JsValue::String(text) => *text = text.replace(directory, "<tmpdir>").replace(name, "<uuid>.end"),
            JsValue::Object(items) => {
                for value in items.values_mut() {
                    replace(value, directory, name);
                }
            }
            _ => {}
        }
    }
    replace(&mut value, &directory, name);
    value
}
#[tokio::test]
async fn temporary_creation_failure_messages_and_outer_paths_match_frozen() {
    let root = tempfile::tempdir().unwrap();
    let env = env(root.path().to_str().unwrap());
    let mut expected = oracle().request(&json!({"kind":"temp-errors","root":root.path()}));
    let first = env.create_temp_dir(&"missing-parent/pre".into()).await.unwrap_err();
    let second = env.create_temp_dir(&"bad\0prefix".into()).await.unwrap_err();
    let third = env
        .create_temp_file(pi_agent_core::harness::CreateTempFileOptions {
            prefix: Some("missing/".into()),
            suffix: Some(".end".into()),
            abort: None,
        })
        .await
        .unwrap_err();
    let path = std::path::PathBuf::from(pi_js_compat::js_json::encode_node_utf8(third.path.as_ref().unwrap()));
    std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    fn normalize_random_suffix(mut error: JsValue) -> JsValue {
        let message = error["error"]["message"].as_string().unwrap();
        let suffix = message.slice(message.len() - 7, message.len() - 1);
        assert_eq!(suffix.len(), 6);
        assert!(
            suffix
                .units()
                .iter()
                .all(|unit| u8::try_from(*unit).is_ok_and(|byte| byte.is_ascii_alphanumeric()))
        );
        let message = message.slice(0, message.len() - 7) + "<random>'";
        error["error"]["message"] = JsValue::String(message);
        error
    }
    let third = normalize_temp_file_error(file_error_value(third));
    let JsValue::Array(expected_items) = &mut expected else {
        panic!("expected result array")
    };
    expected_items[0] = normalize_random_suffix(expected_items[0].clone());
    expected_items[2] = normalize_temp_file_error(expected_items[2].clone());
    let actual = json!([
        normalize_random_suffix(file_error_value(first)),
        file_error_value(second),
        third
    ]);
    assert!(
        actual == expected,
        "actual={}\nexpected={}",
        pi_js_compat::js_json::stringify_lossless(&actual),
        pi_js_compat::js_json::stringify_lossless(&expected)
    );
}

#[tokio::test]
async fn deleted_process_cwd_rejects_without_returned_file_error_or_panic() {
    const PROBE: &str = "PI_DELETED_CWD_PROBE";
    if let Some(path) = std::env::var_os(PROBE) {
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&path).unwrap();
        std::fs::remove_dir(&path).unwrap();
        let native = env("base");
        let result = tokio::spawn(async move { native.absolute_path(&"child".into(), None).await }).await;
        std::env::set_current_dir(original).unwrap();
        let actual = match result {
            Ok(Ok(path)) => json!({"returned":{"ok":true,"value":path}}),
            Ok(Err(OperationFailure::Returned(error))) => json!({"returned":file_error_value(error)}),
            Ok(Err(OperationFailure::Rejected(error))) => json!({"rejected":rejection_value(error)}),
            Err(_) => json!({"panic":true}),
        };
        let expected =
            pi_js_compat::js_json::parse_lossless(&std::env::var("PI_DELETED_CWD_EXPECTED").unwrap()).unwrap();
        assert!(
            actual == expected,
            "actual={}\nexpected={}",
            pi_js_compat::js_json::stringify_lossless(&actual),
            pi_js_compat::js_json::stringify_lossless(&expected)
        );
        return;
    }
    let native = tempfile::tempdir().unwrap();
    let reference = tempfile::tempdir().unwrap();
    let expected = oracle().request(&json!({"kind":"deleted-cwd","root":reference.path()}));
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "deleted_process_cwd_rejects_without_returned_file_error_or_panic",
            "--nocapture",
        ])
        .env(PROBE, native.path())
        .env(
            "PI_DELETED_CWD_EXPECTED",
            pi_js_compat::js_json::stringify_lossless(&expected),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn rejection_value(rejection: NodeRejection) -> JsValue {
    rejection.error.value()
}

#[tokio::test]
async fn deleted_cwd_rejection_propagates_through_all_resolve_methods_and_catches() {
    const PROBE: &str = "PI_DELETED_CWD_CALLERS";
    if let Some(path) = std::env::var_os(PROBE) {
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&path).unwrap();
        std::fs::remove_dir(&path).unwrap();
        let native = env("base");
        let relative = JsString::from("child");
        let content = JsString::from("a");
        let mut actual = json!({});
        for name in [
            "absolutePath",
            "readTextFile",
            "readTextLines",
            "readBinaryFile",
            "writeFile",
            "appendFile",
            "fileInfo",
            "listDir",
            "canonicalPath",
            "exists",
            "createDir",
            "remove",
        ] {
            let result = match name {
                "absolutePath" => native.absolute_path(&relative, None).await.map(|value| json!(value)),
                "readTextFile" => native.read_text_file(&relative, None).await.map(|value| json!(value)),
                "readTextLines" => native
                    .read_text_lines(
                        &relative,
                        pi_agent_core::harness::ReadTextLinesOptions {
                            max_lines: Some(-1.0),
                            abort: None,
                        },
                    )
                    .await
                    .map(|value| json!(value)),
                "readBinaryFile" => native.read_binary_file(&relative, None).await.map(|value| json!(value)),
                "writeFile" => native
                    .write_file(&relative, FileContent::Text(&content), None)
                    .await
                    .map(|_| json!(null)),
                "appendFile" => native
                    .append_file(&relative, FileContent::Text(&content), None)
                    .await
                    .map(|_| json!(null)),
                "fileInfo" => native.file_info(&relative).await.map(file_info_value),
                "listDir" => native.list_dir(&relative, None).await.map(|_| json!(null)),
                "canonicalPath" => native.canonical_path(&relative).await.map(|value| json!(value)),
                "exists" => native.exists(&relative, None).await.map(|value| json!(value)),
                "createDir" => native
                    .create_dir(&relative, Default::default())
                    .await
                    .map(|_| json!(null)),
                "remove" => native.remove(&relative, Default::default()).await.map(|_| json!(null)),
                _ => unreachable!(),
            };
            actual[name] = file_error_value(result.unwrap_err());
        }
        actual["exec"] = shell_result(
            native
                .exec(
                    &"true".into(),
                    ShellExecOptions {
                        cwd: Some(relative.clone()),
                        ..Default::default()
                    },
                )
                .await,
        );
        actual["joinPath"] =
            json!({"returned":{"ok":true,"value":native.join_path(&["a".into(),"b".into()],None).await.unwrap()}});
        let prompts = pi_agent_core::harness::load_prompt_templates(native.as_ref(), &[&relative])
            .await
            .unwrap_err();
        actual["prompts"] = json!({"rejected":rejection_value(prompts)});
        let skills = pi_agent_core::harness::load_skills(native.as_ref(), &[&relative])
            .await
            .unwrap_err();
        actual["skills"] = match skills {
            OperationFailure::Rejected(error) => json!({"rejected":rejection_value(error)}),
            OperationFailure::Returned(error) => json!({"wrongIgnoreError":error.message}),
        };
        use pi_agent_core::harness::session::{JsonlSessionRepo, JsonlSessionRepoOptions};
        let repo = JsonlSessionRepo::new(JsonlSessionRepoOptions {
            fs: native.clone(),
            sessions_root: "sessions".into(),
        });
        actual["repo"] = match repo.list(Default::default()).await.unwrap_err() {
            OperationFailure::Rejected(error) => json!({"rejected":rejection_value(error)}),
            OperationFailure::Returned(error) => json!({"wrongSessionError":error.message}),
        };
        let environment: Arc<dyn pi_agent_core::harness::ExecutionEnv> = native;
        let capture = pi_agent_core::harness::utils::execute_shell_with_capture(
            &environment,
            &"true".into(),
            pi_agent_core::harness::utils::ShellCaptureOptions {
                cwd: Some(relative),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        let cause = capture
            .cause
            .as_ref()
            .unwrap()
            .downcast_ref::<NodeRejection>()
            .expect("original native Node cause")
            .clone();
        actual["capture"] = json!({"returned":{"ok":false,"error":{"name":"ExecutionError","code":capture.code.as_str(),"message":capture.message,"cause":rejection_value(cause)}}});
        std::env::set_current_dir(original).unwrap();
        let expected =
            pi_js_compat::js_json::parse_lossless(&std::env::var("PI_DELETED_CWD_EXPECTED").unwrap()).unwrap();
        assert!(
            actual == expected,
            "actual={}\nexpected={}",
            pi_js_compat::js_json::stringify_lossless(&actual),
            pi_js_compat::js_json::stringify_lossless(&expected)
        );
        return;
    }
    let native = tempfile::tempdir().unwrap();
    let reference = tempfile::tempdir().unwrap();
    let expected = oracle().request(&json!({"kind":"deleted-cwd-callers","root":reference.path()}));
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "deleted_cwd_rejection_propagates_through_all_resolve_methods_and_catches",
            "--nocapture",
        ])
        .env(PROBE, native.path())
        .env(
            "PI_DELETED_CWD_EXPECTED",
            pi_js_compat::js_json::stringify_lossless(&expected),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn raw_os_filename_and_realpath_bytes_match_node_repair_boundary() {
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().unwrap();
    let name = std::ffi::OsString::from_vec(b"raw-\xff-\xed\xa0\x80".to_vec());
    std::fs::write(root.path().join(&name), "raw bytes").unwrap();
    std::os::unix::fs::symlink(&name, root.path().join("link")).unwrap();
    let operations = json!([["canonicalPath", "link"], ["listDir", "."]]);
    let expected = oracle().request(&json!({"kind":"fs","root":root.path(),"operations":operations}));
    let env = env(root.path().to_str().unwrap());
    let mut actual = Vec::new();
    for args in operations.as_array().unwrap() {
        actual.push(
            read_operation(
                &env,
                &args[0].as_string().unwrap().to_utf8().unwrap(),
                args[1].as_string().unwrap(),
            )
            .await,
        );
    }
    let actual = JsValue::Array(actual);
    assert!(
        actual == expected,
        "actual={}\nexpected={}",
        pi_js_compat::js_json::stringify_lossless(&actual),
        pi_js_compat::js_json::stringify_lossless(&expected)
    );
    assert_eq!(std::fs::read(root.path().join(&name)).unwrap(), b"raw bytes");
}

fn native_error_shape(error: &(dyn std::error::Error + 'static)) -> JsValue {
    if let Some(error) = error.downcast_ref::<pi_agent_core::harness::FileError>() {
        let mut value = json!({"name":error.name,"message":error.message,"code":error.code.as_str()});
        if let Some(path) = &error.path {
            value["path"] = json!(path);
        }
        if let Some(cause) = &error.cause {
            value["cause"] = native_error_shape(cause.as_ref());
        }
        return value;
    }
    if let Some(error) = error.downcast_ref::<pi_agent_core::harness::ExecutionError>() {
        let mut value = json!({"name":error.name,"message":error.message,"code":error.code.as_str()});
        if let Some(cause) = &error.cause {
            value["cause"] = native_error_shape(cause.as_ref());
        }
        return value;
    }
    if let Some(error) = error.downcast_ref::<pi_js_compat::node_error::NativeNodeError>() {
        return error.value();
    }
    if let Some(error) = error.downcast_ref::<NodeRejection>() {
        return rejection_value(error.clone());
    }
    json!({"message":error.to_string()})
}
#[tokio::test]
async fn native_error_causes_retain_node_error_metadata_and_utf16() {
    let root = tempfile::tempdir().unwrap();
    let env = env(root.path().to_str().unwrap());
    let expected = oracle().request(&json!({"kind":"error-causes","root":root.path(),"withCauses":true}));
    let missing = JsString::from("missing") + JsString::from_utf16(vec![0xd800]);
    let file = operation::returned_error(env.read_text_file(&missing, None).await.unwrap_err());
    let invalid = operation::returned_error(env.file_info(&"bad\0path".into()).await.unwrap_err());
    let spawn = operation::returned_error(
        env.exec(
            &"true".into(),
            ShellExecOptions {
                cwd: Some("missing".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err(),
    );
    let arguments = operation::returned_error(env.exec(&"bad\0command".into(), Default::default()).await.unwrap_err());
    let callback = operation::returned_error(
        env.exec(
            &"printf out".into(),
            ShellExecOptions {
                on_stdout: Some(Arc::new(|_| {
                    Err(JsString::from("native") + JsString::from_utf16(vec![0xd800]) + "cause")
                })),
                ..Default::default()
            },
        )
        .await
        .unwrap_err(),
    );
    let actual = json!([
        native_error_shape(&file),
        native_error_shape(&invalid),
        native_error_shape(&spawn),
        native_error_shape(&arguments),
        native_error_shape(&callback)
    ]);
    assert!(
        actual == expected,
        "actual={}\nexpected={}",
        pi_js_compat::js_json::stringify_lossless(&actual),
        pi_js_compat::js_json::stringify_lossless(&expected)
    );
}

#[tokio::test]
async fn in_flight_abort_errors_preserve_nested_native_cause_records() {
    use std::io::Write;
    for mode in ["read", "binary", "write", "lines"] {
        let native = tempfile::tempdir().unwrap();
        let reference = tempfile::tempdir().unwrap();
        let kind = match mode {
            "write" => "write-abort",
            "lines" => "read-lines-abort",
            _ => "read-abort",
        };
        let method = if mode == "binary" {
            "readBinaryFile"
        } else {
            "readTextFile"
        };
        let expected =
            oracle().request(&json!({"kind":kind,"root":reference.path(),"method":method,"withCauses":true}));
        let path = native
            .path()
            .join(if mode == "write" { "write.fifo" } else { "read.fifo" });
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let environment = env(native.path().to_str().unwrap());
        let token = CancellationToken::new();
        let signal = token.clone();
        let actual = if mode == "write" {
            let task = tokio::spawn(async move {
                environment
                    .write_file(&"write.fifo".into(), FileContent::Text(&"hello".into()), Some(&signal))
                    .await
            });
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            token.cancel();
            let text = tokio::task::spawn_blocking(move || std::fs::read_to_string(path).unwrap())
                .await
                .unwrap();
            let error = operation::returned_error(task.await.unwrap().unwrap_err());
            json!({"write":{"ok":false,"error":native_error_shape(&error)},"text":text})
        } else {
            let mut writer = std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
            writer.write_all(b"prefix").unwrap();
            let task = tokio::spawn(async move {
                match mode {
                    "read" => environment
                        .read_text_file(&"read.fifo".into(), Some(&signal))
                        .await
                        .map(|_| ()),
                    "binary" => environment
                        .read_binary_file(&"read.fifo".into(), Some(&signal))
                        .await
                        .map(|_| ()),
                    "lines" => environment
                        .read_text_lines(
                            &"read.fifo".into(),
                            pi_agent_core::harness::ReadTextLinesOptions {
                                abort: Some(signal),
                                max_lines: None,
                            },
                        )
                        .await
                        .map(|_| ()),
                    _ => unreachable!(),
                }
            });
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            token.cancel();
            if mode != "lines" {
                drop(writer);
                let error = operation::returned_error(task.await.unwrap().unwrap_err());
                json!({"ok":false,"error":native_error_shape(&error)})
            } else {
                let result = tokio::time::timeout(std::time::Duration::from_secs(2), task).await;
                drop(writer);
                let error = operation::returned_error(result.unwrap().unwrap().unwrap_err());
                json!({"ok":false,"error":native_error_shape(&error)})
            }
        };
        let actual = normalize_root(actual, native.path().to_str().unwrap());
        let expected = normalize_root(expected, reference.path().to_str().unwrap());
        assert!(
            actual == expected,
            "{mode}: actual={}\nexpected={}",
            pi_js_compat::js_json::stringify_lossless(&actual),
            pi_js_compat::js_json::stringify_lossless(&expected)
        );
    }
}
