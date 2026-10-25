//! Differential test for `package-manager-cli.ts` argument/validation output.

use crate::shared_oracle as oracle;

use oracle::TsOracle;
use pi_coding_agent::package_manager_cli::{handle_config_command, handle_package_command};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

fn check(oracle: &mut TsOracle, operation: &str, args: &[&str]) {
    let response = oracle.request(&json!({
        "id": format!("{operation}:{args:?}"),
        "operation": operation,
        "args": args,
    }));
    assert_eq!(response["ok"], true, "oracle failed for {args:?}: {response}");
    let output = &response["output"];

    let outcome = if operation == "handle_config" {
        handle_config_command(
            &args
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect::<Vec<_>>()
                .iter()
                .map(JsString::from)
                .collect::<Vec<_>>(),
        )
    } else {
        handle_package_command(
            &args
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect::<Vec<_>>()
                .iter()
                .map(JsString::from)
                .collect::<Vec<_>>(),
        )
    };
    let Some(outcome) = outcome else {
        assert_eq!(output["handled"], Value::Bool(false), "{args:?}");
        return;
    };
    assert_eq!(output["handled"], Value::Bool(true), "{args:?}");
    assert_eq!(json!(outcome.stdout), output["stdout"], "stdout differs for {args:?}");
    assert_eq!(json!(outcome.stderr), output["stderr"], "stderr differs for {args:?}");
    assert_eq!(
        json!(outcome.exit_code),
        output["exitCode"],
        "exit code differs for {args:?}"
    );
}

#[test]
fn differential_help_and_validation_output() {
    let mut oracle = TsOracle::spawn(
        "package-manager-cli-oracle.ts",
        "packages/coding-agent/src/package-manager-cli.ts",
    );
    let cases: &[&[&str]] = &[
        &["install", "--help"],
        &["remove", "-h"],
        &["uninstall", "--help"],
        &["update", "--help"],
        &["list", "--help"],
        &["install", "--bogus"],
        &["install", "-l", "--bogus"],
        &["install"],
        &["remove"],
        &["uninstall"],
        &["install", "npm:x", "extra"],
        &["list", "--local"],
        &["update", "--bogus"],
        &["update", "--extension"],
        &["update", "--extension", "--self"],
        &["update", "--extension", "a", "--extension", "b"],
        &["update", "--all", "--self"],
        &["update", "--all", "--models"],
        &["update", "--models", "--all"],
        &["update", "--models", "source"],
        &["update", "--all", "source"],
        &["update", "--extension", "a", "source"],
        &["update", "--self", "source"],
        &["update", "--extensions", "source"],
    ];
    for args in cases {
        let operation = if args[0] == "config" {
            "handle_config"
        } else {
            "handle_package_command"
        };
        check(&mut oracle, operation, args);
    }
}

#[test]
fn differential_non_package_commands_are_not_handled() {
    let mut oracle = TsOracle::spawn(
        "package-manager-cli-oracle.ts",
        "packages/coding-agent/src/package-manager-cli.ts",
    );
    for args in [&["prompt"][..], &["help"][..], &[][..]] {
        let response = oracle.request(&json!({
            "id": "non-command",
            "operation": "handle_package_command",
            "args": args,
        }));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["output"]["handled"], Value::Bool(false), "{args:?}");
        let owned: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        assert!(
            handle_package_command(&owned.iter().map(JsString::from).collect::<Vec<_>>()).is_none(),
            "{args:?}"
        );
        assert!(
            handle_config_command(&owned.iter().map(JsString::from).collect::<Vec<_>>()).is_none(),
            "{args:?}"
        );
    }
}
