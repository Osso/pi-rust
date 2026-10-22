#[path = "support/required_fd.rs"]
mod required_fd;

#[test]
fn unusable_fd_fails_instead_of_skipping() {
    required_fd::assert_unusable_fd_fails("fuzzy_empty_query_returns_files_and_folders");
}

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::Path;

use pi_tui::autocomplete::{
    AutocompleteItem, CombinedAutocompleteProvider, CommandEntry, SlashCommand, SuggestionsOptions,
};
use tempfile::TempDir;

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

fn item(value: &str) -> AutocompleteItem {
    AutocompleteItem::new(value, value)
}

fn command(name: &str) -> SlashCommand {
    SlashCommand::new(name)
}

fn provider(commands: Vec<CommandEntry>, base_path: &str, fd_path: Option<String>) -> CombinedAutocompleteProvider {
    CombinedAutocompleteProvider::new(commands, base_path, fd_path.map(Into::into))
}

async fn suggestions(
    provider: &CombinedAutocompleteProvider,
    lines: &[&str],
    cursor_line: usize,
    cursor_col: usize,
    force: bool,
) -> Option<pi_tui::autocomplete::AutocompleteSuggestions> {
    let lines: Vec<JsString> = lines.iter().map(|line| JsString::from(*line)).collect();
    provider
        .get_suggestions(
            &lines,
            cursor_line,
            cursor_col,
            SuggestionsOptions { signal: None, force },
        )
        .await
}

#[tokio::test]
async fn forced_extraction_returns_root_prefix() {
    let provider = provider(Vec::new(), "/tmp", None);
    let result = suggestions(&provider, &["hey /"], 0, 5, true).await;
    let result = result.expect("root suggestions");
    assert_eq!(result.prefix, "/");
}

#[tokio::test]
async fn forced_absolute_path_keeps_prefix() {
    let provider = provider(Vec::new(), "/tmp", None);
    if let Some(result) = suggestions(&provider, &["/A"], 0, 2, true).await {
        assert_eq!(result.prefix, "/A");
    }
}

#[tokio::test]
async fn slash_command_names_do_not_trigger_file_completion() {
    let provider = provider(Vec::new(), "/tmp", None);
    assert!(suggestions(&provider, &["/model"], 0, 6, true).await.is_none());
}

#[tokio::test]
async fn slash_command_argument_completions_are_used_when_forced() {
    let mut cd = command("cd");
    cd.get_argument_completions = Some(Box::new(|_| Some(vec![item("src/")])));
    let provider = provider(vec![CommandEntry::Command(cd)], "/tmp", None);
    let line = "/cd s";
    let result = suggestions(&provider, &[line], 0, line.len(), true)
        .await
        .expect("argument suggestions");
    assert_eq!(result.prefix, "s");
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].value, "src/");
}

#[tokio::test]
async fn empty_argument_completions_do_not_fall_back_to_files() {
    let base = TempDir::new().expect("temp dir");
    setup_folder(base.path(), &[], &[("src.txt", "not a directory")]);
    let mut cd = command("cd");
    cd.get_argument_completions = Some(Box::new(|_| Some(Vec::new())));
    let provider = provider(vec![CommandEntry::Command(cd)], base.path().to_str().unwrap(), None);
    let line = "/cd s";
    assert!(suggestions(&provider, &[line], 0, line.len(), true).await.is_none());
}

#[tokio::test]
async fn absolute_paths_trigger_after_slash_command_arguments() {
    let provider = provider(Vec::new(), "/tmp", None);
    let result = suggestions(&provider, &["/command /"], 0, 10, true)
        .await
        .expect("absolute path suggestions");
    assert_eq!(result.prefix, "/");
}

