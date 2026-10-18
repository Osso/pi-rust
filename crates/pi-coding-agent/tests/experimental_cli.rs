//! Ports `test/experimental-cli-command.test.ts` and `test/experimental-cli-resolution.test.ts`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pi_coding_agent::cli::experimental::auth::AuthInput;
use pi_coding_agent::cli::experimental::cli::{ExperimentalCliContext, ExperimentalCommand, experimental_cli};
use pi_coding_agent::cli::experimental::command::{CommandParseResult, NamedCommandInvocation};
use pi_coding_agent::cli::experimental::commands::client::ClientCommand;
use pi_coding_agent::cli::experimental::commands::pi::PiCommand;
use pi_coding_agent::cli::experimental::commands::server::ServerCommand;
use pi_coding_agent::cli::experimental::transport_address::{TransportAddress, TransportKind};

const UNSUPPORTED_SERVER_OPTIONS: &str = "The experimental server command does not support existing CLI options yet";
const UNSUPPORTED_CLIENT_OPTIONS: &str = "The experimental client command does not support existing CLI options yet";

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn parse(values: &[&str]) -> CommandParseResult<ExperimentalCommand> {
    experimental_cli().parse(&args(values))
}

fn ok(values: &[&str]) -> ExperimentalCommand {
    match parse(values) {
        CommandParseResult::Ok(command) => command,
        CommandParseResult::Errors(errors) => panic!("expected success for {values:?}, got {errors:?}"),
    }
}

fn errors(values: &[&str]) -> Vec<String> {
    match parse(values) {
        CommandParseResult::Errors(errors) => errors,
        CommandParseResult::Ok(command) => panic!("expected errors for {values:?}, got {command:?}"),
    }
}

fn pi(values: &[&str]) -> PiCommand {
    match ok(values) {
        ExperimentalCommand::Pi(command) => *command,
        other => panic!("expected pi command, got {other:?}"),
    }
}

fn unix(path: &str) -> TransportAddress {
    TransportAddress {
        transport: TransportKind::Unix,
        path: path.to_owned(),
    }
}

#[test]
fn selects_pi_mode_and_parses_existing_cli_arguments() {
    let command = pi(&[
        "--provider",
        "anthropic",
        "--model",
        "claude-sonnet",
        "--thinking",
        "high",
        "inspect",
        "the project",
    ]);
    assert_eq!(command.options.provider.as_deref(), Some("anthropic"));
    assert_eq!(command.options.model.as_deref(), Some("claude-sonnet"));
    assert_eq!(
        command.options.thinking,
        Some(pi_agent_core::harness::ThinkingLevel::High)
    );
    assert_eq!(command.options.messages, vec!["inspect", "the project"]);
    assert_eq!(command.auth, None);
    assert!(command.listen.is_empty());
}

#[test]
fn parses_a_server_listener() {
    match ok(&["server", "--listen", "unix:///tmp/pi.sock"]) {
        ExperimentalCommand::Server(command) => {
            assert_eq!(command.listen, vec![unix("/tmp/pi.sock")]);
            assert_eq!(command.auth, None);
        }
        other => panic!("expected server command, got {other:?}"),
    }
}

#[test]
fn leaves_experimental_looking_existing_option_values_with_the_existing_parser() {
    let command = pi(&["--system-prompt", "--listen", "unix:///tmp/pi.sock"]);
    assert_eq!(command.options.system_prompt.as_deref(), Some("--listen"));
    assert_eq!(command.options.messages, vec!["unix:///tmp/pi.sock"]);
    assert!(command.listen.is_empty());
}

#[test]
fn stops_parsing_command_options_when_existing_cli_arguments_begin() {
    let command = pi(&["--model", "claude-sonnet", "--listen=unix:///tmp/second.sock"]);
    assert_eq!(command.options.model.as_deref(), Some("claude-sonnet"));
    assert!(command.listen.is_empty());
    assert_eq!(
        command.options.unknown_flags.get("listen"),
        Some(&pi_coding_agent::cli::args::UnknownFlagValue::String(
            "unix:///tmp/second.sock".to_owned()
        ))
    );
}

#[test]
fn parses_a_client_transport_address() {
    match ok(&["client", "--connect", "unix:///tmp/pi.sock"]) {
        ExperimentalCommand::Client(command) => {
            assert_eq!(command.connect, Some(unix("/tmp/pi.sock")));
            assert_eq!(command.auth, None);
        }
        other => panic!("expected client command, got {other:?}"),
    }
}

