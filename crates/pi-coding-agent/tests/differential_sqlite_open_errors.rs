use crate::shared_oracle as oracle;

use pi_coding_agent::core::sqlite::{create_read_only_sqlite_database, create_sqlite_database};
use pi_js_compat::{callback_error::CallbackError, js_json as json, js_json::JsValue};

#[test]
fn node_sqlite_open_errors_match_for_unopenable_directory_and_missing_parent_paths() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("not-a-directory");
    std::fs::write(&file, b"fixture").unwrap();
    let directory = root.path().join("directory");
    std::fs::create_dir(&directory).unwrap();
    let paths = [
        file.join("database.sqlite"),
        directory,
        root.path().join("missing-parent/database.sqlite"),
    ];
    let cases:Vec<_>=[false,true].into_iter().flat_map(|read_only|paths.iter().map(move|path|json!({"path":pi_js_compat::node_fs::decode_node_path(path.as_os_str()),"readOnly":read_only}))).collect();
    let mut oracle = oracle::TsOracle::spawn(
        "sqlite-open-errors-oracle.ts",
        "packages/coding-agent/src/core/sqlite.ts",
    );
    let expected = oracle.request(&json!({"id":1,"cases":cases}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let actual: Vec<JsValue> = cases
        .iter()
        .map(|input| {
            let path = input["path"].as_string().unwrap().to_utf8().unwrap();
            let opened = if input["readOnly"].as_bool() == Some(true) {
                create_read_only_sqlite_database(&path)
            } else {
                create_sqlite_database(&path)
            };
            match opened {
                Ok(db) => {
                    db.close().unwrap();
                    json!({"ok":true})
                }
                Err(error) => json!({"ok":false,"error":CallbackError::from(error).thrown_value().value()}),
            }
        })
        .collect();
    assert_eq!(json!(actual), expected["output"]);
}
