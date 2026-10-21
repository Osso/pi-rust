#[path = "support/oracle.rs"]
mod oracle;

#[path = "support/required_fd.rs"]
mod required_fd;

#[test]
fn unusable_fd_fails_instead_of_skipping() {
    required_fd::assert_unusable_fd_fails("autocomplete_matches_the_pinned_typescript_source");
}

use std::fs;
use std::path::Path;

use pi_tui::autocomplete::{
    AutocompleteItem, CombinedAutocompleteProvider, CommandEntry, SlashCommand, SuggestionsOptions,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const SOURCE_DIRECTORY: &str = "packages/tui/src";

fn find_fd() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join("fd");
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

fn setup_folder(base: &Path, dirs: &[&str], files: &[(&str, &str)]) {
    for directory in dirs {
        fs::create_dir_all(base.join(directory)).expect("create fixture directory");
    }
    for (file, contents) in files {
        let path = base.join(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture parent");
        }
        fs::write(path, contents).expect("write fixture file");
    }
}

fn build_fixture(base: &Path) {
    setup_folder(
        base,
        &["src", "packages/tui/src", "packages/ai/src", ".pi", ".github", ".git"],
        &[
            ("README.md", "readme"),
            ("file.txt", "content"),
            ("apple.txt", "apple"),
            ("Zebra.txt", "zebra"),
            ("update.sh", "#!/bin/bash"),
            ("utils.ts", "export {};"),
            ("src/index.ts", "export {};"),
            ("src/components/Button.tsx", "export {};"),
            ("src/utils/helpers.ts", "export {};"),
            ("packages/tui/src/autocomplete.ts", "export {};"),
            ("packages/ai/src/autocomplete.ts", "export {};"),
            ("my folder/test.txt", "content"),
            ("my folder/other.txt", "content"),
            (".pi/config.json", "{}"),
            (".github/workflows/ci.yml", "name: ci"),
            (".git/config", "[core]"),
        ],
    );
}

fn rust_commands() -> Vec<CommandEntry> {
    let mut model = SlashCommand::new("model");
    model.description = Some("Select a model".to_string());
    let mut cd = SlashCommand::new("cd");
    cd.argument_hint = Some("<dir>".to_string());
    cd.description = Some("Change directory".to_string());
    cd.get_argument_completions = Some(Box::new(|_| Some(vec![AutocompleteItem::new("src/", "src/")])));
    let mut plain = AutocompleteItem::new("help", "help");
    plain.description = Some("Show help".to_string());

    vec![
        CommandEntry::Command(model),
        CommandEntry::Command(cd),
        CommandEntry::Item(plain),
    ]
}

fn oracle_commands() -> Value {
    json!([
        {
            "name": "model",
            "description": "Select a model",
        },
        {
            "name": "cd",
            "description": "Change directory",
            "argumentHint": "<dir>",
            "argumentCompletions": [{ "value": "src/", "label": "src/" }],
        },
        {
            "value": "help",
            "label": "help",
            "description": "Show help",
        }
    ])
}

struct SuggestionCase {
    line: &'static str,
    force: bool,
}

const SUGGESTION_CASES: &[SuggestionCase] = &[
    SuggestionCase {
        line: "@",
        force: false,
    },
    SuggestionCase {
        line: "@re",
        force: false,
    },
    SuggestionCase {
        line: "@src",
        force: false,
    },
    SuggestionCase {
        line: "@file.txt",
        force: false,
    },
    SuggestionCase {
        line: "@apple",
        force: false,
    },
    SuggestionCase {
        line: "@zeb",
        force: false,
    },
    SuggestionCase {
        line: "@tui/src/auto",
        force: false,
    },
    SuggestionCase {
        line: "@../outside/a",
        force: false,
    },
    SuggestionCase {
        line: "@symlinked",
        force: false,
    },
    SuggestionCase {
        line: "@link",
        force: false,
    },
    SuggestionCase {
        line: "@components/",
        force: false,
    },
    SuggestionCase {
        line: "@.",
        force: false,
    },
    SuggestionCase {
        line: "./up",
        force: true,
    },
    SuggestionCase {
        line: "./sr",
        force: true,
    },
    SuggestionCase {
        line: "my",
        force: true,
    },
    SuggestionCase {
        line: "hey /",
        force: true,
    },
    SuggestionCase {
        line: "/A",
        force: true,
    },
    SuggestionCase {
        line: "/model",
        force: false,
    },
    SuggestionCase {
        line: "/mod",
        force: false,
    },
    SuggestionCase {
        line: "/mod",
        force: true,
    },
    SuggestionCase {
        line: "/model arg",
        force: false,
    },
    SuggestionCase {
        line: "/model arg",
        force: true,
    },
    SuggestionCase {
        line: "/cd s",
        force: false,
    },
    SuggestionCase {
        line: "/cd s",
        force: true,
    },
    SuggestionCase {
        line: "/cd x",
        force: true,
    },
    SuggestionCase {
        line: "/command /",
        force: true,
    },
    SuggestionCase { line: "", force: true },
    SuggestionCase { line: " ", force: true },
];

#[test]
fn autocomplete_matches_the_pinned_typescript_source() {
    let fd = required_fd::require_fd(find_fd());

    let root = TempDir::new().expect("temp dir");
    let base = root.path().join("cwd");
    let outside = root.path().join("outside");
    setup_folder(&base, &[], &[]);
    build_fixture(&base);
    setup_folder(
        &outside,
        &[],
        &[
            ("nested/alpha.ts", "export {};"),
            ("nested/deeper/also-alpha.ts", "export {};"),
            ("nested/deeper/zzz.ts", "export {};"),
        ],
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink("../outside", base.join("symlinked_dir")).expect("symlink directory");
        symlink("file.txt", base.join("link.txt")).expect("symlink file");
    }

    let cases: Vec<Value> = SUGGESTION_CASES
        .iter()
        .map(|case| {
            json!({
                "lines": [case.line],
                "cursorLine": 0,
                "cursorCol": case.line.len(),
                "force": case.force,
            })
        })
        .collect();

    let mut oracle = oracle::TsOracle::spawn("autocomplete-oracle.ts", SOURCE_DIRECTORY);
    let response = oracle.request(&json!({
        "id": "autocomplete",
        "operation": "autocomplete",
        "basePath": base.to_str().unwrap(),
        "fdPath": fd,
        "commands": oracle_commands(),
        "cases": cases,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let results = response["output"]["results"].as_array().expect("results array");
    assert_eq!(results.len(), SUGGESTION_CASES.len());

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let provider = CombinedAutocompleteProvider::new(rust_commands(), base.to_str().unwrap(), Some(fd));

    for (case, expected) in SUGGESTION_CASES.iter().zip(results) {
        let lines = vec![case.line.to_string()];
        let actual = runtime.block_on(provider.get_suggestions(
            &lines,
            0,
            case.line.len(),
            SuggestionsOptions {
                signal: None,
                force: case.force,
            },
        ));
        let actual = actual.map(|suggestions| {
            json!({
                "prefix": suggestions.prefix,
                "items": suggestions.items.iter().map(|item| json!({
                    "value": item.value,
                    "label": item.label,
                    "description": item.description,
                })).collect::<Vec<_>>(),
            })
        });
        let expected = if expected.is_null() {
            None
        } else {
            Some(json!({
                "prefix": expected["prefix"],
                "items": expected["items"].as_array().expect("items").iter().map(|item| json!({
                    "value": item["value"],
                    "label": item["label"],
                    "description": item["description"],
                })).collect::<Vec<_>>(),
            }))
        };
        assert_eq!(
            actual, expected,
            "suggestion mismatch for {:?} (force={})",
            case.line, case.force
        );
    }
}

#[test]
fn apply_completion_matches_the_pinned_typescript_source() {
    let root = TempDir::new().expect("temp dir");
    let base = root.path().join("cwd");
    setup_folder(&base, &[], &[]);

    struct ApplyCase {
        line: &'static str,
        cursor: usize,
        value: &'static str,
        label: &'static str,
        prefix: &'static str,
    }

    let cases = [
        ApplyCase {
            line: "@src",
            cursor: 4,
            value: "@src/",
            label: "src/",
            prefix: "@src",
        },
        ApplyCase {
            line: "@src",
            cursor: 4,
            value: "@src.txt",
            label: "src.txt",
            prefix: "@src",
        },
        ApplyCase {
            line: "@\"my folder/te\"",
            cursor: 15,
            value: "@\"my folder/test.txt\"",
            label: "test.txt",
            prefix: "@\"my folder/te",
        },
        ApplyCase {
            line: "\"my folder/te\"",
            cursor: 13,
            value: "\"my folder/test.txt\"",
            label: "test.txt",
            prefix: "\"my folder/te",
        },
        ApplyCase {
            line: "/mod",
            cursor: 4,
            value: "model",
            label: "model",
            prefix: "/mod",
        },
        ApplyCase {
            line: "/cd s",
            cursor: 5,
            value: "src/",
            label: "src/",
            prefix: "s",
        },
        ApplyCase {
            line: "src",
            cursor: 3,
            value: "src/",
            label: "src/",
            prefix: "src",
        },
    ];

    let requests: Vec<Value> = cases
        .iter()
        .map(|case| {
            json!({
                "lines": [case.line],
                "cursorLine": 0,
                "cursorCol": case.cursor,
                "item": { "value": case.value, "label": case.label },
                "prefix": case.prefix,
            })
        })
        .collect();

    let mut oracle = oracle::TsOracle::spawn("autocomplete-oracle.ts", SOURCE_DIRECTORY);
    let response = oracle.request(&json!({
        "id": "autocomplete-apply",
        "operation": "autocomplete_apply",
        "basePath": base.to_str().unwrap(),
        "cases": requests,
    }));
    assert_eq!(response["ok"], true, "oracle failed: {response}");

    let results = response["output"]["results"].as_array().expect("results array");
    assert_eq!(results.len(), cases.len());

    let provider = CombinedAutocompleteProvider::new(Vec::new(), base.to_str().unwrap(), None);
    for (case, expected) in cases.iter().zip(results) {
        let item = AutocompleteItem::new(case.value, case.label);
        let actual = provider.apply_completion(&[case.line.to_string()], 0, case.cursor, &item, case.prefix);
        assert_eq!(
            actual.lines,
            expected["lines"]
                .as_array()
                .expect("lines")
                .iter()
                .map(|line| line.as_str().expect("line").to_string())
                .collect::<Vec<_>>(),
            "line mismatch for {:?}",
            case.line
        );
        assert_eq!(
            actual.cursor_line,
            expected["cursorLine"].as_u64().expect("cursorLine") as usize
        );
        assert_eq!(
            actual.cursor_col,
            expected["cursorCol"].as_u64().expect("cursorCol") as usize,
            "cursor mismatch for {:?}",
            case.line
        );
    }
}
