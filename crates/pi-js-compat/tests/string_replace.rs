use pi_js_compat::string::{replace, replace_global};

#[test]
fn replaces_only_the_first_occurrence() {
    assert_eq!(replace("a-b-a", "a", "x"), "x-b-a");
    assert_eq!(replace("abc", "z", "x"), "abc");
}

#[test]
fn expands_dollar_escapes_in_the_replacement() {
    assert_eq!(replace("{{A}}", "{{A}}", "$$"), "$");
    assert_eq!(replace("{{A}}", "{{A}}", "$&"), "{{A}}");
    assert_eq!(replace("pre{{A}}post", "{{A}}", "$`"), "preprepost");
    assert_eq!(replace("pre{{A}}post", "{{A}}", "$'"), "prepostpost");
    assert_eq!(replace("{{A}}", "{{A}}", "$1 $<name> $"), "$1 $<name> $");
}

#[test]
fn replaces_at_the_start_for_an_empty_search() {
    assert_eq!(replace("abc", "", "x"), "xabc");
}

#[test]
fn globally_expands_against_each_original_match() {
    assert_eq!(
        replace_global("aXaXz", "X", "[$$|$&|$`|$'|$1]"),
        "a[$|X|a|aXz|$1]a[$|X|aXa|z|$1]z"
    );
    assert_eq!(replace_global("XX", "X", "X"), "XX");
    assert_eq!(replace_global("abc", "z", "x"), "abc");
}

#[test]
fn keeps_regex_character_class_dollars_out_of_the_expansion() {
    // highlight.min.js contains `$&` inside a regex character class; the
    // injected template placeholder is substituted by JavaScript.
    assert_eq!(
        replace("<script>{{H}}</script>", "{{H}}", "[-+$&|]"),
        "<script>[-+{{H}}|]</script>"
    );
}
