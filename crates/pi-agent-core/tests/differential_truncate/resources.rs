use pi_agent_core::harness::{AgentHarnessResources, PromptTemplate, Skill};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_024;
const EXPECTED_FIXTURE_COUNT: usize = 12;

#[derive(Clone, Copy)]
enum OptionalBool {
    Absent,
    Undefined,
    False,
    True,
}

impl OptionalBool {
    const fn source_state(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Undefined => "undefined",
            Self::False => "false",
            Self::True => "true",
        }
    }

    const fn rust_value(self) -> Option<bool> {
        match self {
            Self::Absent | Self::Undefined => None,
            Self::False => Some(false),
            Self::True => Some(true),
        }
    }
}

#[derive(Clone, Copy)]
enum OptionalDescription {
    Absent,
    Undefined,
    Empty,
    Value(&'static str),
}

impl OptionalDescription {
    const fn source_state(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Undefined => "undefined",
            Self::Empty => "empty",
            Self::Value(_) => "value",
        }
    }

    const fn rust_value(self) -> Option<&'static str> {
        match self {
            Self::Absent | Self::Undefined => None,
            Self::Empty => Some(""),
            Self::Value(value) => Some(value),
        }
    }
}

#[derive(Clone, Copy)]
struct SkillFixture {
    name: &'static str,
    description: &'static str,
    content: &'static str,
    file_path: &'static str,
    disable_model_invocation: OptionalBool,
}

#[derive(Clone, Copy)]
struct PromptTemplateFixture {
    name: &'static str,
    description: OptionalDescription,
    content: &'static str,
}

enum SkillArray {
    Absent,
    Empty,
    Values(&'static [SkillFixture]),
}

enum PromptTemplateArray {
    Absent,
    Empty,
    Values(&'static [PromptTemplateFixture]),
}

struct Fixture {
    name: &'static str,
    skills: SkillArray,
    prompt_templates: PromptTemplateArray,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    assert_eq!(FIXTURES.len(), EXPECTED_FIXTURE_COUNT);

