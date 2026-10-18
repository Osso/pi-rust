#[path = "../support/oracle.rs"]
mod oracle;

use std::collections::HashMap;

use pi_coding_agent::core::skills::Skill;
use pi_coding_agent::core::source_info::{SyntheticSourceInfoOptions, create_synthetic_source_info};
use pi_coding_agent::core::system_prompt::{BuildSystemPromptOptions, ContextFile, build_system_prompt};
use serde_json::{Value, json};

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-system-prompt-oracle.ts",
        "packages/coding-agent/src/core/system-prompt.ts",
    );

    for (index, options) in cases().into_iter().enumerate() {
        let response = oracle.request(&json!({"id": index, "options": options_json(&options)}));
        assert_eq!(response["ok"], true, "oracle failed: {response}");

        let actual = normalize_paths(&build_system_prompt(&options));
        let expected = normalize_paths(response["output"]["prompt"].as_str().expect("prompt string"));
        assert_eq!(actual, expected, "case {index}");
    }
}

fn normalize_paths(prompt: &str) -> String {
    prompt
        .lines()
        .map(|line| {
            for (prefix, placeholder) in [
                ("- Main documentation: ", "- Main documentation: <readme>"),
                ("- Additional docs: ", "- Additional docs: <docs>"),
                ("- Examples: ", "- Examples: <examples>"),
            ] {
                if line.starts_with(prefix) {
                    return placeholder.to_owned();
                }
            }
            line.to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn options_json(options: &BuildSystemPromptOptions) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("cwd".to_owned(), json!(options.cwd));
    if let Some(custom_prompt) = &options.custom_prompt {
        object.insert("customPrompt".to_owned(), json!(custom_prompt));
    }
    if let Some(selected_tools) = &options.selected_tools {
        object.insert("selectedTools".to_owned(), json!(selected_tools));
    }
    if let Some(tool_snippets) = &options.tool_snippets {
        object.insert("toolSnippets".to_owned(), json!(tool_snippets));
    }
    if let Some(prompt_guidelines) = &options.prompt_guidelines {
        object.insert("promptGuidelines".to_owned(), json!(prompt_guidelines));
    }
    if let Some(append_system_prompt) = &options.append_system_prompt {
        object.insert("appendSystemPrompt".to_owned(), json!(append_system_prompt));
    }
    if let Some(context_files) = &options.context_files {
        object.insert("contextFiles".to_owned(), json!(context_files));
    }
    if let Some(rules_content) = &options.rules_content {
        object.insert("rulesContent".to_owned(), json!(rules_content));
    }
    if let Some(skills) = &options.skills {
        object.insert("skills".to_owned(), json!(skills));
    }
    Value::Object(object)
}

fn cases() -> Vec<BuildSystemPromptOptions> {
    let base = |cwd: &str| BuildSystemPromptOptions {
        cwd: cwd.to_owned(),
        ..BuildSystemPromptOptions::default()
    };
    let mut cases = Vec::new();

    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(Vec::new()),
        context_files: Some(Vec::new()),
        skills: Some(Vec::new()),
        ..base("/repo/project-feature")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(Vec::new()),
        ..base("/repo/project")
    });

    let snippets: HashMap<String, String> = [
        ("read", "Read file contents"),
        ("bash", "Execute bash commands"),
        ("edit", "Make surgical edits"),
        ("write", "Create or overwrite files"),
        ("grep", "Search file contents"),
        ("find", "Search for files by glob"),
        ("ls", "List directory contents"),
    ]
    .into_iter()
    .map(|(name, snippet)| (name.to_owned(), snippet.to_owned()))
    .collect();
    cases.push(BuildSystemPromptOptions {
        tool_snippets: Some(snippets.clone()),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["read".to_owned(), "dynamic_tool".to_owned()]),
        tool_snippets: Some([("dynamic_tool".to_owned(), "Run dynamic test behavior".to_owned())].into()),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["read".to_owned(), "dynamic_tool".to_owned()]),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["bash".to_owned(), "wait_agent".to_owned()]),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["bash".to_owned()]),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec![
            "read".to_owned(),
            "grep".to_owned(),
            "find".to_owned(),
            "spawn_agent".to_owned(),
        ]),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["spawn_agent".to_owned()]),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["read".to_owned(), "grep".to_owned(), "find".to_owned()]),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["read".to_owned(), "dynamic_tool".to_owned()]),
        prompt_guidelines: Some(vec![
            "Use dynamic_tool for summaries.".to_owned(),
            "  Use dynamic_tool for summaries.  ".to_owned(),
            "   ".to_owned(),
        ]),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        context_files: Some(vec![ContextFile {
            path: "/repo/AGENTS.md".to_owned(),
            content: "Project instructions.".to_owned(),
        }]),
        rules_content: Some("Global rule.\n\nProject rule.".to_owned()),
        skills: Some(Vec::new()),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        rules_content: Some(String::new()),
        ..base("/repo/project")
    });

    let skills = vec![
        skill("visible-skill", "A visible skill.", "/path/visible/SKILL.md", false),
        skill("hidden-skill", "A hidden skill.", "/path/hidden/SKILL.md", true),
    ];
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["read".to_owned()]),
        skills: Some(skills.clone()),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["grep".to_owned()]),
        skills: Some(skills.clone()),
        ..base("/repo/project")
    });

    cases.push(BuildSystemPromptOptions {
        custom_prompt: Some("Custom prompt.".to_owned()),
        append_system_prompt: Some("Appended.".to_owned()),
        context_files: Some(vec![ContextFile {
            path: "/repo/AGENTS.md".to_owned(),
            content: "Project instructions.".to_owned(),
        }]),
        rules_content: Some("Global rule.".to_owned()),
        selected_tools: Some(vec!["read".to_owned()]),
        skills: Some(skills.clone()),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        custom_prompt: Some("Custom prompt without read.".to_owned()),
        selected_tools: Some(vec!["grep".to_owned()]),
        skills: Some(skills),
        ..base("C:\\repo\\project")
    });
    cases.push(BuildSystemPromptOptions {
        custom_prompt: Some(String::new()),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        append_system_prompt: Some("Extra instructions.".to_owned()),
        ..base("/repo/project")
    });

    cases
}

fn skill(name: &str, description: &str, file_path: &str, hidden: bool) -> Skill {
    let base_dir = std::path::Path::new(file_path)
        .parent()
        .map_or_else(String::new, |path| path.to_string_lossy().into_owned());
    Skill {
        name: name.to_owned(),
        description: description.to_owned(),
        file_path: file_path.to_owned(),
        base_dir: base_dir.clone(),
        source_info: create_synthetic_source_info(
            file_path,
            &SyntheticSourceInfoOptions {
                source: "test".to_owned(),
                scope: None,
                origin: None,
                base_dir: Some(base_dir),
            },
        ),
        disable_model_invocation: hidden,
    }
}
