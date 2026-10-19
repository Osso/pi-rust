//! `cli/resident-console-command.ts` argument parsing.

use pi_coding_agent::cli::resident_console_command::{
    ResidentConsoleCommand, ResidentConsoleEvent, ResidentConsoleUi, parse_resident_console_args,
};
use pi_coding_agent::core::resident_console_transport::{ResidentConsoleService, ResidentConsoleSnapshot};
use pi_coding_agent::core::session_manager::SessionEntry;
use serde_json::json;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn non_console_commands_are_not_handled() {
    assert_eq!(parse_resident_console_args(&[]), Ok(None));
    assert_eq!(parse_resident_console_args(&args(&["--help"])), Ok(None));
    assert_eq!(parse_resident_console_args(&args(&["--supervisors"])), Ok(None));
}

#[test]
fn service_flags_map_to_their_services() {
    assert_eq!(
        parse_resident_console_args(&args(&["--supervisor"])),
        Ok(Some(ResidentConsoleCommand {
            service: ResidentConsoleService::Supervisor,
            initial_prompt: None,
        }))
    );
    assert_eq!(
        parse_resident_console_args(&args(&["--architect"])),
        Ok(Some(ResidentConsoleCommand {
            service: ResidentConsoleService::Architect,
            initial_prompt: None,
        }))
    );
}

#[test]
fn prompt_parts_are_joined_and_trimmed() {
    assert_eq!(
        parse_resident_console_args(&args(&["--architect", "  hello", "world  "])),
        Ok(Some(ResidentConsoleCommand {
            service: ResidentConsoleService::Architect,
            initial_prompt: Some("hello world".to_owned()),
        }))
    );
    assert_eq!(
        parse_resident_console_args(&args(&["--supervisor", "   "])),
        Ok(Some(ResidentConsoleCommand {
            service: ResidentConsoleService::Supervisor,
            initial_prompt: None,
        }))
    );
}

#[test]
fn normal_cli_flags_are_rejected_with_the_service_name() {
    assert_eq!(
        parse_resident_console_args(&args(&["--supervisor", "prompt", "--model", "x"])),
        Err("--supervisor cannot be combined with normal Pi CLI flags".to_owned())
    );
    assert_eq!(
        parse_resident_console_args(&args(&["--architect", "-l"])),
        Err("--architect cannot be combined with normal Pi CLI flags".to_owned())
    );
}

fn snapshot(cwd: &str, branch: Vec<SessionEntry>) -> ResidentConsoleSnapshot<SessionEntry> {
    ResidentConsoleSnapshot {
        service: ResidentConsoleService::Supervisor,
        session_id: "session-1".to_owned(),
        cwd: cwd.to_owned(),
        generation: 7,
        branch,
    }
}

fn entry(value: serde_json::Value) -> SessionEntry {
    serde_json::from_value(value).expect("session entry")
}

fn user_entry(text: &str) -> SessionEntry {
    entry(json!({
        "type": "message",
        "id": "entry-1",
        "parentId": null,
        "timestamp": "2026-01-01T00:00:00.000Z",
        "message": { "role": "user", "content": text, "timestamp": 1 },
    }))
}

/// Theme state is thread-local, so each test thread installs its own theme.
fn init_theme_once() {
    pi_coding_agent::modes::interactive::theme::init_theme(Some("dark"), false).expect("dark theme");
}

fn rendered(console: &ResidentConsoleUi) -> String {
    console.render(80).join("\n")
}

#[test]
fn console_renders_the_header_and_snapshot_branch() {
    init_theme_once();
    let console = ResidentConsoleUi::new(
        ResidentConsoleService::Supervisor,
        &snapshot("/tmp/project", vec![user_entry("hello from snapshot")]),
    );
    let output = rendered(&console);
    assert!(output.contains("supervisor resident console"), "{output}");
    assert!(output.contains("/tmp/project · pid 7"), "{output}");
    assert!(output.contains("hello from snapshot"), "{output}");
}