    for (offset, fixture) in FIXTURES.iter().enumerate() {
        let id = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        let expected = oracle.request(&request(id, fixture));
        let actual = rust_response(id, fixture);

        assert_eq!(actual, expected, "resource-value mismatch: {}", fixture.name);
    }
}

fn request(id: u64, fixture: &Fixture) -> Value {
    json!({
        "id": id,
        "operation": "resource_values",
        "skills": skill_array_request(&fixture.skills),
        "promptTemplates": prompt_template_array_request(&fixture.prompt_templates),
    })
}

fn skill_array_request(array: &SkillArray) -> Value {
    match array {
        SkillArray::Absent => json!({ "state": "absent" }),
        SkillArray::Empty => json!({ "state": "empty" }),
        SkillArray::Values(values) => json!({
            "state": "values",
            "values": values.iter().map(skill_request).collect::<Vec<_>>(),
        }),
    }
}

fn prompt_template_array_request(array: &PromptTemplateArray) -> Value {
    match array {
        PromptTemplateArray::Absent => json!({ "state": "absent" }),
        PromptTemplateArray::Empty => json!({ "state": "empty" }),
        PromptTemplateArray::Values(values) => json!({
            "state": "values",
            "values": values.iter().map(prompt_template_request).collect::<Vec<_>>(),
        }),
    }
}

fn skill_request(fixture: &SkillFixture) -> Value {
    json!({
        "name": fixture.name,
        "description": fixture.description,
        "content": fixture.content,
        "filePath": fixture.file_path,
        "disableModelInvocationState": fixture.disable_model_invocation.source_state(),
    })
}

fn prompt_template_request(fixture: &PromptTemplateFixture) -> Value {
    json!({
        "name": fixture.name,
        "descriptionState": fixture.description.source_state(),
        "descriptionValue": fixture.description.rust_value().unwrap_or_default(),
        "content": fixture.content,
    })
}

fn rust_response(id: u64, fixture: &Fixture) -> Value {
    let resources = build_resources(fixture);
    ok_response(id, normalize_resources(&resources))
}

fn build_resources(fixture: &Fixture) -> AgentHarnessResources {
    AgentHarnessResources {
        prompt_templates: build_prompt_templates(&fixture.prompt_templates),
        skills: build_skills(&fixture.skills),
    }
}

fn build_skills(array: &SkillArray) -> Option<Vec<Skill>> {
    match array {
        SkillArray::Absent => None,
        SkillArray::Empty => Some(Vec::new()),
        SkillArray::Values(values) => Some(values.iter().map(build_skill).collect()),
    }
}

fn build_skill(fixture: &SkillFixture) -> Skill {
    Skill {
        name: JsString::from(fixture.name.to_owned()),
        description: fixture.description.into(),
        content: fixture.content.into(),
        file_path: JsString::from(fixture.file_path.to_owned()),
        disable_model_invocation: fixture.disable_model_invocation.rust_value(),
    }
}

fn build_prompt_templates(array: &PromptTemplateArray) -> Option<Vec<PromptTemplate>> {
    match array {
        PromptTemplateArray::Absent => None,
        PromptTemplateArray::Empty => Some(Vec::new()),
        PromptTemplateArray::Values(values) => Some(values.iter().map(build_prompt_template).collect()),
    }
}

fn build_prompt_template(fixture: &PromptTemplateFixture) -> PromptTemplate {
    PromptTemplate {
        name: JsString::from(fixture.name.to_owned()),
        description: fixture.description.rust_value().map(JsString::from),
        content: fixture.content.into(),
    }
}

fn normalize_resources(resources: &AgentHarnessResources) -> Value {
    json!({
        "prompt_templates": normalize_prompt_templates(resources.prompt_templates.as_deref()),
        "skills": normalize_skills(resources.skills.as_deref()),
    })
}

fn normalize_skills(skills: Option<&[Skill]>) -> Value {
    skills
        .map(|values| values.iter().map(normalize_skill).collect::<Vec<_>>())
        .map(Value::from)
        .unwrap_or(Value::Null)
}

fn normalize_skill(skill: &Skill) -> Value {
    json!({
        "name": skill.name,
        "description": skill.description,
        "content": skill.content,
        "file_path": skill.file_path,
        "disable_model_invocation": skill.disable_model_invocation,
    })
}

fn normalize_prompt_templates(templates: Option<&[PromptTemplate]>) -> Value {
    templates
        .map(|values| values.iter().map(normalize_prompt_template).collect::<Vec<_>>())
        .map(Value::from)
        .unwrap_or(Value::Null)
}

fn normalize_prompt_template(template: &PromptTemplate) -> Value {
    json!({
        "name": template.name,
        "description": template.description,
        "content": template.content,
    })
}

const MINIMAL_SKILL: SkillFixture = SkillFixture {
    name: "skill",
    description: "description",
    content: "content",
    file_path: "/skills/skill/SKILL.md",
    disable_model_invocation: OptionalBool::Absent,
};

const UNDEFINED_SKILL: SkillFixture = SkillFixture {
    disable_model_invocation: OptionalBool::Undefined,
    ..MINIMAL_SKILL
};

const FALSE_SKILL: SkillFixture = SkillFixture {
    disable_model_invocation: OptionalBool::False,
    ..MINIMAL_SKILL
};

const TRUE_SKILL: SkillFixture = SkillFixture {
    disable_model_invocation: OptionalBool::True,
    ..MINIMAL_SKILL
};

const EMPTY_SKILL: SkillFixture = SkillFixture {
    name: "",
    description: "",
    content: "",
    file_path: "",
    disable_model_invocation: OptionalBool::Absent,
};

const ORDERED_SKILLS: [SkillFixture; 3] = [
    SkillFixture {
        name: "duplicate",
        description: "first",
        content: "one",
        file_path: "/first",
        disable_model_invocation: OptionalBool::Absent,
    },
    SkillFixture {
        name: "duplicate",
        description: "second",
        content: "two",
        file_path: "/second",
        disable_model_invocation: OptionalBool::False,
    },
    SkillFixture {
        name: "duplicate",
        description: "third",
        content: "three",
        file_path: "/third",
        disable_model_invocation: OptionalBool::True,
    },
];

const UNICODE_SKILL: SkillFixture = SkillFixture {
    name: "技能🙂e\u{301}\0,!",
    description: "描述🙂e\u{301}\0, punctuation!?",
    content: "内容🙂e\u{301}\0, <skill>!",
    file_path: "路径/技能🙂e\u{301}\0/SKILL.md,!",
    disable_model_invocation: OptionalBool::True,
};

const MINIMAL_PROMPT: PromptTemplateFixture = PromptTemplateFixture {
    name: "prompt",
    description: OptionalDescription::Absent,
    content: "content",
};

const UNDEFINED_PROMPT: PromptTemplateFixture = PromptTemplateFixture {
    description: OptionalDescription::Undefined,
    ..MINIMAL_PROMPT
};

const EMPTY_PROMPT: PromptTemplateFixture = PromptTemplateFixture {
    name: "empty",
    description: OptionalDescription::Empty,
    content: "",
};

const ORDERED_PROMPTS: [PromptTemplateFixture; 2] = [
    PromptTemplateFixture {
        name: "duplicate",
        description: OptionalDescription::Absent,
        content: "first",
    },
    PromptTemplateFixture {
        name: "duplicate",
        description: OptionalDescription::Empty,
        content: "second",
    },
];

const UNICODE_PROMPT: PromptTemplateFixture = PromptTemplateFixture {
    name: "模板🙂e\u{301}\0,!",
    description: OptionalDescription::Value("描述🙂e\u{301}\0, punctuation?"),
    content: "内容🙂e\u{301}\0, punctuation!\n",
};

const FIXTURES: [Fixture; EXPECTED_FIXTURE_COUNT] = [
    Fixture {
        name: "minimal skill with absent option",
        skills: SkillArray::Values(&[MINIMAL_SKILL]),
        prompt_templates: PromptTemplateArray::Absent,
    },
    Fixture {
        name: "skill with explicit undefined option",
        skills: SkillArray::Values(&[UNDEFINED_SKILL]),
        prompt_templates: PromptTemplateArray::Absent,
    },
    Fixture {
        name: "skill with false option",
        skills: SkillArray::Values(&[FALSE_SKILL]),
        prompt_templates: PromptTemplateArray::Absent,
    },
    Fixture {
        name: "skill with true option",
        skills: SkillArray::Values(&[TRUE_SKILL]),
        prompt_templates: PromptTemplateArray::Absent,
    },
    Fixture {
        name: "minimal prompt with absent description",
        skills: SkillArray::Absent,
        prompt_templates: PromptTemplateArray::Values(&[MINIMAL_PROMPT]),
    },
    Fixture {
        name: "prompt with explicit undefined description",
        skills: SkillArray::Absent,
        prompt_templates: PromptTemplateArray::Values(&[UNDEFINED_PROMPT]),
    },
    Fixture {
        name: "empty prompt fields and skill strings",
        skills: SkillArray::Values(&[EMPTY_SKILL]),
        prompt_templates: PromptTemplateArray::Values(&[EMPTY_PROMPT]),
    },
    Fixture {
        name: "unicode prompt strings",
        skills: SkillArray::Absent,
        prompt_templates: PromptTemplateArray::Values(&[UNICODE_PROMPT]),
    },
    Fixture {
        name: "resources absent arrays",
        skills: SkillArray::Absent,
        prompt_templates: PromptTemplateArray::Absent,
    },
    Fixture {
        name: "resources explicit empty arrays",
        skills: SkillArray::Empty,
        prompt_templates: PromptTemplateArray::Empty,
    },
    Fixture {
        name: "ordered duplicate resources",
        skills: SkillArray::Values(&ORDERED_SKILLS),
        prompt_templates: PromptTemplateArray::Values(&ORDERED_PROMPTS),
    },
    Fixture {
        name: "unicode skill and prompt resources",
        skills: SkillArray::Values(&[UNICODE_SKILL]),
        prompt_templates: PromptTemplateArray::Values(&[UNICODE_PROMPT]),
    },
];
