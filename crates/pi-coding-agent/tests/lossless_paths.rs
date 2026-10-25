use crate::shared_oracle as oracle;

use pi_coding_agent::utils::paths::{
    PathInputOptions, canonicalize_path, format_path_relative_to_cwd_or_absolute, get_cwd_relative_path, is_local_path,
    normalize_path, resolve_path,
};
use pi_js_compat::js_json::{JsString, JsValue};

#[test]
fn realpath_preserves_source_units_for_existing_encoded_names_and_symlinks() {
    use pi_js_compat::js_json::encode_node_utf8;
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let root = JsString::from(directory.path().to_str().unwrap());
    std::fs::create_dir(directory.path().join("target")).unwrap();
    std::fs::write(directory.path().join("target/�"), b"x").unwrap();
    symlink(directory.path().join("target"), directory.path().join("link")).unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-paths-oracle.ts",
        "packages/coding-agent/src/utils/paths.ts",
    );
    for name in ["target", "link"] {
        for unit in [0xd800, 0xdfff] {
            let path = root.clone() + "/" + name + "/" + JsString::from_utf16(vec![unit]);
            assert!(std::path::Path::new(&encode_node_utf8(&path)).exists());
            let expected = oracle.request(
                &pi_js_compat::js_json!({"id":1,"cases":[{"kind":"canonicalize","name":"existing","path":path}]}),
            );
            assert_eq!(
                JsValue::String(canonicalize_path(&path)),
                expected["output"]["results"].as_array().unwrap()[0]["value"],
                "{path:?}"
            );
        }
    }
}

#[test]
fn native_posix_path_helpers_match_node_with_raw_components() {
    use pi_coding_agent::utils::paths::{
        node_basename, node_dirname, node_extname, node_is_absolute, node_join, node_normalize, node_resolve,
        relative_path,
    };
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-node-path-oracle.ts",
        "packages/coding-agent/src/utils/paths.ts",
    );
    let mut texts = [
        "",
        ".",
        "./",
        "..",
        "../..",
        "../a/../../",
        "/",
        "//",
        "///",
        "a//b/./",
        "//a",
        "//a/b",
        "///a",
        "a\\b",
        "/a/../b/",
        "a/../../b/",
        ".bashrc",
        "a.b.c",
        "a.",
        "...",
        "a..",
        "/x/.y",
        "x.y/",
        "s.jsonl.zst",
    ]
    .into_iter()
    .map(JsString::from)
    .collect::<Vec<_>>();
    texts.extend([
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdfff]),
        JsString::from("a/") + JsString::from_utf16(vec![0xd800]) + "/../" + JsString::from_utf16(vec![0xdfff]),
    ]);
    for base in &texts {
        for input in &texts {
            let expected = oracle.request(&pi_js_compat::js_json!({"id":1,"cases":[
                {"kind":"normalize","input":input,"base":base}, {"kind":"join","input":input,"base":base},
                {"kind":"resolve","input":input,"base":base}, {"kind":"relative","input":input,"base":base},
                {"kind":"dirname","input":input,"base":base}, {"kind":"basename","input":input,"base":base},
                {"kind":"absolute","input":input,"base":base}, {"kind":"extname","input":input,"base":base}
            ]}));
            let actual = [
                JsValue::String(node_normalize(input)),
                JsValue::String(node_join(base, input)),
                JsValue::String(node_resolve(base, input).unwrap()),
                JsValue::String(relative_path(base, input).unwrap()),
                JsValue::String(node_dirname(input)),
                JsValue::String(node_basename(input)),
                JsValue::Bool(node_is_absolute(input)),
                JsValue::String(node_extname(input)),
            ];
            assert_eq!(
                JsValue::Array(actual.into()),
                expected["output"]["results"],
                "base={base:?}, input={input:?}"
            );
        }
    }
}

#[test]
fn path_operations_keep_native_units_until_explicit_url_or_filesystem_boundaries() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-paths-oracle.ts",
        "packages/coding-agent/src/utils/paths.ts",
    );
    let options = PathInputOptions {
        home_dir: Some(JsString::from_utf16(vec![0x2f, 0xd800])),
        trim: true,
        normalize_unicode_spaces: true,
        strip_at_prefix: true,
        ..Default::default()
    };
    let texts = [
        "",
        ".",
        "..",
        "../..",
        "a/../../b/",
        "a//b/./",
        "/../a/",
        "~",
        "~/../x",
        "~//x",
    ]
    .into_iter()
    .map(JsString::from)
    .chain([
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdfff]),
        JsString::from_utf16(vec![0x20, 0x40, 0xd800, 0x2f, 0xdfff, 0x20]),
        JsString::from_utf16(vec![0x2f, 0xd800, 0x2f, 0x2e, 0x2e, 0x2f, 0xdfff]),
        JsString::from_utf16(vec![0x7e, 0x2f, 0xdfff]),
        JsString::from("file:///tmp/") + JsString::from_utf16(vec![0xd800]),
    ])
    .collect::<Vec<_>>();
    for text in texts {
        let request = pi_js_compat::js_json!({
            "id":1,"cases":[
                {"kind":"normalize","name":"normalize","input":text,"options":{"homeDir":options.home_dir,"trim":true,"normalizeUnicodeSpaces":true,"stripAtPrefix":true}},
                {"kind":"resolve","name":"resolve","input":text,"baseDir":"/base/dir","options":{"homeDir":options.home_dir,"trim":true,"normalizeUnicodeSpaces":true,"stripAtPrefix":true}},
                {"kind":"relative","name":"relative","filePath":text,"cwd":"/base/dir"},
                {"kind":"format_relative","name":"format","filePath":text,"cwd":"/base/dir"},
                {"kind":"is_local","name":"local","input":text},
                {"kind":"canonicalize","name":"canonical","path":text}
            ]
        });
        let expected = oracle.request(&request);
        let rows = expected["output"]["results"].as_array().unwrap();
        let actual = [
            normalize_path(&text, &options).map(JsValue::String),
            resolve_path(&text, "/base/dir", &options).map(JsValue::String),
            get_cwd_relative_path(&text, "/base/dir").map(|value| value.map(JsValue::String).unwrap_or(JsValue::Null)),
            format_path_relative_to_cwd_or_absolute(&text, "/base/dir").map(JsValue::String),
            Ok(JsValue::Bool(is_local_path(&text))),
            Ok(JsValue::String(canonicalize_path(&text))),
        ];
        for (actual, expected) in actual.into_iter().zip(rows) {
            match actual {
                Ok(actual) => {
                    assert_eq!(expected["ok"], JsValue::Bool(true), "{text:?}: {expected:?}");
                    assert_eq!(actual, expected["value"], "{text:?}");
                }
                Err(_) => assert_eq!(expected["ok"], JsValue::Bool(false), "{text:?}"),
            }
        }
    }
}
