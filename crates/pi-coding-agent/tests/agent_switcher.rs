//! Behavioral tests for `modes/interactive/components/agent-switcher.ts`.
//! Ports every case of `test/agent-switcher-selector.test.ts`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::keybindings::keybindings;
use pi_coding_agent::core::multi_agent_store::{AgentLifecycle, AgentPermission, AgentSlot, AgentSnapshot};
use pi_coding_agent::modes::interactive::components::agent_switcher::AgentSwitcherComponent;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_tui::{Component, KeybindingsManager, set_keybindings, strip_terminal_sequences};

/// Keybindings are process-global; serialize the cases that reset them.
static KEYBINDINGS_LOCK: Mutex<()> = Mutex::new(());

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

fn reset_keybindings(bindings: &[(&str, &str)]) {
    let user = bindings
        .iter()
        .map(|(id, key)| ((*id).to_string(), vec![(*key).to_string()]))
        .collect();
    set_keybindings(KeybindingsManager::new(keybindings(), user));
}

fn agent(id: &str, display_name: &str) -> AgentSnapshot {
    AgentSnapshot {
        id: id.to_string(),
        parent_id: None,
        display_name: display_name.to_string(),
        agent_type: "worker".to_string(),
        origin: None,
        lifecycle: AgentLifecycle::Running,
        revision: 1,
        created_at: "2026-06-30T00:00:00.000Z".to_string(),
        updated_at: "2026-06-30T00:00:00.000Z".to_string(),
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

fn rendered_text(component: &mut AgentSwitcherComponent, width: usize) -> String {
    component
        .render(width)
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect::<Vec<_>>()
        .join("\n")
}

type SelectLog = Rc<RefCell<Vec<String>>>;

fn selector(
    agents: Vec<AgentSnapshot>,
    selected: Option<&str>,
    on_select: SelectLog,
    on_cancel: Rc<dyn Fn()>,
) -> AgentSwitcherComponent {
    let theme = install_theme();
    AgentSwitcherComponent::new(
        agents,
        selected.map(str::to_string),
        Rc::new(move |agent_id: &str| on_select.borrow_mut().push(agent_id.to_string())),
        on_cancel,
        Rc::new(|_agent: &AgentSnapshot| {}),
        theme,
    )
}

#[test]
fn renders_slot_row_index_display_name_id_lifecycle_type_and_selected_marker() {
    let _guard = KEYBINDINGS_LOCK.lock().expect("keybindings lock");
    reset_keybindings(&[]);
    let mut scout = agent("agent-1", "Scout");
    scout.slot = Some(AgentSlot { index: 2, pinned: true });
    let mut builder = agent("agent-2", "Builder");
    builder.agent_type = "background".to_string();
    builder.lifecycle = AgentLifecycle::WaitingForInput;

    let mut component = selector(
        vec![scout, builder],
        Some("agent-2"),
        SelectLog::default(),
        Rc::new(|| {}),
    );
    let text = rendered_text(&mut component, 100);

    for expected in [
        "Agents",
        "#2",
        "2.",
        "Scout",
        "agent-1",
        "running",
        "worker",
        "selected",
        "Builder",
        "waiting for input",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in {text}");
    }
}

#[test]
fn renders_child_agents_nested_under_their_parent() {
    let _guard = KEYBINDINGS_LOCK.lock().expect("keybindings lock");
    reset_keybindings(&[]);
    let mut child = agent("agent-3", "Verifier");
    child.parent_id = Some("agent-2".to_string());
    let mut component = selector(
        vec![agent("agent-2", "Builder"), child],
        None,
        SelectLog::default(),
        Rc::new(|| {}),
    );
    let text = rendered_text(&mut component, 100);
    assert!(text.contains("2. Builder"), "{text}");
    assert!(text.contains("└─ -- 3. Verifier"), "{text}");
}

#[test]
fn selects_the_highlighted_agent_by_id() {
    let _guard = KEYBINDINGS_LOCK.lock().expect("keybindings lock");
    reset_keybindings(&[]);
    let selected = SelectLog::default();
    let mut component = selector(
        vec![agent("agent-1", "Scout"), agent("agent-2", "Builder")],
        Some("agent-1"),
        selected.clone(),
        Rc::new(|| {}),
    );
    component.handle_input("\u{1b}[B");
    component.handle_input("\r");
    assert_eq!(*selected.borrow(), vec!["agent-2"]);
}

#[test]
fn hides_inactive_agents_by_default() {
    let _guard = KEYBINDINGS_LOCK.lock().expect("keybindings lock");
    reset_keybindings(&[]);
    let mut done = agent("agent-2", "Done");
    done.lifecycle = AgentLifecycle::Completed;
    let mut component = selector(
        vec![agent("agent-1", "Scout"), done],
        Some("agent-1"),
        SelectLog::default(),
        Rc::new(|| {}),
    );
    let text = rendered_text(&mut component, 100);
    assert!(text.contains("Scout"), "{text}");
    assert!(text.contains("1 closed agent hidden"), "{text}");
    assert!(!text.contains("Done"), "{text}");
    assert!(!text.contains("completed inactive"), "{text}");
}

#[test]
fn toggles_inactive_agents_and_refuses_to_select_them() {
    let _guard = KEYBINDINGS_LOCK.lock().expect("keybindings lock");
    reset_keybindings(&[("app.agent.toggleClosed", "ctrl+x")]);
    let selected = SelectLog::default();
    let inactive_selects = Rc::new(RefCell::new(0));
    let mut done = agent("agent-2", "Done");
    done.lifecycle = AgentLifecycle::Completed;
    let theme = install_theme();
    let mut component = AgentSwitcherComponent::new(
        vec![agent("agent-1", "Scout"), done],
        Some("agent-1".to_string()),
        {
            let selected = selected.clone();
            Rc::new(move |agent_id: &str| selected.borrow_mut().push(agent_id.to_string()))
        },
        Rc::new(|| {}),
        {
            let inactive_selects = inactive_selects.clone();
            Rc::new(move |_agent: &AgentSnapshot| *inactive_selects.borrow_mut() += 1)
        },
        theme,
    );

    component.handle_input("\u{18}");
    component.handle_input("\u{1b}[B");
    component.handle_input("\r");

    let text = rendered_text(&mut component, 100);
    assert!(text.contains("Done"), "{text}");
    assert!(text.contains("completed inactive"), "{text}");
    assert!(text.contains("Showing 1 closed agent"), "{text}");
    assert!(selected.borrow().is_empty());
    assert_eq!(*inactive_selects.borrow(), 1);
}

#[test]
fn does_not_hardcode_enter_when_confirm_is_remapped() {
    let _guard = KEYBINDINGS_LOCK.lock().expect("keybindings lock");
    reset_keybindings(&[("tui.select.confirm", "ctrl+x")]);
    let selected = SelectLog::default();
    let mut component = selector(
        vec![agent("agent-1", "Scout")],
        Some("agent-1"),
        selected.clone(),
        Rc::new(|| {}),
    );
    component.handle_input("\r");
    component.handle_input("\u{18}");
    assert_eq!(*selected.borrow(), vec!["agent-1"]);
}

#[test]
fn renders_main_thread_as_current_when_no_child_agents_exist() {
    let _guard = KEYBINDINGS_LOCK.lock().expect("keybindings lock");
    reset_keybindings(&[]);
    let mut component = selector(vec![], None, SelectLog::default(), Rc::new(|| {}));
    let text = rendered_text(&mut component, 100);
    assert!(text.contains("Main thread"), "{text}");
    assert!(text.contains("current"), "{text}");
    assert!(!text.contains("No agents"), "{text}");
}

#[test]
fn renders_main_thread_as_a_selectable_current_row_above_child_agents() {
    let _guard = KEYBINDINGS_LOCK.lock().expect("keybindings lock");
    reset_keybindings(&[]);
    let selected = SelectLog::default();
    let mut component = selector(vec![agent("agent-1", "Scout")], None, selected.clone(), Rc::new(|| {}));
    let text = rendered_text(&mut component, 100);
    assert!(text.contains("Main thread"), "{text}");
    assert!(text.contains("current"), "{text}");
    assert!(text.contains("Scout"), "{text}");

    component.handle_input("\r");
    assert_eq!(*selected.borrow(), vec!["main"]);
}

#[test]
fn selects_main_when_confirming_the_main_thread_row() {
    let _guard = KEYBINDINGS_LOCK.lock().expect("keybindings lock");
    reset_keybindings(&[]);
    let selected = SelectLog::default();
    let cancelled = Rc::new(RefCell::new(0));
    let mut component = selector(vec![], None, selected.clone(), {
        let cancelled = cancelled.clone();
        Rc::new(move || *cancelled.borrow_mut() += 1)
    });
    component.handle_input("\r");
    assert_eq!(*selected.borrow(), vec!["main"]);
    assert_eq!(*cancelled.borrow(), 0);
}
