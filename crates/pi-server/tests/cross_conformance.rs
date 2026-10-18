#[path = "support/ts_script.rs"]
mod ts_script;

use std::ffi::OsString;
use std::sync::Arc;

use pi_server::testing::TestServerService;
use pi_server::transports::unix::{UnixServerOptions, create_unix_server};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typescript_testing_client_conforms_with_the_rust_server() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("server.sock").to_string_lossy().into_owned();
    let service = Arc::new(TestServerService::new());
    let server = Arc::new(
        create_unix_server(
            service,
            UnixServerOptions {
                path: path.clone(),
                ..UnixServerOptions::default()
            },
        )
        .expect("server options"),
    );
    server.start().await.expect("server must start");

    let output = ts_script::run_oracle_script(
        "server-conformance-client.ts",
        &[OsString::from(&path), ts_script::source_worktree().into_os_string()],
    );
    let stdout = String::from_utf8(output.stdout).expect("oracle stdout must be UTF-8");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "TypeScript conformance client failed: {stdout}\n{stderr}"
    );
    let last_line = stdout.lines().last().expect("conformance client output");
    let response: serde_json::Value = serde_json::from_str(last_line).expect("JSON response");
    assert_eq!(response["ok"], true, "conformance client reported: {response}");

    server.close().await.expect("server must close");
}
