//! Behavioral tests for interactive components whose inputs are OS/state bound.

use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pi_coding_agent::ParsedSkillBlock;
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, AgentPermission, AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions,
};
use pi_coding_agent::modes::interactive::components::agent_selection_banner::AgentSelectionBannerComponent;
use pi_coding_agent::modes::interactive::components::countdown_timer::CountdownTimer;
use pi_coding_agent::modes::interactive::components::earendil_announcement::EarendilAnnouncementComponent;
use pi_coding_agent::modes::interactive::components::skill_invocation_message::SkillInvocationMessageComponent;
use pi_coding_agent::modes::interactive::components::user_message_selector::UserMessageSelectorComponent;
use pi_coding_agent::modes::interactive::theme::{
    ColorMode, ColorValue, REQUIRED_COLORS, Theme, ThemeColor, set_theme_instance,
};
use pi_tui::{Component, MarkdownTheme, strip_terminal_sequences};

fn install_theme() -> Arc<Theme> {
    let colors: Vec<(String, ColorValue)> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                key.to_string(),
                ColorValue::Text(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * index) % 256,
                    (32 + 11 * index) % 256,
                    (48 + 13 * index) % 256
                )),
            )
        })
        .collect();
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).expect("test theme"));
    set_theme_instance(theme.clone());
    theme
}

fn theme() -> Arc<Theme> {
    install_theme()
}

fn plain(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect::<Vec<_>>()
        .join("\n")
}

struct PlainMarkdownTheme;

impl MarkdownTheme for PlainMarkdownTheme {
    fn heading(&self, text: &str) -> String {
        text.to_string()
    }
    fn link(&self, text: &str) -> String {
        text.to_string()
    }
    fn link_url(&self, text: &str) -> String {
        text.to_string()
    }
    fn code(&self, text: &str) -> String {
        text.to_string()
    }
    fn code_block(&self, text: &str) -> String {
        text.to_string()
    }
    fn code_block_border(&self, text: &str) -> String {
        text.to_string()
    }
    fn quote(&self, text: &str) -> String {
        text.to_string()
    }
    fn quote_border(&self, text: &str) -> String {
        text.to_string()
    }
    fn hr(&self, text: &str) -> String {
        text.to_string()
    }
    fn list_bullet(&self, text: &str) -> String {
        text.to_string()
    }
    fn bold(&self, text: &str) -> String {
        text.to_string()
    }
    fn italic(&self, text: &str) -> String {
        text.to_string()
    }
    fn strikethrough(&self, text: &str) -> String {
        text.to_string()
    }
    fn underline(&self, text: &str) -> String {
        text.to_string()
    }
}

fn agent(id: &str, display_name: &str, lifecycle: AgentLifecycle) -> AgentSnapshot {
    AgentSnapshot {
        id: id.to_string(),
        parent_id: None,
        display_name: display_name.to_string(),
        agent_type: "scout".to_string(),
        origin: None,
        lifecycle,
        revision: 1,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        cwd: "/repo".to_string(),
        worktree: None,
        model: None,
        account: None,
        permission: AgentPermission {
            policy: "on-request".to_string(),
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
        extra: serde_json::Map::new(),
    }
}

#[test]
fn agent_selection_banner_renders_the_selected_agent() {
    let store = MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| "2026-01-01T00:00:00.000Z".to_string())),
    });
    store.publish_lifecycle_coordinator_snapshot(&agent("agent-1", "Scout", AgentLifecycle::WaitingForInput));
    store.select_agent_view("agent-1");

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
        name: "code-review".to_string(),
        location: "/skills/code-review".to_string(),
        content: "Review the diff.".to_string(),
        user_message: None,
    };
    let theme = theme();
    let mut component = SkillInvocationMessageComponent::new(block, Rc::new(PlainMarkdownTheme), theme.clone());

    let collapsed = component.render(80);
    let collapsed_raw = collapsed.join("\n");
    assert!(collapsed_raw.contains("\u{1b}[1m[skill]\u{1b}[22m"), "{collapsed:?}");
    let collapsed_text = plain(&collapsed);
    assert!(collapsed_text.contains("code-review"), "{collapsed:?}");
    assert!(collapsed_text.contains("to expand"), "{collapsed:?}");
    let background = theme.get_bg_ansi(pi_coding_agent::modes::interactive::theme::ThemeBg::CustomMessageBg);
    assert!(collapsed.iter().any(|line| line.contains(background)), "{collapsed:?}");

    component.set_expanded(true);
    let expanded = component.render(80);
    assert!(expanded.len() > collapsed.len(), "{expanded:?}");
    let expanded_raw = expanded.join("\n");
    assert!(expanded_raw.contains("\u{1b}[1m[skill]\u{1b}[22m"), "{expanded:?}");
    let joined = plain(&expanded);
    assert!(joined.contains("code-review"), "{joined}");
    assert!(joined.contains("Review the diff."), "{joined}");
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
    assert!(joined.contains("pi has joined Earendil"), "{joined}");
    assert!(joined.contains("Read the blog post:"), "{joined}");
    assert!(
        joined.contains("https://mariozechner.at/posts/2026-04-08-ive-sold-out/"),
        "{joined}"
    );
    let accent = theme.get_fg_ansi(ThemeColor::Accent);
    assert!(lines[0].starts_with(accent), "{lines:?}");
    assert!(lines.last().expect("bottom border").starts_with(accent), "{lines:?}");
}

