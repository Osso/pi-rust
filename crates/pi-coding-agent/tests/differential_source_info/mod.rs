use crate::shared_oracle as oracle;

use pi_coding_agent::core::source_info::{
    SourceInfo, SourceOrigin, SourceScope, SyntheticSourceInfoOptions, create_synthetic_source_info,
};
use pi_js_compat::js_json::JsString;
use serde_json::{Map, Value, json};

const REQUEST_ID: u64 = 13_182;
const EXPECTED_CASE_COUNT: usize = 10;

#[derive(Clone, Copy)]
enum OptionalState {
    Absent,
    Present,
}

impl OptionalState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Present => "present",
        }
    }
}

#[derive(Clone, Copy)]
enum ScopeState {
    Absent,
    User,
    Project,
    Temporary,
}

impl ScopeState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::User => "user",
            Self::Project => "project",
            Self::Temporary => "temporary",
        }
    }

    const fn value(self) -> Option<SourceScope> {
        match self {
            Self::Absent => None,
            Self::User => Some(SourceScope::User),
            Self::Project => Some(SourceScope::Project),
            Self::Temporary => Some(SourceScope::Temporary),
        }
    }
}

#[derive(Clone, Copy)]
enum OriginState {
    Absent,
    Package,
    TopLevel,
}

impl OriginState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Package => "package",
            Self::TopLevel => "top-level",
        }
    }

    const fn value(self) -> Option<SourceOrigin> {
        match self {
            Self::Absent => None,
            Self::Package => Some(SourceOrigin::Package),
            Self::TopLevel => Some(SourceOrigin::TopLevel),
        }
    }
}

struct Case {
    name: &'static str,
    path: &'static str,
    source: &'static str,
    scope: ScopeState,
    origin: OriginState,
    base_dir_state: OptionalState,
    base_dir: &'static str,
}

pub(crate) fn compare() {
    let cases = cases();
    assert_eq!(cases.len(), EXPECTED_CASE_COUNT);

    let mut oracle = oracle::TsOracle::spawn("source-info-oracle.ts", "packages/coding-agent/src/core/source-info.ts");
    let expected = oracle.request(&request(&cases));
    let actual = rust_response(&cases);
    assert_eq!(actual, expected, "source info mismatch");
}

fn request(cases: &[Case]) -> Value {
    json!({
        "id": REQUEST_ID,
        "operation": "source_info",
        "cases": cases.iter().map(case_request).collect::<Vec<_>>(),
    })
}

fn case_request(case: &Case) -> Value {
    json!({
        "name": case.name,
        "path": case.path,
        "source": case.source,
        "scopeState": case.scope.as_str(),
        "originState": case.origin.as_str(),
        "baseDirState": case.base_dir_state.as_str(),
        "baseDir": case.base_dir,
    })
}

fn rust_response(cases: &[Case]) -> Value {
    let results = cases.iter().map(normalize_case).collect::<Vec<_>>();
    json!({
        "id": REQUEST_ID,
        "ok": true,
        "output": {"results": results},
    })
}

fn normalize_case(case: &Case) -> Value {
    let options = SyntheticSourceInfoOptions {
        source: case.source.to_owned().into(),
        scope: case.scope.value(),
        origin: case.origin.value(),
        base_dir: (optional_string(case.base_dir_state, case.base_dir)).map(JsString::from),
    };
    let result = create_synthetic_source_info(&JsString::from(case.path), &options);
    json!({
        "name": case.name,
        "result": source_info_value(&result),
        "optionsAfter": options_value(&options),
    })
}

fn optional_string(state: OptionalState, value: &str) -> Option<String> {
    match state {
        OptionalState::Absent => None,
        OptionalState::Present => Some(value.to_owned()),
    }
}

fn source_info_value(result: &SourceInfo) -> Value {
    let mut value = Map::new();
    value.insert("path".to_owned(), json!(result.path));
    value.insert("source".to_owned(), json!(result.source));
    value.insert("scope".to_owned(), json!(result.scope.as_str()));
    value.insert("origin".to_owned(), json!(result.origin.as_str()));
    if let Some(base_dir) = &result.base_dir {
        value.insert("baseDir".to_owned(), json!(base_dir));
    }
    Value::Object(value)
}

fn options_value(options: &SyntheticSourceInfoOptions) -> Value {
    let mut value = Map::new();
    value.insert("source".to_owned(), json!(options.source));
    if let Some(scope) = options.scope {
        value.insert("scope".to_owned(), json!(scope.as_str()));
    }
    if let Some(origin) = options.origin {
        value.insert("origin".to_owned(), json!(origin.as_str()));
    }
    if let Some(base_dir) = &options.base_dir {
        value.insert("baseDir".to_owned(), json!(base_dir));
    }
    Value::Object(value)
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "defaults and omitted base dir",
            path: "",
            source: "",
            scope: ScopeState::Absent,
            origin: OriginState::Absent,
            base_dir_state: OptionalState::Absent,
            base_dir: "ignored",
        },
        Case {
            name: "user package empty base dir",
            path: "代理🙂\0\n",
            source: "source\r\n",
            scope: ScopeState::User,
            origin: OriginState::Package,
            base_dir_state: OptionalState::Present,
            base_dir: "",
        },
        Case {
            name: "project top level unicode base dir",
            path: "project/path",
            source: "project source",
            scope: ScopeState::Project,
            origin: OriginState::TopLevel,
            base_dir_state: OptionalState::Present,
            base_dir: "旧🙂\r\n",
        },
        Case {
            name: "temporary package",
            path: "temporary",
            source: "temporary source",
            scope: ScopeState::Temporary,
            origin: OriginState::Package,
            base_dir_state: OptionalState::Absent,
            base_dir: "ignored",
        },
        Case {
            name: "user top level",
            path: "user",
            source: "user source",
            scope: ScopeState::User,
            origin: OriginState::TopLevel,
            base_dir_state: OptionalState::Absent,
            base_dir: "ignored",
        },
        Case {
            name: "project package",
            path: "project",
            source: "project source",
            scope: ScopeState::Project,
            origin: OriginState::Package,
            base_dir_state: OptionalState::Absent,
            base_dir: "ignored",
        },
        Case {
            name: "temporary top level",
            path: "temporary",
            source: "temporary source",
            scope: ScopeState::Temporary,
            origin: OriginState::TopLevel,
            base_dir_state: OptionalState::Present,
            base_dir: "/tmp/base",
        },
        Case {
            name: "carriage returns",
            path: "before\r\nafter",
            source: "before\rafter",
            scope: ScopeState::Absent,
            origin: OriginState::Absent,
            base_dir_state: OptionalState::Present,
            base_dir: "base\r\n",
        },
        Case {
            name: "nul and newline",
            path: "nul\0newline\n",
            source: "nul\0newline\n",
            scope: ScopeState::Project,
            origin: OriginState::TopLevel,
            base_dir_state: OptionalState::Present,
            base_dir: "base\0\n",
        },
        Case {
            name: "all explicit values",
            path: "all",
            source: "all source",
            scope: ScopeState::Temporary,
            origin: OriginState::TopLevel,
            base_dir_state: OptionalState::Present,
            base_dir: "all base",
        },
    ]
}

