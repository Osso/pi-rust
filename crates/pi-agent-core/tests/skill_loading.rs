//! Behavioral port of `packages/agent/test/harness/skills.test.ts` plus loader branch coverage.

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::skills::{
    SkillDiagnostic, SkillDiagnosticCode, SkillDiagnosticType, load_skills, load_sourced_skills,
};
use pi_agent_core::harness::types::Skill;
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

fn skill(name: &str, description: &str, content: &str, file_path: PathBuf, disable_model_invocation: bool) -> Skill {
    Skill {
        name: JsString::from(name.to_owned()),
        description: description.to_owned(),
        content: content.to_owned(),
        file_path: JsString::from(file_path.to_str().expect("utf-8 test path").to_owned()),
        disable_model_invocation: Some(disable_model_invocation),
    }
}

fn diagnostic(code: SkillDiagnosticCode, message: &str, path: PathBuf) -> SkillDiagnostic {
    SkillDiagnostic {
        r#type: SkillDiagnosticType::Warning,
        code,
        message: JsString::from(message.to_owned()),
        path: JsString::from(path.to_str().expect("utf-8 test path").to_owned()),
    }
}

#[tokio::test]
async fn loads_skill_files_through_the_execution_environment() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(
        root.path(),
        ".agents/skills/example/SKILL.md",
        "---\nname: example\ndescription: Example skill\ndisable-model-invocation: true\n---\nUse this skill.\n",
    );

    let result = load_skills(&env, &[&JsString::from(".agents/skills")])
        .await
        .expect("load skills");

    assert_eq!(result.diagnostics, Vec::new());
    assert_eq!(
        result.skills,
        vec![skill(
            "example",
            "Example skill",
            "Use this skill.",
            root.path().join(".agents/skills/example/SKILL.md"),
            true
        )]
    );
}

#[tokio::test]
async fn loads_skills_through_symlinked_directories() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(
        root.path(),
        "actual/example/SKILL.md",
        "---\nname: example\ndescription: Example skill\n---\nUse this skill.",
    );
    symlink(root.path().join("actual"), root.path().join("skills-link")).unwrap();

    let result = load_skills(&env, &[&JsString::from("skills-link")])
        .await
        .expect("load skills");

    let names: Vec<JsString> = result.skills.iter().map(|skill| skill.name.clone()).collect();
    assert_eq!(names, vec!["example"]);
    assert_eq!(
        result.skills[0].file_path,
        root.path()
            .join("skills-link/example/SKILL.md")
            .to_str()
            .expect("utf-8 test path")
    );
    assert_eq!(result.skills[0].disable_model_invocation, Some(false));
}

#[tokio::test]
async fn preserves_source_info_for_sourced_skills() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(
        root.path(),
        "user/example/SKILL.md",
        "---\nname: example\ndescription: Example skill\n---\nUse this skill.",
    );

    let result = load_sourced_skills(&env, &[(JsString::from("user"), "user-source".to_owned())], None)
        .await
        .expect("load skills");

    assert_eq!(result.diagnostics, Vec::new());
    assert_eq!(result.skills.len(), 1);
    assert_eq!(
        result.skills[0].skill,
        skill(
            "example",
            "Example skill",
            "Use this skill.",
            root.path().join("user/example/SKILL.md"),
            false
        )
    );
    assert_eq!(result.skills[0].source, "user-source");
}

#[tokio::test]
async fn attaches_source_info_to_diagnostics() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(
        root.path(),
        "user/broken/SKILL.md",
        "---\nname: broken\n---\nMissing description.",
    );

    let result = load_sourced_skills(&env, &[(JsString::from("user"), "user-source".to_owned())], None)
        .await
        .expect("load skills");

    assert!(result.skills.is_empty());
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics[0].diagnostic,
        diagnostic(
            SkillDiagnosticCode::InvalidMetadata,
            "description is required",
            root.path().join("user/broken/SKILL.md")
        )
    );
    assert_eq!(result.diagnostics[0].source, "user-source");
}

