use std::fmt::Debug;

use pi_agent_core::harness::compaction::CutPointResult;

fn assert_debug<T: Debug>(_: &T) {}

fn result(first_kept_entry_index: f64, turn_start_index: f64, is_split_turn: bool) -> CutPointResult {
    CutPointResult {
        first_kept_entry_index,
        turn_start_index,
        is_split_turn,
    }
}

#[test]
fn preserves_source_shaped_default_and_split_values() {
    let mut default = result(0.0, -1.0, false);
    assert_eq!(default.first_kept_entry_index, 0.0);
    assert_eq!(default.turn_start_index, -1.0);
    assert!(!default.is_split_turn);
    assert_debug(&default);

    default.first_kept_entry_index = 12.0;
    default.turn_start_index = 7.0;
    default.is_split_turn = true;

    assert_eq!(default.first_kept_entry_index, 12.0);
    assert_eq!(default.turn_start_index, 7.0);
    assert!(default.is_split_turn);
}

#[test]
fn preserves_unrestricted_binary64_bits_in_numeric_fields() {
    const BIT_PATTERNS: [u64; 8] = [
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x7fe_ffff_ffff_ffff,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff8_0000_0000_0001,
        0x7ff8_0000_0000_0042,
    ];

    for bits in BIT_PATTERNS {
        let mut value = result(f64::from_bits(bits), f64::from_bits(bits), false);
        assert_eq!(value.first_kept_entry_index.to_bits(), bits);
        assert_eq!(value.turn_start_index.to_bits(), bits);

        value.first_kept_entry_index = f64::from_bits(bits);
        value.turn_start_index = f64::from_bits(bits);
        assert_eq!(value.first_kept_entry_index.to_bits(), bits);
        assert_eq!(value.turn_start_index.to_bits(), bits);
    }
}

#[test]
fn preserves_distinct_nan_payloads_and_bool_mutation() {
    const FIRST_NAN_BITS: u64 = 0x7ff8_0000_0000_0001;
    const SECOND_NAN_BITS: u64 = 0x7ff8_0000_0000_0042;

    let mut value = result(f64::from_bits(FIRST_NAN_BITS), f64::from_bits(SECOND_NAN_BITS), true);

    assert_eq!(value.first_kept_entry_index.to_bits(), FIRST_NAN_BITS);
    assert_eq!(value.turn_start_index.to_bits(), SECOND_NAN_BITS);
    assert!(value.is_split_turn);

    value.is_split_turn = false;
    assert!(!value.is_split_turn);
}
