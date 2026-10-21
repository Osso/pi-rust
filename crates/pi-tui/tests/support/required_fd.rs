use std::os::unix::fs::PermissionsExt;

pub fn assert_unusable_fd_fails(test_name: &str) {
    let directory = tempfile::tempdir().unwrap();
    let fd = directory.path().join("fd");
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
}
