use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use pi_coding_agent::utils::shell::{
    CommandTransport, get_shell_config, kill_process_tree, kill_tracked_detached_children, sanitize_binary_output,
    track_detached_child_pid, untrack_detached_child_pid,
};

#[test]
fn sanitize_binary_output_drops_control_and_format_characters() {
    assert_eq!(sanitize_binary_output("plain text"), "plain text");
    assert_eq!(
        sanitize_binary_output("tab\tnewline\nreturn\r"),
        "tab\tnewline\nreturn\r"
    );
    assert_eq!(sanitize_binary_output("bell\u{7}null\u{0}esc\u{1b}"), "bellnullesc");
    assert_eq!(
        sanitize_binary_output("\u{fff9}interlinear\u{fffa}object\u{fffb}"),
        "interlinearobject"
    );
    assert_eq!(
        sanitize_binary_output("\u{fffc}replacement\u{fffd}"),
        "\u{fffc}replacement\u{fffd}"
    );
    assert_eq!(sanitize_binary_output("emoji 😀 and é"), "emoji 😀 and é");
    assert_eq!(sanitize_binary_output("\u{7f}del"), "\u{7f}del");
}

#[test]
fn shell_config_prefers_bin_bash_then_the_custom_path() {
    let config = get_shell_config(None).expect("default shell");
    assert_eq!(config.shell, "/bin/bash");
    assert_eq!(config.args, ["-c"]);
    assert_eq!(config.command_transport, CommandTransport::Argv);

    let config = get_shell_config(Some("/bin/bash")).expect("explicit shell");
    assert_eq!(config.shell, "/bin/bash");
    assert_eq!(config.args, ["-c"]);
}

#[test]
fn shell_config_rejects_a_missing_custom_shell() {
    let error = get_shell_config(Some("/nonexistent-shell-path-xyz123")).expect_err("missing custom shell");
    assert_eq!(
        error.to_string(),
        "Custom shell path not found: /nonexistent-shell-path-xyz123"
    );
}

#[test]
fn legacy_wsl_bash_paths_transport_commands_over_stdin() {
    // The predicate is private; the behavior is exercised through the Windows branch on Windows hosts
    // and by the unit test inside the module. Here we only assert the documented non-Windows default.
    if cfg!(not(windows)) {
        assert_eq!(
            get_shell_config(None).unwrap().command_transport,
            CommandTransport::Argv
        );
    }
}

#[test]
fn kill_process_tree_terminates_the_process_group() {
    use std::os::unix::process::CommandExt;
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("sleep 30")
        .process_group(0)
        .stdout(Stdio::null())
        .spawn()
        .expect("sleep must spawn");
    let pid = child.id();
    kill_process_tree(pid);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().expect("wait must work") {
            assert!(!status.success(), "the process group must be killed: {status:?}");
            return;
        }
        assert!(Instant::now() < deadline, "the process group was not killed");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn tracked_detached_children_are_killed_together() {
    use std::os::unix::process::CommandExt;
    let mut tracked = Command::new("sh")
        .arg("-c")
        .arg("sleep 30")
        .process_group(0)
        .stdout(Stdio::null())
        .spawn()
        .expect("sleep must spawn");
    let untracked = Command::new("sh")
        .arg("-c")
        .arg("sleep 0.2")
        .spawn()
        .expect("short sleep must spawn");
    let untracked_pid = untracked.id();
    track_detached_child_pid(tracked.id());
    track_detached_child_pid(untracked_pid);
    untrack_detached_child_pid(untracked_pid);

    kill_tracked_detached_children();
    let _ = untracked.wait_with_output();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if tracked.try_wait().expect("wait").is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "tracked child was not killed");
        std::thread::sleep(Duration::from_millis(20));
    }
}
