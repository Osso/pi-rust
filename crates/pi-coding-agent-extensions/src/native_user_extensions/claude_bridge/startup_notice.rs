//! Notice is deferred until a terminal user's first bridge provider call.
use crate::core::extensions::types::{ExtensionMode, ExtensionUiContext, NotifyType};
use pi_js_compat::{
    callback_error::CallbackError,
    js_json::{JsString, JsValue},
};
use std::path::PathBuf;

pub(super) struct StartupNotice {
    pending: Vec<JsString>,
    agent_dir: PathBuf,
}
impl StartupNotice {
    pub fn from_config(config: &JsValue, agent_dir: PathBuf) -> Self {
        let mut pending = Vec::new();
        let shown = config
            .get("startupNoticeShown")
            .and_then(JsValue::as_string)
            .is_some_and(|s| !s.is_empty());
        if !shown {
            if config.get("provider").and_then(|p| p.get("plan")).is_none() {
                pending.push(
                    "Are you using a Max plan? You need to set provider.plan to \"max\" to unlock 1M context in Opus."
                        .into(),
                );
            }
            if config.get("askClaude").and_then(|p| p.get("enabled")).is_none() {
                pending.push("The AskClaude tool is opt-in only. Set askClaude.enabled to use it.".into());
            }
        }
        Self { pending, agent_dir }
    }
    pub fn show(&mut self, mode: ExtensionMode, ui: &dyn ExtensionUiContext) -> Result<(), CallbackError> {
        if mode != ExtensionMode::Tui || self.pending.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending);
        let date = jiff::Zoned::now().date().to_string();
        let path = super::config::write_startup_notice(&self.agent_dir, &date)?;
        let title = pi_js_compat::js_format!(
            "\u{1b}[33mWelcome to pi-claude-bridge\u{1b}[39m — settings live in {}",
            path
        );
        let bullets = pending
            .into_iter()
            .chain(std::iter::once(
                "This message only appears once. See README.md for more.".into(),
            ))
            .map(|text| JsString::from("• ") + text);
        let lines = std::iter::once(title)
            .chain(bullets)
            .chain(std::iter::once("─".repeat(64).into()));
        ui.notify(&JsString::join(lines, "\n"), Some(NotifyType::Info))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    #[derive(Default)]
    struct Ui(Mutex<Vec<JsString>>);
    impl ExtensionUiContext for Ui {
        fn notify(&self, text: &JsString, kind: Option<NotifyType>) -> Result<(), CallbackError> {
            assert_eq!(kind, Some(NotifyType::Info));
            self.0.lock().unwrap().push(text.clone());
            Ok(())
        }
    }
    #[tokio::test]
    async fn claude_bridge_first_tui_provider_query_triggers_the_notice_not_activation_or_rpc() {
        use std::sync::Arc;
        let (dir, config, _) = super::super::integration_tests::setup().await;
        let bridge = super::super::provider::BridgeProvider::new(
            JsValue::Null,
            Ok(dir.path().join("claude")),
            "AskClaude".into(),
        );
        let notices = dir.path().join("notices");
        *bridge.startup_notice.lock().unwrap() = Some(StartupNotice::from_config(&JsValue::Null, notices.clone()));
        let ui = Arc::new(Ui::default());
        *bridge.ui.lock().unwrap() = Some(ui.clone());
        let model = super::super::integration_tests::model(&config);
        let ctx: pi_ai::types::Context = pi_js_compat::js_json::from_value(
            &pi_js_compat::js_json::parse_lossless(r#"{"messages":[{"role":"user","content":"hello","timestamp":1}]}"#)
                .unwrap(),
        )
        .unwrap();
        let mut opts = pi_ai::request::SimpleStreamOptions::default();
        opts.stream.cwd = Some(dir.path().to_str().unwrap().into());
        *bridge.mode.lock().unwrap() = Some(ExtensionMode::Rpc);
        bridge
            .stream(model.clone(), ctx.clone(), Some(opts.clone()))
            .unwrap()
            .result()
            .await
            .unwrap();
        assert!(!notices.exists());
        assert!(ui.0.lock().unwrap().is_empty());
        *bridge.mode.lock().unwrap() = Some(ExtensionMode::Tui);
        for _ in 0..2 {
            bridge
                .stream(model.clone(), ctx.clone(), Some(opts.clone()))
                .unwrap()
                .result()
                .await
                .unwrap();
        }
        assert!(notices.join("claude-bridge.json").is_file());
        assert_eq!(ui.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn claude_bridge_startup_notice_only_shows_and_persists_on_first_tui_query() {
        let dir = tempfile::tempdir().unwrap();
        let ui = Ui::default();
        let mut notice = StartupNotice::from_config(&JsValue::Null, dir.path().into());
        for mode in [ExtensionMode::Print, ExtensionMode::Json, ExtensionMode::Rpc] {
            notice.show(mode, &ui).unwrap();
        }
        assert!(ui.0.lock().unwrap().is_empty());
        assert!(!dir.path().join("claude-bridge.json").exists());
        notice.show(ExtensionMode::Tui, &ui).unwrap();
        notice.show(ExtensionMode::Tui, &ui).unwrap();
        let messages = ui.0.lock().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].starts_with("\u{1b}[33mWelcome to pi-claude-bridge\u{1b}[39m — settings live in "));
        assert!(messages[0].contains("• The AskClaude tool is opt-in only."));
        assert!(messages[0].ends_with("─".repeat(64)));
        let value = pi_js_compat::js_json::parse_lossless(
            std::fs::read_to_string(dir.path().join("claude-bridge.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            value.get("startupNoticeShown").and_then(JsValue::as_string).unwrap(),
            jiff::Zoned::now().date().to_string().as_str()
        );
    }
}
