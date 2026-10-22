use pi_coding_agent::utils::html::{DecodedHtmlEntity, decode_html_entity, decode_html_entity_at};
use pi_js_compat::js_json::JsString;

#[test]
fn decodes_named_entities() {
    assert_eq!(decode_html_entity(&"amp".into()), Some("&".into()));
    assert_eq!(decode_html_entity(&"lt".into()), Some("<".into()));
    assert_eq!(decode_html_entity(&"gt".into()), Some(">".into()));
    assert_eq!(decode_html_entity(&"quot".into()), Some("\"".into()));
    assert_eq!(decode_html_entity(&"apos".into()), Some("'".into()));
}

#[test]
fn rejects_unknown_named_entities() {
    assert_eq!(decode_html_entity(&"nbsp".into()), None);
    assert_eq!(decode_html_entity(&"AMP".into()), None);
    assert_eq!(decode_html_entity(&"".into()), None);
}

#[test]
fn decodes_hex_and_decimal_code_points() {
    assert_eq!(decode_html_entity(&"#x41".into()), Some("A".into()));
    assert_eq!(decode_html_entity(&"#X41".into()), Some("A".into()));
    assert_eq!(decode_html_entity(&"#x1f600".into()), Some("\u{1f600}".into()));
    assert_eq!(decode_html_entity(&"#65".into()), Some("A".into()));
    assert_eq!(decode_html_entity(&"#0".into()), Some("\0".into()));
}

#[test]
fn follows_javascript_parse_int_prefix_rules() {
    assert_eq!(decode_html_entity(&"#x41zz".into()), Some("A".into()));
    assert_eq!(decode_html_entity(&"# 65".into()), Some("A".into()));
    assert_eq!(decode_html_entity(&"#x 41".into()), Some("A".into()));
    assert_eq!(decode_html_entity(&"#x+41".into()), Some("A".into()));
    assert_eq!(decode_html_entity(&"#x-41".into()), None);
    assert_eq!(decode_html_entity(&"#".into()), None);
    assert_eq!(decode_html_entity(&"#x".into()), None);
    assert_eq!(decode_html_entity(&"#xzz".into()), None);
}

#[test]
fn decodes_surrogate_code_points_without_replacement() {
    assert_eq!(
        decode_html_entity(&"#xD800".into()),
        Some(JsString::from_utf16(vec![0xd800]))
    );
    assert_eq!(
        decode_html_entity(&"#56320".into()),
        Some(JsString::from_utf16(vec![0xdc00]))
    );
}

#[test]
fn rejects_out_of_range_code_points() {
    assert_eq!(decode_html_entity(&"#x110000".into()), None);
    assert_eq!(decode_html_entity(&"#-1".into()), None);
}

#[test]
fn decodes_entity_at_index_with_consumed_length() {
    let html = "a&amp;b";
    assert_eq!(
        decode_html_entity_at(&html.into(), 1),
        Some(DecodedHtmlEntity {
            text: "&".into(),
            length: 5,
        })
    );
    assert_eq!(
        decode_html_entity_at(&"&#65;".into(), 0),
        Some(DecodedHtmlEntity {
            text: "A".into(),
            length: 5,
        })
    );
}

#[test]
fn rejects_entities_without_semicolon_or_beyond_16_characters() {
    assert_eq!(decode_html_entity_at(&"a&amp b".into(), 1), None);
    assert_eq!(decode_html_entity_at(&"&0123456789abcdef;".into(), 0), None);
    assert_eq!(decode_html_entity_at(&"&unknown;".into(), 0), None);
}

#[test]
fn accepts_the_exact_16_character_entity_boundary() {
    let accepted = "&#00000000000065;";
    assert_eq!(accepted.len(), 17);
    assert_eq!(
        decode_html_entity_at(&accepted.into(), 0),
        Some(DecodedHtmlEntity {
            text: "A".into(),
            length: 17,
        })
    );
}

#[test]
fn rejects_entities_at_the_wrong_position() {
    assert_eq!(decode_html_entity_at(&"&amp;".into(), 1), None);
    assert_eq!(decode_html_entity_at(&"".into(), 0), None);
}
