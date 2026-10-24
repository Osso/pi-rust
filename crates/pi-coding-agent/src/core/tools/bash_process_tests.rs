use super::terminate_foreground_bash_runner;
use crate::test_oracle as oracle;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// Frozen bash.ts:515-517 delegates to shell.killProcessTree, whose POSIX branch
// sends SIGKILL. Probe the helper directly: the real runner's parent-death gate
// can mask a TERM/KILL difference by killing its payload after runner death.
#[test]
fn foreground_runner_cancellation_kills_a_term_resistant_process_group() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn(
        "bash-runner-termination-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected: JsValue = source.request(&json!({"id":28060,"cwd":root.path().to_str().unwrap()}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(
        expected["output"],
        json!({"survived":false,"exitCode":null,"signal":"SIGKILL"})
    );
    let ready = root.path().join("native-ready");
    let survived = root.path().join("native-survived");
    let mut child = Command::new("/bin/sh")
        .args([
            "-c",
            &format!(
                "trap '' TERM; printf ready > '{}'; sleep .25; printf survived > '{}'",
                ready.display(),
                survived.display()
            ),
        ])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready.exists() {
        assert!(Instant::now() < deadline, "TERM-resistant fixture did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
    terminate_foreground_bash_runner(child.id());
    let status = child.wait().unwrap();
    let actual = json!({"survived":survived.exists(),"exitCode":status.code(),"signal":status.signal().map(crate::utils::child_process::signal_name)});
    assert_eq!(actual, expected["output"]);
}
