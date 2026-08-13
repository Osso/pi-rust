use pi_agent_core::harness::Skill;
use pi_agent_core::harness::skills::format_skill_invocation;
use pi_agent_core::harness::system_prompt::format_skills_for_system_prompt;
use serde_json::{Value, json};

use super::{oracle::TsOracle, support::ok_response};

const FIRST_REQUEST_ID: u64 = 13_061;
const EXPECTED_FIXTURE_COUNT: usize = 8;

#[derive(Clone, Copy)]
enum OptionalBool {
    Absent,
    False,
    True,
}

impl OptionalBool {
    const fn source_state(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::False => "false",
            Self::True => "true",
        }
    }

    const fn rust_value(self) -> Option<bool> {
        match self {
            Self::Absent => None,
            Self::False => Some(false),
            Self::True => Some(true),
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
enum AdditionalInstructions {
    Absent,
    Empty,
    Value(&'static str),
}

impl AdditionalInstructions {
    const fn source_state(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Empty => "empty",
            Self::Value(_) => "value",
        }
    }

    const fn value(self) -> &'static str {
        match self {
            Self::Absent | Self::Empty => "",
            Self::Value(value) => value,
        }
    }

    const fn rust_value(self) -> Option<&'static str> {
        match self {
            Self::Absent => None,
            Self::Empty => Some(""),
            Self::Value(value) => Some(value),
        }
    }
}

#[derive(Clone, Copy)]
struct InvocationFixture {
    skill: SkillFixture,
    additional_instructions: AdditionalInstructions,
}

struct Fixture {
    request_id: u64,
    name: &'static str,
    system_prompt_cases: Vec<Vec<SkillFixture>>,
    invocation_cases: Vec<InvocationFixture>,
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), EXPECTED_FIXTURE_COUNT);
    assert_request_ids(&fixtures);

    for fixture in &fixtures {
        let expected = oracle.request(&request(fixture));
        let actual = rust_response(fixture);
        assert_eq!(actual, expected, "skill-formatting mismatch: {}", fixture.name);
    }
}

fn assert_request_ids(fixtures: &[Fixture]) {
    for (offset, fixture) in fixtures.iter().enumerate() {
        let expected = FIRST_REQUEST_ID + u64::try_from(offset).expect("fixture offset must fit u64");
        assert_eq!(fixture.request_id, expected, "fixture IDs must remain contiguous");
    }
}

