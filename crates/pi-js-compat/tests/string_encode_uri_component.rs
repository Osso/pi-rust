use pi_js_compat::js_json::JsString;
use pi_js_compat::string::encode_uri_component;

fn encode(value: &str) -> String {
    encode_uri_component(&JsString::from(value)).expect("scalar input encodes")
}

#[test]
fn encodes_uri_components_like_javascript() {
    assert_eq!(encode("session-1"), "session-1");
    assert_eq!(encode("a b"), "a%20b");
    assert_eq!(encode("a/b?c=d&e"), "a%2Fb%3Fc%3Dd%26e");
    assert_eq!(encode("-_.!~*'()"), "-_.!~*'()");
    assert_eq!(encode("café"), "caf%C3%A9");
    assert_eq!(encode("😀"), "%F0%9F%98%80");
}

#[test]
fn rejects_unpaired_surrogates_with_uri_error() {
    for units in [vec![0x61, 0xd800], vec![0xdc00, 0x62], vec![0xde00, 0xd83d]] {
        let error = encode_uri_component(&JsString::from_utf16(units)).expect_err("lone surrogate throws");
        assert_eq!(error.to_string(), "URIError: URI malformed");
    }
    let pair = JsString::from_utf16(vec![0xd83d, 0xde00]);
    assert_eq!(encode_uri_component(&pair).expect("pair encodes"), "%F0%9F%98%80");
}
