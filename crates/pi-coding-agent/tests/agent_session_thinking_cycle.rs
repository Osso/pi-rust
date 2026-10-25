use super::*;

#[tokio::test]
async fn thinking_cycle_starts_at_the_first_available_level_when_current_is_unlisted_like_frozen() {
    let fixture = fixture().await;
    fixture.session.agent().set_thinking_level(ThinkingLevel::XHigh);
    let before = fixture.session.thinking_level();
    let available = fixture.session.get_available_thinking_levels();
    assert!(!available.contains(&before));
    let next = fixture.session.cycle_thinking_level().unwrap();
    let actual = json!({"before":before,"available":available,"next":next,"current":fixture.session.thinking_level()});
    fixture.session.dispose();
    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({"operation":"thinking-cycle-unlisted","dir":fixture._temp_dir.path().join("oracle").display().to_string()}));
    assert_eq!(
        expected,
        json!({"ok":true,"value":{"before":"xhigh","available":["off","minimal","low","medium","high"],"next":"off","current":"off"}})
    );
    assert_eq!(actual, expected["value"]);
}