#[tokio::test]
async fn fuzzy_empty_query_returns_files_and_folders() {
    let fd = required_fd::require_fd(find_fd());
    let base = TempDir::new().expect("temp dir");
    setup_folder(base.path(), &["src"], &[("README.md", "readme")]);
    let provider = provider(Vec::new(), base.path().to_str().unwrap(), Some(fd));
    let result = suggestions(&provider, &["@"], 0, 1, false)
        .await
        .expect("@ suggestions");
    let mut values: Vec<JsString> = result.items.iter().map(|item| item.value.clone()).collect();
    values.sort_by(|a, b| a.units().cmp(b.units()));
    assert_eq!(values, vec!["@README.md", "@src/"]);
}

#[tokio::test]
async fn fuzzy_matches_extension_case_insensitively() {
    let fd = required_fd::require_fd(find_fd());
    let base = TempDir::new().expect("temp dir");
    setup_folder(base.path(), &[], &[("file.txt", "content"), ("README.md", "readme")]);
    let provider = provider(Vec::new(), base.path().to_str().unwrap(), Some(fd));
    let result = suggestions(&provider, &["@file.txt"], 0, 9, false)
        .await
        .expect("file suggestions");
    assert!(result.items.iter().any(|item| item.value == "@file.txt"));

    let result = suggestions(&provider, &["@re"], 0, 3, false)
        .await
        .expect("case-insensitive suggestions");
    assert_eq!(
        result.items.iter().map(|item| item.value.clone()).collect::<Vec<_>>(),
        vec!["@README.md"]
    );
}

#[tokio::test]
async fn fuzzy_ranks_directories_before_files() {
    let fd = required_fd::require_fd(find_fd());
    let base = TempDir::new().expect("temp dir");
    setup_folder(
        base.path(),
        &["src"],
        &[("src.txt", "text"), ("src/index.ts", "export {};")],
    );
    let provider = provider(Vec::new(), base.path().to_str().unwrap(), Some(fd));
    let result = suggestions(&provider, &["@src"], 0, 4, false)
        .await
        .expect("directory ranking");
    assert_eq!(result.items[0].value, "@src/");
    assert!(result.items.iter().any(|item| item.value == "@src.txt"));
}

#[tokio::test]
async fn fuzzy_returns_nested_paths_and_full_path_matches() {
    let fd = required_fd::require_fd(find_fd());
    let base = TempDir::new().expect("temp dir");
    setup_folder(
        base.path(),
        &[],
        &[
            ("src/index.ts", "export {};"),
            ("packages/tui/src/autocomplete.ts", "export {};"),
            ("packages/ai/src/autocomplete.ts", "export {};"),
            ("src/components/Button.tsx", "export {};"),
            ("src/utils/helpers.ts", "export {};"),
        ],
    );
    let provider = provider(Vec::new(), base.path().to_str().unwrap(), Some(fd));

    let result = suggestions(&provider, &["@index"], 0, 6, false)
        .await
        .expect("nested path");
    assert!(result.items.iter().any(|item| item.value == "@src/index.ts"));

    let result = suggestions(&provider, &["@tui/src/auto"], 0, 13, false)
        .await
        .expect("deep path");
    let values: Vec<JsString> = result.items.iter().map(|item| item.value.clone()).collect();
    assert!(values.contains(&JsString::from("@packages/tui/src/autocomplete.ts")));
    assert!(!values.contains(&JsString::from("@packages/ai/src/autocomplete.ts")));

    let result = suggestions(&provider, &["@components/"], 0, 12, false)
        .await
        .expect("full path match");
    let values: Vec<JsString> = result.items.iter().map(|item| item.value.clone()).collect();
    assert!(values.contains(&JsString::from("@src/components/Button.tsx")));
    assert!(!values.contains(&JsString::from("@src/utils/helpers.ts")));
}