fn request(fixture: &Fixture) -> Value {
    json!({
        "id": fixture.request_id,
        "operation": "skill_formatting",
        "systemPromptCases": fixture
            .system_prompt_cases
            .iter()
            .map(|skills| skills.iter().map(skill_request).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        "invocationCases": fixture
            .invocation_cases
            .iter()
            .map(invocation_request)
            .collect::<Vec<_>>(),
    })
}

fn skill_request(skill: &SkillFixture) -> Value {
    json!({
        "name": skill.name,
        "description": skill.description,
        "content": skill.content,
        "filePath": skill.file_path,
        "disableModelInvocationState": skill.disable_model_invocation.source_state(),
    })
}

fn invocation_request(invocation: &InvocationFixture) -> Value {
    json!({
        "skill": skill_request(&invocation.skill),
        "additionalInstructionsState": invocation.additional_instructions.source_state(),
        "additionalInstructions": invocation.additional_instructions.value(),
    })
}

fn rust_response(fixture: &Fixture) -> Value {
    let system_prompts = fixture
        .system_prompt_cases
        .iter()
        .map(|skills| {
            let skills = skills.iter().map(build_skill).collect::<Vec<_>>();
            format_skills_for_system_prompt(&skills)
        })
        .collect::<Vec<_>>();
    let invocations = fixture
        .invocation_cases
        .iter()
        .map(|invocation| {
            let skill = build_skill(&invocation.skill);
            format_skill_invocation(&skill, invocation.additional_instructions.rust_value())
        })
        .collect::<Vec<_>>();

    ok_response(
        fixture.request_id,
        json!({
            "systemPrompts": system_prompts,
            "invocations": invocations,
        }),
    )
}

fn build_skill(fixture: &SkillFixture) -> Skill {
    Skill {
        name: fixture.name.to_owned(),
        description: fixture.description.to_owned(),
        content: fixture.content.to_owned(),
        file_path: fixture.file_path.to_owned(),
        disable_model_invocation: fixture.disable_model_invocation.rust_value(),
    }
}

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            request_id: 13_061,
            name: "visibility states and empty lists",
            system_prompt_cases: vec![
                vec![],
                vec![SkillFixture {
                    name: "hidden",
                    description: "hidden",
                    content: "hidden content",
                    file_path: "/hidden/SKILL.md",
                    disable_model_invocation: OptionalBool::True,
                }],
                vec![
                    SkillFixture {
                        name: "none",
                        description: "visible",
                        content: "ignored",
                        file_path: "/none/SKILL.md",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    SkillFixture {
                        name: "false",
                        description: "visible",
                        content: "ignored",
                        file_path: "/false/SKILL.md",
                        disable_model_invocation: OptionalBool::False,
                    },
                    SkillFixture {
                        name: "true",
                        description: "hidden",
                        content: "ignored",
                        file_path: "/true/SKILL.md",
                        disable_model_invocation: OptionalBool::True,
                    },
                ],
            ],
            invocation_cases: vec![],
        },
        Fixture {
            request_id: 13_062,
            name: "system ordering duplicates and content exclusion",
            system_prompt_cases: vec![vec![
                SkillFixture {
                    name: "first",
                    description: "First",
                    content: "<not included>",
                    file_path: "/skills/first/SKILL.md",
                    disable_model_invocation: OptionalBool::Absent,
                },
                SkillFixture {
                    name: "hidden",
                    description: "Hidden",
                    content: "hidden",
                    file_path: "/skills/hidden/SKILL.md",
                    disable_model_invocation: OptionalBool::True,
                },
                SkillFixture {
                    name: "first",
                    description: "First",
                    content: "duplicate content",
                    file_path: "/skills/first/SKILL.md",
                    disable_model_invocation: OptionalBool::False,
                },
            ]],
            invocation_cases: vec![InvocationFixture {
                skill: SkillFixture {
                    name: "first",
                    description: "First",
                    content: "content",
                    file_path: "/skills/first/SKILL.md",
                    disable_model_invocation: OptionalBool::Absent,
                },
                additional_instructions: AdditionalInstructions::Absent,
            }],
        },
        Fixture {
            request_id: 13_063,
            name: "xml escaping and double escaping",
            system_prompt_cases: vec![vec![SkillFixture {
                name: "& < > \" '",
                description: "already &amp; & < > \" '",
                content: "ignored & < > \" '",
                file_path: "/skills/& < > \" '/SKILL.md",
                disable_model_invocation: OptionalBool::False,
            }]],
            invocation_cases: vec![],
        },
        Fixture {
            request_id: 13_064,
            name: "system string boundaries",
            system_prompt_cases: vec![vec![SkillFixture {
                name: "😀e\u{301}\0,!",
                description: "",
                content: "ignored",
                file_path: "é/技能/SKILL.md",
                disable_model_invocation: OptionalBool::Absent,
            }]],
            invocation_cases: vec![],
        },
        Fixture {
            request_id: 13_065,
            name: "instruction states and exact suffix",
            system_prompt_cases: vec![],
            invocation_cases: vec![
                InvocationFixture {
                    skill: SkillFixture {
                        name: "skill",
                        description: "description",
                        content: "content",
                        file_path: "/skills/skill/SKILL.md",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "skill",
                        description: "description",
                        content: "content",
                        file_path: "/skills/skill/SKILL.md",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Empty,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "skill",
                        description: "description",
                        content: "content",
                        file_path: "/skills/skill/SKILL.md",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Value("  extra\n\0"),
                },
            ],
        },
        Fixture {
            request_id: 13_066,
            name: "raw invocation fields",
            system_prompt_cases: vec![],
            invocation_cases: vec![InvocationFixture {
                skill: SkillFixture {
                    name: "<&\"'😀",
                    description: "ignored",
                    content: "raw <&\"'\n😀\0",
                    file_path: "dir/<&\"'/SKILL.md",
                    disable_model_invocation: OptionalBool::Absent,
                },
                additional_instructions: AdditionalInstructions::Value("<&\"'\n😀\0"),
            }],
        },
        Fixture {
            request_id: 13_067,
            name: "posix relative trailing duplicate and unc paths",
            system_prompt_cases: vec![],
            invocation_cases: vec![
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "///",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "SKILL.md",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "/a/b///",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "foo//bar///baz",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "//server/share/SKILL.md",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
            ],
        },
        Fixture {
            request_id: 13_068,
            name: "windows mixed and utf16 drive paths",
            system_prompt_cases: vec![],
            invocation_cases: vec![
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "C:/",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "C:\\",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "C:/a",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "C:\\a",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "C:a/b",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "C:/a\\b/SKILL.md",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "é:/x",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
                InvocationFixture {
                    skill: SkillFixture {
                        name: "path",
                        description: "",
                        content: "",
                        file_path: "😀:/x",
                        disable_model_invocation: OptionalBool::Absent,
                    },
                    additional_instructions: AdditionalInstructions::Absent,
                },
            ],
        },
    ]
}
