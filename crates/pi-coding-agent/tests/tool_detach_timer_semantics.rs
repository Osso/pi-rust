use crate::shared_oracle as oracle;
use pi_coding_agent::core::tool_detach_registry::{ToolDetachHandle, ToolDetachRegistry};
use pi_js_compat::{callback_error::CallbackError, js_json as json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};
struct CountHandle(Arc<AtomicUsize>);
impl ToolDetachHandle for CountHandle {
    fn detach(&self) -> Result<bool, CallbackError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(true)
    }
}

#[test]
fn old_duplicate_timers_check_membership_after_unregister_or_reregister() {
    let mut source = oracle::TsOracle::spawn(
        "tool-detach-registry-oracle.ts",
        "packages/coding-agent/src/core/tool-detach-registry.ts",
    );
    for register_again in [false, true] {
        let expected =
            source.request(&json!({"id":1,"operation":"duplicateTimerReference","registerAgain":register_again}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        let expected_calls = if register_again { 2 } else { 0 };
        assert_eq!(
            expected["output"],
            json!({"calls":expected_calls,"hasRunning":register_again}),
            "the older timer must check current membership"
        );
        let calls = Arc::new(AtomicUsize::new(0));
        let registry = Arc::new(ToolDetachRegistry::with_auto_detach_after_ms(40.0));
        let handle = Arc::new(CountHandle(calls.clone()));
        let first = registry.register(handle.clone());
        let second = registry.register(handle.clone());
        first.unregister();
        let third = register_again.then(|| registry.register(handle));
        std::thread::sleep(Duration::from_millis(160));
        assert_eq!(
            json!({"calls":calls.load(Ordering::SeqCst),"hasRunning":registry.has_running()}),
            expected["output"]
        );
        drop((first, second, third));
    }
}
struct NotifyHandle(std::sync::mpsc::Sender<()>);
impl ToolDetachHandle for NotifyHandle {
    fn detach(&self) -> Result<bool, CallbackError> {
        self.0.send(()).expect("live numeric timer receiver");
        Ok(true)
    }
}
#[test]
fn binary64_delays_use_the_existing_node_timer_normalization() {
    let mut source = oracle::TsOracle::spawn(
        "tool-detach-registry-oracle.ts",
        "packages/coding-agent/src/core/tool-detach-registry.ts",
    );
    for (kind, delay) in [
        ("negative", -3.0),
        ("zero", 0.0),
        ("fraction", 0.25),
        ("validFraction", 1.75),
        ("nan", f64::NAN),
        ("infinity", f64::INFINITY),
        ("negativeInfinity", f64::NEG_INFINITY),
    ] {
        let expected = source.request(&json!({"id":2,"operation":"numericDelay","kind":kind}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        assert_eq!(expected["output"], json!({"called":true}));
        let registry = Arc::new(ToolDetachRegistry::with_auto_detach_after_ms(delay));
        let (send, receive) = std::sync::mpsc::channel();
        let subscription = registry.register(Arc::new(NotifyHandle(send)));
        let called = receive.recv_timeout(Duration::from_secs(1)).is_ok();
        drop(subscription);
        assert_eq!(json!({"called":called}), expected["output"], "delay {kind}");
    }
}
fn wait_for_unref_exit(mut child: std::process::Child) -> std::process::Output {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!(
                "active unref timer kept the process alive: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn an_active_default_timer_does_not_keep_the_process_alive() {
    use std::io::Write;
    use std::os::unix::process::CommandExt;
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    let mut source = oracle::spawn_oracle_process(
        &lock,
        "tool-detach-registry-oracle.ts",
        "packages/coding-agent/src/core/tool-detach-registry.ts",
        &[],
        std::process::Stdio::piped(),
    );
    let mut stdin = source.stdin.take().unwrap();
    writeln!(
        stdin,
        "{}",
        pi_js_compat::js_json::stringify_lossless(&json!({"id":3,"operation":"unrefExit"}))
    )
    .unwrap();
    drop(stdin);
    let expected = wait_for_unref_exit(source);
    assert_eq!(
        expected.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    let value = pi_js_compat::js_json::parse_lossless(String::from_utf8(expected.stdout).unwrap()).unwrap();
    assert_eq!(value["output"], json!({"hasRunning":true}));
    let native = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "active_unref_timer_probe", "--ignored", "--nocapture"])
        .env("PI_ACTIVE_UNREF_PROBE", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap();
    let actual = wait_for_unref_exit(native);
    assert_eq!(
        actual.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert!(
        String::from_utf8(actual.stdout)
            .unwrap()
            .contains("NATIVE_ACTIVE_UNREF=true")
    );
}
#[test]
#[ignore = "private process-exit probe with a deliberately retained active timer"]
fn active_unref_timer_probe() {
    use std::io::Write;
    assert!(std::env::var_os("PI_ACTIVE_UNREF_PROBE").is_some());
    let registry = Arc::new(ToolDetachRegistry::new());
    let subscription = registry.register(Arc::new(CountHandle(Arc::new(AtomicUsize::new(0)))));
    assert!(registry.has_running());
    // Test-only: retain the subscription through natural process exit, not Drop cancellation.
    std::mem::forget(subscription);
    let mut output = std::io::stdout().lock();
    writeln!(output, "NATIVE_ACTIVE_UNREF=true").unwrap();
    output.flush().unwrap();
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
