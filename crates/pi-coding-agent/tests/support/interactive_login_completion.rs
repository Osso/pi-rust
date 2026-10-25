fn authentication_ui_snapshot(mode: &InteractiveMode) -> JsValue {
    let chat: Vec<_> = mode
        .chat_container
        .borrow_mut()
        .render(200)
        .iter()
        .map(|line| line.units().to_vec())
        .collect();
    JsValue::from(serde_json::json!({"model":mode.session().model(),"count":mode.footer_data_provider.get_available_provider_count(),"chat":chat}))
}
