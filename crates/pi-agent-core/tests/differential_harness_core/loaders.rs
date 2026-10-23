use pi_js_compat::js_json::JsString;
use std::os::unix::fs::symlink;
use std::path::Path;

use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::prompt_templates::{
    PromptTemplateDiagnostic, SourcedPromptTemplateLoadResult, load_prompt_templates, load_sourced_prompt_templates,
};
use pi_agent_core::harness::skills::{SkillDiagnostic, SourcedSkillLoadResult, load_skills, load_sourced_skills};
use pi_agent_core::harness::types::{PromptTemplate, Skill};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::support::{TsOracle, expect_ok, oracle_request};

struct Fixture {
    id: u64,
    operation: &'static str,
    paths: Vec<String>,
    files: Vec<(String, String)>,
    dirs: Vec<String>,
    symlinks: Vec<(String, String)>,
    sources: Vec<(String, String)>,
}

fn fixture(id: u64, operation: &'static str) -> Fixture {
    Fixture {
        id,
        operation,
        paths: Vec::new(),
        files: Vec::new(),
        dirs: Vec::new(),
        symlinks: Vec::new(),
        sources: Vec::new(),
    }
}

impl Fixture {
    fn paths(mut self, paths: &[&str]) -> Self {
        self.paths = paths.iter().map(|path| (*path).to_owned()).collect();
        self
    }

    fn file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.files.push((path.into(), content.into()));
        self
    }

    fn symlink(mut self, target: &str, path: &str) -> Self {
        self.symlinks.push((target.to_owned(), path.to_owned()));
        self
    }

    fn source(mut self, path: &str, source: &str) -> Self {
        self.sources.push((path.to_owned(), source.to_owned()));
        self
    }
}

fn long_description() -> String {
    "d".repeat(1025)
}

fn long_name() -> String {
    "a".repeat(65)
}

