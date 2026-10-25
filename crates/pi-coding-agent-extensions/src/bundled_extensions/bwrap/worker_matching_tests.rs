use super::*;
mod oracle {
    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/../pi-js-compat/tests/support/oracle.rs"
    ));
}

fn raw(units: &[u16]) -> JsString {
    JsString::from_utf16(units.to_vec())
}

#[test]
fn actual_worker_glob_function_matches_raw_units_and_node_options() {
    let mut oracle = oracle::TsOracle::spawn(
        "bwrap-worker-matching-oracle.cjs",
        "packages/coding-agent/extensions/bwrap/src/fs-worker.cjs",
    );
    let atoms = [
        raw(&[0xd800]),
        raw(&[0xdc00]),
        raw(&[0xd83d, 0xde00]),
        raw(&[0xfffd]),
        raw(&[0xe000]),
        JsString::from("a"),
    ];
    let mut paths: Vec<JsString> = [
        "", ".", "..", ".a.x", "a/b.x", "a\\b.x", "a/b/", "!a", "#a", "a", "a/./b", "a/../b", "b", "./b", "a/b/c",
    ]
    .into_iter()
    .map(JsString::from)
    .collect();
    let mut patterns: Vec<JsString> = [
        "*",
        "**/*",
        "*.x",
        "@(?)",
        "@(??)",
        "a\\b.x",
        "!a",
        "#a",
        "a/**/../b",
        "**/..",
        "a/**/b",
        "**/b",
        "a/../b",
        "a/./b",
        "{a,b}",
        "[[:alpha:]]",
        "[]",
        "[!a]",
        ".*",
        "**",
        "a/**/c",
    ]
    .into_iter()
    .map(JsString::from)
    .collect();
    for atom in &atoms {
        paths.extend([atom.clone(), JsString::from("dir/") + atom, atom.clone() + ".x"]);
        patterns.extend([
            atom.clone(),
            JsString::from("{") + atom + ",b}",
            JsString::from("@(") + atom + "|b)",
            JsString::from("[") + atom + "]",
            JsString::from("*") + atom,
        ]);
    }
    let mut cases = Vec::new();
    let mut actual = Vec::new();
    for pattern in &patterns {
        for path in &paths {
            cases.push(serde_json::json!({"kind":"glob","pattern":pattern.units(),"text":path.units()}));
            actual.push(matches_glob(path, pattern).unwrap());
        }
    }
    let response = oracle.request(&serde_json::json!({"id":1,"cases":cases}));
    for (index, result) in actual.iter().enumerate() {
        assert_eq!(
            *result,
            response["output"][index]["matched"].as_bool().unwrap(),
            "case {:?}",
            cases[index]
        );
    }
}

#[test]
fn regex_matcher_preserves_legacy_units_and_literal_lowercase() {
    let options = GrepOptions {
        context: 0,
        glob: None,
        ignore_case: false,
        limit: 100,
        literal: false,
        pattern: JsString::from("^..$"),
        root_is_directory: false,
        root_path: PathBuf::from("."),
    };
    assert!(compile_matcher(&options).unwrap().is_match("😀").unwrap());
    let options = GrepOptions {
        pattern: raw(&[0xd800]),
        ..options
    };
    assert!(!compile_matcher(&options).unwrap().is_match(raw(&[0xdc00])).unwrap());
    assert!(compile_matcher(&options).unwrap().is_match(raw(&[0xd800])).unwrap());
    let options = GrepOptions {
        literal: true,
        ignore_case: true,
        pattern: raw(&[65, 0xd800]),
        ..options
    };
    assert!(compile_matcher(&options).unwrap().is_match(raw(&[97, 0xd800])).unwrap());
    assert!(!compile_matcher(&options).unwrap().is_match(raw(&[97, 0xdc00])).unwrap());
}

#[test]
fn invalid_regex_errors_retain_raw_pattern_units() {
    let mut oracle = oracle::TsOracle::spawn(
        "bwrap-worker-matching-oracle.cjs",
        "packages/coding-agent/extensions/bwrap/src/fs-worker.cjs",
    );
    let patterns = [
        raw(&[91, 0xd800]),
        raw(&[40, 0xdc00]),
        raw(&[42, 0xd800]),
        raw(&[91, 122, 45, 97, 93]),
        raw(&[92]),
        raw(&[41, 0xd800]),
        JsString::from("a{3,2}"),
        JsString::from("^*"),
        JsString::from("(?x)"),
        JsString::from("(?<1a>x)"),
        JsString::from("(?<a>x)(?<a>y)"),
        JsString::from("(?<a>x)\\k<b>"),
        JsString::from("(?<=a)*"),
    ];
    for ignore_case in [false, true] {
        let cases: Vec<_> = patterns.iter().map(|pattern| serde_json::json!({"kind":"regex","pattern":pattern.units(),"text":[],"ignoreCase":ignore_case})).collect();
        let response = oracle.request(&serde_json::json!({"id":2,"cases":cases}));
        for (index, pattern) in patterns.iter().enumerate() {
            let options = GrepOptions {
                context: 0,
                glob: None,
                ignore_case,
                limit: 1,
                literal: false,
                pattern: pattern.clone(),
                root_is_directory: false,
                root_path: PathBuf::from("."),
            };
            let error = compile_matcher(&options).err().unwrap();
            assert!(error.contains(pattern));
            assert_eq!(response["output"][index]["error"]["name"], "SyntaxError");
            let expected: Vec<u16> =
                serde_json::from_value(response["output"][index]["error"]["message"].clone()).unwrap();
            assert_eq!(
                error.units(),
                expected,
                "invalid pattern {:?} ignoreCase={ignore_case}",
                pattern
            );
        }
    }
}

