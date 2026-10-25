use crate::shared_oracle as oracle;

use std::fs;
use std::path::Path;

use pi_coding_agent::core::tools::path_utils::{
    expand_path, path_exists_sync, resolve_read_path, resolve_read_path_async, resolve_to_cwd,
};
use pi_coding_agent::utils::paths::PathError;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use tokio::runtime::Runtime;

const FILES: [&str; 6] = [
    "test-file.txt",
    "file\u{0065}\u{0301}.txt",
    "Capture d\u{2019}cran.txt",
    "Capture d\u{2019}e\u{0301}cran.txt",
    "Screenshot 2024-01-01 at 10.00.00\u{202F}AM.png",
    "Screenshot 2024-01-01 at 10.00.00\u{202F}am.png",
];

#[test]
fn coding_agent_path_utils_matches_the_pinned_typescript_source() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().to_str().expect("utf-8 test path").to_owned();
    for file in FILES {
        fs::write(temp.path().join(file), "content").expect("fixture file");
    }

    let cases = cases(&cwd);
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-path-utils-oracle.ts",
        "packages/coding-agent/src/core/tools/path-utils.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_002,
        "operation": "coding_agent_path_utils",
        "cases": cases.iter().map(|case| case.request(&cwd)).collect::<Vec<_>>(),
    }));
    let runtime = Runtime::new().expect("tokio runtime");
    let actual = json!({"id": 27_002, "ok": true, "output": cases.iter().map(|case| case.actual(&cwd, &runtime)).collect::<Vec<_>>()});

    assert_eq!(expected["ok"], Value::Bool(true), "{expected:?}");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    let actual_results = actual["output"].as_array().expect("actual output array");
    assert_eq!(expected_results.len(), cases.len());
    for (index, (actual, expected)) in actual_results.iter().zip(expected_results).enumerate() {
        assert_eq!(actual, expected, "case {index}");
    }
}

enum Case {
    Expand(String),
    ToCwd(String, String),
    Read(String),
    ReadAsync(String),
}

impl Case {
    fn request(&self, cwd: &str) -> Value {
        match self {
            Self::Expand(path) => json!({"kind": "expand", "path": path}),
            Self::ToCwd(path, base) => json!({"kind": "toCwd", "path": path, "cwd": base}),
            Self::Read(path) => json!({"kind": "read", "path": path, "cwd": cwd}),
            Self::ReadAsync(path) => json!({"kind": "readAsync", "path": path, "cwd": cwd}),
        }
    }

    fn actual(&self, cwd: &str, runtime: &Runtime) -> Value {
        match self {
            Self::Expand(path) => resolve(expand_path(path)),
            Self::ToCwd(path, base) => resolve(resolve_to_cwd(path, base)),
            Self::Read(path) => resolve(resolve_read_path(path, cwd)),
            Self::ReadAsync(path) => resolve(runtime.block_on(resolve_read_path_async(path, cwd))),
        }
    }
}

fn resolve(result: Result<JsString, PathError>) -> Value {
    match result {
        Ok(value) => json!({"ok": true, "value": value}),
        Err(error) => json!({"ok": false, "error": pi_js_compat::js_format!("Error: {}", error.message())}),
    }
}

fn cases(cwd: &str) -> Vec<Case> {
    let mut cases = vec![
        Case::Expand("~".to_owned()),
        Case::Expand("~/Documents/file.txt".to_owned()),
        Case::Expand("~draft.md".to_owned()),
        Case::Expand("@~draft.md".to_owned()),
        Case::Expand("file\u{00A0}name.txt".to_owned()),
        Case::Expand("  padded.txt".to_owned()),
        Case::ToCwd("relative/file.txt".to_owned(), "/some/cwd".to_owned()),
        Case::ToCwd("/absolute/file.txt".to_owned(), "/some/cwd".to_owned()),
        Case::ToCwd("~draft.md".to_owned(), cwd.to_owned()),
        Case::ToCwd("@~draft.md".to_owned(), cwd.to_owned()),
        Case::ToCwd("a/../b/./c.txt".to_owned(), cwd.to_owned()),
        Case::ToCwd("../outside.txt".to_owned(), cwd.to_owned()),
        Case::ToCwd("dir\u{00A0}name/x".to_owned(), cwd.to_owned()),
    ];
    for file in FILES {
        cases.push(Case::Read(file.to_owned()));
        cases.push(Case::ReadAsync(file.to_owned()));
    }
    cases.extend([
        Case::Read("file\u{00e9}.txt".to_owned()),
        Case::ReadAsync("file\u{00e9}.txt".to_owned()),
        Case::Read("Capture d'cran.txt".to_owned()),
        Case::ReadAsync("Capture d'cran.txt".to_owned()),
        Case::Read("Capture d'\u{00e9}cran.txt".to_owned()),
        Case::ReadAsync("Capture d'\u{00e9}cran.txt".to_owned()),
        Case::Read("Screenshot 2024-01-01 at 10.00.00 AM.png".to_owned()),
        Case::ReadAsync("Screenshot 2024-01-01 at 10.00.00 am.png".to_owned()),
        Case::Read("missing 1 PM.txt".to_owned()),
        Case::ReadAsync("missing\u{00A0}file.txt".to_owned()),
        Case::Read("missing-capture d'\u{00e9}.txt".to_owned()),
        Case::Read("~/missing.txt".to_owned()),
        Case::Read("@relative-missing.txt".to_owned()),
    ]);
    assert!(path_exists_sync(&JsString::from(
        Path::new(cwd).join(FILES[0]).to_str().expect("utf-8 fixture path")
    )));
    cases
}
