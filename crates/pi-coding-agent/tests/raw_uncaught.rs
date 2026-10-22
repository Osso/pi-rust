use pi_coding_agent::utils::uncaught::{UncaughtError, format_uncaught, install_uncaught_handler, throw_uncaught};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
fn raw(case: &str) -> JsString {
    match case {
        "high" => JsString::from_utf16(vec![0xd800]),
        "low" => JsString::from_utf16(vec![0xdc00]),
        _ => JsString::from("😀"),
    }
}
#[test]
fn native_uncaught_payload_and_formatter_keep_source_units() {
    let value = raw("high");
    let error = std::panic::catch_unwind(|| throw_uncaught("Error", value.clone())).unwrap_err();
    let error = error.downcast_ref::<UncaughtError>().unwrap();
    assert_eq!(error.message, value);
    assert_eq!(format_uncaught(error), JsString::from("Error: ") + value);
}
#[test]
fn actual_uncaught_handler_encodes_only_at_stderr_write() {
    for case in ["high", "low", "pair"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "uncaught_child", "--nocapture"])
            .env("PI_RAW_UNCAUGHT_CHILD", case)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(
            output.stderr,
            encode_node_utf8(&(JsString::from("Error: ") + raw(case) + "\n")).as_bytes()
        );
    }
}
#[test]
fn uncaught_child() {
    if let Ok(case) = std::env::var("PI_RAW_UNCAUGHT_CHILD") {
        install_uncaught_handler();
        throw_uncaught("Error", raw(&case));
    }
}
