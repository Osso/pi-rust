use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::Path;

use pi_coding_agent::core::prompt_templates::{
    LoadPromptTemplatesOptions, PromptTemplate, expand_prompt_template, load_prompt_templates, parse_command_args,
    substitute_args,
};
use serde_json::json;
use tempfile::TempDir;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-prompt-templates-oracle.ts",
        "packages/coding-agent/src/core/prompt-templates.ts",
    );

    compare_parse_args(&mut oracle);
    compare_substitute(&mut oracle);
    compare_expand(&mut oracle);
    compare_load(&mut oracle);
}

fn compare_parse_args(oracle: &mut oracle::TsOracle) {
    let cases = [
        "a b c",
        "\"first arg\" second",
        "'first arg' second",
        "\"double\" 'single' \"double again\"",
        "",
        "a  b   c",
        "a\tb\tc",
        "\"\" \" \"",
        "$100 @user #tag",
        "日本語 🎉 café",
        "\"line1\nline2\" second",
        "label-2\n\nHere is some description #2.",
        "a\n\n\tb  c",
        "\"quoted \\\"text\\\"\"",
        "a b c   ",
        "   a b c",
    ];

    for (index, input) in cases.iter().enumerate() {
        let response = oracle.request(&json!({"id": index, "operation": "parse_args", "input": input}));
        assert_eq!(response["ok"], true, "oracle failed: {response}");
        assert_eq!(
            json!(parse_command_args(*input)),
            response["output"]["args"],
            "{input:?}"
        );
    }
}

fn compare_substitute(oracle: &mut oracle::TsOracle) {
    let cases: Vec<(&str, Vec<&str>)> = vec![
        ("Test: $ARGUMENTS", vec!["a", "b", "c"]),
        ("Test: $@", vec!["a", "b", "c"]),
        ("$ARGUMENTS", vec!["$1", "$ARGUMENTS"]),
        ("$@", vec!["$100", "$1"]),
        ("$1: $ARGUMENTS", vec!["prefix", "a", "b"]),
        ("$1: $@", vec!["prefix", "a", "b"]),
        ("Test: $ARGUMENTS", vec![]),
        ("Test: $1", vec![]),
        ("$ARGUMENTS and $ARGUMENTS", vec!["a", "b"]),
        ("$1 $2: $ARGUMENTS", vec!["arg100", "@user"]),
        ("$1 $2 $3 $4 $5", vec!["a", "b"]),
        ("$ARGUMENTS", vec!["日本語", "🎉", "café"]),
        ("$1 $2", vec!["line1\nline2", "tab\tthere"]),
        ("$1$2", vec!["a", "b"]),
        ("$0", vec!["a", "b"]),
        ("$1.5", vec!["a"]),
        ("pre$ARGUMENTS", vec!["a", "b"]),
        ("$ARGUMENTS", vec!["a", "", "c"]),
        ("$ARGUMENTS", vec!["  leading  ", "trailing  "]),
        ("Prefix $ARGUMENTS suffix", vec!["ARGUMENTS"]),
        ("$A $$ $ $ARGS", vec!["a"]),
        ("$arguments $Arguments $ARGUMENTS", vec!["a", "b"]),
        ("Price: \\$100", vec![]),
        ("$1: $@ ($ARGUMENTS)", vec!["first", "second", "third"]),
        ("Just plain text", vec!["a", "b"]),
        ("$1 $2 $@", vec!["a", "b", "c"]),
        ("List exactly ${1:-7} next steps", vec![]),
        ("${@:-default}\n${ARGUMENTS:-default}", vec![]),
        (
            "${@:-default}\n${ARGUMENTS:-default}",
            vec!["This", "would", "be", "the", "arguments"],
        ),
        ("List exactly ${1:-7} next steps", vec!["3"]),
        ("Mode: ${1:-brief}", vec![""]),
        ("${1:-7} ${2:-brief}", vec![]),
        ("${1:-7} ${2:-brief}", vec!["3"]),
        ("${1:-7} ${2:-brief}", vec!["3", "verbose"]),
        ("${1:-7}", vec!["$ARGUMENTS"]),
        ("${1:-$ARGUMENTS}", vec!["a", "b"]),
        ("${3:-$ARGUMENTS}", vec!["a", "b"]),
        ("${1:-seven steps}", vec![]),
        ("${3:-fallback}", vec!["a", "b"]),
        ("$1 ${2:-x} $ARGUMENTS", vec!["a"]),
        ("${@:2}", vec!["a", "b", "c", "d"]),
        ("${@:1}", vec!["a", "b", "c"]),
        ("${@:2:2}", vec!["a", "b", "c", "d"]),
        ("${@:1:1}", vec!["a", "b", "c"]),
        ("${@:2:3}", vec!["a", "b", "c", "d", "e"]),
        ("${@:99}", vec!["a", "b"]),
        ("${@:2:0}", vec!["a", "b", "c"]),
        ("${@:2:99}", vec!["a", "b", "c"]),
        ("${@:2} vs $@", vec!["a", "b", "c"]),
        ("First: ${@:1:1}, All: $@", vec!["x", "y", "z"]),
        ("${@:1}", vec!["${@:2}", "test"]),
        ("${@:2}", vec!["a", "${@:3}", "c"]),
        ("$1: ${@:2}", vec!["cmd", "arg1", "arg2"]),
        ("${@:0}", vec!["a", "b", "c"]),
        ("${@:2}", vec![]),
        ("${@:1}", vec!["only"]),
        ("Process ${@:2} with $1", vec!["tool", "file1", "file2"]),
        ("${@:1:1} and ${@:2}", vec!["a", "b", "c"]),
        ("${@:1:2} vs ${@:3:2}", vec!["a", "b", "c", "d", "e"]),
        ("prefix${@:2}suffix", vec!["a", "b", "c"]),
        (
            "${@:5:100}",
            vec![
                "arg1", "arg2", "arg3", "arg4", "arg5", "arg6", "arg7", "arg8", "arg9", "arg10",
            ],
        ),
    ];

    for (index, (content, args)) in cases.iter().enumerate() {
        let response = oracle.request(&json!({
            "id": 100 + index,
            "operation": "substitute",
            "content": content,
            "args": args,
        }));
        assert_eq!(response["ok"], true, "oracle failed: {response}");
        assert_eq!(
            json!(substitute_args(
                &JsString::from(*content),
                &args.iter().map(|arg| JsString::from(*arg)).collect::<Vec<_>>()
            )),
            response["output"]["content"],
            "{content:?} with {args:?}"
        );
    }
}

