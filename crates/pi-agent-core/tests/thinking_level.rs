use pi_agent_core::harness::ThinkingLevel as RootThinkingLevel;
use pi_agent_core::harness::types::ThinkingLevel;

fn clone_value<T: Clone>(value: &T) -> T {
    value.clone()
}

#[test]
fn variants_return_exact_source_literals() {
    assert_eq!(ThinkingLevel::Off.as_str(), "off");
    assert_eq!(ThinkingLevel::Minimal.as_str(), "minimal");
    assert_eq!(ThinkingLevel::Low.as_str(), "low");
    assert_eq!(ThinkingLevel::Medium.as_str(), "medium");
    assert_eq!(ThinkingLevel::High.as_str(), "high");
    assert_eq!(ThinkingLevel::XHigh.as_str(), "xhigh");
    assert_eq!(ThinkingLevel::Max.as_str(), "max");
    assert_eq!(ThinkingLevel::Ultra.as_str(), "ultra");
}

#[test]
fn enum_values_support_equality_copy_clone_and_debug() {
    let variants = [
        (ThinkingLevel::Off, "Off"),
        (ThinkingLevel::Minimal, "Minimal"),
        (ThinkingLevel::Low, "Low"),
        (ThinkingLevel::Medium, "Medium"),
        (ThinkingLevel::High, "High"),
        (ThinkingLevel::XHigh, "XHigh"),
        (ThinkingLevel::Max, "Max"),
        (ThinkingLevel::Ultra, "Ultra"),
    ];

    for (level, debug_name) in variants {
        let copied = level;
        let cloned = clone_value(&level);

        assert_eq!(level, copied);
        assert_eq!(level, cloned);
        assert_eq!(format!("{level:?}"), debug_name);
    }
}

#[test]
fn harness_root_reexports_the_same_enum() {
    let level: RootThinkingLevel = RootThinkingLevel::Ultra;

    assert_eq!(level, ThinkingLevel::Ultra);
    assert_eq!(level.as_str(), "ultra");
}

#[test]
fn ultra_is_distinct_from_max() {
    assert_ne!(ThinkingLevel::Max, ThinkingLevel::Ultra);
    assert_ne!(ThinkingLevel::Max.as_str(), ThinkingLevel::Ultra.as_str());
}
