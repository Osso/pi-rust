//! `rpc-entry.ts` entry wiring.

use std::sync::Mutex;

use pi_coding_agent::rpc_entry::{RPC_MODE_ARGS, RPC_PROCESS_ENV, RpcMain, run_rpc_entry};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn rpc_process_env_matches_the_typescript_entry_point() {
    assert_eq!(RPC_PROCESS_ENV, [("PI_CODING_AGENT", "true")]);
    assert_eq!(RPC_MODE_ARGS, ["--mode", "rpc"]);
}

#[tokio::test]
async fn rpc_entry_prepends_the_rpc_mode_arguments_and_returns_the_configured_fetch() {
    let seen = Mutex::new(Vec::new());
    let main: RpcMain<'_> = Box::new(|args: Vec<String>| {
        *seen.lock().unwrap() = args;
        Box::pin(async { Ok(()) })
    });

    let fetch = run_rpc_entry(&args(&["--provider", "faux"]), main)
        .await
        .expect("rpc entry must configure the dispatcher");

    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            "--mode".to_owned(),
            "rpc".to_owned(),
            "--provider".to_owned(),
            "faux".to_owned()
        ]
    );
    assert!(
        fetch
            .fetch(
                pi_ai::request::HttpRequest {
                    method: "GET".to_owned(),
                    url: "http://127.0.0.1:1/never".to_owned(),
                    headers: Vec::new(),
                    body: None,
                },
                Some(pi_js_compat::abort::AbortSignal::default()),
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rpc_entry_propagates_main_errors() {
    let main: RpcMain<'_> = Box::new(|_args: Vec<String>| Box::pin(async { Err("boom".to_owned()) }));
    let error = run_rpc_entry(&[], main).await.err().expect("main error propagates");
    assert_eq!(error, "boom");
}
