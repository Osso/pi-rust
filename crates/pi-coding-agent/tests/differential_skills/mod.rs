use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::{Path, PathBuf};

use pi_coding_agent::core::skills::{
    LoadSkillsFromDirOptions, LoadSkillsOptions, Skill, format_skills_for_prompt, load_skills, load_skills_from_dir,
};
use serde_json::json;
use tempfile::TempDir;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-skills-oracle.ts",
        "packages/coding-agent/src/core/skills.ts",
    );

    let temp = TempDir::new().expect("temp dir");
    let fixtures = write_fixtures(temp.path());
    compare_single_dirs(&mut oracle, &fixtures);
    compare_tree(&mut oracle, temp.path());
    compare_format(&mut oracle);
    compare_load(&mut oracle, temp.path(), &fixtures);
}

fn compare_single_dirs(oracle: &mut oracle::TsOracle, fixtures: &Path) {
    let dirs = [
        "valid-skill",
        "invalid-name-chars",
        "long-name",
        "missing-description",
        "multiline-description",
        "consecutive-hyphens",
        "disable-model-invocation",
        "nested",
        "root-skill-preferred",
        "no-frontmatter",
        "name-mismatch",
        "unknown-field",
    ];
    for (index, dir) in dirs.iter().enumerate() {
        let path = fixtures.join(dir).to_string_lossy().into_owned();
        let response = oracle.request(&json!({"id": index, "operation": "load_dir", "dir": path, "source": "test"}));
        assert_eq!(response["ok"], true, "oracle failed for {dir}: {response}");

        let actual = load_skills_from_dir(&LoadSkillsFromDirOptions {
            dir: path.into(),
            source: "test".to_owned().into(),
        })
        .unwrap();
        assert_eq!(json!(actual.skills), response["output"]["skills"], "{dir} skills");
        assert_eq!(
            json!(actual.diagnostics),
            response["output"]["diagnostics"],
            "{dir} diagnostics"
        );
    }
}

fn compare_tree(oracle: &mut oracle::TsOracle, root: &Path) {
    let path = root.to_string_lossy().into_owned();
    let response = oracle.request(&json!({"id": 100, "operation": "load_dir", "dir": path, "source": "test"}));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let actual = load_skills_from_dir(&LoadSkillsFromDirOptions {
        dir: path.into(),
        source: "test".to_owned().into(),
    })
    .unwrap();
    assert_eq!(json!(actual.skills), response["output"]["skills"]);
    assert_eq!(json!(actual.diagnostics), response["output"]["diagnostics"]);
}

fn compare_format(oracle: &mut oracle::TsOracle) {
    let skills = vec![
        skill(
            "visible-skill",
            "A visible skill with <special> & \"characters\".",
            "/path/visible/SKILL.md",
            "/path/visible",
            false,
        ),
        skill(
            "hidden-skill",
            "A hidden skill.",
            "/path/hidden/SKILL.md",
            "/path/hidden",
            true,
        ),
        skill(
            "second-skill",
            "Second skill.",
            "/path/second/SKILL.md",
            "/path/second",
            false,
        ),
    ];
    let response = oracle.request(&json!({"id": 200, "operation": "format", "skills": skills}));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    assert_eq!(json!(format_skills_for_prompt(&skills)), response["output"]["prompt"]);
    assert_eq!(format_skills_for_prompt(&[]), "");
    assert_eq!(format_skills_for_prompt(&skills[1..2]), "");
}

fn compare_load(oracle: &mut oracle::TsOracle, root: &Path, fixtures: &Path) {
    let agent_dir = root.join("agent");
    let cwd = root.join("project");
    let user_skill = agent_dir.join("skills").join("user-skill");
    let project_skill = cwd.join(".pi").join("skills").join("project-skill");
    write_skill(&user_skill, "user-skill", "A user skill.");
    write_skill(&project_skill, "project-skill", "A project skill.");

    let extra_dir = root.join("extra");
    write_skill(
        &extra_dir.join("valid-skill"),
        "valid-skill",
        "A valid skill for testing purposes.",
    );
    fs::write(
        root.join("extra").join("direct.md"),
        "---\ndescription: Direct template.\n---\n",
    )
    .unwrap();
    fs::write(root.join("extra").join("notes.txt"), "not markdown").unwrap();

    let cases: Vec<(Vec<String>, bool)> = vec![
        (
            vec![
                extra_dir.to_string_lossy().into_owned(),
                fixtures.join("valid-skill").to_string_lossy().into_owned(),
                root.join("missing").to_string_lossy().into_owned(),
                root.join("extra").join("notes.txt").to_string_lossy().into_owned(),
            ],
            false,
        ),
        (Vec::new(), true),
        (vec![root.join("extra").to_string_lossy().into_owned()], true),
        (
            vec![
                user_skill.to_string_lossy().into_owned(),
                project_skill.to_string_lossy().into_owned(),
            ],
            false,
        ),
    ];

    for (index, (skill_paths, include_defaults)) in cases.iter().enumerate() {
        let request = json!({
            "id": 300 + index,
            "operation": "load",
            "cwd": cwd.to_string_lossy(),
            "agentDir": agent_dir.to_string_lossy(),
            "skillPaths": skill_paths,
            "includeDefaults": include_defaults,
        });
        let response = oracle.request(&request);
        assert_eq!(response["ok"], true, "oracle failed: {response}");

        let actual = load_skills(&LoadSkillsOptions {
            cwd: cwd.to_string_lossy().into_owned().into(),
            agent_dir: agent_dir.to_string_lossy().into_owned().into(),
            skill_paths: skill_paths.iter().map(JsString::from).collect(),
            include_defaults: *include_defaults,
        })
        .unwrap();
        assert_eq!(json!(actual.skills), response["output"]["skills"], "case {index}");
        assert_eq!(
            json!(actual.diagnostics),
            response["output"]["diagnostics"],
            "case {index}"
        );
    }
}

