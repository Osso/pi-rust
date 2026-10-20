//! Git-using tests relaunch only themselves with an isolated git environment.
//! Runs beneath cargo's private-state runner; all git cwd values are temp dirs.
pub fn run_in_isolated_git_child(test: &str) -> bool {
    if std::env::var_os("PI_EXAMPLE_E_GIT_CHILD").is_some() {
        assert_eq!(std::env::var("GIT_CONFIG_GLOBAL").unwrap(), "/dev/null");
        assert_eq!(std::env::var("GIT_CONFIG_SYSTEM").unwrap(), "/dev/null");
        assert_eq!(std::env::var("GIT_CONFIG_NOSYSTEM").unwrap(), "1");
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("PI_EXAMPLE_E_GIT_CHILD", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}