fn compare_expand(oracle: &mut oracle::TsOracle) {
    let templates = vec![
        template("arg-test", "- arg1: $1\n- rest: ${@:2}"),
        template("plain", "Plain content"),
    ];
    let cases = [
        ("/arg-test label-2\n\nHere is some description #2.", templates.clone()),
        ("/arg-test\nlabel-2", templates.clone()),
        ("/arg-test", templates.clone()),
        ("/missing arg", templates.clone()),
        ("not a command", templates.clone()),
        ("/plain", templates.clone()),
        ("/plain   ", templates.clone()),
    ];

    for (index, (text, templates)) in cases.iter().enumerate() {
        let response = oracle.request(&json!({
            "id": 300 + index,
            "operation": "expand",
            "text": text,
            "templates": templates,
        }));
        assert_eq!(response["ok"], true, "oracle failed: {response}");
        assert_eq!(
            json!(expand_prompt_template(*text, templates)),
            response["output"]["content"],
            "{text:?}"
        );
    }
}

fn compare_load(oracle: &mut oracle::TsOracle) {
    let temp = TempDir::new().expect("temp dir");
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("project");
    let global_prompts = agent_dir.join("prompts");
    let project_prompts = cwd.join(".pi").join("prompts");
    let explicit = temp.path().join("explicit");

    write_template(
        &global_prompts,
        "pr",
        "---\ndescription: Review PRs from URLs with structured issue and code analysis\nargument-hint: \"<PR-URL>\"\n---\nYou are given one or more GitHub PR URLs: $@",
    );
    write_template(
        &project_prompts,
        "wr",
        "---\ndescription: Finish the current task end-to-end with changelog, commit, and push\nargument-hint: \"[instructions]\"\n---\nWrap it. Additional instructions: $ARGUMENTS",
    );
    write_template(
        &explicit,
        "cl",
        "---\ndescription: Audit changelog entries before release\n---\nAudit changelog entries for all commits since the last release.",
    );
    write_template(
        &explicit,
        "empty-hint",
        "---\ndescription: A command with empty hint\nargument-hint: \"\"\n---\nDo something",
    );
    write_template(&explicit, "no-frontmatter", "First line description.\n\nBody text.");
    write_template(&explicit, "long-first-line", &format!("{}\n", "x".repeat(70)));
    fs::write(explicit.join("notes.txt"), "not markdown").unwrap();
    let explicit_file = explicit.join("single.md");
    fs::write(
        &explicit_file,
        "---\ndescription: Single explicit file\n---\nSingle body $1",
    )
    .unwrap();

    let cases: Vec<(Vec<String>, bool)> = vec![
        (Vec::new(), true),
        (vec![explicit.to_string_lossy().into_owned()], false),
        (
            vec![
                explicit_file.to_string_lossy().into_owned(),
                temp.path().join("missing").to_string_lossy().into_owned(),
                explicit.join("notes.txt").to_string_lossy().into_owned(),
            ],
            true,
        ),
    ];

    for (index, (prompt_paths, include_defaults)) in cases.iter().enumerate() {
        let response = oracle.request(&json!({
            "id": 400 + index,
            "operation": "load",
            "cwd": cwd.to_string_lossy(),
            "agentDir": agent_dir.to_string_lossy(),
            "promptPaths": prompt_paths,
            "includeDefaults": include_defaults,
        }));
        assert_eq!(response["ok"], true, "oracle failed: {response}");

        let actual = load_prompt_templates(&LoadPromptTemplatesOptions {
            cwd: cwd.to_string_lossy().into_owned().into(),
            agent_dir: agent_dir.to_string_lossy().into_owned().into(),
            prompt_paths: prompt_paths.iter().map(JsString::from).collect(),
            include_defaults: *include_defaults,
        })
        .expect("templates load");
        assert_eq!(json!(actual), response["output"]["templates"], "case {index}");
    }
}

fn template(name: &str, content: &str) -> PromptTemplate {
    let file_path = format!("/tmp/{name}.md");
    PromptTemplate {
        name: name.to_owned().into(),
        description: "test".to_owned().into(),
        argument_hint: None,
        content: content.to_owned().into(),
        source_info: pi_coding_agent::core::source_info::create_synthetic_source_info(
            &JsString::from(&file_path),
            &pi_coding_agent::core::source_info::SyntheticSourceInfoOptions {
                source: "local".to_owned().into(),
                scope: None,
                origin: None,
                base_dir: None,
            },
        ),
        file_path: file_path.into(),
    }
}

fn write_template(dir: &Path, name: &str, content: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(format!("{name}.md")), content).unwrap();
}
