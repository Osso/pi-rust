//! Port of `test/global-setup.ts`: every test process gets a private state
//! directory, so tests never open the real `~/.local/state/pi/control.sqlite`.
//! The cargo runner (`scripts/cargo-runner.sh`) provides it.

use std::path::{Path, PathBuf};

#[test]
fn test_processes_resolve_a_private_state_root() {
    let state_dir =
        std::env::var("PI_CODING_AGENT_STATE_DIR").expect("the cargo runner sets PI_CODING_AGENT_STATE_DIR");
    assert!(Path::new(&state_dir).is_dir(), "{state_dir} must exist");
    let home = PathBuf::from(std::env::var_os("HOME").expect("private HOME"));
    let private_root = home.parent().expect("runner root");
    assert!(Path::new(&state_dir).starts_with(private_root));
    assert!(!private_root.starts_with("/home/osso"));
    assert!(!private_root.starts_with("/root"));
    assert!(!Path::new(&state_dir).starts_with(&home));
    let probe = Path::new(&state_dir).join("state-isolation-probe");
    std::fs::write(&probe, "private state").expect("state directory must be writable");
    assert_eq!(std::fs::read_to_string(&probe).unwrap(), "private state");
    std::fs::remove_file(probe).unwrap();
    assert_eq!(
        pi_coding_agent::config::get_user_state_root().unwrap(),
        PathBuf::from(&state_dir)
    );
}
