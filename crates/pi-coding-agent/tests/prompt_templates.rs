use pi_coding_agent::core::prompt_templates::{
    PromptTemplate, expand_prompt_template, parse_command_args, substitute_args,
};
use pi_js_compat::js_json::JsString;

fn template(name: &str, content: &str) -> PromptTemplate {
    PromptTemplate {
        name: name.to_owned().into(),
        description: "test".to_owned().into(),
        argument_hint: None,
        content: content.to_owned().into(),
        source_info: pi_coding_agent::core::source_info::create_synthetic_source_info(
            &format!("/tmp/{name}.md").into(),
            &pi_coding_agent::core::source_info::SyntheticSourceInfoOptions {
                source: "local".to_owned().into(),
                scope: None,
                origin: None,
                base_dir: None,
            },
        ),
        file_path: format!("/tmp/{name}.md").into(),
    }
}

#[test]
fn parses_quoted_arguments_and_drops_empty_quotes() {
    assert_eq!(parse_command_args("a b c"), vec!["a", "b", "c"]);
    assert_eq!(parse_command_args("\"first arg\" second"), vec!["first arg", "second"]);
    assert_eq!(parse_command_args("'first arg' second"), vec!["first arg", "second"]);
    assert_eq!(parse_command_args("\"\" \" \""), vec![" "]);
    assert_eq!(parse_command_args("a\n\n\tb  c"), vec!["a", "b", "c"]);
    assert_eq!(parse_command_args(""), Vec::<String>::new());
}

#[test]
fn substitutes_positional_wildcards_and_slices() {
    let args = ["cmd", "arg1", "arg2", "arg3"].map(JsString::from);
    assert_eq!(
        substitute_args(&JsString::from("$1: ${@:2}"), &args),
        "cmd: arg1 arg2 arg3"
    );
    assert_eq!(substitute_args(&JsString::from("${@:2:2}"), &args), "arg1 arg2");
    assert_eq!(substitute_args(&JsString::from("${@:0}"), &args), "cmd arg1 arg2 arg3");
    assert_eq!(substitute_args(&JsString::from("${3:-fallback}"), &args), "arg2");
    assert_eq!(substitute_args(&JsString::from("${9:-fallback}"), &args), "fallback");
    assert_eq!(
        substitute_args(&JsString::from("$1.5"), &["a".to_owned().into()]),
        "a.5"
    );
}

#[test]
fn expands_matching_templates_and_leaves_others_untouched() {
    let templates = vec![template("arg-test", "- arg1: $1\n- rest: ${@:2}")];

    assert_eq!(
        expand_prompt_template("/arg-test label-2\n\nHere is some description #2.", &templates),
        "- arg1: label-2\n- rest: Here is some description #2."
    );
    assert_eq!(
        expand_prompt_template("/arg-test\nlabel-2", &templates),
        "- arg1: label-2\n- rest: "
    );
    assert_eq!(expand_prompt_template("/missing arg", &templates), "/missing arg");
    assert_eq!(expand_prompt_template("plain text", &templates), "plain text");
}

#[test]
fn keeps_trailing_whitespace_inside_unclosed_quoted_argument() {
    // Frozen prompt-templates.ts:275-280 passes the captured tail untrimmed.
    let templates = vec![template("greet", "[$1]")];
    assert_eq!(expand_prompt_template("/greet \"hello ", &templates), "[hello ]");
    assert_eq!(expand_prompt_template("/greet   world", &templates), "[world]");
}
