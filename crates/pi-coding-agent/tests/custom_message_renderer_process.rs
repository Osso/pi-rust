use pi_coding_agent::core::extensions::types::{CustomMessageContent, CustomMessageValue, MessageRenderer};
use pi_coding_agent::modes::interactive::components::custom_message::CustomMessageComponent;
use pi_coding_agent::utils::uncaught::{install_uncaught_handler, throw_uncaught};
use pi_tui::Component;
use std::sync::Arc;

#[test]
fn caught_renderer_throw_does_not_reach_process_crash_handler() {
    const CHILD: &str = "PI_CUSTOM_MESSAGE_RENDERER_PROCESS_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "caught_renderer_throw_does_not_reach_process_crash_handler",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child status {:?}; stdout {}; stderr {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("renderer caught; default frame retained"));
        return;
    }
    pi_coding_agent::modes::interactive::theme::init_theme(Some(&"dark".into()), false).unwrap();
    install_uncaught_handler();
    let message = CustomMessageValue {
        custom_type: "fixture".into(),
        content: CustomMessageContent::Text("default body".into()),
        display: true,
        details: None,
        timestamp: 0.0,
    };
    let mut default = CustomMessageComponent::new(message.clone(), None, None);
    let expected = default.render(40);
    let renderer: MessageRenderer = Arc::new(|_, _, _| throw_uncaught("Error", "caught renderer error"));
    let mut custom = CustomMessageComponent::new(message, Some(renderer), None);
    assert_eq!(custom.render(40), expected);
    custom.set_expanded(true);
    assert_eq!(custom.render(40), expected);
    custom.invalidate();
    assert_eq!(custom.render(40), expected);
    println!("renderer caught; default frame retained");
}
