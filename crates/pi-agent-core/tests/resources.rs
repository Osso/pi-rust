use pi_js_compat::js_json::JsString;
use std::fmt::Debug;

use pi_agent_core::harness::{AgentHarnessResources, PromptTemplate, Skill};

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn skill_preserves_required_fields_and_none_option() {
    let mut skill = Skill {
        name: JsString::from("skill-name"),
        description: "skill description".into(),
        content: "skill content".into(),
        file_path: JsString::from("/skills/skill-name/SKILL.md"),
        disable_model_invocation: None,
    };

    assert_eq!(skill.name, "skill-name");
    assert_eq!(skill.description, "skill description");
    assert_eq!(skill.content, "skill content");
    assert_eq!(skill.file_path, "/skills/skill-name/SKILL.md");
    assert_eq!(skill.disable_model_invocation, None);

    skill.name = JsString::from("renamed-skill");
    skill.description = "updated description".into();
    skill.content = "updated content".into();
    skill.file_path = JsString::from("/skills/renamed-skill/SKILL.md");
    skill.disable_model_invocation = Some(true);

    assert_eq!(skill.name, "renamed-skill");
    assert_eq!(skill.description, "updated description");
    assert_eq!(skill.content, "updated content");
    assert_eq!(skill.file_path, "/skills/renamed-skill/SKILL.md");
    assert_eq!(skill.disable_model_invocation, Some(true));
    assert_debug(&skill);
}

#[test]
fn skill_preserves_optional_disable_model_invocation_states() {
    let states = [None, Some(false), Some(true)];

    for state in states {
        let skill = Skill {
            name: JsString::from("name"),
            description: "description".into(),
            content: "content".into(),
            file_path: JsString::from("/skill"),
            disable_model_invocation: state,
        };

        assert_eq!(skill.disable_model_invocation, state);
    }
}

#[test]
fn skill_preserves_empty_unicode_and_embedded_nul_strings() {
    let skill = Skill {
        name: JsString::from(""),
        description: "描述🙂e\u{301}\0, punctuation!?".into(),
        content: "内容🙂e\u{301}\0, <skill>!".into(),
        file_path: JsString::from("路径/技能🙂e\u{301}\0/SKILL.md,!"),
        disable_model_invocation: Some(false),
    };

    assert_eq!(skill.name, "");
    assert_eq!(skill.description, "描述🙂e\u{301}\0, punctuation!?");
    assert_eq!(skill.content, "内容🙂e\u{301}\0, <skill>!");
    assert_eq!(skill.file_path, "路径/技能🙂e\u{301}\0/SKILL.md,!".to_owned());
    assert_eq!(skill.disable_model_invocation, Some(false));
}

#[test]
fn skill_preserves_empty_strings_in_every_string_field() {
    let skill = Skill {
        name: JsString::default(),
        description: JsString::default(),
        content: JsString::default(),
        file_path: JsString::default(),
        disable_model_invocation: None,
    };

    assert!(skill.name.is_empty());
    assert!(skill.description.is_empty());
    assert!(skill.content.is_empty());
    assert!(skill.file_path.is_empty());
}

#[test]
fn prompt_template_preserves_optional_description_states() {
    let no_description = PromptTemplate {
        name: JsString::from("minimal"),
        description: None,
        content: "Use this prompt.".into(),
    };
    assert_eq!(no_description.description, None);

    let empty_description = PromptTemplate {
        name: JsString::from("empty-description"),
        description: Some(JsString::default()),
        content: "Content".into(),
    };
    assert_eq!(empty_description.description, Some(JsString::default()));

    let described = PromptTemplate {
        name: JsString::from("described"),
        description: Some("A prompt template".into()),
        content: "Content".into(),
    };
    assert_eq!(described.description, Some(JsString::from("A prompt template")));
}

#[test]
fn prompt_template_preserves_fields_and_public_mutation() {
    let mut template = PromptTemplate {
        name: JsString::from("template"),
        description: Some("description".into()),
        content: "content".into(),
    };

    assert_eq!(template.name, "template");
    assert_eq!(template.description.as_ref(), Some(&JsString::from("description")));
    assert_eq!(template.content, "content");

    template.name = JsString::from("renamed-template");
    template.description = None;
    template.content = "updated content".into();

    assert_eq!(template.name, "renamed-template");
    assert_eq!(template.description, None);
    assert_eq!(template.content, "updated content");
    assert_debug(&template);
}