#[test]
fn frozen_source_fallback_is_not_the_active_glob_contract() {
    let mut oracle = oracle::TsOracle::spawn(
        "bwrap-worker-matching-oracle.cjs",
        "packages/coding-agent/extensions/bwrap/src/fs-worker.cjs",
    );
    let response = oracle.request(&serde_json::json!({"id":3,"cases":[
        {"kind":"glob","text":[46,97],"pattern":[42],"fallback":false},
        {"kind":"glob","text":[46,97],"pattern":[42],"fallback":true}
    ]}));
    assert_eq!(response["output"][0]["matched"], false);
    assert_eq!(response["output"][1]["matched"], true);
    assert!(!matches_glob(".a", &JsString::from("*")).unwrap());
}

#[test]
fn oversized_glob_throws_frozen_error_instead_of_no_match() {
    let mut oracle = oracle::TsOracle::spawn(
        "bwrap-worker-matching-oracle.cjs",
        "packages/coding-agent/extensions/bwrap/src/fs-worker.cjs",
    );
    let pattern = JsString::from_utf16(vec![0xd800; 65537]);
    let response =
        oracle.request(&serde_json::json!({"id":4,"cases":[{"kind":"glob","pattern":pattern.units(),"text":[97]}]}));
    let expected: Vec<u16> = serde_json::from_value(response["output"][0]["error"]["message"].clone()).unwrap();
    assert_eq!(expected, JsString::from("pattern is too long").units());
    let directory = std::env::temp_dir().join(format!("q18-glob-error-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("a"), "a").unwrap();
    let payload = json!({"workspace":directory.to_string_lossy().into_owned(),"cwd":".","pattern":pattern});
    let error = run_operation_json("find", &payload).err().map(|error| error.message);
    std::fs::remove_dir_all(directory).unwrap();
    assert_eq!(error, Some(JsString::from("TypeError: pattern is too long")));
}

#[test]
fn empty_directory_still_rejects_invalid_regex_before_scanning() {
    let directory = std::env::temp_dir().join(format!("q18-empty-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let payload = json!({"workspace":directory.to_string_lossy().into_owned(),"path":".","pattern":raw(&[91,0xd800])});
    let result = run_operation_json("grep", &payload);
    std::fs::remove_dir_all(directory).unwrap();
    assert_eq!(
        result.err().map(|error| error.message),
        Some(
            JsString::from("SyntaxError: Invalid regular expression: /")
                + raw(&[91, 0xd800])
                + "/: Unterminated character class"
        )
    );
}

#[test]
fn worker_grep_operation_distinguishes_pair_and_unit_regex() {
    let directory = std::env::temp_dir().join(format!("q18-worker-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("sample"), "😀\nA\n").unwrap();
    let workspace = directory.to_string_lossy().into_owned();
    let payload = json!({"workspace":workspace,"path":"sample","pattern":"^..$"});
    let result = run_operation_json("grep", &payload).unwrap();
    assert_eq!(
        result.get("text").and_then(Value::as_string),
        Some(&JsString::from("sample:1: 😀"))
    );
    let payload = json!({"workspace":workspace,"path":"sample","pattern":raw(&[91,0xd800,45,0xdfff,93])});
    assert_eq!(
        run_operation_json("grep", &payload)
            .unwrap()
            .get("text")
            .and_then(Value::as_string),
        Some(&JsString::from("sample:1: 😀"))
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn find_matching_retains_linux_backslash_filename() {
    let directory = std::env::temp_dir().join(format!("q18-backslash-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("a\\b.x"), "a").unwrap();
    let payload = json!({"workspace":directory.to_string_lossy().into_owned(),"cwd":".","pattern":"*.x"});
    let result = run_operation_json("find", &payload).unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    assert_eq!(result, json!({"results":["a\\b.x"]}));
}