fn skills_fixtures() -> Vec<Fixture> {
    vec![
        fixture(14_400, "skills_load").paths(&[".agents/skills"]).file(
            ".agents/skills/example/SKILL.md",
            "---\nname: example\ndescription: Example skill\ndisable-model-invocation: true\n---\nUse this skill.\n",
        ),
        fixture(14_401, "skills_load")
            .paths(&["skills"])
            .file(
                "skills/plain/SKILL.md",
                "---\nname: plain\ndescription: Plain skill\n---\nbody",
            )
            .file(
                "skills/crlf/SKILL.md",
                "---\r\nname: crlf\r\ndescription: CRLF skill\r\n---\r\nline one\r\nline two",
            )
            .file("skills/root.md", "---\ndescription: Root skill\n---\nRoot content")
            .file("skills/nested/ignored.md", "---\ndescription: Ignored\n---\nIgnored"),
        fixture(14_402, "skills_load")
            .paths(&["skills-link"])
            .file(
                "actual/example/SKILL.md",
                "---\nname: example\ndescription: Example skill\n---\nUse this skill.",
            )
            .symlink("actual", "skills-link"),
        fixture(14_403, "skills_load")
            .paths(&["."])
            .file("skills/.gitignore", "ignored-by-git/\n*.tmp.md\n!keep.md\n")
            .file("skills/.ignore", "ignored-by-ignore/\n")
            .file("skills/.fdignore", "ignored-by-fd/\n")
            .file("skills/ignored-by-git/SKILL.md", "---\ndescription: A\n---\nbody")
            .file("skills/ignored-by-ignore/SKILL.md", "---\ndescription: B\n---\nbody")
            .file("skills/ignored-by-fd/SKILL.md", "---\ndescription: C\n---\nbody")
            .file(
                "skills/kept/SKILL.md",
                "---\nname: kept\ndescription: Kept skill\n---\nbody",
            )
            .file("skills/dropped.tmp.md", "---\ndescription: Dropped\n---\nbody")
            .file("skills/keep.md", "---\ndescription: Kept root file\n---\nbody")
            .file("skills/nested/.gitignore", "nested-only/\n")
            .file(
                "skills/nested/nested-only/SKILL.md",
                "---\nname: nested-only\ndescription: Nested\n---\nbody",
            )
            .file(
                "skills/nested/visible/SKILL.md",
                "---\nname: visible\ndescription: Visible\n---\nbody",
            ),
        fixture(14_404, "skills_load")
            .paths(&["skills"])
            .file(
                "skills/-edge/SKILL.md",
                "---\ndescription: Valid description\n---\nbody",
            )
            .file("skills/a--b/SKILL.md", "---\ndescription: Valid description\n---\nbody")
            .file(
                format!("skills/{}/SKILL.md", long_name()),
                "---\ndescription: Valid description\n---\nbody",
            )
            .file(
                "skills/bad-name/SKILL.md",
                "---\nname: Other\ndescription: Bad name\n---\nbody",
            )
            .file("skills/Invalid/SKILL.md", "---\ndescription: Invalid name\n---\nbody")
            .file(
                "skills/verbose/SKILL.md",
                format!("---\ndescription: {}\n---\nbody", long_description()),
            )
            .file(
                "skills/empty-description/SKILL.md",
                "---\nname: empty-description\n---\nbody",
            )
            .file(
                "skills/blank-description/SKILL.md",
                "---\nname: blank-description\ndescription: \"   \"\n---\nbody",
            )
            .file(
                "skills/numeric-description/SKILL.md",
                "---\nname: numeric-description\ndescription: 42\n---\nbody",
            ),
        fixture(14_405, "skills_load")
            .paths(&["skills"])
            .file("skills/broken/SKILL.md", "---\ndescription: [unterminated\n---\nbody")
            .file("skills/no-frontmatter/SKILL.md", "Plain body without frontmatter")
            .file("skills/empty/SKILL.md", ""),
        fixture(14_406, "skills_load")
            .paths(&["missing", "file.md", "first", "second"])
            .file("file.md", "---\ndescription: File\n---\nbody")
            .file(
                "first/one/SKILL.md",
                "---\nname: one\ndescription: First one\n---\nbody",
            )
            .file(
                "second/two/SKILL.md",
                "---\nname: two\ndescription: Second two\n---\nbody",
            )
            .file("second/.hidden/SKILL.md", "---\ndescription: Hidden\n---\nbody")
            .file("second/node_modules/SKILL.md", "---\ndescription: Modules\n---\nbody"),
        fixture(14_407, "skills_load")
            .paths(&["skills"])
            .file(
                "skills/SKILL.md",
                "---\nname: skills\ndescription: Root skill\n---\nroot body",
            )
            .file(
                "skills/nested/SKILL.md",
                "---\nname: nested\ndescription: Nested\n---\nbody",
            ),
        fixture(14_408, "skills_load")
            .paths(&["skills"])
            .file(
                "skills/deep/a/b/c/SKILL.md",
                "---\nname: c\ndescription: Deep\n---\ndeep body",
            )
            .file("skills/linked.md", "---\ndescription: Linked file\n---\nlinked body")
            .file("skills/target.md", "---\ndescription: Target file\n---\ntarget body")
            .symlink("skills/target.md", "skills/linked-symlink.md"),
        fixture(14_409, "skills_load_sourced")
            .source("user", "user-source")
            .source("project", "project-source")
            .file(
                "user/example/SKILL.md",
                "---\nname: example\ndescription: Example skill\n---\nbody",
            )
            .file("user/broken/SKILL.md", "---\nname: broken\n---\nMissing description.")
            .file(
                "project/tool/SKILL.md",
                "---\nname: tool\ndescription: Tool skill\ndisable-model-invocation: true\n---\nbody",
            ),
        fixture(14_410, "prompt_templates_load")
            .paths(&["a", "b"])
            .file("a/one.md", "---\ndescription: One template\n---\nHello $1")
            .file("a/nested/ignored.md", "Ignored")
            .file("b/two.md", "First line description\nBody"),
        fixture(14_411, "prompt_templates_load")
            .paths(&["target.md", "link.md", "missing.md", "notes.txt"])
            .file("target.md", "---\ndescription: Target\n---\nTarget body")
            .file("notes.txt", "plain text")
            .symlink("target.md", "link.md"),
        fixture(14_412, "prompt_templates_load")
            .paths(&["templates"])
            .file("templates/long.md", format!("\n  \n{}\nBody", "x".repeat(61)))
            .file("templates/short.md", "\nShort line\nBody")
            .file(
                "templates/empty-description.md",
                "---\ndescription: \"\"\n---\nFallback line\nBody",
            )
            .file(
                "templates/numeric-description.md",
                "---\ndescription: 42\n---\nNumeric fallback",
            )
            .file("templates/blank.md", "")
            .file(
                "templates/crlf.md",
                "---\r\ndescription: CRLF template\r\n---\r\nLine one\r\nLine two",
            )
            .file("templates/Review.MD", "Uppercase extension")
            .file("templates/two.md.md", "Body"),
        fixture(14_413, "prompt_templates_load")
            .paths(&["broken.md", "plain.md"])
            .file("broken.md", "---\ndescription: [unterminated\n---\nBody")
            .file("plain.md", "Plain body without frontmatter"),
        fixture(14_414, "prompt_templates_load_sourced")
            .source("prompts", "project-source")
            .source("user-prompts", "user-source")
            .file("prompts/example.md", "---\ndescription: Example\n---\nExample body")
            .file("user-prompts/broken.md", "---\ndescription: [unterminated\n---\nBody")
            .file("user-prompts/plain.md", "Plain first line\nBody"),
    ]
}

