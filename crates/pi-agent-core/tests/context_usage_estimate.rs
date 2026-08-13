use std::fmt::Debug;

use pi_agent_core::harness::compaction::ContextUsageEstimate;

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn preserves_source_shaped_fields_optional_index_and_public_mutation() {
    let mut estimate = ContextUsageEstimate {
        tokens: 42.5,
        usage_tokens: 17.25,
        trailing_tokens: 3.75,
        last_usage_index: None,
    };

    assert_eq!(estimate.tokens, 42.5);
    assert_eq!(estimate.usage_tokens, 17.25);
    assert_eq!(estimate.trailing_tokens, 3.75);
    assert_eq!(estimate.last_usage_index, None);
    assert_debug(&estimate);

    estimate.tokens = -0.5;
    estimate.usage_tokens = 9.0;
    estimate.trailing_tokens = 0.0;
    estimate.last_usage_index = Some(-2.5);

    assert_eq!(estimate.tokens, -0.5);
    assert_eq!(estimate.usage_tokens, 9.0);
    assert_eq!(estimate.trailing_tokens, 0.0);
    assert_eq!(estimate.last_usage_index, Some(-2.5));
}

#[test]
fn preserves_unrestricted_binary64_bits_in_all_numeric_fields() {
    const BIT_PATTERNS: [u64; 12] = [
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x8000_0000_0000_0001,
        0x7fef_ffff_ffff_ffff,
        0xffef_ffff_ffff_ffff,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff8_0000_0000_0001,
        0x7ff8_0000_0000_0042,
        0xfff8_0000_0000_0001,
        0xfff8_0000_0000_0042,
    ];

    for bits in BIT_PATTERNS {
        let estimate = ContextUsageEstimate {
            tokens: f64::from_bits(bits),
            usage_tokens: f64::from_bits(bits),
            trailing_tokens: f64::from_bits(bits),
            last_usage_index: Some(f64::from_bits(bits)),
        };

        assert_eq!(estimate.tokens.to_bits(), bits);
        assert_eq!(estimate.usage_tokens.to_bits(), bits);
        assert_eq!(estimate.trailing_tokens.to_bits(), bits);
        assert_eq!(estimate.last_usage_index.expect("index is present").to_bits(), bits);
    }
}
