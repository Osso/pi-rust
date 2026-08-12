use pi_protocol::{ClientHello, PROTOCOL_VERSION};

#[test]
fn protocol_version_matches_the_pinned_source() {
    assert_eq!(PROTOCOL_VERSION, 1);
}

#[test]
fn client_hello_preserves_non_negative_versions() {
    for version in [0, PROTOCOL_VERSION, 2, u64::MAX] {
        let hello = ClientHello::new(version);

        assert_eq!(hello.version(), version);
    }
}