struct MetadataCase {
    name: &'static str,
    path: &'static str,
    source: &'static str,
    scope: &'static str,
    origin: &'static str,
    base_dir_state: OptionalState,
    base_dir: &'static str,
}

pub(crate) fn compare_create_source_info() {
    let cases = metadata_cases();
    let mut oracle = oracle::TsOracle::spawn("source-info-oracle.ts", "packages/coding-agent/src/core/source-info.ts");
    let expected = oracle.request(&json!({
        "id": REQUEST_ID + 1,
        "operation": "create_source_info",
        "metadataCases": cases.iter().map(metadata_case_request).collect::<Vec<_>>(),
    }));
    let results = cases.iter().map(normalize_metadata_case).collect::<Vec<_>>();
    let actual = json!({
        "id": REQUEST_ID + 1,
        "ok": true,
        "output": {"results": results},
    });
    assert_eq!(actual, expected, "createSourceInfo mismatch");
}

fn metadata_case_request(case: &MetadataCase) -> Value {
    json!({
        "name": case.name,
        "path": case.path,
        "source": case.source,
        "scope": case.scope,
        "origin": case.origin,
        "baseDirState": case.base_dir_state.as_str(),
        "baseDir": case.base_dir,
    })
}

fn normalize_metadata_case(case: &MetadataCase) -> Value {
    let metadata = pi_coding_agent::core::package_manager::PathMetadata {
        source: case.source.to_owned().into(),
        scope: match case.scope {
            "user" => pi_coding_agent::core::package_manager::SourceScope::User,
            "project" => pi_coding_agent::core::package_manager::SourceScope::Project,
            "temporary" => pi_coding_agent::core::package_manager::SourceScope::Temporary,
            other => panic!("unknown scope {other}"),
        },
        origin: match case.origin {
            "package" => pi_coding_agent::core::package_manager::MetadataOrigin::Package,
            "top-level" => pi_coding_agent::core::package_manager::MetadataOrigin::TopLevel,
            other => panic!("unknown origin {other}"),
        },
        base_dir: (optional_string(case.base_dir_state, case.base_dir)).map(JsString::from),
    };
    let result = pi_coding_agent::core::source_info::create_source_info(&JsString::from(case.path), &metadata);
    json!({
        "name": case.name,
        "result": source_info_value(&result),
        "metadataAfter": metadata_value(&metadata),
    })
}

fn metadata_value(metadata: &pi_coding_agent::core::package_manager::PathMetadata) -> Value {
    let mut value = Map::new();
    value.insert("source".to_owned(), json!(metadata.source));
    value.insert("scope".to_owned(), json!(metadata.scope));
    value.insert("origin".to_owned(), json!(metadata.origin));
    if let Some(base_dir) = &metadata.base_dir {
        value.insert("baseDir".to_owned(), json!(base_dir));
    }
    Value::Object(value)
}

fn metadata_cases() -> Vec<MetadataCase> {
    vec![
        MetadataCase {
            name: "npm user package",
            path: "/agent/npm/node_modules/pkg/skills/a/SKILL.md",
            source: "npm:pkg",
            scope: "user",
            origin: "package",
            base_dir_state: OptionalState::Absent,
            base_dir: "ignored",
        },
        MetadataCase {
            name: "git project package",
            path: "/repo/.pi/git/pkg/SKILL.md",
            source: "git:github.com/acme/pkg",
            scope: "project",
            origin: "package",
            base_dir_state: OptionalState::Present,
            base_dir: "/repo/.pi/git/pkg",
        },
        MetadataCase {
            name: "local temporary",
            path: "/tmp/extension.md",
            source: "local",
            scope: "temporary",
            origin: "top-level",
            base_dir_state: OptionalState::Present,
            base_dir: "",
        },
        MetadataCase {
            name: "empty source and unicode",
            path: "路径🙂\0\n",
            source: "",
            scope: "temporary",
            origin: "top-level",
            base_dir_state: OptionalState::Present,
            base_dir: "旧🙂\r\n",
        },
        MetadataCase {
            name: "cli temporary",
            path: "cli.ts",
            source: "cli",
            scope: "temporary",
            origin: "top-level",
            base_dir_state: OptionalState::Absent,
            base_dir: "ignored",
        },
    ]
}
