#[path = "../examples/extensions/plan_mode/utils.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
use serde_json::{Value, json};

#[test]
fn all_helpers_match_broad_pinned_corpus() {
    let mut texts: Vec<String> = vec![
        "",
        "Plan:\n1. Create the sample file\n2) Verify all results\n9. `skip this`\n4. /skip this\n5. -skip this",
        "**Plan:**\n  99. **Read the source file**\n1) tiny\n2. Run `cargo test` now",
        "[DONE:1][done:1][DONE:9][DONE:0][DONE:9007199254740993]",
        "Use the **bold** and `code`",
        "READ the source",
        "Use\u{feff}the\u{a0}file name",
        "é中🦀",
        "***odd** and *text*",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for command in [
        "cat",
        "head",
        "tail",
        "less",
        "more",
        "grep",
        "find",
        "ls",
        "pwd",
        "echo",
        "printf",
        "wc",
        "sort",
        "uniq",
        "diff",
        "file",
        "stat",
        "du",
        "df",
        "tree",
        "which",
        "whereis",
        "type",
        "env",
        "printenv",
        "uname",
        "whoami",
        "id",
        "date",
        "cal",
        "uptime",
        "ps",
        "top",
        "htop",
        "free",
        "jq",
        "awk",
        "rg",
        "fd",
        "bat",
        "eza",
        "git status",
        "git branch -v",
        "git ls-files",
        "GIT LOG",
        "git config --get foo",
        "npm audit",
        "yarn why x",
        "node --version",
        "python --version",
        "curl x",
        "wget -O - x",
        "sed -n '1p' x",
    ] {
        for suffix in [
            "",
            " x",
            " > out",
            " >> out",
            " | rm x",
            " && touch x",
            " ; git checkout x",
            " sudo",
            "\nkill 2",
        ] {
            texts.push(format!("  {command}{suffix}"));
        }
    }
    for word in [
        "rm",
        "rmdir",
        "mv",
        "cp",
        "mkdir",
        "touch",
        "chmod",
        "chown",
        "chgrp",
        "ln",
        "tee",
        "truncate",
        "dd",
        "shred",
        "npm install",
        "yarn add",
        "pnpm remove",
        "pip uninstall",
        "apt-get update",
        "brew upgrade",
        "git push",
        "sudo",
        "su",
        "pkill",
        "killall",
        "reboot",
        "shutdown",
        "systemctl restart",
        "service x stop",
        "vi",
        "vim",
        "nano",
        "emacs",
        "code",
        "subl",
    ] {
        texts.push(format!("echo {word}"));
        texts.push(format!("echo {}", word.to_uppercase()));
    }
    for length in [3, 5, 46, 47, 48, 50, 51, 70] {
        texts.push(format!("Create {}", "a".repeat(length)));
    }
    let request: Vec<_> = texts.into_iter().map(|text| json!({"text":text,"todos":[{"step":1,"text":"First","completed":false},{"step":9,"text":"Ninth","completed":false}]})).collect();
    let mut oracle = oracle::TsOracle::spawn(
        "extension-examples-g-utils-oracle.ts",
        "packages/coding-agent/examples/extensions/plan-mode/utils.ts",
    );
    let expected = oracle.request(&json!(request));
    let actual: Vec<_> = request.iter().map(|item| {
        let text = item["text"].as_str().unwrap();
        let mut todos: Vec<example::TodoItem> = serde_json::from_value(item["todos"].clone()).unwrap();
        let marked = example::mark_completed_steps(text, &mut todos);
        json!({"safe":example::is_safe_command(text),"clean":example::clean_step_text(text),"todos":example::extract_todo_items(text),"done":example::extract_done_steps(text),"marked":marked,"updated":todos})
    }).collect();
    for ((input, actual), expected) in request.iter().zip(actual).zip(expected.as_array().unwrap()) {
        assert_eq!(
            pi_js_compat::js_json::stringify(&actual),
            pi_js_compat::js_json::stringify(expected),
            "input: {}",
            input["text"]
        );
    }
    assert!(request.len() > 500);
    let _: Value = expected;
}