fn env_at(root: &Path) -> NodeExecutionEnv {
    NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: JsString::from(root.to_str().expect("utf-8 test path").to_owned()),
        shell_path: None,
        shell_env: Default::default(),
    })
}

fn create_fixture(root: &Path, fixture: &Fixture) {
    for (path, content) in &fixture.files {
        let full_path = root.join(path);
        std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
        std::fs::write(full_path, content).unwrap();
    }
    for directory in &fixture.dirs {
        std::fs::create_dir_all(root.join(directory)).unwrap();
    }
    for (target, path) in &fixture.symlinks {
        let full_path = root.join(path);
        std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
        symlink(root.join(target), full_path).unwrap();
    }
}

fn fixture_json(root: &Path, fixture: &Fixture) -> Value {
    let files: serde_json::Map<String, Value> = fixture
        .files
        .iter()
        .map(|(path, content)| (path.clone(), json!(content)))
        .collect();
    let sources: Vec<Value> = fixture
        .sources
        .iter()
        .map(|(path, source)| json!({"path": path, "source": source}))
        .collect();
    let symlinks: Vec<Value> = fixture
        .symlinks
        .iter()
        .map(|(target, path)| json!({"target": target, "path": path}))
        .collect();
    json!({
        "root": root.to_str().expect("utf-8 test path"),
        "files": files,
        "dirs": fixture.dirs,
        "symlinks": symlinks,
        "paths": fixture.paths,
        "inputs": sources,
    })
}

