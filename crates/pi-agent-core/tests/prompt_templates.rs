use pi_agent_core::harness::PromptTemplate;
use pi_agent_core::harness::prompt_templates::{
    format_prompt_template_invocation, parse_command_args, substitute_args,
};

fn owned(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn parse_command_args_handles_quotes_delimiters_and_empty_tokens() {
    assert!(parse_command_args("").is_empty());
    assert!(parse_command_args(" \t  \t").is_empty());
    assert_eq!(
        parse_command_args("  a\t\t'b c' \"d e\" pre'quoted'\"text\" 'f\"g' \"h'i\" '' \"\" "),
        owned(&["a", "b c", "d e", "prequotedtext", "f\"g", "h'i"])
    );
    assert_eq!(parse_command_args("'unterminated"), owned(&["unterminated"]));
    assert_eq!(parse_command_args("a\\b \"c\\\\d\""), owned(&["a\\b", "c\\\\d"]));
}

#[test]
fn parse_command_args_preserves_non_delimiter_content() {
    assert_eq!(parse_command_args("a\nb\rc\u{00a0}d"), owned(&["a\nb\rc\u{00a0}d"]));
    assert_eq!(parse_command_args("😀 e\u{301}\0,!?"), owned(&["😀", "e\u{301}\0,!?"]));
}

#[test]
fn substitute_args_replaces_positional_values_with_source_indexing() {
    let args = owned(&["one", "two"]);
    assert_eq!(
        substitute_args("$0|$01|$0002|$3|$999999999999999999999", &args),
        "|one|two||",
    );

    let replacement = owned(&["$1"]);
    assert_eq!(substitute_args("$1", &replacement), "$1");
}

#[test]
fn substitute_args_replaces_slices_and_preserves_invalid_forms() {
    let args = owned(&["one", "two", "three"]);
    assert_eq!(
        substitute_args(
            "${@:0}|${@:01}|${@:1:0}|${@:0002:003}|${@:999999999999999999999}|${@:1:999999999999999999999}|${@:1:}|${@:-1}|${@:1.5}",
            &args,
        ),
        "one two three|one two three||two three||one two three|${@:1:}|${@:-1}|${@:1.5}",
    );

    let replacement = owned(&["${@:2}", "tail"]);
    assert_eq!(substitute_args("${@:1:1}", &replacement), "${@:2}");
}

#[test]
fn substitute_args_applies_passes_in_source_order() {
    let aggregate = owned(&["$ARGUMENTS", "tail"]);
    assert_eq!(substitute_args("$1", &aggregate), "$ARGUMENTS tail");

    let at = owned(&["$@", "tail"]);
    assert_eq!(substitute_args("$1", &at), "$@ tail");
    assert_eq!(substitute_args("$ARGUMENTS", &at), "$@ tail tail");
}

#[test]
fn substitute_args_expands_javascript_replacement_tokens() {
    let dollars = owned(&["$$"]);
    assert_eq!(substitute_args("$ARGUMENTS", &dollars), "$");

    let match_text = owned(&["$&"]);
    assert_eq!(substitute_args("$ARGUMENTS", &match_text), "$ARGUMENTS");

    let prefix = owned(&["$`"]);
    assert_eq!(
        substitute_args("x$ARGUMENTS y$ARGUMENTS z", &prefix),
        "xx yx$ARGUMENTS y z",
    );

    let suffix = owned(&["$'"]);
    assert_eq!(
        substitute_args("x$ARGUMENTS y$ARGUMENTS z", &suffix),
        "x y$ARGUMENTS z y z z",
    );

    let nonexistent_capture = owned(&["$1"]);
    assert_eq!(substitute_args("$ARGUMENTS", &nonexistent_capture), "$1");

    let nonexistent_named_capture = owned(&["$<name>"]);
    assert_eq!(substitute_args("$ARGUMENTS", &nonexistent_named_capture), "$<name>");

    let at_prefix = owned(&["$`"]);
    assert_eq!(substitute_args("x$@ y$@ z", &at_prefix), "xx yx$@ y z");
}

#[test]
fn substitute_args_preserves_special_argument_values() {
    let args = owned(&["😀\0x", "e\u{301}", ""]);
    assert_eq!(
        substitute_args("[$1]|[$ARGUMENTS]|[$@]", &args),
        "[😀\0x]|[😀\0x e\u{301} ]|[😀\0x e\u{301} ]",
    );

    let empty_values = owned(&["", "x", ""]);
    assert_eq!(substitute_args("<$ARGUMENTS>", &empty_values), "< x >");
}

#[test]
fn format_prompt_template_uses_only_content_and_empty_args() {
    let mut template = PromptTemplate {
        name: "ignored".to_owned(),
        description: Some("also ignored".to_owned()),
        content: "$1".to_owned(),
    };

    assert_eq!(format_prompt_template_invocation(&template, &[]), "");

    template.name = "changed".to_owned();
    template.description = None;
    let args = owned(&["value"]);
    assert_eq!(format_prompt_template_invocation(&template, &args), "value");
}