#[tokio::test]
async fn fuzzy_scopes_relative_directories_recursively() {
    let fd = required_fd::require_fd(find_fd());
    let root = TempDir::new().expect("temp dir");
    let base = root.path().join("cwd");
    let outside = root.path().join("outside");
    fs::create_dir_all(&base).expect("cwd");
    setup_folder(
        &outside,
        &[],
        &[
            ("nested/alpha.ts", "export {};"),
            ("nested/deeper/also-alpha.ts", "export {};"),
            ("nested/deeper/zzz.ts", "export {};"),
        ],
    );
    let provider = provider(Vec::new(), base.to_str().unwrap(), Some(fd));
    let result = suggestions(&provider, &["@../outside/a"], 0, 13, false)
        .await
        .expect("scoped suggestions");
    let values: Vec<JsString> = result.items.iter().map(|item| item.value.clone()).collect();
    assert!(values.contains(&JsString::from("@../outside/nested/alpha.ts")));
    assert!(values.contains(&JsString::from("@../outside/nested/deeper/also-alpha.ts")));
    assert!(!values.contains(&JsString::from("@../outside/nested/deeper/zzz.ts")));
}

#[tokio::test]
async fn fuzzy_quotes_paths_with_spaces_and_continues_quoted_paths() {
    let fd = required_fd::require_fd(find_fd());
    let base = TempDir::new().expect("temp dir");
    setup_folder(
        base.path(),
        &[],
        &[("my folder/test.txt", "content"), ("my folder/other.txt", "content")],
    );
    let provider = provider(Vec::new(), base.path().to_str().unwrap(), Some(fd));

    let result = suggestions(&provider, &["@my"], 0, 3, false)
        .await
        .expect("space quoting");
    assert!(result.items.iter().any(|item| item.value == "@\"my folder/\""));

    let line = "@\"my folder/\"";
    let result = suggestions(&provider, &[line], 0, line.len() - 1, false)
        .await
        .expect("quoted continuation");
    let values: Vec<JsString> = result.items.iter().map(|item| item.value.clone()).collect();
    assert!(values.contains(&JsString::from("@\"my folder/test.txt\"")));
    assert!(values.contains(&JsString::from("@\"my folder/other.txt\"")));

    let line = "@\"my folder/te\"";
    let cursor = line.len() - 1;
    let result = suggestions(&provider, &[line], 0, cursor, false)
        .await
        .expect("quoted path");
    let selected = result
        .items
        .iter()
        .find(|item| item.value == "@\"my folder/test.txt\"")
        .expect("quoted item");
    let applied = provider.apply_completion(&[line.into()], 0, cursor, selected, &result.prefix);
    assert_eq!(applied.lines[0], "@\"my folder/test.txt\" ");
}

#[tokio::test]
async fn fuzzy_includes_hidden_paths_but_excludes_git() {
    let fd = required_fd::require_fd(find_fd());
    let base = TempDir::new().expect("temp dir");
    setup_folder(
        base.path(),
        &[".pi", ".github", ".git"],
        &[
            (".pi/config.json", "{}"),
            (".github/workflows/ci.yml", "name: ci"),
            (".git/config", "[core]"),
        ],
    );
    let provider = provider(Vec::new(), base.path().to_str().unwrap(), Some(fd));
    let result = suggestions(&provider, &["@"], 0, 1, false)
        .await
        .expect("hidden suggestions");
    let values: Vec<JsString> = result.items.iter().map(|item| item.value.clone()).collect();
    assert!(values.contains(&JsString::from("@.pi/")));
    assert!(values.contains(&JsString::from("@.github/")));
    assert!(
        !values
            .iter()
            .any(|value| *value == "@.git" || value.starts_with("@.git/"))
    );
}