#[test]
fn parses_authentication_sources() {
    assert_eq!(
        pi(&["--auth-token", "secret"]).auth,
        Some(AuthInput::Token {
            token: "secret".to_owned()
        })
    );
    assert_eq!(
        pi(&["--auth-token-file", "/tmp/token"]).auth,
        Some(AuthInput::File {
            path: "/tmp/token".to_owned()
        })
    );
}

#[test]
fn permits_omitted_authentication() {
    for argv in [vec![], vec!["server"], vec!["client"]] {
        let command = ok(&argv);
        let auth = match &command {
            ExperimentalCommand::Pi(command) => &command.auth,
            ExperimentalCommand::Server(command) => &command.auth,
            ExperimentalCommand::Client(command) => &command.auth,
        };
        assert_eq!(*auth, None, "argv: {argv:?}");
    }
}

#[test]
fn passes_unknown_options_file_arguments_and_the_separator_to_the_existing_parser() {
    let command = pi(&["--unknown", "@prompt.md", "--", "--listen", "unix:///tmp/pi.sock"]);
    assert_eq!(command.options.file_args, vec!["prompt.md"]);
    assert_eq!(
        command.options.unknown_flags.get("unknown"),
        Some(&pi_coding_agent::cli::args::UnknownFlagValue::Boolean(true))
    );
    assert_eq!(
        command.options.unknown_flags.get("listen"),
        Some(&pi_coding_agent::cli::args::UnknownFlagValue::String(
            "unix:///tmp/pi.sock".to_owned()
        ))
    );
    assert!(command.listen.is_empty());
}

#[test]
fn rejects_invalid_experimental_input() {
    let cases: Vec<(Vec<&str>, &str)> = vec![
        (
            vec![
                "--listen",
                "unix:///tmp/pi.sock",
                "--listen",
                "unix:///tmp/pi-admin.sock",
            ],
            "--listen may only be specified once",
        ),
        (
            vec!["--auth-token", "secret", "--auth-token-file", "/tmp/token"],
            "--auth-token and --auth-token-file are mutually exclusive",
        ),
        (
            vec!["--auth-token", "first", "--auth-token", "second"],
            "--auth-token may only be specified once",
        ),
        (
            vec!["--auth-token-file", "/tmp/first", "--auth-token-file=/tmp/second"],
            "--auth-token-file may only be specified once",
        ),
        (
            vec!["--listen", "/tmp/pi.sock"],
            "Invalid --listen address \"/tmp/pi.sock\"",
        ),
        (
            vec!["--listen", "ws://localhost:8080"],
            "Unsupported --listen transport \"ws:\"",
        ),
        (
            vec!["--listen", "unix://relative.sock"],
            "Unix transport address must not include an authority",
        ),
        (
            vec!["--listen", "unix:///tmp/pi.sock?wrong=value"],
            "Invalid --listen address \"unix:///tmp/pi.sock?wrong=value\"",
        ),
        (
            vec!["--listen", "unix:///tmp/pi.sock#fragment"],
            "Invalid --listen address \"unix:///tmp/pi.sock#fragment\"",
        ),
        (
            vec!["--listen", "unix:/tmp/pi.sock"],
            "Invalid --listen address \"unix:/tmp/pi.sock\"",
        ),
        (
            vec!["--listen", "unix:///tmp/%00pi.sock"],
            "Invalid --listen address \"unix:///tmp/%00pi.sock\"",
        ),
        (
            vec!["client", "--listen", "unix:///tmp/pi.sock"],
            UNSUPPORTED_CLIENT_OPTIONS,
        ),
        (
            vec!["server", "--connect", "unix:///tmp/pi.sock"],
            UNSUPPORTED_SERVER_OPTIONS,
        ),
        (
            vec!["client", "--connect", "ws://localhost:8080"],
            "Unsupported --connect transport \"ws:\"",
        ),
        (vec!["--listen"], "--listen requires a value"),
        (vec!["--connect="], "--connect is only valid for client mode"),
    ];
    for (argv, expected) in cases {
        let errors = errors(&argv);
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "argv: {argv:?}, expected {expected:?}, got {errors:?}"
        );
    }
}

#[test]
fn rejects_unsupported_options_without_parsing_them() {
    assert_eq!(
        errors(&[
            "client",
            "--listen",
            "ws://localhost:8080",
            "--auth-token",
            "secret",
            "--auth-token-file",
            "/tmp/token"
        ]),
        vec![UNSUPPORTED_CLIENT_OPTIONS.to_owned()]
    );
}

