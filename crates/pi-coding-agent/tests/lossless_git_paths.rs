use pi_coding_agent::core::footer_data_provider::find_git_paths;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

#[test]
fn git_directory_walk_retains_unpaired_input_units() {
    let temp = tempfile::tempdir().unwrap();
    let root = JsString::from(temp.path().to_str().unwrap()) + "/" + JsString::from_utf16(vec![0xd800]);
    std::fs::create_dir_all(encode_node_utf8(&(root.clone() + "/.git"))).unwrap();
    std::fs::write(
        encode_node_utf8(&(root.clone() + "/.git/HEAD")),
        "ref: refs/heads/main\n",
    )
    .unwrap();
    let nested = root.clone() + "/nested";
    std::fs::create_dir_all(encode_node_utf8(&nested)).unwrap();
    let paths = find_git_paths(&nested).unwrap();
    assert_eq!(paths.repo_dir, root.clone());
    assert_eq!(paths.common_git_dir, root.clone() + "/.git");
    assert_eq!(paths.head_path, root + "/.git/HEAD");
}