#[tokio::test]
async fn loads_direct_markdown_children_only_from_the_root_directory() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(
        root.path(),
        "skills/root.md",
        "---\ndescription: Root skill\n---\nRoot content",
    );
    write(
        root.path(),
        "skills/nested/ignored.md",
        "---\ndescription: Ignored\n---\nIgnored content",
    );

    let result = load_skills(&env, &[&JsString::from("skills")])
        .await
        .expect("load skills");

    let names: Vec<JsString> = result.skills.iter().map(|skill| skill.name.clone()).collect();
    assert_eq!(names, vec!["skills"]);
    assert_eq!(result.skills[0].content, "Root content");
}

#[tokio::test]
async fn applies_gitignore_ignore_and_fdignore_rules_per_directory() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(
        root.path(),
        "skills/.gitignore",
        "ignored-by-git/\n*.tmp.md\n!keep.md\n",
    );
    write(root.path(), "skills/.ignore", "ignored-by-ignore/\n");
    write(root.path(), "skills/.fdignore", "ignored-by-fd/\n");
    for directory in ["ignored-by-git", "ignored-by-ignore", "ignored-by-fd", "kept"] {
        write(
            root.path(),
            &format!("skills/{directory}/SKILL.md"),
            &format!("---\nname: {directory}\ndescription: {directory} skill\n---\nbody"),
        );
    }
    write(
        root.path(),
        "skills/dropped.tmp.md",
        "---\ndescription: Dropped\n---\nbody",
    );
    write(
        root.path(),
        "skills/keep.md",
        "---\ndescription: Kept root file\n---\nbody",
    );
    write(root.path(), "skills/nested/.gitignore", "nested-only/\n");
    write(
        root.path(),
        "skills/nested/nested-only/SKILL.md",
        "---\nname: nested-only\ndescription: Nested\n---\nbody",
    );
    write(
        root.path(),
        "skills/nested/visible/SKILL.md",
        "---\nname: visible\ndescription: Visible\n---\nbody",
    );

    let result = load_skills(&env, &[&JsString::from("skills")])
        .await
        .expect("load skills");
    let mut names: Vec<JsString> = result.skills.iter().map(|skill| skill.name.clone()).collect();
    names.sort_unstable();

    assert_eq!(names, vec!["kept", "skills", "visible"]);
    assert_eq!(result.diagnostics, Vec::new());
    let root_file = result
        .skills
        .iter()
        .find(|skill| skill.name == "skills")
        .expect("kept root markdown file must load");
    assert_eq!(root_file.description, "Kept root file");
}

#[tokio::test]
async fn reports_metadata_validation_warnings_and_skips_invalid_skills() {
    let root = temp_dir();
    let env = env_at(root.path());
    let long_name = "a".repeat(65);
    for directory in ["-edge", "a--b", long_name.as_str(), "bad-name", "Invalid"] {
        write(
            root.path(),
            &format!("skills/{directory}/SKILL.md"),
            "---\ndescription: Valid description\n---\nbody",
        );
    }
    write(
        root.path(),
        "skills/bad-name/SKILL.md",
        "---\nname: Other\ndescription: Bad name\n---\nbody",
    );

    let result = load_skills(&env, &[&JsString::from("skills")])
        .await
        .expect("load skills");

    assert_eq!(result.skills.len(), 5);
    let mut names: Vec<JsString> = result.skills.iter().map(|skill| skill.name.clone()).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["-edge", "Invalid", "Other", "a--b", &long_name]);

    let messages: Vec<JsString> = result
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.clone())
        .collect();
    assert_eq!(
        messages,
        vec![
            "name must not start or end with a hyphen",
            "name must not contain consecutive hyphens",
            "name exceeds 64 characters (65)",
            "name \"Other\" does not match parent directory \"bad-name\"",
            "name contains invalid characters (must be lowercase a-z, 0-9, hyphens only)",
            "name contains invalid characters (must be lowercase a-z, 0-9, hyphens only)",
        ]
    );
    assert!(
        result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code == SkillDiagnosticCode::InvalidMetadata)
    );
}

