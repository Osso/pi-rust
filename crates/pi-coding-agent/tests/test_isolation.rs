//! Port of `test/global-setup.ts`: every test process gets a private state
//! directory, so tests never open the real `~/.local/state/pi/control.sqlite`.
//! The cargo runner (`scripts/cargo-runner.sh`) provides it.

use std::path::{Path, PathBuf};

#[test]
fn test_processes_resolve_a_private_state_root() {
    let state_dir =
        std::env::var("PI_CODING_AGENT_STATE_DIR").expect("the cargo runner sets PI_CODING_AGENT_STATE_DIR");
    assert!(Path::new(&state_dir).is_dir(), "{state_dir} must exist");
    assert!(
        Path::new(&state_dir)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("pi-cargo-test-state-")),
        "{state_dir} must be the runner's private directory"
    );
    assert_eq!(
        pi_coding_agent::config::get_user_state_root(),
        PathBuf::from(&state_dir)
    );
}
