#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::footer_data_provider::{BranchChangeSubscription, FooterDataProvider};
use pi_js_compat::js_json;
use std::sync::{Arc, Mutex};

use pi_ai::request::CallbackError;
use pi_js_compat::abort::AbortReason;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};

const SOURCE: &str = "packages/coding-agent/src/core/footer-data-provider.ts";

#[tokio::test]
async fn git_discovery_continues_past_non_file_metadata_but_stops_at_missing_head() {
    let root = tempfile::tempdir().unwrap();
    let outer = root.path().join("repo");
    std::fs::create_dir_all(outer.join(".git")).unwrap();
    std::fs::write(outer.join(".git/HEAD"), "ref: refs/heads/parent\n").unwrap();
    let mut source = oracle::TsOracle::spawn("footer-provider-oracle.mts", SOURCE);
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    let mut sockets = Vec::new();
    for kind in [
        "fifo",
        "socket",
        "invalid_file",
        "missing_head",
        "missing_worktree_head",
        "broken_link",
    ] {
        let dir = outer.join(kind);
        std::fs::create_dir(&dir).unwrap();
        let metadata = dir.join(".git");
        match kind {
            "fifo" => rustix::fs::mknodat(
                rustix::fs::CWD,
                &metadata,
                rustix::fs::FileType::Fifo,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                0,
            )
            .unwrap(),
            "socket" => sockets.push(std::os::unix::net::UnixListener::bind(&metadata).unwrap()),
            "invalid_file" => std::fs::write(&metadata, "not a worktree").unwrap(),
            "missing_head" => std::fs::create_dir(&metadata).unwrap(),
            "missing_worktree_head" => std::fs::write(&metadata, "gitdir: ../absent").unwrap(),
            "broken_link" => std::os::unix::fs::symlink("missing", &metadata).unwrap(),
            _ => unreachable!(),
        }
        let cwd = dir.to_str().unwrap();
        expected.push(source.request(&js_json!({"operation":"metadata","cwd":cwd})));
        let provider = FooterDataProvider::new(cwd);
        actual.push(js_json!(provider.get_git_branch()));
        provider.dispose();
    }
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn synchronous_branch_callback_rejection_retains_its_original_error_and_stops_iteration() {
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().to_str().unwrap();
    let mut source = oracle::TsOracle::spawn("footer-provider-oracle.mts", SOURCE);
    let expected = source.request(&js_json!({"operation":"throw","cwd":cwd}));
    let provider = FooterDataProvider::new(cwd);
    let trace = Arc::new(Mutex::new(Vec::<String>::new()));
    let first = trace.clone();
    let _first = provider.on_branch_change(move || -> Result<(), CallbackError> {
        first.lock().unwrap().push("throw".into());
        Err(NativeNodeError::from_constructor(
            NativeErrorClass::TypeError,
            pi_js_compat::js_json::JsString::from("branch-")
                + pi_js_compat::js_json::JsString::from_utf16(vec![0xd800]),
        )
        .with_property("code", JsValue::String("EBRANCH".into()))
        .into())
    });
    let second = trace.clone();
    let _second = provider.on_branch_change(move || {
        second.lock().unwrap().push("removed".into());
        Ok(())
    });
    let error = provider.set_cwd(format!("{cwd}/next")).unwrap_err();
    let error = match error.thrown_value() {
        AbortReason::Error(error) => error.value(),
        _ => panic!("expected original Error"),
    };
    provider.dispose();
    assert_eq!(js_json!({"trace":*trace.lock().unwrap(),"error":error}), expected);
}

#[tokio::test]
async fn branch_callbacks_follow_live_set_removal_addition_and_disposal() {
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().to_str().unwrap();
    let mut source = oracle::TsOracle::spawn("footer-provider-oracle.mts", SOURCE);
    for operation in ["live", "dispose"] {
        let expected = source.request(&js_json!({"operation":operation,"cwd":cwd}));
        let provider = Arc::new(FooterDataProvider::new(cwd));
        let trace = Arc::new(Mutex::new(Vec::<String>::new()));
        let subscriptions = Arc::new(Mutex::new(Vec::<BranchChangeSubscription>::new()));
        let first = Arc::new(Mutex::new(None));
        let second = Arc::new(Mutex::new(None));
        let first_callback = first.clone();
        let second_callback = second.clone();
        let callback_provider = Arc::downgrade(&provider);
        let callback_trace = trace.clone();
        let retained = subscriptions.clone();
        *first.lock().unwrap() = Some(provider.on_branch_change(move || {
            let provider = callback_provider.upgrade().unwrap();
            if operation == "dispose" {
                callback_trace.lock().unwrap().push("dispose".into());
                provider.dispose();
            } else {
                callback_trace.lock().unwrap().push("first".into());
                drop(first_callback.lock().unwrap().take());
                drop(second_callback.lock().unwrap().take());
                let added_trace = callback_trace.clone();
                retained.lock().unwrap().push(provider.on_branch_change(move || {
                    added_trace.lock().unwrap().push("added".into());
                    Ok(())
                }));
            }
            Ok(())
        }));
        let second_trace = trace.clone();
        *second.lock().unwrap() = Some(provider.on_branch_change(move || {
            second_trace.lock().unwrap().push("removed".into());
            Ok(())
        }));
        provider.set_cwd(format!("{cwd}/next")).unwrap();
        if operation == "live" {
            provider.set_cwd(format!("{cwd}/again")).unwrap();
        }
        let actual = js_json!(*trace.lock().unwrap());
        provider.dispose();
        assert_eq!(actual, expected, "{operation}");
    }
}
