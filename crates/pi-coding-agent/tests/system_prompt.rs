use pi_js_compat::js_json::JsString;
use std::collections::HashMap;

use pi_coding_agent::core::skills::Skill;
use pi_coding_agent::core::source_info::{SyntheticSourceInfoOptions, create_synthetic_source_info};
use pi_coding_agent::core::system_prompt::{BuildSystemPromptOptions, ContextFile, build_system_prompt};

fn options(cwd: &str) -> BuildSystemPromptOptions {
    BuildSystemPromptOptions {
        cwd: cwd.to_owned().into(),
        ..BuildSystemPromptOptions::default()
    }
}

#[test]
fn shows_the_resolved_cwd_and_none_for_empty_tools() {
    let prompt = build_system_prompt(&BuildSystemPromptOptions {
        selected_tools: Some(Vec::new()),
        context_files: Some(Vec::new()),
        skills: Some(Vec::new()),
        ..options("/repo/project-feature")
    });

    assert!(prompt.contains("Available tools:\n(none)"));
    assert!(prompt.contains("Current working directory: /repo/project-feature"));
    assert!(!prompt.contains("Current working directory: /repo/project\n"));
    assert!(prompt.contains("Show file paths clearly"));
}

#[test]
fn normalizes_backslashes_in_the_cwd() {
    let prompt = build_system_prompt(&options("C:\\repo\\project"));
    assert!(prompt.ends_with("Current working directory: C:/repo/project"));
}

#[test]
fn warns_about_auto_backgrounding_only_with_wait_agent() {
    let prompt = build_system_prompt(&BuildSystemPromptOptions {
        selected_tools: Some(vec!["bash".to_owned(), "wait_agent".to_owned()]),
        ..options("/repo/project")
    });
    assert!(prompt.contains(
        "Supported long-running command tools such as bash and Pyrun are automatically backgrounded after 10 minutes"
    ));

    let prompt = build_system_prompt(&BuildSystemPromptOptions {
        selected_tools: Some(vec!["bash".to_owned()]),
        ..options("/repo/project")
    });
    assert!(!prompt.contains("use wait_agent to wait for any agent completion"));
}

#[test]
fn wraps_rules_after_project_context_and_omits_empty_rules() {
    let prompt = build_system_prompt(&BuildSystemPromptOptions {
        context_files: Some(vec![ContextFile {
            path: "/repo/AGENTS.md".to_owned().into(),
            content: "Project instructions.".to_owned().into(),
        }]),
        rules_content: Some("Global rule.\n\nProject rule.".to_owned().into()),
        ..options("/repo/project")
    });

    assert!(prompt.contains("</project_context>\n\n<user_rules>\nGlobal rule.\n\nProject rule.\n</user_rules>"));
    assert!(prompt.find("</project_context>").unwrap() < prompt.find("<user_rules>").unwrap());

    let prompt = build_system_prompt(&options("/repo/project"));
    assert!(!prompt.contains("<user_rules>"));
}

#[test]
fn custom_prompt_appends_context_rules_and_skills() {
    let skill = Skill {
        name: "visible-skill".to_owned().into(),
        description: "A visible skill.".to_owned().into(),
        file_path: "/path/visible/SKILL.md".to_owned().into(),
        base_dir: "/path/visible".to_owned().into(),
        source_info: create_synthetic_source_info(
            &JsString::from("/path/visible/SKILL.md"),
            &SyntheticSourceInfoOptions {
                source: "test".to_owned().into(),
                scope: None,
                origin: None,
                base_dir: Some("/path/visible".to_owned().into()),
            },
        ),
        disable_model_invocation: false,
    };

    let prompt = build_system_prompt(&BuildSystemPromptOptions {
        custom_prompt: Some("Custom prompt.".to_owned().into()),
        append_system_prompt: Some("Appended.".to_owned().into()),
        context_files: Some(vec![ContextFile {
            path: "/repo/AGENTS.md".to_owned().into(),
            content: "Project instructions.".to_owned().into(),
        }]),
        rules_content: Some("Global rule.".to_owned().into()),
        selected_tools: Some(vec!["read".to_owned()]),
        skills: Some(vec![skill]),
        ..options("/repo/project")
    });

    assert!(prompt.starts_with("Custom prompt.\n\nAppended.\n\n<project_context>"));
    assert!(prompt.contains("<name>visible-skill</name>"));
    assert!(prompt.ends_with("Current working directory: /repo/project"));
}

#[test]
fn includes_tool_snippets_only_for_selected_tools() {
    let prompt = build_system_prompt(&BuildSystemPromptOptions {
        selected_tools: Some(vec!["read".to_owned(), "dynamic_tool".to_owned()]),
        tool_snippets: Some(HashMap::from([(
            "dynamic_tool".to_owned(),
            JsString::from("Run dynamic test behavior"),
        )])),
        ..options("/repo/project")
    });

    assert!(prompt.contains("- dynamic_tool: Run dynamic test behavior"));
    assert!(!prompt.contains("- read:"));
}