#[test]
fn treats_command_names_after_the_first_argument_as_existing_cli_arguments() {
    let command = pi(&["--cwd", "/workspace", "server"]);
    assert_eq!(command.options.messages, vec!["server"]);
    assert_eq!(
        command.options.unknown_flags.get("cwd"),
        Some(&pi_coding_agent::cli::args::UnknownFlagValue::String(
            "/workspace".to_owned()
        ))
    );
}

#[test]
fn composes_pi_command_options_with_the_existing_parser() {
    let command = pi(&[
        "--listen",
        "unix:///tmp/pi.sock",
        "--auth-token",
        "secret",
        "--provider",
        "anthropic",
        "--model",
        "claude-sonnet",
        "--thinking",
        "high",
        "inspect",
    ]);
    assert_eq!(command.listen, vec![unix("/tmp/pi.sock")]);
    assert_eq!(
        command.auth,
        Some(AuthInput::Token {
            token: "secret".to_owned()
        })
    );
    assert_eq!(command.options.provider.as_deref(), Some("anthropic"));
    assert_eq!(command.options.model.as_deref(), Some("claude-sonnet"));
    assert_eq!(command.options.messages, vec!["inspect"]);
}

#[test]
fn keeps_pi_help_and_version_handling_in_existing_cli_options() {
    assert_eq!(pi(&["--help"]).options.help, Some(true));
    assert_eq!(pi(&["--version"]).options.version, Some(true));
}

#[test]
fn rejects_deferred_help_and_version_for_server_and_client() {
    for (argv, expected) in [
        (vec!["server", "--help"], UNSUPPORTED_SERVER_OPTIONS),
        (vec!["server", "--version"], UNSUPPORTED_SERVER_OPTIONS),
        (vec!["client", "--help"], UNSUPPORTED_CLIENT_OPTIONS),
        (vec!["client", "--version"], UNSUPPORTED_CLIENT_OPTIONS),
    ] {
        assert_eq!(errors(&argv), vec![expected.to_owned()], "argv: {argv:?}");
    }
}

#[test]
fn rejects_existing_options_that_server_and_client_do_not_support_yet() {
    assert_eq!(
        errors(&["server", "--model", "claude-sonnet", "prompt"]),
        vec![UNSUPPORTED_SERVER_OPTIONS.to_owned()]
    );
    assert_eq!(
        errors(&["client", "--tui-mode", "fullscreen", "@prompt.md"]),
        vec![UNSUPPORTED_CLIENT_OPTIONS.to_owned()]
    );
}

#[test]
fn reports_existing_parser_errors_before_capability_errors() {
    assert_eq!(
        errors(&["client", "--tui-mode", "wrong", "--model", "claude-sonnet"]),
        vec![
            "Invalid TUI mode \"wrong\". Valid values: regular, fullscreen".to_owned(),
            UNSUPPORTED_CLIENT_OPTIONS.to_owned()
        ]
    );
}

#[test]
fn parses_an_empty_server_command() {
    match ok(&["server"]) {
        ExperimentalCommand::Server(command) => {
            assert_eq!(command.auth, None);
            assert!(command.listen.is_empty());
        }
        other => panic!("expected server command, got {other:?}"),
    }
}

#[test]
fn executes_the_parsed_command() {
    let pi_calls = Arc::new(AtomicUsize::new(0));
    let server_calls = Arc::new(AtomicUsize::new(0));
    let client_calls = Arc::new(AtomicUsize::new(0));

    let pi_counter = Arc::clone(&pi_calls);
    let server_counter = Arc::clone(&server_calls);
    let client_counter = Arc::clone(&client_calls);
    let mut context = ExperimentalCliContext {
        run_pi: Box::new(move |_: &PiCommand| {
            pi_counter.fetch_add(1, Ordering::SeqCst);
        }),
        run_server: Box::new(move |_: &ServerCommand| {
            server_counter.fetch_add(1, Ordering::SeqCst);
        }),
        run_client: Box::new(move |_: &ClientCommand| {
            client_counter.fetch_add(1, Ordering::SeqCst);
        }),
    };

    let cli = experimental_cli();
    for (argv, expected) in [
        (vec![], ("pi", 1, 0, 0)),
        (vec!["server"], ("server", 1, 1, 0)),
        (vec!["client"], ("client", 1, 1, 1)),
    ] {
        let command = cli.execute(&args(&argv), &mut context).unwrap();
        assert_eq!(command.command(), expected.0);
        assert_eq!(pi_calls.load(Ordering::SeqCst), expected.1);
        assert_eq!(server_calls.load(Ordering::SeqCst), expected.2);
        assert_eq!(client_calls.load(Ordering::SeqCst), expected.3);
    }
}
