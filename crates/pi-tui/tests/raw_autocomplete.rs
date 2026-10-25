use crate::shared_oracle as oracle;
use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::autocomplete::{AutocompleteItem, CombinedAutocompleteProvider};
use serde_json::json;

#[test]
fn completion_applies_at_a_native_cursor_inside_a_surrogate_pair() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let line = "😀tail";
    let expected=oracle.request(&json!({"operation":"autocomplete_apply","units":JsString::from(line).units(),"cursor":1,"item":JsString::from("X").units(),"prefix":[]}));
    assert_eq!(expected["ok"], true, "{expected}");
    let provider = CombinedAutocompleteProvider::new(vec![], ".", None);
    let result = provider.apply_completion(
        &[JsString::from(line)],
        0,
        1,
        &AutocompleteItem::new("X", "X"),
        &JsString::default(),
    );
    let actual = serde_json::to_string(&(result.lines, result.cursor_line, result.cursor_col)).unwrap();
    assert_eq!(
        parse_lossless(actual).unwrap(),
        parse_lossless(expected["json"].as_str().unwrap()).unwrap()
    );
}

#[test]
fn raw_completion_slices_quotes_directories_and_negative_internal_prefix_offsets_match_source() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let provider = CombinedAutocompleteProvider::new(vec![], ".", None);
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
        JsString::from_utf16(vec![97, 0xd800, 98, 0xdc00]),
    ] {
        let line = raw.clone() + "tail";
        for cursor in 0..=line.len() {
            for prefix in [JsString::default(), JsString::from("zz"), JsString::from("@")] {
                for directory in [false, true] {
                    let item = raw.clone() + if directory { "/\"" } else { "x" };
                    let label = raw.clone() + if directory { "/" } else { "x" };
                    let expected = oracle.request(&json!({"operation":"autocomplete_apply","units":line.units(),"cursor":cursor,"item":item.units(),"itemLabel":label.units(),"prefix":prefix.units()}));
                    assert_eq!(expected["ok"], true, "{expected}");
                    let result = provider.apply_completion(
                        std::slice::from_ref(&line),
                        0,
                        cursor,
                        &AutocompleteItem::new(item, label),
                        &prefix,
                    );
                    let actual = serde_json::to_string(&(result.lines, result.cursor_line, result.cursor_col)).unwrap();
                    assert_eq!(
                        parse_lossless(actual).unwrap(),
                        parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
                        "raw={raw:?} cursor={cursor} prefix={prefix:?} directory={directory}"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn command_suggestions_and_argument_callbacks_keep_native_strings() {
    use pi_tui::autocomplete::{CommandEntry, SlashCommand, SuggestionsOptions};
    use std::sync::{Arc, Mutex};
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
        JsString::from_utf16(vec![97, 0xd800, 98, 0xdc00]),
    ] {
        let expected = oracle.request(&json!({"operation":"autocomplete_commands","units":raw.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let captured = Arc::new(Mutex::new(Vec::new()));
        let arguments = captured.clone();
        let value = raw.clone();
        let command = SlashCommand {
            name: raw.clone() + "cmd",
            description: Some(raw.clone() + "description"),
            argument_hint: Some(raw.clone() + "hint"),
            get_argument_completions: Some(Box::new(move |argument| {
                arguments.lock().unwrap().push(argument.clone());
                Some(vec![AutocompleteItem::new(
                    value.clone() + "arg",
                    value.clone() + "arglabel",
                )])
            })),
        };
        let provider = CombinedAutocompleteProvider::new(
            vec![
                CommandEntry::Command(command),
                CommandEntry::Command(SlashCommand::new(raw.clone() + "other")),
            ],
            ".",
            None,
        );
        let queries = [
            JsString::from("/") + raw.slice(0, 1),
            JsString::from("/") + &raw + "cmd",
            JsString::from("/") + &raw + "cmd " + &raw + " arg",
        ];
        let mut results = Vec::new();
        for query in queries {
            let lines = vec![query.clone()];
            let suggestions = provider
                .get_suggestions(&lines, 0, query.len(), SuggestionsOptions::default())
                .await;
            let result = suggestions.map(|suggestions| {
                (
                    suggestions
                        .items
                        .into_iter()
                        .map(|item| (item.value, item.label, item.description))
                        .collect::<Vec<_>>(),
                    suggestions.prefix,
                )
            });
            results.push((result, provider.should_trigger_file_completion(&lines, 0, query.len())));
        }
        let actual = serde_json::to_string(&(results, captured.lock().unwrap().clone())).unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
            "raw={raw:?}"
        );
    }
}

#[tokio::test]
async fn filesystem_completion_retains_raw_path_until_actual_os_boundary() {
    use pi_js_compat::js_json::encode_node_utf8;
    use pi_tui::autocomplete::SuggestionsOptions;
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("root");
    let base = root.join("a/b/c");
    std::fs::create_dir_all(&base).unwrap();
    let raw = JsString::from_utf16(vec![0xd800]) + "dir";
    for parent in [&root, &base] {
        let directory = parent.join(encode_node_utf8(&raw));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("child.txt"), "fixture").unwrap();
    }
    let provider = CombinedAutocompleteProvider::new(vec![], base.to_string_lossy().as_ref(), None);
    for prefix in [
        JsString::from("./") + &raw + "/ch",
        JsString::from("../../../") + &raw + "/ch",
    ] {
        let expected = oracle
            .request(&json!({"operation":"autocomplete_files","base":base.to_string_lossy(),"units":prefix.units()}));
        assert_eq!(expected["ok"], true, "{expected}");
        let result = provider
            .get_suggestions(
                std::slice::from_ref(&prefix),
                0,
                prefix.len(),
                SuggestionsOptions::default(),
            )
            .await;
        let result = result.map(|result| {
            (
                result
                    .items
                    .into_iter()
                    .map(|item| (item.value, item.label, item.description))
                    .collect::<Vec<_>>(),
                result.prefix,
            )
        });
        let actual = serde_json::to_string(&result).unwrap();
        assert_eq!(
            parse_lossless(actual).unwrap(),
            parse_lossless(expected["json"].as_str().unwrap()).unwrap(),
            "prefix={prefix:?}"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn readdir_os_bytes_match_node_filename_decoding() {
    use pi_tui::autocomplete::SuggestionsOptions;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let directory = tempfile::tempdir().unwrap();
    for name in [
        b"a-\xff.md".to_vec(),
        b"b-\xe2\x82.md".to_vec(),
        b"c-\xe2(\xa1.md".to_vec(),
        b"valid-\xf0\x9f\x98\x80.md".to_vec(),
    ] {
        std::fs::write(directory.path().join(OsString::from_vec(name)), b"fixture").unwrap();
    }
    let base = directory.path().to_str().unwrap();
    let prefix = JsString::from("./");
    let provider = CombinedAutocompleteProvider::new(vec![], base, None);
    let result = provider
        .get_suggestions(
            std::slice::from_ref(&prefix),
            0,
            prefix.len(),
            SuggestionsOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(result.items.len(), 4);
    let actual = serde_json::to_string(&(
        result
            .items
            .iter()
            .map(|item| (&item.value, &item.label, &item.description))
            .collect::<Vec<_>>(),
        result.prefix,
    ))
    .unwrap();
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let expected = oracle.request(&json!({"operation":"autocomplete_files","base":base,"units":prefix.units()}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(
        parse_lossless(actual).unwrap(),
        parse_lossless(expected["json"].as_str().unwrap()).unwrap()
    );
}
