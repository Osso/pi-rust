use pi_coding_agent::utils::html::{DecodedHtmlEntity, decode_html_entity, decode_html_entity_at};

#[test]
fn decodes_named_entities() {
    assert_eq!(decode_html_entity("amp"), Some("&".to_owned()));
    assert_eq!(decode_html_entity("lt"), Some("<".to_owned()));
    assert_eq!(decode_html_entity("gt"), Some(">".to_owned()));
    assert_eq!(decode_html_entity("quot"), Some("\"".to_owned()));
    assert_eq!(decode_html_entity("apos"), Some("'".to_owned()));
}

#[test]
fn rejects_unknown_named_entities() {
    assert_eq!(decode_html_entity("nbsp"), None);
    assert_eq!(decode_html_entity("AMP"), None);
    assert_eq!(decode_html_entity(""), None);
}

#[test]
fn decodes_hex_and_decimal_code_points() {
    assert_eq!(decode_html_entity("#x41"), Some("A".to_owned()));
    assert_eq!(decode_html_entity("#X41"), Some("A".to_owned()));
    assert_eq!(decode_html_entity("#x1f600"), Some("\u{1f600}".to_owned()));
    assert_eq!(decode_html_entity("#65"), Some("A".to_owned()));
    assert_eq!(decode_html_entity("#0"), Some("\0".to_owned()));
}

#[test]
fn follows_javascript_parse_int_prefix_rules() {
    assert_eq!(decode_html_entity("#x41zz"), Some("A".to_owned()));
    assert_eq!(decode_html_entity("# 65"), Some("A".to_owned()));
    assert_eq!(decode_html_entity("#x 41"), Some("A".to_owned()));
    assert_eq!(decode_html_entity("#x+41"), Some("A".to_owned()));
    assert_eq!(decode_html_entity("#x-41"), None);
    assert_eq!(decode_html_entity("#"), None);
    assert_eq!(decode_html_entity("#x"), None);
    assert_eq!(decode_html_entity("#xzz"), None);
}

#[test]
fn rejects_out_of_range_code_points() {
    assert_eq!(decode_html_entity("#x110000"), None);
    assert_eq!(decode_html_entity("#-1"), None);
}

#[test]
fn decodes_entity_at_index_with_consumed_length() {
    let html = "a&amp;b";
    assert_eq!(
        decode_html_entity_at(html, 1),
        Some(DecodedHtmlEntity {
            text: "&".to_owned(),
            length: 5,
        })
    );
    assert_eq!(
        decode_html_entity_at("&#65;", 0),
        Some(DecodedHtmlEntity {
            text: "A".to_owned(),
            length: 5,
        })
    );
}

#[test]
fn rejects_entities_without_semicolon_or_beyond_16_characters() {
    assert_eq!(decode_html_entity_at("a&amp b", 1), None);
    assert_eq!(decode_html_entity_at("&0123456789abcdef;", 0), None);
    assert_eq!(decode_html_entity_at("&unknown;", 0), None);
}

#[test]
fn accepts_the_exact_16_character_entity_boundary() {
    let accepted = "&#00000000000065;";
    assert_eq!(accepted.len(), 17);
    assert_eq!(
        decode_html_entity_at(accepted, 0),
        Some(DecodedHtmlEntity {
            text: "A".to_owned(),
            length: 17,
        })
    );
}

#[test]
fn rejects_entities_at_the_wrong_position() {
    assert_eq!(decode_html_entity_at("&amp;", 1), None);
    assert_eq!(decode_html_entity_at("", 0), None);
}
