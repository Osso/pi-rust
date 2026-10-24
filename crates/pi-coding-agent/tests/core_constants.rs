//! Constant and value-only modules: provider display names,
//! tool capabilities, diagnostics records, and session errors.

use pi_coding_agent::core::diagnostics::{ResourceCollision, ResourceDiagnostic, ResourceDiagnosticType, ResourceType};
use pi_coding_agent::core::provider_display_names::{BUILT_IN_PROVIDER_DISPLAY_NAMES, built_in_provider_display_name};
use pi_coding_agent::core::session_errors::SessionImportFileNotFoundError;
use pi_coding_agent::core::tool_capabilities::{CHILD_DISABLED_AGENT_TOOL_NAMES, SUPERVISOR_ONLY_TOOL_NAMES};
use serde_json::json;

#[test]
fn exposes_built_in_provider_display_names_in_source_order() {
    assert_eq!(BUILT_IN_PROVIDER_DISPLAY_NAMES.len(), 32);
    assert_eq!(BUILT_IN_PROVIDER_DISPLAY_NAMES[0], ("anthropic", "Anthropic"));
    assert_eq!(
        BUILT_IN_PROVIDER_DISPLAY_NAMES[31],
        ("xiaomi-token-plan-sgp", "Xiaomi MiMo Token Plan (Singapore)")
    );
    assert_eq!(built_in_provider_display_name("zai"), Some("ZAI Coding Plan (Global)"));
    assert_eq!(built_in_provider_display_name("radius"), None);
}

#[test]
fn exposes_tool_capability_constants() {
    assert_eq!(
        // Frozen tool-capabilities.ts:1-11 includes both answer_question and set_agent_model.
        CHILD_DISABLED_AGENT_TOOL_NAMES.as_slice(),
        [
            "agent_viewer",
            "answer_question",
            "attach_session_agent",
            "close_agent",
            "list_agents",
            "set_agent_model",
            "spawn_agent",
            "steer_agent",
            "wait_agent"
        ]
    );
    assert_eq!(SUPERVISOR_ONLY_TOOL_NAMES, ["ask_supervisor", "manage_goal"]);
}

#[test]
fn serializes_resource_diagnostics_with_typescript_wire_names() {
    let diagnostic = ResourceDiagnostic {
        diagnostic_type: ResourceDiagnosticType::Collision,
        message: "duplicate skill".into(),
        path: Some("/b/SKILL.md".into()),
        collision: Some(ResourceCollision {
            resource_type: ResourceType::Skill,
            name: "review".into(),
            winner_path: "/a/SKILL.md".into(),
            loser_path: "/b/SKILL.md".into(),
            winner_source: Some("local".into()),
            loser_source: None,
        }),
    };
    let value = serde_json::to_value(&diagnostic).unwrap();
    assert_eq!(
        value,
        json!({
            "type": "collision",
            "message": "duplicate skill",
            "path": "/b/SKILL.md",
            "collision": {
                "resourceType": "skill",
                "name": "review",
                "winnerPath": "/a/SKILL.md",
                "loserPath": "/b/SKILL.md",
                "winnerSource": "local"
            }
        })
    );
    assert_eq!(serde_json::from_value::<ResourceDiagnostic>(value).unwrap(), diagnostic);
    let warning: ResourceDiagnostic = serde_json::from_value(json!({"type": "warning", "message": "w"})).unwrap();
    assert_eq!(warning.diagnostic_type, ResourceDiagnosticType::Warning);
    assert_eq!(warning.path, None);
}

#[test]
fn session_import_error_reports_missing_file() {
    let error = SessionImportFileNotFoundError::new("/tmp/missing.jsonl");
    assert_eq!(error.to_string(), "File not found: /tmp/missing.jsonl");
    assert_eq!(error.file_path, "/tmp/missing.jsonl");
    assert_eq!(SessionImportFileNotFoundError::NAME, "SessionImportFileNotFoundError");
    let boxed: Box<dyn std::error::Error> = Box::new(error);
    assert_eq!(boxed.to_string(), "File not found: /tmp/missing.jsonl");
}
