use pi_coding_agent::core::runtime_process::{ProcessIdentity, is_process_identity_alive};

/// Killing the owned tsx process group is asynchronous for its Node child.
/// Establish the fixture's actual exit before asserting dead-owner behavior.
pub(crate) fn wait_for_owned_fixture_exit(identity: &ProcessIdentity) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while is_process_identity_alive(identity) {
        assert!(
            std::time::Instant::now() < deadline,
            "owned fixture process did not exit: {identity:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
