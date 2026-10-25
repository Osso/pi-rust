use super::*;
use std::sync::Mutex;

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

#[tokio::test]
async fn thinking_selection_sync_listener_effect_is_visible_before_setter_returns_like_frozen() {
    use pi_coding_agent::core::extensions::types::{ExtensionContext, ThinkingLevelSelectEvent};
    use pi_coding_agent::core::extensions::{ExtensionFactory, HandlerError};
    let views = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&views);
    let factory: ExtensionFactory = Arc::new(move |api| {
        let captured = Arc::clone(&captured);
        api.on_thinking_level_select(move |event: ThinkingLevelSelectEvent, _ctx: ExtensionContext| {
            captured
                .lock()
                .unwrap()
                .push(json!({"level":event.level,"previousLevel":event.previous_level}));
            Box::pin(async { Ok(None) }) as pi_ai::request::BoxFuture<'static, Result<Option<()>, HandlerError>>
        })
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    fixture.session.set_thinking_level(ThinkingLevel::High).unwrap();
    let actual = json!(views.lock().unwrap().clone());
    fixture.session.dispose();
    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({"operation":"thinking-sync-listener","dir":fixture._temp_dir.path().join("oracle").display().to_string()}));
    assert_eq!(
        expected,
        json!({"ok":true,"value":[{"level":"high","previousLevel":"off"}]})
    );
    assert_eq!(actual, expected["value"]);
}

#[tokio::test]
async fn thinking_selection_pending_remainder_settles_without_losing_the_prefix_like_frozen() {
    use pi_coding_agent::core::extensions::types::{ExtensionContext, ThinkingLevelSelectEvent};
    use pi_coding_agent::core::extensions::{ExtensionFactory, HandlerError};
    let views = Arc::new(Mutex::new(Vec::new()));
    let release = Arc::new(tokio::sync::Notify::new());
    let done = Arc::new(tokio::sync::Notify::new());
    let (captured, gate, settled) = (Arc::clone(&views), Arc::clone(&release), Arc::clone(&done));
    let factory: ExtensionFactory = Arc::new(move |api| {
        let (captured, gate, settled) = (Arc::clone(&captured), Arc::clone(&gate), Arc::clone(&settled));
        api.on_thinking_level_select(move |_event: ThinkingLevelSelectEvent, _ctx: ExtensionContext| {
            let (captured, gate, settled) = (Arc::clone(&captured), Arc::clone(&gate), Arc::clone(&settled));
            Box::pin(async move {
                captured.lock().unwrap().push("start");
                gate.notified().await;
                captured.lock().unwrap().push("finish");
                settled.notify_one();
                Ok(None)
            }) as pi_ai::request::BoxFuture<'static, Result<Option<()>, HandlerError>>
        })
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    fixture.session.set_thinking_level(ThinkingLevel::High).unwrap();
    let immediate = views.lock().unwrap().clone();
    release.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(5), done.notified())
        .await
        .expect("pending listener settled");
    let actual = json!({"immediate":immediate,"settled":views.lock().unwrap().clone()});
    fixture.session.dispose();
    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected=oracle.request(&json!({"operation":"thinking-pending-listener","dir":fixture._temp_dir.path().join("oracle").display().to_string()}));
    assert_eq!(
        expected,
        json!({"ok":true,"value":{"immediate":["start"],"settled":["start","finish"]}})
    );
    assert_eq!(actual, expected["value"]);
}
