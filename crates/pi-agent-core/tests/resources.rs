use std::fmt::Debug;

use pi_agent_core::harness::{AgentHarnessResources, PromptTemplate, Skill};

fn assert_debug<T: Debug>(_: &T) {}

#[test]
fn skill_preserves_required_fields_and_none_option() {
    let mut skill = Skill {
        name: "skill-name".to_owned(),
        description: "skill description".to_owned(),
        content: "skill content".to_owned(),
        file_path: "/skills/skill-name/SKILL.md".to_owned(),
        disable_model_invocation: None,
    };

    assert_eq!(skill.name, "skill-name");
    assert_eq!(skill.description, "skill description");
    assert_eq!(skill.content, "skill content");
    assert_eq!(skill.file_path, "/skills/skill-name/SKILL.md");
    assert_eq!(skill.disable_model_invocation, None);

    skill.name = "renamed-skill".to_owned();
    skill.description = "updated description".to_owned();
    skill.content = "updated content".to_owned();
    skill.file_path = "/skills/renamed-skill/SKILL.md".to_owned();
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
            name: "name".to_owned(),
            description: "description".to_owned(),
            content: "content".to_owned(),
            file_path: "/skill".to_owned(),
            disable_model_invocation: state,
        };

        assert_eq!(skill.disable_model_invocation, state);
    }
}

#[test]
fn skill_preserves_empty_unicode_and_embedded_nul_strings() {
    let skill = Skill {
        name: "".to_owned(),
        description: "描述🙂e\u{301}\0, punctuation!?".to_owned(),
        content: "内容🙂e\u{301}\0, <skill>!".to_owned(),
        file_path: "路径/技能🙂e\u{301}\0/SKILL.md,!".to_owned(),
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
        name: String::new(),
        description: String::new(),
        content: String::new(),
        file_path: String::new(),
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
        name: "minimal".to_owned(),
        description: None,
        content: "Use this prompt.".to_owned(),
    };
    assert_eq!(no_description.description, None);

    let empty_description = PromptTemplate {
        name: "empty-description".to_owned(),
        description: Some(String::new()),
        content: "Content".to_owned(),
    };
    assert_eq!(empty_description.description, Some(String::new()));

    let described = PromptTemplate {
        name: "described".to_owned(),
        description: Some("A prompt template".to_owned()),
        content: "Content".to_owned(),
    };
    assert_eq!(described.description, Some("A prompt template".to_owned()));
}

#[test]
fn prompt_template_preserves_fields_and_public_mutation() {
    let mut template = PromptTemplate {
        name: "template".to_owned(),
        description: Some("description".to_owned()),
        content: "content".to_owned(),
    };

    assert_eq!(template.name, "template");
    assert_eq!(template.description.as_deref(), Some("description"));
    assert_eq!(template.content, "content");

    template.name = "renamed-template".to_owned();
    template.description = None;
    template.content = "updated content".to_owned();

    assert_eq!(template.name, "renamed-template");
    assert_eq!(template.description, None);
    assert_eq!(template.content, "updated content");
    assert_debug(&template);
}

#[test]
fn prompt_template_preserves_unicode_and_embedded_nul_strings() {
    let template = PromptTemplate {
        name: "模板🙂e\u{301}\0,!".to_owned(),
        description: Some("描述🙂e\u{301}\0, punctuation?".to_owned()),
        content: "内容🙂e\u{301}\0, punctuation!\n".to_owned(),
    };

    assert_eq!(template.name, "模板🙂e\u{301}\0,!");
    assert_eq!(template.description.as_deref(), Some("描述🙂e\u{301}\0, punctuation?"));
    assert_eq!(template.content, "内容🙂e\u{301}\0, punctuation!\n");
}

#[test]
fn prompt_template_preserves_empty_strings_in_every_string_field() {
    let template = PromptTemplate {
        name: String::new(),
        description: Some(String::new()),
        content: String::new(),
    };

    assert!(template.name.is_empty());
    assert!(template.description.as_deref().is_some_and(str::is_empty));
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
        name: "duplicate".to_owned(),
        description: "first".to_owned(),
        content: "content-1".to_owned(),
        file_path: "/first".to_owned(),
        disable_model_invocation: None,
    };
    let second_skill = Skill {
        name: "duplicate".to_owned(),
        description: "second".to_owned(),
        content: "content-2".to_owned(),
        file_path: "/second".to_owned(),
        disable_model_invocation: Some(false),
    };
    let first_template = PromptTemplate {
        name: "template-a".to_owned(),
        description: None,
        content: "a".to_owned(),
    };
    let second_template = PromptTemplate {
        name: "template-a".to_owned(),
        description: Some("duplicate".to_owned()),
        content: "b".to_owned(),
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
    skills[0].name = "updated-first".to_owned();
    skills[1].disable_model_invocation = Some(true);
    assert_eq!(skills[0].name, "updated-first");
    assert_eq!(skills[1].disable_model_invocation, Some(true));

    let templates = resources.prompt_templates.as_mut().expect("templates present");
    templates[0].description = Some("added description".to_owned());
    templates[1] = PromptTemplate {
        name: "replacement".to_owned(),
        description: None,
        content: "replacement content".to_owned(),
    };
    assert_eq!(templates[0].description.as_deref(), Some("added description"));
    assert_eq!(templates[1].name, "replacement");

    resources.skills = Some(vec![Skill {
        name: "replacement skill".to_owned(),
        description: "replacement description".to_owned(),
        content: "replacement content".to_owned(),
        file_path: "/replacement".to_owned(),
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
