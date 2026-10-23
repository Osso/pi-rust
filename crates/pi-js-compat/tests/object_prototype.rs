#[path = "support/oracle.rs"]
mod oracle;
use pi_js_compat::object_prototype::{INHERITED_OBJECT_PROPERTIES, inherited_property_string};
use serde_json::json;

#[test]
fn inherited_ordinary_object_values_match_pinned_node() {
    let mut source = oracle::TsOracle::spawn("interactive-login-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"operation":"prototype"}));
    assert_eq!(expected["ok"], true, "{expected}");
    let actual:Vec<_>=INHERITED_OBJECT_PROPERTIES.iter().map(|(name,text)|json!({"name":name,"text":text,"truthy":true,"numericNaN":true,"type":if *name=="__proto__" {"object"}else{"function"}})).collect();
    assert_eq!(json!(actual), expected["output"]["properties"]);
    for (name, text) in INHERITED_OBJECT_PROPERTIES {
        assert_eq!(inherited_property_string(name), Some(*text));
    }
    for key in ["", "prototype", "Constructor", "missing", "__proto__x"] {
        assert_eq!(inherited_property_string(key), None);
    }
}
