//! Behavioral port of `packages/agent/test/harness/prompt-templates.test.ts` plus loader branch coverage.

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::os::unix::fs::symlink;
use std::path::Path;

use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::prompt_templates::{
    PromptTemplateDiagnostic, PromptTemplateDiagnosticCode, PromptTemplateDiagnosticType, load_prompt_templates,
    load_sourced_prompt_templates,
};
use pi_agent_core::harness::types::PromptTemplate;
use tempfile::TempDir;

fn temp_dir() -> TempDir {
    tempfile::tempdir().expect("test temp dir must be created")
}

fn env_at(root: &Path) -> NodeExecutionEnv {
    NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: JsString::from(root.to_str().expect("utf-8 test path").to_owned()),
        shell_path: None,
        shell_env: HashMap::new(),
    })
}

fn write(root: &Path, path: &str, content: &str) {
    let full_path = root.join(path);
    std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
    std::fs::write(full_path, content).unwrap();
}

fn template(name: &str, description: Option<&str>, content: &str) -> PromptTemplate {
    PromptTemplate {
        name: JsString::from(name.to_owned()),
        description: description.map(JsString::from),
        content: content.into(),
    }
}

#[tokio::test]
async fn loads_markdown_templates_non_recursively_from_one_or_more_dirs() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(root.path(), "a/one.md", "---\ndescription: One template\n---\nHello $1");
    write(root.path(), "a/nested/ignored.md", "Ignored");
    write(root.path(), "b/two.md", "First line description\nBody");

    let result = load_prompt_templates(&env, &[&JsString::from("a"), &JsString::from("b")]).await;

    assert_eq!(result.diagnostics, Vec::new());
    assert_eq!(
        result.prompt_templates,
        vec![
            template("one", Some("One template"), "Hello $1"),
            template("two", Some("First line description"), "First line description\nBody"),
        ]
    );
}

#[tokio::test]
async fn preserves_source_info_for_sourced_prompt_templates() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(
        root.path(),
        "prompts/example.md",
        "---\ndescription: Example\n---\nExample body",
    );

    let result =
        load_sourced_prompt_templates(&env, &[(JsString::from("prompts"), "project-source".to_owned())], None).await;

    assert_eq!(result.diagnostics, Vec::new());
    assert_eq!(result.prompt_templates.len(), 1);
    assert_eq!(
        result.prompt_templates[0].prompt_template,
        template("example", Some("Example"), "Example body")
    );
    assert_eq!(result.prompt_templates[0].source, "project-source");
}

#[tokio::test]
async fn attaches_source_info_to_diagnostics() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(root.path(), "broken.md", "---\ndescription: [unterminated\n---\nBody");

    let result =
        load_sourced_prompt_templates(&env, &[(JsString::from("broken.md"), "user-source".to_owned())], None).await;

    assert!(result.prompt_templates.is_empty());
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics[0].diagnostic,
        PromptTemplateDiagnostic {
            r#type: PromptTemplateDiagnosticType::Warning,
            code: PromptTemplateDiagnosticCode::ParseFailed,
            message: result.diagnostics[0].diagnostic.message.clone(),
            path: JsString::from(
                root.path()
                    .join("broken.md")
                    .to_str()
                    .expect("utf-8 test path")
                    .to_owned()
            ),
        }
    );
    assert_eq!(result.diagnostics[0].source, "user-source");
}

#[tokio::test]
async fn loads_explicit_markdown_files_and_symlinked_files() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(root.path(), "target.md", "---\ndescription: Target\n---\nTarget body");
    symlink(root.path().join("target.md"), root.path().join("link.md")).unwrap();

    let result = load_prompt_templates(&env, &[&JsString::from("target.md"), &JsString::from("link.md")]).await;

    assert_eq!(
        result.prompt_templates,
        vec![
            template("target", Some("Target"), "Target body"),
            template("link", Some("Target"), "Target body"),
        ]
    );
}

#[tokio::test]
async fn derives_description_from_the_first_non_empty_body_line() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(
        root.path(),
        "templates/long.md",
        &format!("\n  \n{}\nBody", "x".repeat(61)),
    );
    write(root.path(), "templates/short.md", "\nShort line\nBody");

    let result = load_prompt_templates(&env, &[&JsString::from("templates")]).await;

    assert_eq!(
        result.prompt_templates,
        vec![
            template(
                "long",
                Some(&format!("{}...", "x".repeat(60))),
                &format!("\n  \n{}\nBody", "x".repeat(61))
            ),
            template("short", Some("Short line"), "\nShort line\nBody"),
        ]
    );
}

#[tokio::test]
async fn skips_missing_paths_and_non_markdown_files() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(root.path(), "notes.txt", "plain text");

    let result = load_prompt_templates(&env, &[&JsString::from("missing"), &JsString::from("notes.txt")]).await;

    assert!(result.prompt_templates.is_empty());
    assert_eq!(result.diagnostics, Vec::new());
}

#[tokio::test]
async fn requires_a_lowercase_markdown_extension_and_strips_only_one_suffix() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(root.path(), "templates/Review.MD", "Review body");
    write(root.path(), "templates/two.md.md", "Body");

    let result = load_prompt_templates(&env, &[&JsString::from("templates")]).await;

    let names: Vec<JsString> = result
        .prompt_templates
        .iter()
        .map(|template| template.name.clone())
        .collect();
    assert_eq!(names, vec!["two.md"]);
}
