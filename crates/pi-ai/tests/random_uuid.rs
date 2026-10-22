use std::collections::HashSet;

use pi_ai::utils::random_uuid::random_uuid;

#[test]
fn generates_rfc9562_version_4_uuids_like_node_random_uuid() {
    for _ in 0..200 {
        let uuid = random_uuid();
        let bytes = uuid.as_bytes();
        assert_eq!(uuid.len(), 36, "{uuid}");
        for (index, byte) in bytes.iter().enumerate() {
            if matches!(index, 8 | 13 | 18 | 23) {
                assert_eq!(*byte, b'-', "{uuid}");
            } else {
                assert!(byte.is_ascii_digit() || (b'a'..=b'f').contains(byte), "{uuid}");
            }
        }
        assert_eq!(bytes[14], b'4', "version nibble: {uuid}");
        assert!(matches!(bytes[19], b'8' | b'9' | b'a' | b'b'), "variant nibble: {uuid}");
    }
}

#[test]
fn generates_distinct_uuids() {
    let ids: HashSet<String> = (0..1000).map(|_| random_uuid()).collect();
    assert_eq!(ids.len(), 1000);
}
