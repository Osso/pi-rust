#[test]
fn import_export_path_arguments_match_the_frozen_utf16_parser() {
    mod oracle {
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/oracle.rs"
        ));
    }
    let mut source = oracle::TsOracle::spawn("interactive-path-command-oracle.mts", "packages/coding-agent/src");
    for command in ["/import", "/export"] {
        for suffix in [
            "",
            " ",
            "\tfile.jsonl",
            "file.jsonl",
            " file.jsonl trailing",
            " \"dir name/file.jsonl\" trailing",
            " 'dír name/日本.jsonl' rest",
            " \"unfinished",
            " \"\"",
            " \u{a0}file.jsonl\tignored",
            " '\\quoted' ",
            " \"one\"two",
        ] {
            let text = js_format!("{}{}", command, suffix);
            let expected = source.request(&pi_js_compat::js_json!({"text":text,"command":command}));
            let path = InteractiveMode::path_command_argument(&text, &command.into());
            let actual = pi_js_compat::js_json!({"ok":true,"defined":path.is_some(),"path":path});
            assert_eq!(actual, expected, "text={text:?}");
        }
        let raw = JsString::from_utf16(vec![0xd800, b' ' as u16, 0xdc00]);
        let text = js_format!("{} '{}' suffix", command, raw);
        let expected = source.request(&pi_js_compat::js_json!({"text":text,"command":command}));
        let path = InteractiveMode::path_command_argument(&text, &command.into());
        assert_eq!(
            pi_js_compat::js_json!({"ok":true,"defined":path.is_some(),"path":path}),
            expected
        );
    }
}
