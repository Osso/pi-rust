//! Behavioral tests for interactive components owned by ca-interactive-b.

use pi_js_compat::js_json::JsString;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use pi_coding_agent::ParsedSkillBlock;
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, AgentPermission, AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions,
};
use pi_coding_agent::modes::interactive::components::agent_selection_banner::AgentSelectionBannerComponent;
use pi_coding_agent::modes::interactive::components::earendil_announcement::EarendilAnnouncementComponent;
use pi_coding_agent::modes::interactive::components::skill_invocation_message::SkillInvocationMessageComponent;
use pi_coding_agent::modes::interactive::theme::{
    ColorMode, ColorValue, REQUIRED_COLORS, Theme, ThemeBg, ThemeColor, set_theme_instance,
};
use pi_tui::utils::strip_terminal_sequences;
use pi_tui::{Component, MarkdownTheme};

fn install_theme() -> Arc<Theme> {
    let colors: Vec<(JsString, ColorValue)> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                JsString::from(*key),
                ColorValue::Text(JsString::from(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * index) % 256,
                    (32 + 11 * index) % 256,
                    (48 + 13 * index) % 256
                ))),
            )
        })
        .collect();
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).expect("test theme"));
    set_theme_instance(theme.clone());
    theme
}

fn theme() -> Arc<Theme> {
    static KEYBINDINGS: OnceLock<()> = OnceLock::new();
    KEYBINDINGS.get_or_init(|| {
        pi_tui::set_keybindings(pi_tui::KeybindingsManager::new(
            pi_coding_agent::core::keybindings::keybindings(),
            Vec::new(),
        ));
    });
    install_theme()
}

fn plain(lines: &[JsString]) -> JsString {
    JsString::join(lines.iter().map(|line| strip_terminal_sequences(line)), "\n")
}

struct PlainMarkdownTheme;

impl MarkdownTheme for PlainMarkdownTheme {
    fn heading(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn link(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn link_url(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn code(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn code_block(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn code_block_border(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn quote(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn quote_border(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn hr(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn list_bullet(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn bold(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn italic(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn strikethrough(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn underline(&self, text: &JsString) -> JsString {
        text.clone()
    }
}

fn agent(id: &str, display_name: &str, lifecycle: AgentLifecycle) -> AgentSnapshot {
    AgentSnapshot {
        id: JsString::from(id.to_string()),
        parent_id: None,
        display_name: JsString::from(display_name.to_string()),
        agent_type: JsString::from("scout"),
        origin: None,
        lifecycle,
        revision: 1,
        created_at: JsString::from("2026-01-01T00:00:00.000Z"),
        updated_at: JsString::from("2026-01-01T00:00:00.000Z"),
        cwd: JsString::from("/repo"),
        worktree: None,
        model: None,
        account: None,
        permission: AgentPermission {
            policy: JsString::from("on-request"),
            inherited_from: None,
            narrowed: true,
        },
        slot: None,
        transcript: None,
        event_stream: None,
        worker: None,
        detached: None,
        current_activity: None,
        last_activity: None,
        result: None,
        error: None,
        extra: pi_js_compat::js_json::JsObject::new(),
    }
}

#[test]
fn agent_selection_banner_renders_the_selected_agent() {
    let store = MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| Ok("2026-01-01T00:00:00.000Z".into()))),
    });
    store
        .publish_lifecycle_coordinator_snapshot(&agent("agent-1", "Scout", AgentLifecycle::WaitingForInput))
        .unwrap();
    store.select_agent_view(&JsString::from("agent-1"));

    let theme = theme();
    let mut banner = AgentSelectionBannerComponent::new(Some(Arc::new(store)), theme.clone());
    assert_eq!(
        banner.render(200),
        vec![theme.fg(
            ThemeColor::Accent,
            &theme.bold("Agent agent-1: Scout (waiting for input)")
        )]
    );

    let mut empty_banner = AgentSelectionBannerComponent::new(None, theme.clone());
    assert!(empty_banner.render(200).is_empty());
}

#[test]
fn skill_invocation_message_switches_between_collapsed_and_expanded() {
    let block = ParsedSkillBlock {
        name: JsString::from("code-review"),
        location: JsString::from("/skills/code-review"),
        content: JsString::from("Review the diff."),
        user_message: None,
    };
    let theme = theme();
    let mut component = SkillInvocationMessageComponent::new(block, Rc::new(PlainMarkdownTheme), theme.clone());

    let collapsed = component.render(80);
    let collapsed_raw = JsString::join(&collapsed, "\n");
    assert!(collapsed_raw.contains("\u{1b}[1m[skill]\u{1b}[22m"), "{collapsed:?}");
    let collapsed_text = plain(&collapsed);
    assert!(collapsed_text.contains("code-review"), "{collapsed:?}");
    assert!(collapsed_text.contains("to expand"), "{collapsed:?}");
    let background = theme.get_bg_ansi(ThemeBg::CustomMessageBg);
    assert!(collapsed.iter().any(|line| line.contains(background)), "{collapsed:?}");

    component.set_expanded(true);
    let expanded = component.render(80);
    assert!(expanded.len() > collapsed.len(), "{expanded:?}");
    let expanded_raw = JsString::join(&expanded, "\n");
    assert!(expanded_raw.contains("\u{1b}[1m[skill]\u{1b}[22m"), "{expanded:?}");
    let joined = plain(&expanded);
    assert!(joined.contains("code-review"), "{joined:?}");
    assert!(joined.contains("Review the diff."), "{joined:?}");
    assert!(expanded.iter().any(|line| line.contains(background)), "{expanded:?}");

    component.set_expanded(false);
    assert_eq!(component.render(80), collapsed);
}

#[test]
fn earendil_announcement_renders_text_and_borders_without_the_bundled_image() {
    // The test binary has no `assets/clankolas.png` next to it, so the source's
    // image-load fallback is the observable branch here.
    let theme = theme();
    let mut component = EarendilAnnouncementComponent::new(theme.clone());
    let lines = component.render(120);
    let joined = plain(&lines);
    assert!(joined.contains("pi has joined Earendil"), "{joined:?}");
    assert!(joined.contains("Read the blog post:"), "{joined:?}");
    assert!(
        joined.contains("https://mariozechner.at/posts/2026-04-08-ive-sold-out/"),
        "{joined:?}"
    );
    let accent = theme.get_fg_ansi(ThemeColor::Accent);
    assert!(lines[0].starts_with(accent), "{lines:?}");
    assert!(lines.last().expect("bottom border").starts_with(accent), "{lines:?}");
}
