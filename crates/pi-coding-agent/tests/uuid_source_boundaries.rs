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