#[test]
fn countdown_timer_ticks_once_per_second_and_expires() {
    let ticks = Rc::new(Mutex::new(Vec::new()));
    let expired = Rc::new(Mutex::new(0));
    let renders = Rc::new(Mutex::new(0));
    let timer_ticks = ticks.clone();
    let timer_expired = expired.clone();
    let timer_renders = renders.clone();
    let mut timer = CountdownTimer::new(
        2500.0,
        Some(Rc::new(move || *timer_renders.lock().expect("renders") += 1)),
        Rc::new(move |seconds| timer_ticks.lock().expect("ticks").push(seconds)),
        Rc::new(move || *timer_expired.lock().expect("expired") += 1),
    );

    assert_eq!(*ticks.lock().expect("ticks"), vec![3]);
    let start = Instant::now();
    assert!(!timer.poll(start + Duration::from_millis(999)));
    assert!(timer.poll(start + Duration::from_millis(1000)));
    assert_eq!(*ticks.lock().expect("ticks"), vec![3, 2]);
    assert_eq!(*renders.lock().expect("renders"), 1);

    assert!(timer.poll(start + Duration::from_millis(5000)));
    assert_eq!(*ticks.lock().expect("ticks"), vec![3, 2, 1, 0]);
    assert_eq!(*expired.lock().expect("expired"), 1);
    assert!(timer.is_disposed());
    assert!(!timer.poll(start + Duration::from_millis(9000)));
}

#[test]
fn user_message_selector_auto_cancels_when_empty() {
    let cancelled = Rc::new(Mutex::new(0));
    let on_cancel = {
        let cancelled = cancelled.clone();
        Rc::new(move || *cancelled.lock().expect("cancelled") += 1)
    };
    let mut component =
        UserMessageSelectorComponent::new(Vec::new(), Rc::new(|_id: &str| {}), on_cancel, None, theme());
    assert!(plain(&component.render(40)).contains("  No user messages found"));

    let now = Instant::now();
    assert!(!component.poll(now + Duration::from_millis(99)));
    assert!(component.poll(now + Duration::from_millis(100)));
    assert_eq!(*cancelled.lock().expect("cancelled"), 1);
    assert!(!component.poll(now + Duration::from_millis(500)));
    assert_eq!(*cancelled.lock().expect("cancelled"), 1);
}

#[test]
fn trust_selector_renders_saved_decision_variants() {
    use pi_coding_agent::core::trust_manager::{ProjectTrustDecision, ProjectTrustStoreEntry};
    use pi_coding_agent::modes::interactive::components::trust_selector::{
        TrustSelectorComponent, TrustSelectorOptions,
    };

    let saved = ProjectTrustStoreEntry {
        path: "/work".to_string(),
        decision: ProjectTrustDecision::Untrusted,
    };
    let mut component = TrustSelectorComponent::new(
        TrustSelectorOptions {
            cwd: "/work/project".to_string(),
            saved_decision: Some(saved),
            project_trusted: false,
            on_select: Rc::new(|_| {}),
            on_cancel: Rc::new(|| {}),
        },
        theme(),
    );
    let joined = component.render(100).join("\n");
    assert!(
        joined.contains("Saved decision: untrusted (inherited from /work)"),
        "{joined}"
    );
    assert!(joined.contains("Current session: untrusted"), "{joined}");
    assert_eq!(component.selected_index(), 0);
}
