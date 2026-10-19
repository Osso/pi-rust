//! Process-level uncaught errors (`utils/uncaught.rs`).

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::Command;

use pi_coding_agent::utils::uncaught::{
    ERROR, UncaughtError, format_uncaught, install_uncaught_handler, throw_uncaught,
};

#[test]
fn formats_a_plain_error_like_node() {
    let message = String::from("boom");
    assert_eq!(format_uncaught(&message), "Error: boom");
    assert_eq!(format_uncaught(&"boom"), "Error: boom");
}

#[test]
fn formats_a_named_error_like_node() {
    let payload: Box<dyn Any + Send> = Box::new(UncaughtError {
        label: "SyntaxError",
        message: "Unexpected token".to_owned(),
    });
    assert_eq!(format_uncaught(payload.as_ref()), "SyntaxError: Unexpected token");
}

#[test]
fn throw_uncaught_carries_the_node_label_and_message() {
    let payload = catch_unwind(AssertUnwindSafe(|| throw_uncaught("TypeError", "not a function")))
        .expect_err("throw_uncaught must unwind");
    let error = payload
        .downcast_ref::<UncaughtError>()
        .expect("payload is an UncaughtError");
    assert_eq!(error.label, "TypeError");
    assert_eq!(error.message, "not a function");
    assert_eq!(format_uncaught(payload.as_ref()), "TypeError: not a function");
}

#[test]
fn uncaught_handler_prints_the_error_and_exits_one() {
    let output = Command::new(std::env::current_exe().expect("test executable"))
        .env("PI_RUST_UNCAUGHT_PROBE", "1")
        .args([
            "--ignored",
            "--exact",
            "uncaught_handler_child",
            "--test-threads=1",
            "--nocapture",
        ])
        .output()
        .expect("child must run");
    assert_eq!(output.status.code(), Some(1), "Node exits 1, not Rust's 101");
    assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), "Error: boom");
}

#[test]
#[ignore = "child process entry point"]
fn uncaught_handler_child() {
    if std::env::var_os("PI_RUST_UNCAUGHT_PROBE").is_none() {
        return;
    }
    install_uncaught_handler();
    throw_uncaught(ERROR, "boom");
}