#[tokio::test]
async fn warns_about_overlong_descriptions_but_keeps_the_skill() {
    let root = temp_dir();
    let env = env_at(root.path());
    let long_description = "d".repeat(1025);
    write(
        root.path(),
        "skills/verbose/SKILL.md",
        &format!("---\ndescription: {long_description}\n---\nbody"),
    );

    let result = load_skills(&env, &[&JsString::from("skills")])
        .await
        .expect("load skills");

    assert_eq!(result.skills.len(), 1);
    assert_eq!(
        result.diagnostics,
        vec![diagnostic(
            SkillDiagnosticCode::InvalidMetadata,
            "description exceeds 1024 characters (1025)",
            root.path().join("skills/verbose/SKILL.md")
        )]
    );
}

#[tokio::test]
async fn skips_missing_inputs_and_non_directory_roots() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(root.path(), "file.md", "---\ndescription: File\n---\nbody");

    let result = load_skills(&env, &[&JsString::from("missing"), &JsString::from("file.md")])
        .await
        .expect("load skills");

    assert!(result.skills.is_empty());
    assert_eq!(result.diagnostics, Vec::new());
}

#[tokio::test]
async fn short_circuits_a_directory_containing_skill_markdown() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(
        root.path(),
        "skills/SKILL.md",
        "---\nname: skills\ndescription: Root skill\n---\nroot body",
    );
    write(
        root.path(),
        "skills/nested/SKILL.md",
        "---\nname: nested\ndescription: Nested\n---\nnested body",
    );

    let result = load_skills(&env, &[&JsString::from("skills")])
        .await
        .expect("load skills");

    let names: Vec<JsString> = result.skills.iter().map(|skill| skill.name.clone()).collect();
    assert_eq!(names, vec!["skills"]);
}

#[tokio::test]
async fn reports_parse_failures_and_read_failures() {
    let root = temp_dir();
    let env = env_at(root.path());
    write(
        root.path(),
        "skills/broken/SKILL.md",
        "---\ndescription: [unterminated\n---\nbody",
    );

    let result = load_skills(&env, &[&JsString::from("skills")])
        .await
        .expect("load skills");

    assert!(result.skills.is_empty());
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, SkillDiagnosticCode::ParseFailed);
    assert_eq!(
        result.diagnostics[0].path,
        root.path()
            .join("skills/broken/SKILL.md")
            .to_str()
            .expect("utf-8 test path")
    );
}

#[tokio::test]
async fn skips_dot_directories_and_node_modules() {
    let root = temp_dir();
    let env = env_at(root.path());
    for directory in [".hidden", "node_modules", "visible"] {
        write(
            root.path(),
            &format!("{directory}/SKILL.md"),
            &format!("---\nname: {directory}\ndescription: {directory} skill\n---\nbody"),
        );
    }

    let result = load_skills(&env, &[&JsString::from(".")]).await.expect("load skills");

    let names: Vec<JsString> = result.skills.iter().map(|skill| skill.name.clone()).collect();
    assert_eq!(names, vec!["visible"]);
}

#[tokio::test]
async fn resolves_yaml_binary_tags_to_their_scalar_text() {
    // Discrepancy: the JavaScript `yaml` package decodes `!!binary` to bytes, so the string-typed
    // description is absent and the skill is skipped. `serde_yaml_ng` yields the scalar text, so the
    // port loads a skill whose description is the encoded payload.
    let root = temp_dir();
    let env = env_at(root.path());
    write(
        root.path(),
        "skills/tagged/SKILL.md",
        "---\ndescription: !!binary aGk=\n---\nbody",
    );

    let result = load_skills(&env, &[&JsString::from("skills")])
        .await
        .expect("load skills");

    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].description, "aGk=");
    assert_eq!(result.diagnostics, Vec::new());
}
