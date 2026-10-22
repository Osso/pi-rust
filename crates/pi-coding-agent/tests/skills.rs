use pi_js_compat::js_json::JsString;
use std::fs;

use pi_coding_agent::core::skills::{LoadSkillsFromDirOptions, format_skills_for_prompt, load_skills_from_dir};
use tempfile::TempDir;

#[test]
fn loads_a_valid_skill_with_synthetic_source_info() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("valid-skill");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        "---\nname: valid-skill\ndescription: A valid skill for testing purposes.\n---\n",
    )
    .unwrap();

    let result = load_skills_from_dir(&LoadSkillsFromDirOptions {
        dir: dir.to_string_lossy().into_owned().into(),
        source: "test".to_owned().into(),
    });

    assert_eq!(result.diagnostics, Vec::new());
    assert_eq!(result.skills.len(), 1);
    let skill = &result.skills[0];
    assert_eq!(skill.name, "valid-skill");
    assert_eq!(skill.description, "A valid skill for testing purposes.");
    assert_eq!(skill.source_info.source, "test");
    assert_eq!(
        skill.source_info.scope,
        pi_coding_agent::core::source_info::SourceScope::Temporary
    );
    assert_eq!(skill.base_dir, dir.to_string_lossy());
    assert!(!skill.disable_model_invocation);
}

#[test]
fn warns_but_keeps_skills_with_invalid_names() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("invalid-name-chars");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        "---\nname: Invalid_Name\ndescription: Invalid characters.\n---\n",
    )
    .unwrap();

    let result = load_skills_from_dir(&LoadSkillsFromDirOptions {
        dir: dir.to_string_lossy().into_owned().into(),
        source: "test".to_owned().into(),
    });

    assert_eq!(result.skills.len(), 1);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("invalid characters"))
    );
}

#[test]
fn skips_skills_without_a_description() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("missing-description");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), "---\nname: missing-description\n---\n").unwrap();

    let result = load_skills_from_dir(&LoadSkillsFromDirOptions {
        dir: dir.to_string_lossy().into_owned().into(),
        source: "test".to_owned().into(),
    });

    assert!(result.skills.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("description is required"))
    );
}

#[test]
fn invalid_yaml_reports_one_warning_at_the_file() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("invalid-yaml");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("SKILL.md");
    fs::write(&path, "---\nname: invalid-yaml\ndescription: [unclosed bracket\n---\n").unwrap();

    let result = load_skills_from_dir(&LoadSkillsFromDirOptions {
        dir: dir.to_string_lossy().into_owned().into(),
        source: "test".to_owned().into(),
    });

    assert!(result.skills.is_empty());
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(
        result.diagnostics[0].path.as_deref(),
        Some(path.to_string_lossy().as_ref())
    );
    assert!(result.diagnostics[0].message.contains("line"));
}

#[test]
fn prefers_the_directory_root_skill_and_skips_dot_directories() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root-skill-preferred");
    fs::create_dir_all(root.join("nested-child")).unwrap();
    fs::write(root.join("SKILL.md"), "---\ndescription: Root skill should win.\n---\n").unwrap();
    fs::write(
        root.join("nested-child").join("SKILL.md"),
        "---\ndescription: Nested skill should be ignored.\n---\n",
    )
    .unwrap();
    fs::create_dir_all(temp.path().join(".hidden")).unwrap();
    fs::write(
        temp.path().join(".hidden").join("SKILL.md"),
        "---\ndescription: Hidden skill.\n---\n",
    )
    .unwrap();

    let result = load_skills_from_dir(&LoadSkillsFromDirOptions {
        dir: root.to_string_lossy().into_owned().into(),
        source: "test".to_owned().into(),
    });

    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].description, "Root skill should win.");

    let tree = load_skills_from_dir(&LoadSkillsFromDirOptions {
        dir: temp.path().to_string_lossy().into_owned().into(),
        source: "test".to_owned().into(),
    });
    assert!(
        tree.skills
            .iter()
            .all(|skill| skill.name != ".hidden" && !skill.file_path.contains(".hidden"))
    );
}

#[test]
fn formats_visible_skills_as_xml() {
    let skill = pi_coding_agent::core::skills::Skill {
        name: "test-skill".to_owned().into(),
        description: "A skill with <special> & \"characters\".".to_owned().into(),
        file_path: "/path/to/skill/SKILL.md".to_owned().into(),
        base_dir: "/path/to/skill".to_owned().into(),
        source_info: pi_coding_agent::core::source_info::create_synthetic_source_info(
            &JsString::from("/path/to/skill/SKILL.md"),
            &pi_coding_agent::core::source_info::SyntheticSourceInfoOptions {
                source: "test".to_owned().into(),
                scope: None,
                origin: None,
                base_dir: None,
            },
        ),
        disable_model_invocation: false,
    };

    let prompt = format_skills_for_prompt(std::slice::from_ref(&skill));
    assert!(prompt.contains("<available_skills>"));
    assert!(prompt.contains("<name>test-skill</name>"));
    assert!(prompt.contains("&lt;special&gt;"));
    assert!(prompt.contains("&quot;characters&quot;"));
    assert!(prompt.contains("<location>/path/to/skill/SKILL.md</location>"));
    assert!(prompt.starts_with("\n\nThe following skills provide specialized instructions"));
}
