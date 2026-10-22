//! Process-level uncaught errors (`utils/uncaught.rs`).

use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::Command;
use std::sync::Arc;

use pi_coding_agent::utils::uncaught::{
    ERROR, ProcessCrashHandler, UncaughtError, format_uncaught, install_uncaught_handler, set_process_crash_handler,
    swallow_panics, throw_uncaught,
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
        message: JsString::from("Unexpected token"),
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

fn run_crash_handler_child(mode: &str) -> std::process::Output {
    Command::new(std::env::current_exe().expect("test executable"))
        .env("PI_RUST_UNCAUGHT_PROBE", mode)
        .args([
            "--ignored",
            "--exact",
            "crash_handler_child",
            "--test-threads=1",
            "--nocapture",
        ])
        .output()
        .expect("child must run")
}

fn reporting_handler() -> ProcessCrashHandler {
    Arc::new(|payload| {
        let line = js_format!("crash handler: {}\n", format_uncaught(payload));
        std::io::Write::write_all(&mut std::io::stderr(), encode_node_utf8(&line).as_bytes()).expect("stderr");
        std::process::exit(7);
    })
}

#[test]
fn process_crash_handler_receives_uncaught_panics() {
    let output = run_crash_handler_child("panic");
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "crash handler: Error: boom"
    );
}

#[test]
fn process_crash_handler_receives_uncaught_throws() {
    let output = run_crash_handler_child("throw");
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "crash handler: TypeError: not a function"
    );
}

#[test]
fn clearing_the_process_crash_handler_restores_the_default_report() {
    let output = run_crash_handler_child("cleared");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), "Error: boom");
}

#[test]
fn process_crash_handler_receives_panics_from_other_threads() {
    let output = run_crash_handler_child("worker");
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "crash handler: Error: boom"
    );
}

#[test]
fn unregistering_from_another_thread_restores_the_default_report() {
    let output = run_crash_handler_child("unregister-worker");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), "Error: boom");
}

#[test]
fn swallowed_panics_do_not_reach_the_handler_or_stop_later_steps() {
    let output = run_crash_handler_child("swallowed");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "after swallowed panic\nError: boom"
    );
}

#[test]
fn set_process_crash_handler_reports_the_previous_handler() {
    assert!(set_process_crash_handler(Some(reporting_handler())).is_none());
    assert!(set_process_crash_handler(None).is_some());
    assert!(set_process_crash_handler(None).is_none());
}

#[test]
#[ignore = "child process entry point"]
fn crash_handler_child() {
    let Some(mode) = std::env::var_os("PI_RUST_UNCAUGHT_PROBE") else {
        return;
    };
    install_uncaught_handler();
    match mode.to_str() {
        Some("panic") => {
            set_process_crash_handler(Some(reporting_handler()));
            panic!("boom");
        }
        Some("worker") => {
            set_process_crash_handler(Some(reporting_handler()));
            std::thread::spawn(|| panic!("boom"))
                .join()
                .expect("handler exits first");
        }
        Some("unregister-worker") => {
            set_process_crash_handler(Some(reporting_handler()));
            std::thread::spawn(|| set_process_crash_handler(None))
                .join()
                .expect("unregister");
            panic!("boom");
        }
        Some("throw") => {
            set_process_crash_handler(Some(reporting_handler()));
            throw_uncaught("TypeError", "not a function");
        }
        Some("cleared") => {
            set_process_crash_handler(Some(reporting_handler()));
            set_process_crash_handler(None);
            panic!("boom");
        }
        Some("swallowed") => {
            set_process_crash_handler(Some(reporting_handler()));
            swallow_panics(|| panic!("swallowed"));
            swallow_panics(|| throw_uncaught("TypeError", "also swallowed"));
            eprintln!("after swallowed panic");
            set_process_crash_handler(None);
            panic!("boom");
        }
        _ => panic!("unknown probe mode {mode:?}"),
    }
}
