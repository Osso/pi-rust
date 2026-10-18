//! Runs one ignored test of the current test binary in a child process with a
//! controlled environment. Edition 2024 makes `std::env::set_var` unsafe, so
//! environment-dependent behavior is observed through a real child process.

use std::ffi::OsString;
use std::process::{Command, Output};

pub(crate) const PROBE_ENV: &str = "PI_RUST_ENV_PROBE";

/// Spawn `test_name` (an `#[ignore]` test) with exactly `env_pairs` plus `PATH`.
pub(crate) fn run_probe(test_name: &str, env_pairs: &[(&str, OsString)]) -> Output {
    let executable = std::env::current_exe().expect("test executable must be discoverable");
    let mut command = Command::new(executable);
    command.env_clear().env(PROBE_ENV, "1").args([
        "--ignored",
        "--exact",
        test_name,
        "--nocapture",
        "--test-threads=1",
    ]);
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    for (name, value) in env_pairs {
        command.env(name, value);
    }
    let output = command.output().expect("probe child must spawn");
    assert!(
        output.status.success(),
        "probe {test_name} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// True only inside a probe child; ignored probe tests return early otherwise.
pub(crate) fn in_probe() -> bool {
    std::env::var_os(PROBE_ENV).is_some()
}
