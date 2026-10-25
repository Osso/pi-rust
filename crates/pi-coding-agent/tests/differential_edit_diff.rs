use crate::shared_corpus as corpus;
use crate::shared_oracle as oracle;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use corpus::Corpus;
use pi_coding_agent::core::tools::edit_diff::{
    Edit, EditDiffOutcome, apply_edits_to_normalized_content, compute_edits_diff, fuzzy_find_text,
    generate_diff_string, generate_unified_patch, normalize_for_fuzzy_match,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

enum Case {
    Normalize(String),
    FuzzyFind(String, String),
    ApplyEdits(String, Vec<Edit>, String),
    DiffString(String, String, usize),
    Patch(String, String, String, usize),
    Preview(String, Vec<Edit>),
}

#[test]
fn coding_agent_edit_diff_matches_the_pinned_typescript_source() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().to_str().expect("utf-8 test path").to_owned();
    let cases = cases(&cwd, temp.path());
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-edit-diff-oracle.ts",
        "packages/coding-agent/src/core/tools/edit-diff.ts",
    );
    let expected = oracle.request(&json!({
        "id": 27_005,
        "operation": "coding_agent_edit_diff",
        "cases": cases.iter().map(|case| request_case(case, &cwd)).collect::<Vec<_>>(),
    }));
    let actual = json!({"id": 27_005, "ok": true, "output": cases.iter().map(|case| actual_case(case, &cwd)).collect::<Vec<_>>()});

    assert_eq!(expected["ok"], Value::Bool(true), "{expected:?}");
    let expected_results = expected["output"].as_array().expect("oracle output array");
    let actual_results = actual["output"].as_array().expect("actual output array");
    assert_eq!(expected_results.len(), cases.len());
    for (index, (actual, expected)) in actual_results.iter().zip(expected_results).enumerate() {
        assert_eq!(actual, expected, "case {index}");
    }
}

fn request_case(case: &Case, cwd: &str) -> Value {
    match case {
        Case::Normalize(text) => json!({"kind": "normalize", "text": text}),
        Case::FuzzyFind(content, old_text) => json!({"kind": "fuzzyFind", "content": content, "oldText": old_text}),
        Case::ApplyEdits(content, edits, path) => {
            json!({"kind": "applyEdits", "content": content, "edits": edits_json(edits), "path": path})
        }
        Case::DiffString(old_content, new_content, context) => json!({
            "kind": "diffString",
            "oldContent": old_content,
            "newContent": new_content,
            "contextLines": context,
        }),
        Case::Patch(path, old_content, new_content, context) => json!({
            "kind": "patch",
            "path": path,
            "oldContent": old_content,
            "newContent": new_content,
            "contextLines": context,
        }),
        Case::Preview(path, edits) => json!({"kind": "preview", "path": path, "edits": edits_json(edits), "cwd": cwd}),
    }
}

fn number(value: f64) -> Value {
    pi_js_compat::number::js_number_value(value).into()
}

fn edits_json(edits: &[Edit]) -> Value {
    Value::Array(
        edits
            .iter()
            .map(|edit| json!({"oldText": edit.old_text, "newText": edit.new_text}))
            .collect(),
    )
}

fn actual_case(case: &Case, cwd: &str) -> Value {
    match case {
        Case::Normalize(text) => json!({"value": normalize_for_fuzzy_match(text)}),
        Case::FuzzyFind(content, old_text) => {
            let found = fuzzy_find_text(content, old_text);
            json!({
                "found": found.found,
                "index": number(found.index),
                "matchLength": number(found.match_length),
                "usedFuzzyMatch": found.used_fuzzy_match,
                "contentForReplacement": found.content_for_replacement,
            })
        }
        Case::ApplyEdits(content, edits, path) => match apply_edits_to_normalized_content(content, edits, path) {
            Ok(applied) => json!({"ok": true, "baseContent": applied.base_content, "newContent": applied.new_content}),
            Err(error) => json!({"ok": false, "error": error.message}),
        },
        Case::DiffString(old_content, new_content, context) => {
            let diff = generate_diff_string(old_content, new_content, *context);
            json!({"diff": diff.diff, "firstChangedLine": diff.first_changed_line})
        }
        Case::Patch(path, old_content, new_content, context) => {
            json!({"patch": generate_unified_patch(path, old_content, new_content, *context)})
        }
        Case::Preview(path, edits) => {
            let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
            match runtime.block_on(compute_edits_diff(path, edits, cwd)) {
                Ok(EditDiffOutcome::Diff(diff)) => {
                    json!({"diff": diff.diff, "firstChangedLine": diff.first_changed_line})
                }
                Ok(EditDiffOutcome::Error(error)) => json!({"error": error}),
                Err(error) => json!({"error": error.message()}),
            }
        }
    }
}

