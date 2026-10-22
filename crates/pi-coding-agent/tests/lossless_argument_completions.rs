use pi_coding_agent::core::extensions::extension_runtime::create_extension_runtime;
use pi_coding_agent::core::extensions::loader::{ExtensionFactory, load_extension_from_factory};
use pi_coding_agent::core::extensions::types::{ExtensionApi, HandlerError};
use pi_js_compat::js_json::JsString;
use std::sync::Arc;
#[tokio::test]
async fn registered_completion_callback_preserves_raw_input_and_failure() {
    let name = JsString::from_utf16(vec![0xd801]);
    let lookup = name.clone();
    let factory: ExtensionFactory = Arc::new(move |api: &ExtensionApi| {
        let result = api.register_command(
            name.clone(),
            Some(name.clone()),
            Some(Arc::new(|prefix| Err(prefix.clone()))),
            Arc::new(|_, _| Box::pin(async { Ok(()) })),
        );
        Box::pin(async move { result.map_err(HandlerError::new) })
    });
    let extension = load_extension_from_factory(
        factory,
        &"/fixture".into(),
        Arc::new(pi_coding_agent::core::event_bus::create_event_bus()),
        create_extension_runtime(),
        &"<fixture>".into(),
    )
    .await
    .unwrap();
    let callback = extension
        .read()
        .commands
        .get(&pi_js_compat::js_json::JsString::from(&lookup))
        .unwrap()
        .get_argument_completions
        .clone()
        .unwrap();
    let raw = JsString::from_utf16(vec![0xd800]);
    assert_eq!(callback(&raw).unwrap_err(), raw);
}
