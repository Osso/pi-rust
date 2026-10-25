#[path = "../examples/sdk/11_sessions.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
use serde_json::json;

#[tokio::test]
async fn lists_only_three_of_four_persisted_sessions_and_reopens_latest() {
    let mut a = support::fixture().await;
    let mut b = support::fixture().await;
    let mut c = support::fixture().await;
    let mut d = support::fixture().await;
    // SessionManager continuation uses the runner's private default metadata DB.
    // All four SDK sessions and listing must use that same DB, as in the source.
    for options in [&mut a.options, &mut b.options, &mut c.options, &mut d.options] {
        options.control_db_path = None;
    }
    let cwd = a.options.cwd.clone().unwrap();
    let dir = a.home.path().join("sessions").display().to_string();
    for index in 0..4 {
        let mut session = SessionManager::create(&cwd, Some(&JsString::from(&dir)), None).unwrap();
        session.persist_for_recovery().unwrap();
        let file = std::fs::File::options()
            .write(true)
            .open(encode_node_utf8(session.get_session_file().unwrap()))
            .unwrap();
        file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(10 - index))
            .unwrap();
    }
    let files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|file| {
            let path = file.unwrap().path();
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    let mut state_source = oracle::TsOracle::spawn(
        "sdk-sessions-state-oracle.ts",
        "packages/coding-agent/src/core/session-manager.ts",
    );
    let ts_continued = state_source.request(&json!({"operation":"continue","cwd":cwd,"dir":dir}));
    let rs_continued = SessionManager::continue_recent(&cwd, Some(&JsString::from(&dir))).unwrap();
    assert_eq!(
        json!(rs_continued.get_session_file()),
        ts_continued["path"],
        "real TS/Rust continuation selection"
    );
    let ts_list = state_source
        .request(&json!({"operation":"list","cwd":cwd,"dir":dir,"db":a.home.path().join("ts-list.sqlite")}));
    let rs_list = SessionManager::list(&cwd, Some(&JsString::from(&dir)), None, None).unwrap();
    let rs_list: Vec<_> = rs_list
        .iter()
        .map(|s| json!({"path":s.path,"id":s.id,"firstMessage":s.first_message}))
        .collect();
    assert_eq!(json!(rs_list), ts_list, "real TS/Rust listing order");
    for (path, bytes) in files {
        assert_eq!(
            std::fs::read(path).unwrap(),
            bytes,
            "both implementations consumed identical unchanged fixture bytes"
        );
    }
    let output = support::Output::default();
    let _main = example::main;
    let sessions = example::run(
        [a.options, b.options, c.options, d.options],
        Some(&JsString::from(&dir)),
        output.clone(),
    )
    .await
    .unwrap();
    assert_eq!(sessions.len(), 4);
    assert_eq!(output.text().lines().filter(|line| line.starts_with("  ")).count(), 3);
    let new_file = output
        .text()
        .lines()
        .find_map(|line| line.strip_prefix("New session file: ").map(str::to_owned))
        .unwrap();
    assert!(
        !std::path::Path::new(&new_file).exists(),
        "no assistant response: new session stays lazy"
    );
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/11-sessions.ts",
    );
    let sessions: Vec<_> = sessions
        .iter()
        .map(|s| json!({"path":s.path,"id":s.id,"firstMessage":s.first_message}))
        .collect();
    let expected =
        source.request(&json!({"newFile":new_file,"continuedFile":ts_continued["path"],"sessions":sessions}));
    assert_eq!(output.text(), expected["stdout"]);
    assert!(a.requests.lock().unwrap().is_empty());
    let _homes = (a.home, b.home, c.home, d.home);
}
