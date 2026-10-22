use pi_js_compat::node_fs::decode_node_path;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

#[test]
fn node_path_byte_ingress_replaces_invalid_utf8_sequences() {
    for (bytes, units) in [
        (b"/a\xffb".as_slice(), vec![47, 97, 0xfffd, 98]),
        (b"/\xed\xa0\x80".as_slice(), vec![47, 0xfffd, 0xfffd, 0xfffd]),
        (b"/\xf0\x9f\x98\x80".as_slice(), vec![47, 0xd83d, 0xde00]),
    ] {
        assert_eq!(decode_node_path(OsStr::from_bytes(bytes)).units(), units);
    }
}