#[test]
fn console_skips_duplicate_turn_guard_messages() {
    init_theme_once();
    init_theme_once();
    let console = ResidentConsoleUi::new(ResidentConsoleService::Architect, &snapshot("/tmp", Vec::new()));
    let guard = json!({
        "role": "user",
        "content": [{ "type": "text", "text": "guard prompt" }],
        "inputSource": "extension",
        "timestamp": 2,
    });
    console.handle_event(ResidentConsoleEvent::MessageStart {
        message: serde_json::from_value(guard.clone()).expect("guard message"),
        runtime_message_marker: Some(
            pi_coding_agent::core::runtime_message_markers::RuntimeMessageMarker::DuplicateTurnGuard,
        ),
    });
    assert!(!rendered(&console).contains("guard prompt"));

    let message = serde_json::from_value(json!({
        "role": "user",
        "content": "visible",
        "timestamp": 3,
    }))
    .expect("user message");
    console.handle_event(ResidentConsoleEvent::MessageStart {
        message,
        runtime_message_marker: None,
    });
    assert!(rendered(&console).contains("visible"));
}

#[test]
fn console_removes_duplicate_turn_assistant_messages() {
    init_theme_once();
    let console = ResidentConsoleUi::new(ResidentConsoleService::Supervisor, &snapshot("/tmp", Vec::new()));
    let assistant = json!({
        "role": "assistant",
        "content": [{ "type": "text", "text": "same answer" }],
        "api": "faux",
        "provider": "faux",
        "model": "faux-1",
        "usage": {
            "input": 1, "output": 1, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 2,
            "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0 }
        },
        "stopReason": "stop",
        "timestamp": 4,
    });
    let message: pi_agent_core::agent::types::AgentMessage =
        serde_json::from_value(assistant).expect("assistant message");
    console.handle_event(ResidentConsoleEvent::MessageStart {
        message: message.clone(),
        runtime_message_marker: None,
    });
    assert!(rendered(&console).contains("same answer"));

    console.handle_event(ResidentConsoleEvent::MessageEnd {
        message,
        runtime_message_marker: Some(
            pi_coding_agent::core::runtime_message_markers::RuntimeMessageMarker::DuplicateTurnAssistant,
        ),
    });
    assert!(!rendered(&console).contains("same answer"));
}

#[test]
fn console_renders_supervisor_response_tool_results_as_plain_text() {
    init_theme_once();
    let console = ResidentConsoleUi::new(ResidentConsoleService::Supervisor, &snapshot("/tmp", Vec::new()));
    let message = serde_json::from_value(json!({
        "role": "toolResult",
        "toolCallId": "call-1",
        "toolName": "supervisor_response",
        "content": [{ "type": "text", "text": "decision recorded" }],
        "isError": false,
        "timestamp": 5,
    }))
    .expect("tool result");
    console.handle_event(ResidentConsoleEvent::MessageStart {
        message,
        runtime_message_marker: None,
    });
    assert!(rendered(&console).contains("decision recorded"));
}

#[test]
fn console_wire_events_parse_the_session_stream() {
    let event: ResidentConsoleEvent =
        serde_json::from_value(json!({"type": "agent_end", "messages": [], "willRetry": false})).expect("agent_end");
    assert!(matches!(event, ResidentConsoleEvent::AgentEnd));

    let event: ResidentConsoleEvent = serde_json::from_value(json!({
        "type": "entry_appended",
        "entry": user_entry("from wire"),
    }))
    .expect("entry_appended");
    assert!(matches!(event, ResidentConsoleEvent::EntryAppended { .. }));

    let event: ResidentConsoleEvent = serde_json::from_value(json!({
        "type": "message_start",
        "message": { "role": "user", "content": "guarded", "timestamp": 9 },
        "runtimeMessageMarker": "duplicate_turn_guard",
    }))
    .expect("message_start");
    match event {
        ResidentConsoleEvent::MessageStart {
            runtime_message_marker, ..
        } => assert_eq!(
            runtime_message_marker,
            Some(pi_coding_agent::core::runtime_message_markers::RuntimeMessageMarker::DuplicateTurnGuard)
        ),
        other => panic!("unexpected event: {other:?}"),
    }

    let event: ResidentConsoleEvent =
        serde_json::from_value(json!({"type": "queue_update", "steering": [], "followUp": []})).expect("queue_update");
    assert!(matches!(event, ResidentConsoleEvent::Other));
}