fn skill(name: &str, description: &str, file_path: &str, base_dir: &str, hidden: bool) -> Skill {
    Skill {
        name: name.to_owned().into(),
        description: description.to_owned().into(),
        file_path: file_path.to_owned().into(),
        base_dir: base_dir.to_owned().into(),
        source_info: pi_coding_agent::core::source_info::create_synthetic_source_info(
            &JsString::from(file_path),
            &pi_coding_agent::core::source_info::SyntheticSourceInfoOptions {
                source: "test".to_owned().into(),
                scope: None,
                origin: None,
                base_dir: Some(base_dir.to_owned().into()),
            },
        ),
        disable_model_invocation: hidden,
    }
}

fn write_skill(dir: &Path, name: &str, description: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
    )
    .unwrap();
}

fn write_fixtures(root: &Path) -> PathBuf {
    let fixtures = root.join("fixtures");
    let entries: [(&str, &str); 13] = [
        (
            "valid-skill",
            "---\nname: valid-skill\ndescription: A valid skill for testing purposes.\n---\n\n# Valid Skill\n\nThis is a valid skill that follows the Agent Skills standard.\n",
        ),
        (
            "name-mismatch",
            "---\nname: different-name\ndescription: A skill with a name that doesn't match the directory.\n---\n",
        ),
        (
            "invalid-name-chars",
            "---\nname: Invalid_Name\ndescription: A skill with invalid characters in the name.\n---\n",
        ),
        (
            "long-name",
            "---\nname: this-is-a-very-long-skill-name-that-exceeds-the-sixty-four-character-limit-set-by-the-standard\ndescription: A skill with a name that exceeds 64 characters.\n---\n",
        ),
        ("missing-description", "---\nname: missing-description\n---\n"),
        (
            "unknown-field",
            "---\nname: unknown-field\ndescription: A skill with an unknown frontmatter field.\nauthor: someone\nversion: 1.0\n---\n",
        ),
        (
            "nested/child-skill",
            "---\nname: child-skill\ndescription: A nested skill in a subdirectory.\n---\n",
        ),
        (
            "root-skill-preferred",
            "---\ndescription: Root skill should win.\n---\n",
        ),
        (
            "root-skill-preferred/nested-child",
            "---\ndescription: Nested skill should be ignored.\n---\n",
        ),
        (
            "no-frontmatter",
            "# No Frontmatter\n\nThis skill has no YAML frontmatter at all.\n",
        ),
        (
            "multiline-description",
            "---\nname: multiline-description\ndescription: |\n  This is a multiline description.\n  It spans multiple lines.\n  And should be normalized.\n---\n",
        ),
        (
            "consecutive-hyphens",
            "---\nname: bad--name\ndescription: A skill with consecutive hyphens in the name.\n---\n",
        ),
        (
            "disable-model-invocation",
            "---\nname: disable-model-invocation\ndescription: A skill that cannot be invoked by the model.\ndisable-model-invocation: true\n---\n",
        ),
    ];
    for (dir, content) in entries {
        let skill_dir = fixtures.join(dir);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }
    fs::write(
        fixtures.join("root-skill-preferred").join("ignored.md"),
        "---\ndescription: Ignored root file.\n---\n",
    )
    .unwrap();
    write_skill(
        &fixtures.join("ignored-by-gitignore"),
        "ignored-by-gitignore",
        "Ignored skill.",
    );
    fs::write(fixtures.join(".gitignore"), "ignored-by-gitignore/\n").unwrap();
    write_skill(
        &fixtures.join("node_modules/pkg"),
        "node-modules-skill",
        "Skipped dependency.",
    );
    write_skill(&fixtures.join(".hidden"), "hidden-skill", "Skipped hidden directory.");
    fixtures
}
