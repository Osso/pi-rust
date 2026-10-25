use crate::shared_oracle as oracle;

use pi_coding_agent::core::prompt_templates::substitute_args;
use pi_coding_agent::core::source_info::{SyntheticSourceInfoOptions, create_synthetic_source_info};
use pi_coding_agent::utils::frontmatter::parse_frontmatter;
use pi_js_compat::js_json::{JsString, JsValue};

#[test]
fn frontmatter_body_keeps_raw_units_after_native_extraction() {
    let raw = JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]);
    let content = JsString::from("---\r\ndescription: example\r\n---\r\n ") + &raw + " \r\n";
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-resources-oracle.ts",
        "packages/coding-agent/src/utils/frontmatter.ts",
    );
    let source =
        oracle.request(&pi_js_compat::js_json!({"id":"raw-body", "operation":"frontmatter", "content":content}));
    assert_eq!(source["ok"].as_bool(), Some(true));
    let parsed = parse_frontmatter(&content).unwrap();
    assert_eq!(parsed.body, raw);
    assert_eq!(
        pi_js_compat::js_json::to_value(&parsed.frontmatter).unwrap(),
        source["output"]["frontmatter"]
    );
    assert_eq!(parsed.body, *source["output"]["body"].as_string().unwrap());
    // Native YAML is pending a user decision: fail explicitly, never discard the scalar.
    assert!(parse_frontmatter("---\ndescription: \"\\uD800\"\n---\nbody").is_err());
}

#[test]
fn substitutions_keep_raw_template_defaults_and_arguments() {
    let high = JsString::from_utf16(vec![0xd800]);
    let low = JsString::from_utf16(vec![0xdc00]);
    let content = high.clone() + " ${2:-" + &low + "} $1 $@ ${@:2}";
    let args = vec![high, "".into(), low];
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-resources-oracle.ts",
        "packages/coding-agent/src/core/prompt-templates.ts",
    );
    let source = oracle.request(
        &pi_js_compat::js_json!({"id":"raw-template", "operation":"substitute", "content":content, "args":args}),
    );
    assert_eq!(source["ok"].as_bool(), Some(true));
    assert_eq!(JsValue::String(substitute_args(&content, &args)), source["output"]);
}

#[test]
fn source_metadata_retains_native_path_identity() {
    let path = JsString::from("/skills/") + JsString::from_utf16(vec![0xd800]) + "/SKILL.md";
    let base = JsString::from("/skills/") + JsString::from_utf16(vec![0xd800]);
    let source = create_synthetic_source_info(
        &path,
        &SyntheticSourceInfoOptions {
            source: "local".into(),
            scope: None,
            origin: None,
            base_dir: Some(base.clone()),
        },
    );
    assert_eq!(source.path, path);
    assert_eq!(source.base_dir, Some(base));
}

#[test]
fn ts_created_raw_skill_directory_retains_native_load_and_prompt_paths() {
    use pi_coding_agent::core::skills::{LoadSkillsFromDirOptions, format_skills_for_prompt, load_skills_from_dir};
    let root = tempfile::tempdir().unwrap();
    let dir = JsString::from(root.path().to_str().unwrap()) + "/skill-" + JsString::from_utf16(vec![0xd800]);
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-resources-oracle.ts",
        "packages/coding-agent/src/core/skills.ts",
    );
    let source = oracle.request(&pi_js_compat::js_json!({"id":"raw-skill-dir", "operation":"prepareSkill", "dir":dir}));
    assert_eq!(source["ok"].as_bool(), Some(true));
    let loaded = load_skills_from_dir(&LoadSkillsFromDirOptions {
        dir: dir.clone(),
        source: "path".into(),
    })
    .unwrap();
    assert_eq!(loaded.skills.len(), 1);
    assert_eq!(loaded.skills[0].file_path, dir + "/SKILL.md");
    assert_eq!(pi_js_compat::js_json::to_value(&loaded).unwrap(), source["output"]);
    assert!(format_skills_for_prompt(&loaded.skills).contains(&loaded.skills[0].file_path));
}
