use std::time::{SystemTime, UNIX_EPOCH};

use pi_agent_core::uuidv7;

fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

fn timestamp_of(uuid: &str) -> i64 {
    i64::from_str_radix(&uuid.replace('-', "")[..12], 16).unwrap()
}

fn assert_rfc9562_layout(uuid: &str) {
    let bytes = uuid.as_bytes();
    assert_eq!(uuid.len(), 36);
    for index in [8, 13, 18, 23] {
        assert_eq!(bytes[index], b'-');
    }
    assert!(
        uuid.chars()
            .all(|character| character.is_ascii_hexdigit() || character == '-')
    );
    assert_eq!(&uuid[14..15], "7", "version nibble");
    assert!(matches!(&uuid[19..20], "8" | "9" | "a" | "b"), "variant nibble");
}

#[test]
fn generates_rfc9562_version_7_uuids_with_a_current_timestamp() {
    let before = now_millis();
    let uuid = uuidv7();
    let after = now_millis();

    assert_rfc9562_layout(&uuid);
    let timestamp = timestamp_of(&uuid);
    assert!(
        timestamp >= before && timestamp <= after,
        "{timestamp} not in [{before}, {after}]"
    );
}

#[test]
fn preserves_monotonic_order_across_calls() {
    let mut previous = uuidv7();
    for _ in 0..100 {
        let next = uuidv7();
        assert!(previous < next, "{previous} must sort before {next}");
        previous = next;
    }
}

#[test]
fn generates_distinct_uuids() {
    let first = uuidv7();
    let second = uuidv7();
    assert_ne!(first, second);
}
