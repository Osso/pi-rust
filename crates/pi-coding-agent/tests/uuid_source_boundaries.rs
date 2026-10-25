#[path = "support/oracle.rs"]
mod oracle;

#[test]
fn titlebar_timer_set_title_failure_is_process_uncaught_in_actual_frozen_extension() {
    let mut source = oracle::TsOracle::spawn(
        "titlebar-uuid-error-oracle.mts",
        "packages/coding-agent/examples/extensions/titlebar-spinner.ts",
    );
    let observed = source.request(&serde_json::json!({}));
    assert_eq!(
        observed,
        serde_json::json!({
            "boundary":"uncaughtException", "calls":2,
            "error":{"name":"Error","message":"Deriving bits failed","code":"ERR_CRYPTO_OPERATION_FAILED"}
        })
    );
}

#[path = "support/extensions.rs"]
mod fixture;
#[path = "../examples/extensions/titlebar_spinner.rs"]
mod titlebar;

use pi_coding_agent::core::extensions::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn native_titlebar_timer_preserves_the_proven_frozen_uncaught_boundary() {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "uuid_timer_native_child", "--nocapture"])
        .env("PI_UUID_TIMER_CHILD", "1")
        .output()
        .unwrap();
    assert_eq!(
        child.status.code(),
        Some(23),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    let line = String::from_utf8(child.stdout)
        .unwrap()
        .lines()
        .find(|line| line.starts_with("UUID_TIMER="))
        .unwrap()
        .to_owned();
    let observed: serde_json::Value = serde_json::from_str(line.strip_prefix("UUID_TIMER=").unwrap()).unwrap();
    assert_eq!(
        observed,
        serde_json::json!({"calls":2,"error":{"name":"Error","message":"Deriving bits failed","code":"ERR_CRYPTO_OPERATION_FAILED"}})
    );
}

struct RejectTitle(Arc<AtomicUsize>);
impl ExtensionUiContext for RejectTitle {
    fn set_title(&self, _: &pi_js_compat::js_json::JsString) -> Result<(), pi_ai::request::CallbackError> {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            return Ok(());
        }
        Err(
            pi_js_compat::node_error::NativeNodeError::new("Error", "Deriving bits failed")
                .with_property("code", pi_js_compat::js_json!("ERR_CRYPTO_OPERATION_FAILED"))
                .into(),
        )
    }
}

#[tokio::test]
#[ignore = "private process crash-hook entry point"]
async fn uuid_timer_native_child() {
    if std::env::var_os("PI_UUID_TIMER_CHILD").is_none() {
        return;
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    pi_coding_agent::utils::uncaught::set_process_crash_handler(Some(Arc::new(move |payload| {
        let error = payload
            .downcast_ref::<pi_ai::request::CallbackError>()
            .expect("original callback error");
        println!(
            "UUID_TIMER={}",
            pi_js_compat::js_json::stringify_lossless(
                &pi_js_compat::js_json!({"calls":observed.load(Ordering::SeqCst),"error":error.thrown_value().value()})
            )
        );
        std::process::exit(23);
    })));
    let runtime = fixture::runtime();
    let extension = fixture::extension(&runtime, "/fixture", "<uuid-timer>", |api| {
        titlebar::titlebar_spinner_extension(api).unwrap()
    })
    .await;
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        "/fixture",
        fixture::session_manager("/fixture"),
        fixture::model_registry().await,
        None,
    );
    runner
        .bind_core(fixture::actions(), fixture::context_actions(), None)
        .unwrap();
    runner.set_ui_context(Some(Arc::new(RejectTitle(calls))), ExtensionMode::Rpc);
    let mut event: ExtensionEvent = serde_json::from_value(serde_json::json!({"type":"agent_start"})).unwrap();
    runner.emit(&mut event).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    std::process::exit(0);
}