#[test]
fn prompt_template_preserves_unicode_and_embedded_nul_strings() {
    let template = PromptTemplate {
        name: JsString::from("模板🙂e\u{301}\0,!"),
        description: Some("描述🙂e\u{301}\0, punctuation?".into()),
        content: "内容🙂e\u{301}\0, punctuation!\n".into(),
    };

    assert_eq!(template.name, "模板🙂e\u{301}\0,!");
    assert_eq!(
        template.description.as_ref(),
        Some(&JsString::from("描述🙂e\u{301}\0, punctuation?"))
    );
    assert_eq!(template.content, "内容🙂e\u{301}\0, punctuation!\n");
}

#[test]
fn prompt_template_preserves_empty_strings_in_every_string_field() {
    let template = PromptTemplate {
        name: JsString::default(),
        description: Some(JsString::default()),
        content: JsString::default(),
    };

    assert!(template.name.is_empty());
    assert!(template.description.as_ref().is_some_and(JsString::is_empty));
    assert!(template.content.is_empty());
}

#[test]
fn resources_preserve_none_and_empty_array_distinction() {
    let without_resources = AgentHarnessResources {
        prompt_templates: None,
        skills: None,
    };
    assert!(without_resources.prompt_templates.is_none());
    assert!(without_resources.skills.is_none());

    let with_empty_resources = AgentHarnessResources {
        prompt_templates: Some(Vec::new()),
        skills: Some(Vec::new()),
    };
    assert!(matches!(with_empty_resources.prompt_templates, Some(values) if values.is_empty()));
    assert!(matches!(with_empty_resources.skills, Some(values) if values.is_empty()));
}

#[test]
fn resources_preserve_order_duplicates_and_nested_mutation() {
    let first_skill = Skill {
        name: JsString::from("duplicate"),
        description: "first".into(),
        content: "content-1".into(),
        file_path: JsString::from("/first"),
        disable_model_invocation: None,
    };
    let second_skill = Skill {
        name: JsString::from("duplicate"),
        description: "second".into(),
        content: "content-2".into(),
        file_path: JsString::from("/second"),
        disable_model_invocation: Some(false),
    };
    let first_template = PromptTemplate {
        name: JsString::from("template-a"),
        description: None,
        content: "a".into(),
    };
    let second_template = PromptTemplate {
        name: JsString::from("template-a"),
        description: Some("duplicate".into()),
        content: "b".into(),
    };

    let mut resources = AgentHarnessResources {
        prompt_templates: Some(vec![first_template, second_template]),
        skills: Some(vec![first_skill, second_skill]),
    };

    let templates = resources.prompt_templates.as_ref().expect("templates present");
    assert_eq!(templates[0].content, "a");
    assert_eq!(templates[1].content, "b");
    assert_eq!(templates[0].name, templates[1].name);

    let skills = resources.skills.as_ref().expect("skills present");
    assert_eq!(skills[0].description, "first");
    assert_eq!(skills[1].description, "second");
    assert_eq!(skills[0].name, skills[1].name);

    let skills = resources.skills.as_mut().expect("skills present");
    skills[0].name = JsString::from("updated-first");
    skills[1].disable_model_invocation = Some(true);
    assert_eq!(skills[0].name, "updated-first");
    assert_eq!(skills[1].disable_model_invocation, Some(true));

    let templates = resources.prompt_templates.as_mut().expect("templates present");
    templates[0].description = Some("added description".into());
    templates[1] = PromptTemplate {
        name: JsString::from("replacement"),
        description: None,
        content: "replacement content".into(),
    };
    assert_eq!(
        templates[0].description.as_ref(),
        Some(&JsString::from("added description"))
    );
    assert_eq!(templates[1].name, "replacement");

    resources.skills = Some(vec![Skill {
        name: JsString::from("replacement skill"),
        description: "replacement description".into(),
        content: "replacement content".into(),
        file_path: JsString::from("/replacement"),
        disable_model_invocation: None,
    }]);
    resources.prompt_templates = None;
    assert_eq!(
        resources.skills.as_ref().expect("skills present")[0].name,
        "replacement skill"
    );
    assert!(resources.prompt_templates.is_none());
    assert_debug(&resources);
}
