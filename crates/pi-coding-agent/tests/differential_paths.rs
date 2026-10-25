#![cfg(unix)]

use crate::shared_oracle as oracle;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use pi_coding_agent::utils::paths::{
    PathInputOptions, canonicalize_path, format_path_relative_to_cwd_or_absolute, get_cwd_relative_path, is_local_path,
    normalize_path, resolve_path,
};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};
use tempfile::TempDir;

const REQUEST_ID: u64 = 26_009;
const HOME: &str = "/home/example";

#[derive(Clone, Default)]
struct Options {
    trim: bool,
    expand_tilde: Option<bool>,
    home_dir: Option<&'static str>,
    strip_at_prefix: bool,
    normalize_unicode_spaces: bool,
}

impl Options {
    fn home() -> Self {
        Self {
            home_dir: Some(HOME),
            ..Self::default()
        }
    }

    fn to_json(&self) -> Value {
        let mut value = serde_json::Map::new();
        if self.trim {
            value.insert("trim".to_owned(), Value::Bool(true));
        }
        if let Some(expand_tilde) = self.expand_tilde {
            value.insert("expandTilde".to_owned(), Value::Bool(expand_tilde));
        }
        if let Some(home_dir) = self.home_dir {
            value.insert("homeDir".to_owned(), Value::String(home_dir.to_owned()));
        }
        if self.strip_at_prefix {
            value.insert("stripAtPrefix".to_owned(), Value::Bool(true));
        }
        if self.normalize_unicode_spaces {
            value.insert("normalizeUnicodeSpaces".to_owned(), Value::Bool(true));
        }
        Value::Object(value)
    }

    fn to_path_options(&self) -> PathInputOptions {
        PathInputOptions {
            trim: self.trim,
            expand_tilde: self.expand_tilde,
            home_dir: self.home_dir.map(JsString::from),
            strip_at_prefix: self.strip_at_prefix,
            normalize_unicode_spaces: self.normalize_unicode_spaces,
        }
    }
}

