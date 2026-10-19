//! `~` expansion observes runtime `process.env` overlay mutations like Node.

use pi_agent_core::harness::session::jsonl::types::{JsonlSessionRepoFileSystem, NativeFileSystem};
use pi_js_compat::process_env;

#[tokio::test]
async fn home_expansion_observes_process_env_overlay() {
    process_env::set("HOME", "/overlay/home");
    let filesystem = NativeFileSystem::new("/work");
    let resolved = filesystem
        .absolute_path("~/session.jsonl")
        .await
        .expect("absolute path");
    process_env::remove("HOME");

    assert_eq!(resolved, "/overlay/home/session.jsonl");
}
