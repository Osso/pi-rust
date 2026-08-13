use pi_agent_core::harness::ThinkingLevel;
use pi_coding_agent::core::defaults::DEFAULT_THINKING_LEVEL;

#[test]
fn default_thinking_level_is_medium_and_copyable() {
    let copied = DEFAULT_THINKING_LEVEL;

    assert_eq!(DEFAULT_THINKING_LEVEL, ThinkingLevel::Medium);
    assert_eq!(copied, ThinkingLevel::Medium);
    assert_eq!(DEFAULT_THINKING_LEVEL.as_str(), "medium");
}