enum Case {
    IsLocal(&'static str),
    Normalize(&'static str, Options),
    Resolve(&'static str, &'static str, Options),
    Relative(&'static str, &'static str),
    FormatRelative(&'static str, &'static str),
    Canonicalize(PathBuf),
}

fn cases() -> Vec<Case> {
    let mut cases = vec![
        Case::IsLocal("my-package"),
        Case::IsLocal("./foo"),
        Case::IsLocal("file:///tmp/foo"),
        Case::IsLocal("/tmp/foo"),
        Case::IsLocal("  spaced  "),
        Case::IsLocal("npm:package"),
        Case::IsLocal("git://repo"),
        Case::IsLocal("github:user/repo"),
        Case::IsLocal("http://example.com"),
        Case::IsLocal("https://example.com"),
        Case::IsLocal("ssh://host/repo"),
        Case::IsLocal("FILE:///tmp"),
        Case::IsLocal("npm"),
        Case::Normalize("~", Options::home()),
        Case::Normalize("~/file.txt", Options::home()),
        Case::Normalize("~draft.md", Options::home()),
        Case::Normalize("~//x", Options::home()),
        Case::Normalize(
            "  /tmp/file  ",
            Options {
                trim: true,
                ..Options::default()
            },
        ),
        Case::Normalize(
            "@/tmp/file",
            Options {
                strip_at_prefix: true,
                ..Options::default()
            },
        ),
        Case::Normalize("@file", Options::default()),
        Case::Normalize(
            "a\u{a0}b\u{2000}c\u{202f}d\u{205f}e\u{3000}f",
            Options {
                normalize_unicode_spaces: true,
                ..Options::default()
            },
        ),
        Case::Normalize("file:///tmp/file%20with%20spaces.txt", Options::default()),
        Case::Normalize("file:///tmp/report%252026.md", Options::default()),
        Case::Normalize("file://localhost/tmp/x", Options::default()),
        Case::Normalize("file:///%E0%A4%A", Options::default()),
        Case::Normalize("file://server/share", Options::default()),
        Case::Normalize("/tmp/a/../b", Options::default()),
        Case::Normalize(
            "~",
            Options {
                expand_tilde: Some(false),
                ..Options::home()
            },
        ),
        Case::Normalize("", Options::default()),
        Case::Normalize(
            "   ",
            Options {
                trim: true,
                ..Options::default()
            },
        ),
        Case::Normalize("file:///tmp/a%2Fb", Options::default()),
        Case::Resolve("subdir/file.txt", "/tmp/pi-paths-cwd", Options::default()),
        Case::Resolve("/tmp/a/../b/./c", "/tmp/base", Options::default()),
        Case::Resolve("/tmp/report%2026.md", "/tmp/base", Options::default()),
        Case::Resolve("file:///tmp/pi-paths-cwd", "/tmp/base", Options::default()),
        Case::Resolve("", "/tmp/base", Options::default()),
        Case::Resolve(".", "/tmp/base", Options::default()),
        Case::Resolve("../x", "/tmp/base", Options::default()),
        Case::Resolve("a//b", "/tmp/base", Options::default()),
        Case::Resolve("/tmp/base/", "/tmp/base", Options::default()),
        Case::Resolve("file:///%E0%A4%A", "/tmp/base", Options::default()),
        Case::Resolve("file://server/share", "/tmp/base", Options::default()),
        Case::Resolve("~/x", "/tmp/base", Options::home()),
        Case::Resolve(
            "@x",
            "/tmp/base",
            Options {
                strip_at_prefix: true,
                ..Options::default()
            },
        ),
        Case::Relative("/tmp/pi-paths-cwd/..config/AGENTS.md", "/tmp/pi-paths-cwd"),
        Case::Relative("/tmp/pi-paths-cwd", "/tmp/pi-paths-cwd"),
        Case::Relative("/tmp/pi-paths-cwd/sub/file", "/tmp/pi-paths-cwd"),
        Case::Relative("/tmp/AGENTS.md", "/tmp/pi-paths-cwd"),
        Case::Relative("/tmp/pi-paths-cwd/../AGENTS.md", "/tmp/pi-paths-cwd"),
        Case::Relative("/tmp/pi-paths-cwd/a/b", "/tmp/pi-paths-cwd/a"),
        Case::Relative("/tmp/pi-paths-cwd/a", "/tmp/pi-paths-cwd/a/b"),
        Case::Relative("/tmp/pi-paths-cwd/..config", "/tmp/pi-paths-cwd"),
        Case::FormatRelative("sub/file", "/tmp/pi-paths-cwd"),
        Case::FormatRelative("/tmp/other/file", "/tmp/pi-paths-cwd"),
        Case::FormatRelative("/tmp/pi-paths-cwd", "/tmp/pi-paths-cwd"),
    ];

    let dir = TempDir::new().expect("temp dir");
    let file = dir.path().join("file.txt");
    fs::write(&file, "hello").expect("write file");
    let link = dir.path().join("link.txt");
    symlink(&file, &link).expect("symlink");
    let dangling = dir.path().join("dangling.txt");
    symlink(dir.path().join("missing-target"), &dangling).expect("dangling symlink");
    let missing = dir.path().join("missing.txt");

    cases.push(Case::Canonicalize(file.clone()));
    cases.push(Case::Canonicalize(dir.path().to_path_buf()));
    cases.push(Case::Canonicalize(link));
    cases.push(Case::Canonicalize(dangling));
    cases.push(Case::Canonicalize(missing));

    cases
}

#[test]
fn coding_agent_paths_matches_the_pinned_typescript_source() {
    let cases = cases();
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-paths-oracle.ts",
        "packages/coding-agent/src/utils/paths.ts",
    );
    let expected = oracle.request(&request(&cases));
    let actual = response(&cases);

    compare_results(&actual, &expected, "coding-agent paths");
}

fn compare_results(actual: &Value, expected: &Value, label: &str) {
    let actual_results = actual["output"]["results"].as_array().expect("actual results");
    let expected_results = expected["output"]["results"].as_array().expect("expected results");
    assert_eq!(actual_results.len(), expected_results.len(), "{label} result count");
    for (actual_case, expected_case) in actual_results.iter().zip(expected_results) {
        assert_eq!(
            actual_case, expected_case,
            "{label} mismatch: {}",
            expected_case["name"]
        );
    }
}

fn request(cases: &[Case]) -> Value {
    let cases = cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            let name = format!("case {index}");
            match case {
                Case::IsLocal(input) => json!({"kind": "is_local", "name": name, "input": input}),
                Case::Normalize(input, options) => {
                    json!({"kind": "normalize", "name": name, "input": input, "options": options.to_json()})
                }
                Case::Resolve(input, base, options) => json!({
                    "kind": "resolve",
                    "name": name,
                    "input": input,
                    "baseDir": base,
                    "options": options.to_json(),
                }),
                Case::Relative(file_path, cwd) => {
                    json!({"kind": "relative", "name": name, "filePath": file_path, "cwd": cwd})
                }
                Case::FormatRelative(file_path, cwd) => {
                    json!({"kind": "format_relative", "name": name, "filePath": file_path, "cwd": cwd})
                }
                Case::Canonicalize(path) => {
                    json!({"kind": "canonicalize", "name": name, "path": path.to_string_lossy()})
                }
            }
        })
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "operation": "coding_agent_paths", "cases": cases})
}

/// Every fixture in this matrix is scalar; raw-unit cases live in `lossless_paths`.
fn scalar(value: JsString) -> Value {
    Value::String(value.to_utf8().expect("scalar path fixture"))
}

fn response(cases: &[Case]) -> Value {
    let results = cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            let name = format!("case {index}");
            let value = match case {
                Case::IsLocal(input) => Ok(json!(is_local_path(*input))),
                Case::Normalize(input, options) => normalize_path(*input, &options.to_path_options()).map(scalar),
                Case::Resolve(input, base, options) => {
                    resolve_path(*input, *base, &options.to_path_options()).map(scalar)
                }
                Case::Relative(file_path, cwd) => {
                    get_cwd_relative_path(*file_path, *cwd).map(|value| value.map_or(Value::Null, scalar))
                }
                Case::FormatRelative(file_path, cwd) => {
                    format_path_relative_to_cwd_or_absolute(*file_path, *cwd).map(scalar)
                }
                Case::Canonicalize(path) => Ok(scalar(canonicalize_path(path.to_str().expect("scalar fixture path")))),
            };
            match value {
                Ok(value) => json!({"name": name, "ok": true, "value": value}),
                Err(_) => json!({"name": name, "ok": false}),
            }
        })
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "ok": true, "output": {"results": results}})
}