fn line_alphabet() -> Vec<&'static str> {
    vec!["alpha\n", "beta\n", "gamma\n", "\n", "delta\n", "alpha\n", "omega"]
}

fn text(corpus: &mut Corpus, alphabet: &[&str], length: usize) -> String {
    (0..length).map(|_| alphabet[corpus.below(alphabet.len())]).collect()
}

fn cases(cwd: &str, dir: &std::path::Path) -> Vec<Case> {
    let mut cases = targeted_cases(cwd, dir);
    let mut corpus = Corpus::new(0x6564_6974);
    let alphabet = line_alphabet();
    for _ in 0..90 {
        let old_length = corpus.below(8);
        let old_content = text(&mut corpus, &alphabet, old_length);
        let mut new_content = old_content.clone();
        match corpus.below(4) {
            0 => new_content.push_str(&text(&mut corpus, &alphabet, 1)),
            1 if !new_content.is_empty() => {
                let cut = new_content.len() / 2;
                let cut = (0..=cut)
                    .rev()
                    .find(|index| new_content.is_char_boundary(*index))
                    .unwrap_or(0);
                new_content.truncate(cut);
            }
            _ => {
                if let Some(position) = new_content.find("alpha\n") {
                    new_content.replace_range(position..position + 6, "ALPHA\n");
                }
            }
        }
        let context = corpus.below(6);
        cases.push(Case::DiffString(old_content.clone(), new_content.clone(), context));
        cases.push(Case::Patch("file.txt".to_owned(), old_content, new_content, context));
    }
    let repeated = ["a\n", "b\n", "c\n", "a\nb\n"];
    for _ in 0..250 {
        let old_length = corpus.below(12);
        let mut old_content = text(&mut corpus, &repeated, old_length);
        let mut new_content = old_content.clone();
        for _ in 0..corpus.below(3) {
            let tokens = old_content.split_inclusive('\n').collect::<Vec<_>>();
            let position = corpus.below(tokens.len() + 1);
            let byte_position = tokens[..position].iter().map(|token| token.len()).sum::<usize>();
            match corpus.below(3) {
                0 => new_content.insert_str(byte_position.min(new_content.len()), repeated[corpus.below(3)]),
                1 if byte_position < new_content.len() => {
                    let end = (byte_position + repeated[corpus.below(3)].len()).min(new_content.len());
                    new_content.replace_range(byte_position..end, "");
                }
                _ if byte_position < new_content.len() => {
                    let end = (byte_position + repeated[corpus.below(3)].len()).min(new_content.len());
                    new_content.replace_range(byte_position..end, "X\n");
                }
                _ => {}
            }
            old_content = new_content.clone();
        }
        let context = corpus.below(6);
        cases.push(Case::DiffString(old_content.clone(), new_content.clone(), context));
        cases.push(Case::Patch("file.txt".to_owned(), old_content, new_content, context));
    }
    for _ in 0..60 {
        let content_length = corpus.below(6);
        let content = text(&mut corpus, &alphabet, content_length);
        let target = alphabet[corpus.below(alphabet.len())].trim_end_matches('\n');
        let replacement = format!("{}-{}", target.to_uppercase(), corpus.below(3));
        cases.push(Case::ApplyEdits(
            content,
            vec![Edit {
                old_text: target.into(),
                new_text: replacement.into(),
            }],
            "sample.txt".to_owned(),
        ));
    }
    cases
}