fn normalize(value: &Value, root: &str) -> Value {
    match value {
        Value::String(text) => Value::String(text.replace(root, "<root>")),
        Value::Array(items) => Value::Array(items.iter().map(|item| normalize(item, root)).collect()),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, item)| (key.clone(), normalize(item, root)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// YAML parser error text differs from the JavaScript `yaml` package; compare occurrence only.
fn mask_parse_errors(value: &mut Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(mask_parse_errors),
        Value::Object(object) => {
            if object.get("code").and_then(Value::as_str) == Some("parse_failed") && object.contains_key("message") {
                object.insert("message".to_owned(), Value::String("<parse-error>".to_owned()));
            }
            object.values_mut().for_each(mask_parse_errors);
        }
        _ => {}
    }
}

fn skill_json(skill: &Skill) -> Value {
    json!({
        "name": skill.name,
        "description": skill.description,
        "content": skill.content,
        "filePath": skill.file_path,
        "disableModelInvocation": skill.disable_model_invocation.unwrap_or(false),
    })
}

fn skill_diagnostic_json(diagnostic: &SkillDiagnostic) -> Value {
    json!({
        "type": diagnostic.r#type.as_str(),
        "code": diagnostic.code.as_str(),
        "message": diagnostic.message,
        "path": diagnostic.path,
    })
}

fn template_json(template: &PromptTemplate) -> Value {
    json!({
        "name": template.name,
        "description": template.description,
        "content": template.content,
    })
}

fn template_diagnostic_json(diagnostic: &PromptTemplateDiagnostic) -> Value {
    json!({
        "type": diagnostic.r#type.as_str(),
        "code": diagnostic.code.as_str(),
        "message": diagnostic.message,
        "path": diagnostic.path,
    })
}

async fn rust_output(fixture: &Fixture, root: &Path) -> Value {
    let env = env_at(root);
    match fixture.operation {
        "skills_load" => {
            let native_paths: Vec<JsString> = fixture.paths.iter().map(JsString::from).collect();
            let paths: Vec<&JsString> = native_paths.iter().collect();
            let result = load_skills(&env, &paths).await.expect("load fixture skills");
            json!({
                "skills": result.skills.iter().map(skill_json).collect::<Vec<_>>(),
                "diagnostics": result.diagnostics.iter().map(skill_diagnostic_json).collect::<Vec<_>>(),
            })
        }
        "skills_load_sourced" => {
            let inputs: Vec<(JsString, String)> = fixture
                .sources
                .iter()
                .map(|(path, source)| (JsString::from(path), source.clone()))
                .collect();
            let result: SourcedSkillLoadResult<String> = load_sourced_skills(&env, &inputs, None)
                .await
                .expect("load sourced fixture skills");
            json!({
                "skills": result.skills.iter().map(|sourced| json!({
                    "skill": skill_json(&sourced.skill),
                    "source": sourced.source,
                })).collect::<Vec<_>>(),
                "diagnostics": result.diagnostics.iter().map(|sourced| {
                    let mut value = skill_diagnostic_json(&sourced.diagnostic);
                    value["source"] = json!(sourced.source);
                    value
                }).collect::<Vec<_>>(),
            })
        }
        "prompt_templates_load" => {
            let native_paths: Vec<JsString> = fixture.paths.iter().map(JsString::from).collect();
            let paths: Vec<&JsString> = native_paths.iter().collect();
            let result = load_prompt_templates(&env, &paths).await.unwrap();
            json!({
                "promptTemplates": result.prompt_templates.iter().map(template_json).collect::<Vec<_>>(),
                "diagnostics": result.diagnostics.iter().map(template_diagnostic_json).collect::<Vec<_>>(),
            })
        }
        "prompt_templates_load_sourced" => {
            let inputs: Vec<(JsString, String)> = fixture
                .sources
                .iter()
                .map(|(path, source)| (JsString::from(path), source.clone()))
                .collect();
            let result: SourcedPromptTemplateLoadResult<String> =
                load_sourced_prompt_templates(&env, &inputs, None).await.unwrap();
            json!({
                "promptTemplates": result.prompt_templates.iter().map(|sourced| json!({
                    "promptTemplate": template_json(&sourced.prompt_template),
                    "source": sourced.source,
                })).collect::<Vec<_>>(),
                "diagnostics": result.diagnostics.iter().map(|sourced| {
                    let mut value = template_diagnostic_json(&sourced.diagnostic);
                    value["source"] = json!(sourced.source);
                    value
                }).collect::<Vec<_>>(),
            })
        }
        other => panic!("unsupported fixture operation {other}"),
    }
}

pub(crate) async fn compare_all_fixtures(oracle: &mut TsOracle) {
    for fixture in skills_fixtures() {
        let rust_root = TempDir::new().unwrap();
        let ts_root = TempDir::new().unwrap();
        create_fixture(rust_root.path(), &fixture);

        let request = oracle_request(
            fixture.id,
            fixture.operation,
            json!({"fixture": fixture_json(ts_root.path(), &fixture)}),
        );
        let response = oracle.request(&request);
        let expected = expect_ok(&response).clone();

        let actual = normalize(
            &rust_output(&fixture, rust_root.path()).await,
            rust_root.path().to_str().expect("utf-8 test path"),
        );
        let mut actual = actual;
        let mut expected = expected;
        mask_parse_errors(&mut actual);
        mask_parse_errors(&mut expected);
        assert_eq!(actual, expected, "fixture {} ({})", fixture.id, fixture.operation);
    }
}