#[cfg(unix)]
#[tokio::test]
async fn fuzzy_follows_symlinked_directories_and_files() {
    let fd = required_fd::require_fd(find_fd());
    use std::os::unix::fs::symlink;

    let root = TempDir::new().expect("temp dir");
    let base = root.path().join("cwd");
    let outside = root.path().join("outside");
    setup_folder(&base, &[], &[("dir/some_file.txt", "real")]);
    setup_folder(
        &outside,
        &[],
        &[("some_file.txt", "symlinked"), ("nested/file.txt", "symlinked")],
    );
    symlink("../outside", base.join("symlinked_dir")).expect("symlink directory");
    symlink("original.txt", base.join("link.txt")).expect("symlink file");
    fs::write(base.join("original.txt"), "content").expect("original file");

    let provider = provider(Vec::new(), base.to_str().unwrap(), Some(fd));
    let result = suggestions(&provider, &["@some"], 0, 5, false)
        .await
        .expect("symlink suggestions");
    let values: Vec<JsString> = result.items.iter().map(|item| item.value.clone()).collect();
    assert!(values.contains(&JsString::from("@dir/some_file.txt")));
    assert!(values.contains(&JsString::from("@symlinked_dir/some_file.txt")));

    let result = suggestions(&provider, &["@symlinked"], 0, 10, false)
        .await
        .expect("symlinked directory suggestion");
    assert!(result.items.iter().any(|item| item.value == "@symlinked_dir/"));

    let result = suggestions(&provider, &["@link"], 0, 5, false)
        .await
        .expect("symlinked file suggestion");
    assert!(result.items.iter().any(|item| item.value == "@link.txt"));
}

#[tokio::test]
async fn fuzzy_results_ignore_query_text_in_the_base_path() {
    let fd = required_fd::require_fd(find_fd());
    let root = TempDir::new().expect("temp dir");
    let normal = root.path().join("cwd-normal");
    let containing = root.path().join("cwd-plan-repro");
    let structure = [
        (
            "packages/coding-agent/examples/extensions/plan-mode/README.md",
            "readme",
        ),
        ("packages/tui/docs/plan.md", "plan"),
    ];
    setup_folder(
        &normal,
        &["packages/coding-agent/examples/extensions/plan-mode"],
        &structure,
    );
    setup_folder(
        &containing,
        &["packages/coding-agent/examples/extensions/plan-mode"],
        &structure,
    );

    let normal_provider = provider(Vec::new(), normal.to_str().unwrap(), Some(fd.clone()));
    let containing_provider = provider(Vec::new(), containing.to_str().unwrap(), Some(fd));
    let normal_result = suggestions(&normal_provider, &["@plan"], 0, 5, false)
        .await
        .expect("normal result");
    let containing_result = suggestions(&containing_provider, &["@plan"], 0, 5, false)
        .await
        .expect("containing result");

    let normalize = |items: &[AutocompleteItem]| {
        let mut rows: Vec<JsString> = items
            .iter()
            .map(|item| item.label.clone() + " :: " + item.description.clone().unwrap_or_default())
            .collect();
        rows.sort_by(|a, b| a.units().cmp(b.units()));
        rows
    };
    assert_eq!(normalize(&containing_result.items), normalize(&normal_result.items));
    let rows = normalize(&normal_result.items);
    assert!(rows.contains(&JsString::from(
        "plan-mode/ :: packages/coding-agent/examples/extensions/plan-mode"
    )));
    assert!(rows.contains(&JsString::from("plan.md :: packages/tui/docs/plan.md")));
}

#[tokio::test]
async fn dot_slash_completion_preserves_prefix() {
    let base = TempDir::new().expect("temp dir");
    setup_folder(
        base.path(),
        &["src"],
        &[
            ("update.sh", "#!/bin/bash"),
            ("utils.ts", "export {};"),
            ("src/index.ts", "export {};"),
        ],
    );
    let provider = provider(Vec::new(), base.path().to_str().unwrap(), None);

    let result = suggestions(&provider, &["./up"], 0, 4, true)
        .await
        .expect("./ file completion");
    assert!(result.items.iter().any(|item| item.value == "./update.sh"));

    let result = suggestions(&provider, &["./sr"], 0, 4, true)
        .await
        .expect("./ directory completion");
    assert!(result.items.iter().any(|item| item.value == "./src/"));
}