fn targeted_cases(cwd: &str, dir: &std::path::Path) -> Vec<Case> {
    let apply = |content: &str, edits: &[(&str, &str)]| {
        Case::ApplyEdits(
            content.to_owned(),
            edits
                .iter()
                .map(|(old_text, new_text)| Edit {
                    old_text: (*old_text).into(),
                    new_text: (*new_text).into(),
                })
                .collect(),
            "sample.txt".to_owned(),
        )
    };

    let preview_file = dir.join("preview.txt");
    fs::write(&preview_file, "\u{FEFF}one\r\ntwo\r\nthree\r\n").unwrap();
    let fuzzy_file = dir.join("fuzzy.txt");
    fs::write(&fuzzy_file, "keep  \nsay \u{201C}hi\u{201D}  \nkeep2\n").unwrap();
    let missing_file = dir.join("missing.txt");
    let unreadable_file = dir.join("unreadable.txt");
    fs::write(&unreadable_file, "hello\n").unwrap();
    fs::set_permissions(&unreadable_file, fs::Permissions::from_mode(0o222)).unwrap();

    let mut cases = vec![
        Case::Normalize("\u{201C}quoted\u{201D}  \n\u{2018}single\u{2019}\t\nem\u{2014}dash \u{2212} minus\u{00A0}space\n\u{FB01}le \u{FF21}".to_owned()),
        Case::Normalize("e\u{0301}".to_owned()),
        Case::Normalize("".to_owned()),
        Case::Normalize("\u{FEFF}leading".to_owned()),
        Case::Normalize("trailing\u{0085}".to_owned()),
        Case::FuzzyFind("a\u{1F600}target".to_owned(), "target".to_owned()),
        Case::FuzzyFind("say \u{201C}hello\u{201D}  \nnext".to_owned(), "say \"hello\"\nnext".to_owned()),
        Case::FuzzyFind("abc".to_owned(), "xyz".to_owned()),
        Case::FuzzyFind("".to_owned(), "".to_owned()),
        apply("alpha\nbeta\ngamma\n", &[("alpha", "ALPHA"), ("gamma", "GAMMA")]),
        apply("abc", &[("zzz", "x")]),
        apply("abc", &[("a", "A"), ("zzz", "x")]),
        apply("abc", &[("", "x")]),
        apply("abc", &[("a", "A"), ("", "x")]),
        apply("abcdef", &[("abcd", "x"), ("cdef", "y")]),
        apply("abc", &[("b", "b")]),
        apply("abc", &[("a", "a"), ("c", "c")]),
        apply("alpha\ntarget\nbeta\ntarget\ngamma\n", &[("alpha", "ALPHA"), ("target", "TARGET")]),
        apply(
            "function first() {\n\treturn value;\n}\n\nfunction second() {\n\treturn value;\n}",
            &[("\treturn value;", "\treturn changed;")],
        ),
        apply("keep  \nsay \u{201C}hi\u{201D}  \nkeep2\n", &[("say \"hi\"", "say \"bye\"")]),
        apply("a\r\nb\r\n", &[("b", "B")]),
        apply("line\n".repeat(8).as_str(), &[("line", "LINE")]),
        apply("\u{1F600}\n\u{1F600}\n", &[("\u{1F600}", "star")]),
        Case::Preview(preview_file.to_str().expect("utf-8 test path").to_owned(), vec![Edit {
            old_text: "two".into(),
            new_text: "TWO".into(),
        }]),
        Case::Preview(fuzzy_file.to_str().expect("utf-8 test path").to_owned(), vec![Edit {
            old_text: "say \"hi\"".into(),
            new_text: "say \"bye\"".into(),
        }]),
        Case::Preview(missing_file.to_str().expect("utf-8 test path").to_owned(), vec![Edit {
            old_text: "hello".into(),
            new_text: "world".into(),
        }]),
        Case::Preview(unreadable_file.to_str().expect("utf-8 test path").to_owned(), vec![Edit {
            old_text: "hello".into(),
            new_text: "world".into(),
        }]),
        Case::Preview(cwd.to_owned(), vec![Edit {
            old_text: "hello".into(),
            new_text: "world".into(),
        }]),
        Case::Preview(preview_file.to_str().expect("utf-8 test path").to_owned(), vec![Edit {
            old_text: "missing text".into(),
            new_text: "x".into(),
        }]),
        Case::DiffString("a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n".to_owned(), "a\nb\nc\nd\ne\nf\nG\nh\ni\nj\n".to_owned(), 1),
        Case::DiffString("a\nb\n".to_owned(), "a\nnew\nb\n".to_owned(), 4),
        Case::DiffString("1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n".to_owned(), "1\n2\n3\n4\nX\n6\n7\n8\n9\n10\n".to_owned(), 1),
        Case::DiffString("".to_owned(), "new".to_owned(), 4),
        Case::DiffString("gone".to_owned(), "".to_owned(), 4),
        Case::DiffString("a\r\nb\r\n".to_owned(), "a\r\nc\r\n".to_owned(), 4),
        Case::DiffString("same\n".to_owned(), "same\n".to_owned(), 4),
        Case::DiffString("x".repeat(1_000), "y".repeat(1_000), 4),
        Case::Patch("file.txt".to_owned(), "a\nb".to_owned(), "a\nc".to_owned(), 4),
        Case::Patch("file.txt".to_owned(), "a\nb\n".to_owned(), "a\nc\n".to_owned(), 4),
        Case::Patch("file.txt".to_owned(), "".to_owned(), "new\n".to_owned(), 0),
        Case::Patch("file.txt".to_owned(), "gone\n".to_owned(), "".to_owned(), 0),
        Case::Patch("file.txt".to_owned(), "a\nb\nc\nd\ne\nf\ng\n".to_owned(), "a\nb\nc\nd\nX\nf\ng\n".to_owned(), 1),
    ];
    cases.shrink_to_fit();
    cases
}
