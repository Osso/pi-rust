use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::{ImageContent, TextContent, TextOrImageContent};
use pi_coding_agent::core::tools::render_utils::{
    get_text_output, link_path, normalize_display_text, replace_tabs, shorten_path, str_value,
};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_tui::{ImageProtocol, TerminalCapabilities, set_capabilities};
use serde_json::json;

fn text_result(texts: &[&str]) -> AgentToolResult {
    AgentToolResult {
        content: texts
            .iter()
            .map(|text| TextOrImageContent::Text(TextContent::new(*text)))
            .collect(),
        details: None,
        is_error: None,
        terminate: None,
    }
}

#[test]
fn shorten_path_replaces_the_home_prefix() {
    let home = std::env::var("HOME").expect("HOME");
    assert_eq!(
        shorten_path(&pi_js_compat::js_format!("{}/projects/file.txt", home)).unwrap(),
        "~/projects/file.txt"
    );
    assert_eq!(shorten_path(&JsString::from("/tmp/file.txt")).unwrap(), "/tmp/file.txt");
}

#[test]
fn str_value_maps_types_like_the_source() {
    assert_eq!(
        str_value(Some(&JsValue::from(json!("text").clone()))),
        Some(JsString::from("text".to_owned()))
    );
    assert_eq!(
        str_value(Some(&JsValue::from(json!(null).clone()))),
        Some(JsString::from(String::new()))
    );
    assert_eq!(str_value(None), Some(JsString::from(String::new())));
    assert_eq!(str_value(Some(&JsValue::from(json!(4).clone()))), None);
    assert_eq!(str_value(Some(&JsValue::from(json!(true).clone()))), None);
}

#[test]
fn replace_tabs_and_normalize_display_text() {
    assert_eq!(replace_tabs(&JsString::from("a\tb\t\tc")), "a   b      c");
    assert_eq!(normalize_display_text(&JsString::from("a\r\nb\rc")), "a\nbc");
}

#[test]
fn get_text_output_joins_blocks_and_strips_terminal_sequences() {
    let result = text_result(&["\u{1b}[31mred\u{1b}[0m\r\n", "second\u{0}line"]);
    assert_eq!(get_text_output(Some(&result), true), "red\n\nsecondline");
    assert_eq!(get_text_output(None, true), "");
}

#[test]
fn capability_dependent_output_matches_the_source() {
    set_capabilities(TerminalCapabilities {
        images: None,
        true_color: true,
        hyperlinks: false,
    });
    let result = AgentToolResult {
        content: vec![
            TextOrImageContent::Text(TextContent::new("before")),
            TextOrImageContent::Image(ImageContent::new(String::new(), "image/unknown")),
        ],
        details: None,
        is_error: None,
        terminate: None,
    };
    assert_eq!(get_text_output(Some(&result), true), "before\n[Image: [image/unknown]]");
    let styled = "\u{1b}[34mfile.txt\u{1b}[0m";
    assert_eq!(
        link_path(
            &JsString::from(styled),
            &JsString::from("file.txt"),
            &JsString::from("/tmp")
        )
        .unwrap(),
        styled
    );

    set_capabilities(TerminalCapabilities {
        images: Some(ImageProtocol::Kitty),
        true_color: true,
        hyperlinks: true,
    });
    assert_eq!(get_text_output(Some(&result), true), "before");
    assert_eq!(
        get_text_output(Some(&result), false),
        "before\n[Image: [image/unknown]]"
    );
    assert_eq!(
        link_path(
            &JsString::from(styled),
            &JsString::from("/tmp/file.txt"),
            &JsString::from("/tmp")
        )
        .unwrap(),
        format!("\u{1b}]8;;file:///tmp/file.txt\u{1b}\\{styled}\u{1b}]8;;\u{1b}\\")
    );
}
