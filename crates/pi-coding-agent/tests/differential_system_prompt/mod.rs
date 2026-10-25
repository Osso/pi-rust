use crate::shared_oracle as oracle;
#[path = "../support/system_prompt_probe.rs"]
mod probe;

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;

use pi_coding_agent::core::skills::Skill;
use pi_coding_agent::core::source_info::{SyntheticSourceInfoOptions, create_synthetic_source_info};
use pi_coding_agent::core::system_prompt::{BuildSystemPromptOptions, ContextFile};
use serde_json::Value;

pub(crate) fn run_probe() {
    probe::run_child_probe();
}

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-system-prompt-oracle.ts",
        "packages/coding-agent/src/core/system-prompt.ts",
    );

    for (now_millis, time_zone) in [
        (1709251200000.0, "UTC"),
        (1709251200000.0, "America/Los_Angeles"),
        (-62135510400000.0, "UTC"),
    ] {
        for (index, options) in cases().into_iter().enumerate() {
            let request = options_json(&options);
            let response = oracle.request(
                &pi_js_compat::js_json!({"id":index,"options":request,"nowMillis":now_millis,"timeZone":time_zone}),
            );
            assert_eq!(
                response.get("ok"),
                Some(&pi_js_compat::js_json::JsValue::Bool(true)),
                "oracle failed: {response:?}"
            );
            let actual = probe::spawn_child_probe(&package_dir(), &request, now_millis, time_zone);
            let expected = response["output"]["prompt"].as_string().expect("native prompt string");
            assert_eq!(
                pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json::JsValue::String(actual)),
                pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json::JsValue::String(expected.clone())),
                "case {index} at {now_millis} in {time_zone}"
            );
        }
    }
}

/// The pinned package directory both implementations resolve their docs paths from.
fn package_dir() -> String {
    let lock: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/source-lock.json"
    )))
    .expect("source lock is valid JSON");
    let worktree = lock["worktree"].as_str().expect("source lock worktree");
    format!("{worktree}/packages/coding-agent")
}

fn options_json(options: &BuildSystemPromptOptions) -> pi_js_compat::js_json::JsValue {
    let mut value = pi_js_compat::js_json::to_value(options).expect("typed prompt options");
    value
        .as_object_mut()
        .unwrap()
        .retain(|_, value| !matches!(value, pi_js_compat::js_json::JsValue::Null));
    value
}

fn cases() -> Vec<BuildSystemPromptOptions> {
    let base = |cwd: &str| BuildSystemPromptOptions {
        cwd: cwd.to_owned().into(),
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

    let snippets: HashMap<String, JsString> = [
        ("read", "Read file contents"),
        ("bash", "Execute bash commands"),
        ("edit", "Make surgical edits"),
        ("write", "Create or overwrite files"),
        ("grep", "Search file contents"),
        ("find", "Search for files by glob"),
        ("ls", "List directory contents"),
    ]
    .into_iter()
    .map(|(name, snippet)| (name.to_owned(), JsString::from(snippet)))
    .collect();
    cases.push(BuildSystemPromptOptions {
        tool_snippets: Some(snippets.clone()),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["read".to_owned(), "dynamic_tool".to_owned()]),
        tool_snippets: Some([("dynamic_tool".to_owned(), JsString::from("Run dynamic test behavior"))].into()),
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
            "Use dynamic_tool for summaries.".to_owned().into(),
            "  Use dynamic_tool for summaries.  ".to_owned().into(),
            JsString::from("   "),
        ]),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        context_files: Some(vec![ContextFile {
            path: "/repo/AGENTS.md".to_owned().into(),
            content: "Project instructions.".to_owned().into(),
        }]),
        rules_content: Some("Global rule.\n\nProject rule.".to_owned().into()),
        skills: Some(Vec::new()),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        rules_content: Some(String::new().into()),
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
        custom_prompt: Some("Custom prompt.".to_owned().into()),
        append_system_prompt: Some("Appended.".to_owned().into()),
        context_files: Some(vec![ContextFile {
            path: "/repo/AGENTS.md".to_owned().into(),
            content: "Project instructions.".to_owned().into(),
        }]),
        rules_content: Some("Global rule.".to_owned().into()),
        selected_tools: Some(vec!["read".to_owned()]),
        skills: Some(skills.clone()),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        custom_prompt: Some("Custom prompt without read.".to_owned().into()),
        selected_tools: Some(vec!["grep".to_owned()]),
        skills: Some(skills),
        ..base("C:\\repo\\project")
    });
    cases.push(BuildSystemPromptOptions {
        custom_prompt: Some(String::new().into()),
        ..base("/repo/project")
    });
    cases.push(BuildSystemPromptOptions {
        append_system_prompt: Some("Extra instructions.".to_owned().into()),
        ..base("/repo/project")
    });

    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["spawn_agent".into(), "pyrun_eval".into(), "wait_agent".into()]),
        ..base("/repo/pyrun")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["read".into(), "bash".into()]),
        tool_snippets: Some([("read".into(), JsString::default()), ("bash".into(), " ".into())].into()),
        append_system_prompt: Some(JsString::default()),
        ..base("/repo/empty")
    });
    cases.push(BuildSystemPromptOptions {
        custom_prompt: Some("Header\n".into()),
        rules_content: Some("Rule".into()),
        append_system_prompt: Some(JsString::default()),
        ..base("/repo/custom")
    });
    let raw = JsString::from_utf16(vec![0xd800, 0x21, 0xdc00]);
    cases.push(BuildSystemPromptOptions {
        custom_prompt: Some(raw.clone()),
        append_system_prompt: Some(raw.clone()),
        cwd: JsString::from("C:\\raw-") + &raw,
        context_files: Some(vec![ContextFile {
            path: raw.clone(),
            content: raw.clone(),
        }]),
        rules_content: Some(raw.clone()),
        ..base("/repo/raw")
    });
    cases.push(BuildSystemPromptOptions {
        selected_tools: Some(vec!["read".into()]),
        tool_snippets: Some([("read".into(), raw.clone())].into()),
        prompt_guidelines: Some(vec![raw.clone(), raw.clone(), "  ".into()]),
        context_files: Some(vec![ContextFile {
            path: raw.clone(),
            content: raw.clone(),
        }]),
        rules_content: Some(raw),
        ..base("/repo/raw-default")
    });
    cases
}

fn skill(name: &str, description: &str, file_path: &str, hidden: bool) -> Skill {
    let base_dir = std::path::Path::new(file_path)
        .parent()
        .map_or_else(String::new, |path| path.to_string_lossy().into_owned());
    Skill {
        name: name.to_owned().into(),
        description: description.to_owned().into(),
        file_path: file_path.to_owned().into(),
        base_dir: base_dir.clone().into(),
        source_info: create_synthetic_source_info(
            &JsString::from(file_path),
            &SyntheticSourceInfoOptions {
                source: "test".to_owned().into(),
                scope: None,
                origin: None,
                base_dir: Some(base_dir.into()),
            },
        ),
        disable_model_invocation: hidden,
    }
}
