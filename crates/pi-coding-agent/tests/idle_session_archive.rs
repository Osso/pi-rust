use crate::shared_oracle as oracle;

use pi_ai::request::{BoxFuture, CallbackError};
use pi_coding_agent::core::agent_session_runtime::{
    CreateAgentSessionRuntimeFactory, CreateAgentSessionRuntimeResult, create_agent_session_runtime,
};
use pi_coding_agent::core::session_control_db::read_session_metadata;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json::JsString;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

const CONTENT: &str = "{\"type\":\"session\",\"version\":3,\"id\":\"idle-old\",\"cwd\":\"/fixture-project\",\"timestamp\":\"2020-01-01T00:00:00.000Z\"}\n{\"type\":\"message\",\"id\":\"m1\",\"parentId\":null,\"timestamp\":\"2020-01-01T00:00:00.000Z\",\"message\":{\"role\":\"user\",\"content\":\"archive fixture\",\"timestamp\":1}}\n";

#[path = "support/idle_archive_cases.rs"]
mod cases;

#[tokio::test]
async fn startup_archive_runs_before_factory_failure_without_blocking_the_factory_result() {
    let temp = tempfile::tempdir().unwrap();
    let source_root = temp.path().join("source");
    let native_root = temp.path().join("native");
    std::fs::create_dir(&source_root).unwrap();
    std::fs::create_dir(&native_root).unwrap();
    let mut source = oracle::TsOracle::spawn("idle-session-archive-startup-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"root":source_root,"content":CONTENT}));
    assert_eq!(
        expected,
        json!({"ok":true,"factoryError":"fixture factory stop","archived":true,"archivedMetadata":true,"content":CONTENT})
    );
    let cwd = native_root.join("work");
    let agent_dir = native_root.join("agent");
    let project = agent_dir.join("sessions/old-project");
    std::fs::create_dir(&cwd).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    let file = project.join("old.jsonl");
    std::fs::write(&file, CONTENT).unwrap();
    std::fs::File::open(&file)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1600000000))
        .unwrap();
    let db = JsString::from(native_root.join("control.db").to_str().unwrap());
    let mut manager = SessionManager::create(
        &cwd.to_str().unwrap().into(),
        Some(&native_root.join("current").to_str().unwrap().into()),
        None,
    )
    .unwrap();
    manager
        .set_metadata_control_db_path(Some(&db), Default::default())
        .unwrap();
    let factory: CreateAgentSessionRuntimeFactory = Arc::new(|_| {
        Box::pin(async { Err(CallbackError::new("fixture factory stop")) })
            as BoxFuture<'static, Result<CreateAgentSessionRuntimeResult, CallbackError>>
    });
    let factory_error = match create_agent_session_runtime(
        factory,
        &cwd.to_str().unwrap().into(),
        &agent_dir.to_str().unwrap().into(),
        manager,
        None,
        None,
    )
    .await
    {
        Err(error) => error.message,
        Ok(_) => panic!("fixture factory must reject"),
    };
    let archive = project.join("old.jsonl.zst");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while (!archive.exists() || file.exists()) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let metadata = read_session_metadata(&db, JsString::from(archive.to_str().unwrap())).unwrap();
    let content = archive.exists().then(|| {
        String::from_utf8(zstd::stream::decode_all(std::fs::read(&archive).unwrap().as_slice()).unwrap()).unwrap()
    });
    let actual = json!({"ok":true,"factoryError":factory_error,"archived":archive.exists() && !file.exists(),"archivedMetadata":metadata.and_then(|metadata|metadata.is_archived)==Some(true),"content":content});
    assert_eq!(actual, expected);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
