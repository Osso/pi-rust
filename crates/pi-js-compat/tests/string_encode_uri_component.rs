use pi_js_compat::string::encode_uri_component;

#[test]
fn encodes_uri_components_like_javascript() {
    assert_eq!(encode_uri_component("session-1"), "session-1");
    assert_eq!(encode_uri_component("a b"), "a%20b");
    assert_eq!(encode_uri_component("a/b?c=d&e"), "a%2Fb%3Fc%3Dd%26e");
    assert_eq!(encode_uri_component("-_.!~*'()"), "-_.!~*'()");
    assert_eq!(encode_uri_component("café"), "caf%C3%A9");
    assert_eq!(encode_uri_component("😀"), "%F0%9F%98%80");
}