#[tokio::test]
async fn quoted_completion_quotes_paths_with_spaces() {
    let base = TempDir::new().expect("temp dir");
    setup_folder(
        base.path(),
        &[],
        &[("my folder/test.txt", "content"), ("my folder/other.txt", "content")],
    );
    let provider = provider(Vec::new(), base.path().to_str().unwrap(), None);

    let result = suggestions(&provider, &["my"], 0, 2, true)
        .await
        .expect("quoted path completion");
    assert!(result.items.iter().any(|item| item.value == "\"my folder/\""));

    let line = "\"my folder/\"";
    let result = suggestions(&provider, &[line], 0, line.len() - 1, true)
        .await
        .expect("quoted continuation");
    let values: Vec<JsString> = result.items.iter().map(|item| item.value.clone()).collect();
    assert!(values.contains(&JsString::from("\"my folder/test.txt\"")));
    assert!(values.contains(&JsString::from("\"my folder/other.txt\"")));

    let line = "\"my folder/te\"";
    let cursor = line.len() - 1;
    let result = suggestions(&provider, &[line], 0, cursor, true)
        .await
        .expect("quoted path");
    let selected = result
        .items
        .iter()
        .find(|item| item.value == "\"my folder/test.txt\"")
        .expect("quoted item");
    let applied = provider.apply_completion(&[line.into()], 0, cursor, selected, &result.prefix);
    assert_eq!(applied.lines[0], "\"my folder/test.txt\"");
}

#[test]
fn apply_completion_for_slash_commands() {
    let provider = provider(Vec::new(), "/tmp", None);
    let applied = provider.apply_completion(&[JsString::from("/mod")], 0, 4, &item("model"), &"/mod".into());
    assert_eq!(applied.lines[0], "/model ");
    assert_eq!(applied.cursor_col, 7);
}

#[test]
fn apply_completion_adds_space_after_files_but_not_directories() {
    let provider = provider(Vec::new(), "/tmp", None);
    let applied = provider.apply_completion(
        &[JsString::from("@src")],
        0,
        4,
        &AutocompleteItem::new("@src/", "src/"),
        &"@src".into(),
    );
    assert_eq!(applied.lines[0], "@src/");
    assert_eq!(applied.cursor_col, 5);

    let applied = provider.apply_completion(
        &[JsString::from("@src")],
        0,
        4,
        &AutocompleteItem::new("@src.txt", "src.txt"),
        &"@src".into(),
    );
    assert_eq!(applied.lines[0], "@src.txt ");
    assert_eq!(applied.cursor_col, 9);
}

#[test]
fn should_trigger_file_completion_rejects_slash_command_names() {
    let provider = provider(Vec::new(), "/tmp", None);
    assert!(!provider.should_trigger_file_completion(&[JsString::from("/model")], 0, 6));
    assert!(provider.should_trigger_file_completion(&[JsString::from("/model arg")], 0, 10));
    assert!(provider.should_trigger_file_completion(&[JsString::from("hello")], 0, 5));
}

#[test]
fn home_directory_expansion_is_used_for_tilde_prefixes() {
    let home = std::env::var("HOME").expect("HOME is set");
    let entry = tempfile::Builder::new()
        .prefix("pi-home-completion-")
        .tempfile_in(&home)
        .expect("HOME completion fixture");
    let expected = format!("~/{}", entry.path().file_name().unwrap().to_string_lossy());
    let provider = provider(Vec::new(), "/tmp", None);
    let suggestions = futures_lite_block_on(provider.get_suggestions(
        &[JsString::from("~/")],
        0,
        2,
        SuggestionsOptions {
            signal: None,
            force: true,
        },
    ));
    let suggestions = suggestions.expect("home suggestions");
    assert_eq!(suggestions.prefix, "~/");
    assert!(suggestions.items.iter().all(|item| {
        item.value
            .strip_prefix('"')
            .unwrap_or_else(|| item.value.clone())
            .starts_with("~/")
    }));
    assert!(suggestions.items.iter().any(|item| item.value == expected));
}

fn futures_lite_block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}
