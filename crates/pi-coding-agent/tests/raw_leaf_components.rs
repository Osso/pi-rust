use pi_coding_agent::modes::interactive::components::bordered_loader::{BorderedLoader, BorderedLoaderOptions};
use pi_js_compat::js_json::JsString;
use pi_tui::Component;
use std::rc::Rc;

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
