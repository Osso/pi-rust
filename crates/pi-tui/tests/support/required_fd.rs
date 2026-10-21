use std::os::unix::fs::PermissionsExt;

pub fn require_fd(candidate: Option<String>) -> String {
    let fd = candidate.expect("fd is required for autocomplete coverage");
    let output = std::process::Command::new(&fd)
        .arg("--version")
        .output()
        .expect("fd is required: failed to launch version probe");
    assert!(
        output.status.success(),
        "fd probe exited with {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    fd
}

pub fn assert_unusable_fd_fails(test_name: &str) {
    let directory = tempfile::tempdir().unwrap();
    let fd = directory.path().join("fd");
    // Keep oracle prerequisites available: failures must concern fd, not git/Node.
    for executable in ["git", "node"] {
        std::os::unix::fs::symlink(format!("/usr/bin/{executable}"), directory.path().join(executable)).unwrap();
    }
    for mode in ["missing", "broken", "nonexecutable"] {
        if mode != "missing" {
            std::fs::write(&fd, "#!/usr/bin/python3\nraise SystemExit(42)\n").unwrap();
            let permissions = if mode == "broken" { 0o755 } else { 0o644 };
            std::fs::set_permissions(&fd, std::fs::Permissions::from_mode(permissions)).unwrap();
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test_name, "--nocapture"])
            .env("PATH", directory.path())
            .output()
            .unwrap();
        assert!(!output.status.success(), "{mode} fd silently passed: {output:?}");
    }
    std::fs::remove_file(&fd).unwrap();
    std::os::unix::fs::symlink("/usr/bin/fd", &fd).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test_name, "--nocapture"])
        .env("PATH", directory.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "working fd must execute coverage: {output:?}");
}
