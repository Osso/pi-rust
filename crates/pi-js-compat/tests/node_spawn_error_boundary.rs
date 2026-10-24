use pi_js_compat::node_child_process::spawn_error_is_async;
use rustix::io::Errno;

#[test]
fn spawn_error_delivery_matches_node_26_uv_spawn_classification() {
    for errno in [Errno::ACCESS, Errno::AGAIN, Errno::MFILE, Errno::NFILE, Errno::NOENT] {
        assert!(
            spawn_error_is_async(&std::io::Error::from_raw_os_error(errno.raw_os_error())),
            "{errno:?}"
        );
    }
    for errno in [
        Errno::TOOBIG,
        Errno::INVAL,
        Errno::NOEXEC,
        Errno::IO,
        Errno::NOTDIR,
        Errno::BADF,
    ] {
        assert!(
            !spawn_error_is_async(&std::io::Error::from_raw_os_error(errno.raw_os_error())),
            "{errno:?}"
        );
    }
    assert!(!spawn_error_is_async(&std::io::Error::other("synthetic failure")));
}
