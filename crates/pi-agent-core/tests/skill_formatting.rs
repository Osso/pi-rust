use pi_agent_core::harness::Skill;
use pi_agent_core::harness::skills::format_skill_invocation;
use pi_agent_core::harness::system_prompt::format_skills_for_system_prompt;
use pi_js_compat::js_json::JsString;

fn skill(name: &str, description: &str, content: &str, file_path: &str) -> Skill {
    Skill {
        name: JsString::from(name.to_owned()),
        description: description.into(),
        content: content.into(),
        file_path: JsString::from(file_path.to_owned()),
        disable_model_invocation: None,
    }
}

fn relative_directory(file_path: &str) -> JsString {
    let output = format_skill_invocation(&skill("name", "description", "content", file_path), None);
    output
        .split('\n')
        .nth(1)
        .expect("skill invocation must include a relative-directory line")
        .to_owned()
}

#[test]
fn system_prompt_filters_hidden_skills_and_preserves_visible_order_and_duplicates() {
    let mut hidden = skill("hidden", "hidden", "hidden content", "/skills/hidden/SKILL.md");
    hidden.disable_model_invocation = Some(true);
    let mut visible_false = skill("second", "second", "second content", "/skills/second/SKILL.md");
    visible_false.disable_model_invocation = Some(false);
    let skills = vec![
        skill("first", "first", "first content", "/skills/first/SKILL.md"),
        hidden,
        visible_false,
        skill(
            "second",
            "duplicate",
            "duplicate content",
            "/skills/second-duplicate/SKILL.md",
        ),
    ];

    let output = format_skills_for_system_prompt(&skills);

    assert!(!output.contains("hidden"));
    assert!(!output.contains("hidden content"));
    assert!(output.find("<name>first</name>").unwrap() < output.find("<name>second</name>").unwrap());
    assert_eq!(
        output
            .units()
            .windows(JsString::from("<name>second</name>").len())
            .filter(|units| *units == JsString::from("<name>second</name>").units())
            .count(),
        2
    );
    assert_eq!(format_skills_for_system_prompt(&[]), "");

    let mut all_hidden = skill("hidden", "hidden", "hidden content", "/skills/hidden/SKILL.md");
    all_hidden.disable_model_invocation = Some(true);
    assert_eq!(format_skills_for_system_prompt(&[all_hidden]), "");
}

#[test]
fn system_prompt_emits_exact_fixed_output_without_content_or_trailing_lf() {
    let output = format_skills_for_system_prompt(&[skill(
        "visible",
        "Use this skill",
        "content must stay out of the system prompt",
        "/skills/visible/SKILL.md",
    )]);

    assert_eq!(
        output,
        concat!(
            "The following skills provide specialized instructions for specific tasks.\n",
            "Read the full skill file when the task matches its description.\n",
            "When a skill file references a relative path, resolve it against the skill directory (parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.\n",
            "\n",
            "<available_skills>\n",
            "  <skill>\n",
            "    <name>visible</name>\n",
            "    <description>Use this skill</description>\n",
            "    <location>/skills/visible/SKILL.md</location>\n",
            "  </skill>\n",
            "</available_skills>"
        )
    );
    assert!(!output.ends_with('\n'));
    assert!(!output.contains("content must stay out"));
}

#[test]
fn system_prompt_escapes_all_metadata_entities_in_source_order() {
    let value = "&<>'\"&amp;";
    let output = format_skills_for_system_prompt(&[skill(value, value, "ignored", value)]);
    let escaped = "&amp;&lt;&gt;&apos;&quot;&amp;amp;";

    assert!(output.contains(format!("<name>{escaped}</name>")));
    assert!(output.contains(format!("<description>{escaped}</description>")));
    assert!(output.contains(format!("<location>{escaped}</location>")));
}

#[test]
fn system_prompt_preserves_empty_unicode_and_control_string_values() {
    let output =
        format_skills_for_system_prompt(&[skill("😀e\u{301}\0,!?", "", "ignored\ncontent", "é/技能/SKILL.md")]);

    assert!(output.contains("<name>😀e\u{301}\0,!?</name>"));
    assert!(output.contains("<description></description>"));
    assert!(output.contains("<location>é/技能/SKILL.md</location>"));
    assert!(!output.contains("ignored\ncontent"));
}

#[test]
fn skill_invocation_preserves_raw_fields_and_omits_empty_instructions() {
    let skill = skill("name<&\"'", "description", "content<&\"'\0", "/a/b/SKILL.md");
    let expected = "<skill name=\"name<&\"'\" location=\"/a/b/SKILL.md\">\nReferences are relative to /a/b.\n\ncontent<&\"'\0\n</skill>";

    assert_eq!(format_skill_invocation(&skill, None), expected);
    assert_eq!(format_skill_invocation(&skill, Some(&JsString::from(""))), expected);
}

#[test]
fn skill_invocation_appends_nonempty_instructions_verbatim() {
    let skill = skill("name", "description", "content", "relative/SKILL.md");

    assert_eq!(
        format_skill_invocation(&skill, Some(&JsString::from("Check <this> & that\nnext."))),
        "<skill name=\"name\" location=\"relative/SKILL.md\">\nReferences are relative to relative.\n\ncontent\n</skill>\n\nCheck <this> & that\nnext."
    );
}

#[test]
fn dirname_uses_lexical_posix_and_separator_rules() {
    assert_eq!(relative_directory(""), "References are relative to /.");
    assert_eq!(relative_directory("/"), "References are relative to /.");
    assert_eq!(relative_directory("///"), "References are relative to /.");
    assert_eq!(relative_directory("SKILL.md"), "References are relative to /.");
    assert_eq!(relative_directory("a\\b"), "References are relative to /.");
    assert_eq!(relative_directory("/a/b"), "References are relative to /a.");
    assert_eq!(relative_directory("/a/b///"), "References are relative to /a.");
    assert_eq!(
        relative_directory("foo//bar///baz"),
        "References are relative to foo//bar//."
    );
    assert_eq!(
        relative_directory("//server/share"),
        "References are relative to //server."
    );
}

#[test]
fn dirname_uses_only_frozen_forward_slashes_without_drive_special_cases() {
    assert_eq!(relative_directory("C:/"), "References are relative to /.");
    assert_eq!(relative_directory("C:\\"), "References are relative to /.");
    assert_eq!(relative_directory("C:/a"), "References are relative to C:.");
    assert_eq!(relative_directory("C:\\a"), "References are relative to /.");
    assert_eq!(relative_directory("C:/a/b"), "References are relative to C:/a.");
    assert_eq!(relative_directory("C:\\a\\b\\"), "References are relative to /.");
    assert_eq!(
        relative_directory("C:/a\\b/SKILL.md"),
        "References are relative to C:/a\\b."
    );
    assert_eq!(relative_directory("é:/x"), "References are relative to é:.");
    assert_eq!(relative_directory("😀:/x"), "References are relative to 😀:.");
}
