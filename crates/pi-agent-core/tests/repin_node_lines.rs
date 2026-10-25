use crate::shared_operation as operation;
use crate::shared_oracle as oracle;
use pi_agent_core::harness::{FileSystem, NodeExecutionEnv, NodeExecutionEnvOptions, ReadTextLinesOptions};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue},
};
#[tokio::test]
async fn read_lines_matches_frozen_binary64_limit_and_line_ending_domains() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-node-lines-oracle.ts",
        "packages/agent-core/src/harness/env/nodejs.ts",
    );
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_str().unwrap();
    let env = NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.into(),
        ..Default::default()
    });
    for content in ["one\ntwo\nthree\n", "a\rb\r\nc\n\n", "", "\r\n", "😀\rtext"] {
        std::fs::write(temp.path().join("native.txt"), content).unwrap();
        for (encoded, limit) in [
            ("0", 0.0),
            ("-1", -1.0),
            ("-Infinity", f64::NEG_INFINITY),
            ("0.1", 0.1),
            ("1.1", 1.1),
            ("2.25", 2.25),
            ("NaN", f64::NAN),
            ("Infinity", f64::INFINITY),
            ("9007199254740992", 9007199254740992.0),
        ] {
            let expected: JsValue = oracle.request(&json!({"root":root,"content":content,"limit":encoded}));
            let value = env
                .read_text_lines(
                    &JsString::from("native.txt"),
                    ReadTextLinesOptions {
                        max_lines: Some(limit),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                json!({"ok":true,"value":value}),
                expected,
                "limit {encoded} content {content:?}"
            );
        }
    }
    let expected: JsValue = oracle
        .request(&json!({"root":tempfile::tempdir().unwrap().path().to_str().unwrap(),"missing":true,"limit":"-1"}));
    let value = env
        .read_text_lines(
            &"missing.txt".into(),
            ReadTextLinesOptions {
                max_lines: Some(-1.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(json!({"ok":true,"value":value}), expected);
}

#[tokio::test]
async fn read_line_limit_closes_fifo_reader_before_writer_eof_like_frozen() {
    use std::io::Write;
    use std::sync::Arc;
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-node-lines-oracle.ts",
        "packages/agent-core/src/harness/env/nodejs.ts",
    );
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_str().unwrap();
    let expected: JsValue = oracle.request(&json!({"root":root,"fifo":true}));
    let path = temp.path().join("native.fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let mut writer = std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
    writer.write_all(b"first\n").unwrap();
    let env = Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.into(),
        ..Default::default()
    }));
    let path: JsString = path.to_str().unwrap().into();
    let mut reader = tokio::spawn(async move {
        env.read_text_lines(
            &path,
            ReadTextLinesOptions {
                max_lines: Some(1.0),
                ..Default::default()
            },
        )
        .await
    });
    let early = tokio::time::timeout(std::time::Duration::from_millis(200), &mut reader).await;
    let returned_before_eof = early.is_ok();
    drop(writer);
    let value = match early {
        Ok(result) => result.unwrap().unwrap(),
        Err(_) => reader.await.unwrap().unwrap(),
    };
    assert!(returned_before_eof, "line limit must not wait for FIFO EOF");
    assert_eq!(json!({"ok":true,"value":value}), expected);
}

#[tokio::test]
async fn read_lines_aborts_a_pending_fifo_read_before_writer_eof_like_frozen() {
    use std::io::Write;
    use std::sync::Arc;
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-node-lines-oracle.ts",
        "packages/agent-core/src/harness/env/nodejs.ts",
    );
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_str().unwrap();
    let expected: JsValue = oracle.request(&json!({"root":root,"fifo":true,"abort":true}));
    let path = temp.path().join("native.fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let mut writer = std::fs::OpenOptions::new().read(true).write(true).open(&path).unwrap();
    writer.write_all(b"partial").unwrap();
    let env = Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: root.into(),
        ..Default::default()
    }));
    let native_path: JsString = path.to_str().unwrap().into();
    let read_path = native_path.clone();
    let controller_abort = pi_js_compat::abort::AbortController::new();
    let abort = controller_abort.signal();
    let token = abort.clone();
    let mut reader = tokio::spawn(async move {
        env.read_text_lines(
            &read_path,
            ReadTextLinesOptions {
                max_lines: Some(1.0),
                abort: Some(token),
            },
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let opened = std::fs::read_dir("/proc/self/fd")
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| std::fs::read_link(entry.path()).ok().as_ref() == Some(&path))
                .count();
            if opened >= 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    controller_abort.abort(None);
    let early = tokio::time::timeout(std::time::Duration::from_millis(200), &mut reader).await;
    let returned_before_eof = early.is_ok();
    drop(writer);
    let error = match early {
        Ok(result) => result.unwrap().map_err(operation::returned_error).unwrap_err(),
        Err(_) => reader.await.unwrap().map_err(operation::returned_error).unwrap_err(),
    };
    assert!(returned_before_eof, "abort must not wait for FIFO EOF");
    assert_eq!(error.path, Some(native_path));
    assert_eq!(json!({"code":error.code.as_str(),"message":error.message}), expected);
}
