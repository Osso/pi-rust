use std::fmt::Debug;

use pi_agent_core::harness::compaction::{
    CompactionSettings as ModuleSettings, should_compact as module_should_compact,
};
use pi_agent_core::harness::{CompactionSettings, should_compact};

fn assert_debug<T: Debug>(_: &T) {}

fn settings(enabled: bool, reserve_tokens: f64, keep_recent_tokens: f64) -> ModuleSettings {
    ModuleSettings {
        enabled,
        reserve_tokens,
        keep_recent_tokens,
    }
}

#[test]
fn module_and_root_exports_expose_mutable_public_fields() {
    let mut module_settings = settings(false, 1.0, 2.0);
    module_settings.enabled = true;
    module_settings.reserve_tokens = 3.0;
    module_settings.keep_recent_tokens = 4.0;

    let mut root_settings = CompactionSettings {
        enabled: false,
        reserve_tokens: 0.0,
        keep_recent_tokens: 0.0,
    };
    root_settings.enabled = module_settings.enabled;
    root_settings.reserve_tokens = module_settings.reserve_tokens;
    root_settings.keep_recent_tokens = module_settings.keep_recent_tokens;

    assert!(module_settings.enabled);
    assert_eq!(module_settings.reserve_tokens, 3.0);
    assert_eq!(module_settings.keep_recent_tokens, 4.0);
    assert!(root_settings.enabled);
    assert_eq!(root_settings.reserve_tokens, 3.0);
    assert_eq!(root_settings.keep_recent_tokens, 4.0);
    assert_debug(&module_settings);
    assert_debug(&root_settings);
    assert!(module_should_compact(99.0, 100.0, &module_settings));
    assert!(should_compact(99.0, 100.0, &root_settings));
}

#[test]
fn preserves_unrestricted_binary64_setting_bits() {
    const RESERVE_BITS: u64 = 0x7ff8_0000_0000_0042;
    const KEEP_RECENT_BITS: u64 = 0x8000_0000_0000_0001;

    let configured = settings(true, f64::from_bits(RESERVE_BITS), f64::from_bits(KEEP_RECENT_BITS));

    assert_eq!(configured.reserve_tokens.to_bits(), RESERVE_BITS);
    assert_eq!(configured.keep_recent_tokens.to_bits(), KEEP_RECENT_BITS);
}

#[test]
fn applies_disabled_short_circuit_and_strict_threshold() {
    let mut configured = settings(true, 10.0, 20.0);

    assert!(should_compact(95.0, 100.0, &configured));
    assert!(!should_compact(90.0, 100.0, &configured));
    assert!(!should_compact(89.0, 100.0, &configured));

    configured.keep_recent_tokens = f64::NAN;
    assert!(should_compact(95.0, 100.0, &configured));

    configured.enabled = false;
    assert!(!should_compact(f64::NAN, f64::NAN, &configured));
}

#[test]
fn preserves_ieee754_predicate_boundaries() {
    let zero = settings(true, 0.0, 0.0);
    assert!(should_compact(f64::from_bits(1), 0.0, &zero));
    assert!(!should_compact(-0.0, 0.0, &zero));
    assert!(!should_compact(0.0, 1.0, &settings(true, 1.0, 0.0)));

    let overflow = settings(true, f64::MAX, 0.0);
    assert!(should_compact(f64::MAX, -f64::MAX, &overflow));

    let infinities = settings(true, f64::INFINITY, 0.0);
    assert!(should_compact(f64::MAX, 0.0, &infinities));
    assert!(!should_compact(f64::INFINITY, f64::INFINITY, &zero));

    let nan_context = settings(true, 0.0, 0.0);
    assert!(!should_compact(f64::NAN, 0.0, &nan_context));
    assert!(!should_compact(0.0, f64::NAN, &nan_context));
    assert!(!should_compact(0.0, 0.0, &settings(true, f64::NAN, 0.0)));
}
