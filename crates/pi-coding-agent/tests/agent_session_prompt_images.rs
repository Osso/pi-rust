use super::*;
use std::sync::Mutex;

#[tokio::test]
async fn prompt_input_preserves_absent_empty_and_populated_images_like_frozen() {
    use pi_coding_agent::core::extensions::types::{ExtensionContext, InputEvent, InputEventResult};
    use pi_coding_agent::core::extensions::{ExtensionFactory, HandlerError};
    let views = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&views);
    let factory: ExtensionFactory = Arc::new(move |api| {
        let captured = Arc::clone(&captured);
        api.on_input(move |event: InputEvent, _ctx: ExtensionContext| {
            let captured = Arc::clone(&captured);
            Box::pin(async move {
                let images = event
                    .images
                    .map_or(json!("absent"), |images| serde_json::to_value(images).expect("images"));
                captured.lock().unwrap().push(images);
                Ok(Some(InputEventResult::Handled))
            }) as pi_ai::request::BoxFuture<'static, Result<Option<InputEventResult>, HandlerError>>
        })
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    let image = json!({"type":"image", "data":"AA==", "mimeType":"image/png"});
    fixture.session.prompt("absent", Default::default()).await.unwrap();
    fixture
        .session
        .prompt(
            "empty",
            PromptOptions {
                images: Some(Vec::new()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    fixture
        .session
        .prompt(
            "image",
            PromptOptions {
                images: Some(vec![serde_json::from_value(image.clone()).unwrap()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    fixture.session.dispose();
    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(
        &json!({"operation":"input-images", "dir":fixture._temp_dir.path().join("oracle").display().to_string()}),
    );
    assert_eq!(expected, json!({"ok":true,"value":["absent",[],[image]]}));
    assert_eq!(json!(views.lock().unwrap().clone()), expected["value"]);
}

#[tokio::test]
async fn prompt_images_preserve_presence_and_empty_transform_through_before_start_like_frozen() {
    use pi_coding_agent::core::extensions::types::{
        BeforeAgentStartEvent, BeforeAgentStartEventResult, ExtensionContext, InputEvent, InputEventResult, InputImages,
    };
    use pi_coding_agent::core::extensions::{ExtensionFactory, HandlerError};
    let views = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&views);
    let factory: ExtensionFactory = Arc::new(move |api| {
        api.on_input(|event: InputEvent, _ctx: ExtensionContext| {
            Box::pin(async move {
                let result = if event.text == "clear" {
                    InputEventResult::Transform {
                        text: event.text,
                        images: Some(InputImages::new(Vec::new())),
                    }
                } else {
                    InputEventResult::Continue
                };
                Ok(Some(result))
            }) as pi_ai::request::BoxFuture<'static, Result<Option<InputEventResult>, HandlerError>>
        })
        .unwrap();
        let captured = Arc::clone(&captured);
        api.on_before_agent_start(move |event: BeforeAgentStartEvent, _ctx: ExtensionContext| {
            let captured = Arc::clone(&captured);
            Box::pin(async move {
                captured.lock().unwrap().push(
                    event
                        .images
                        .map_or(json!("absent"), |images| serde_json::to_value(images).unwrap()),
                );
                Ok(None)
            })
                as pi_ai::request::BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>>
        })
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    fixture.faux.set_responses(
        (0..3)
            .map(|_| faux_assistant_message("reply", Default::default()).into())
            .collect(),
    );
    fixture.session.prompt("absent", Default::default()).await.unwrap();
    fixture
        .session
        .prompt(
            "empty",
            PromptOptions {
                images: Some(Vec::new()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let image = serde_json::from_value(json!({"type":"image", "data":"AA==", "mimeType":"image/png"})).unwrap();
    fixture
        .session
        .prompt(
            "clear",
            PromptOptions {
                images: Some(vec![image]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    fixture.session.dispose();
    let mut oracle = tools_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({"operation":"input-image-propagation", "dir":fixture._temp_dir.path().join("oracle").display().to_string()}));
    assert_eq!(expected, json!({"ok":true,"value":["absent",[],[]]}));
    assert_eq!(json!(views.lock().unwrap().clone()), expected["value"]);
}
