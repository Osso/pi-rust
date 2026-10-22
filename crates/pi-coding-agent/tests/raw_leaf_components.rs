use pi_coding_agent::modes::interactive::components::bordered_loader::{BorderedLoader, BorderedLoaderOptions};
use pi_js_compat::js_json::JsString;
use pi_tui::Component;
use std::rc::Rc;

#[test]
fn custom_entry_renderer_keeps_native_payload_when_expanded() {
    use pi_coding_agent::core::extensions::types::EntryRenderer;
    use pi_coding_agent::core::session_manager::CustomEntry;
    use pi_coding_agent::modes::interactive::components::custom_entry::CustomEntryComponent;
    use std::sync::Arc;
    init_theme();
    let raw = JsString::from_utf16(vec![0xd800]);
    let renderer: EntryRenderer = Arc::new(|entry, options, _| {
        let value = entry.data.as_ref().unwrap().as_string().unwrap();
        let text = if options.expanded {
            JsString::from("expanded:") + value
        } else {
            value.clone()
        };
        Some(pi_tui::tui::component(pi_tui::Text::new(text, 0, 0)))
    });
    let entry = CustomEntry {
        custom_type: "fixture".into(),
        data: Some(pi_js_compat::js_json::JsValue::String(raw.clone())),
        id: "entry".into(),
        parent_id: None,
        timestamp: "2026-10-22T00:00:00.000Z".into(),
    };
    let mut component = CustomEntryComponent::new(
        entry,
        renderer,
        pi_coding_agent::modes::interactive::components::custom_entry::CustomEntryComponentOptions {
            session_id: "fixture".into(),
            request_render: std::rc::Rc::new(|| false),
        },
    );
    assert!(component.render(80).iter().any(|line| line.contains(&raw)));
    component.set_expanded(true);
    assert!(
        component
            .render(80)
            .iter()
            .any(|line| line.contains(JsString::from("expanded:") + &raw))
    );
}

#[test]
fn custom_message_default_renderer_keeps_raw_label_and_markdown() {
    use pi_coding_agent::core::extensions::types::{CustomMessageContent, CustomMessageValue};
    use pi_coding_agent::modes::interactive::components::custom_message::CustomMessageComponent;
    init_theme();
    let label = JsString::from_utf16(vec![0xd800]);
    let body = JsString::from_utf16(vec![0xdc00]);
    let message = CustomMessageValue {
        custom_type: label.clone(),
        content: CustomMessageContent::Text(body.clone()),
        display: true,
        details: None,
        timestamp: 0.0,
    };
    let mut component = CustomMessageComponent::new(message, None, None);
    for expanded in [false, true] {
        component.set_expanded(expanded);
        let output = JsString::join(component.render(80), "\n");
        assert!(output.contains(&label));
        assert!(output.contains(&body));
    }
}

fn init_theme() {
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn bordered_loader_keeps_raw_message_and_cancellation_contract() {
    init_theme();
    let message = JsString::from("Loading ") + JsString::from_utf16(vec![0xd800]);
    for cancellable in [false, true] {
        let mut loader = BorderedLoader::new(
            Rc::new(|| {}),
            message.clone(),
            BorderedLoaderOptions {
                cancellable: Some(cancellable),
            },
        );
        assert!(loader.render(80).iter().any(|line| line.contains(&message)));
        assert!(!loader.signal().is_cancelled());
        loader.handle_input(&"\u{1b}".into());
        assert_eq!(loader.signal().is_cancelled(), cancellable);
        loader.dispose();
    }
}
