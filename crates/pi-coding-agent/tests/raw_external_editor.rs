use pi_coding_agent::modes::interactive::external_editor::{
    ExternalEditorOptions, ExternalEditorResult, edit_in_external_editor,
};
use pi_js_compat::js_json::JsString;

#[test]
fn external_editor_encodes_raw_text_only_at_file_write_and_decodes_saved_content() {
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("edit.py");
    let captured = directory.path().join("captured");
    std::fs::write(
        &script,
        "import pathlib,sys\np=pathlib.Path(sys.argv[2])\nb=p.read_bytes()\npathlib.Path(sys.argv[1]).write_bytes(b)\np.write_bytes(b+b'\\n\\n')\n",
    ).unwrap();
    for raw in [
        JsString::from_utf16(vec![0xd800]),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("😀"),
    ] {
        let result = edit_in_external_editor(&ExternalEditorOptions {
            command: format!("python3 {} {}", script.display(), captured.display()).into(),
            content: raw.clone(),
        })
        .unwrap();
        let encoded = pi_js_compat::js_json::encode_node_utf8(&raw);
        assert_eq!(std::fs::read(&captured).unwrap(), encoded.as_bytes());
        let expected = JsString::from(encoded) + "\n";
        assert_eq!(result, ExternalEditorResult::Complete { content: expected });
    }
}
