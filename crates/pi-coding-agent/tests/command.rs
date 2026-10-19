use pi_coding_agent::cli::experimental::command::{
    Command, CommandExecuteError, CommandOption, CommandParseResult, NamedCommandInvocation, string_option,
    value_option,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Invocation {
    command: &'static str,
    name: Option<String>,
    count: Option<i64>,
    remaining: Vec<String>,
}

impl NamedCommandInvocation for Invocation {
    fn command(&self) -> &str {
        self.command
    }
}

fn count_option() -> CommandOption<i64> {
    value_option("--count", |value| {
        value
            .parse::<i64>()
            .map_err(|_| format!("--count must be a number, got \"{value}\""))
    })
}

fn build_root() -> Command<Invocation, Vec<String>> {
    let mut command = Command::new("pi");
    command.option(string_option("--name")).unwrap();
    command.option(count_option()).unwrap();
    command.build(|input| {
        CommandParseResult::Ok(Invocation {
            command: "pi",
            name: input.value(&string_option("--name")),
            count: input.value(&count_option()),
            remaining: input.remaining_args.to_vec(),
        })
    });
    command.action(|invocation, context: &mut Vec<String>| context.push(format!("pi:{}", invocation.command)));
    command
}

fn parsed<TCtx: 'static>(command: &Command<Invocation, TCtx>, argv: &[&str]) -> CommandParseResult<Invocation> {
    command
        .parse(&argv.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>())
        .expect("parse")
}

#[test]
fn parses_option_values_in_both_forms() {
    let command = build_root();
    let expected = CommandParseResult::Ok(Invocation {
        command: "pi",
        name: Some("demo".into()),
        count: Some(3),
        remaining: vec!["inspect".into(), "the project".into()],
    });
    assert_eq!(
        parsed(&command, &["--name", "demo", "--count", "3", "inspect", "the project"]),
        expected
    );
    assert_eq!(
        parsed(&command, &["--name=demo", "--count=3", "inspect", "the project"]),
        expected
    );
}

#[test]
fn stops_parsing_at_the_first_positional_argument() {
    let command = build_root();
    assert_eq!(
        parsed(&command, &["--name", "demo", "server", "--count", "3"]),
        CommandParseResult::Ok(Invocation {
            command: "pi",
            name: Some("demo".into()),
            count: None,
            remaining: vec!["server".into(), "--count".into(), "3".into()],
        })
    );
}

#[test]
fn stops_parsing_at_an_unknown_option() {
    let command = build_root();
    assert_eq!(
        parsed(&command, &["--unknown", "--name", "demo"]),
        CommandParseResult::Ok(Invocation {
            command: "pi",
            name: None,
            count: None,
            remaining: vec!["--unknown".into(), "--name".into(), "demo".into()],
        })
    );
}

#[test]
fn keeps_the_positional_separator_and_everything_after_it() {
    let command = build_root();
    assert_eq!(
        parsed(&command, &["--name", "demo", "--", "--count", "3"]),
        CommandParseResult::Ok(Invocation {
            command: "pi",
            name: Some("demo".into()),
            count: None,
            remaining: vec!["--".into(), "--count".into(), "3".into()],
        })
    );
}

#[test]
fn rejects_missing_and_duplicate_values() {
    let command = build_root();
    assert_eq!(
        parsed(&command, &["--name"]),
        CommandParseResult::Errors(vec!["--name requires a value".into()])
    );
    assert_eq!(
        parsed(&command, &["--name="]),
        CommandParseResult::Errors(vec!["--name requires a value".into()])
    );
    assert_eq!(
        parsed(&command, &["--name", "--count", "2"]),
        CommandParseResult::Errors(vec!["--name requires a value".into()])
    );
    assert_eq!(
        parsed(&command, &["--name", "a", "--name=b"]),
        CommandParseResult::Errors(vec!["--name may only be specified once".into()])
    );
}

#[test]
fn reports_option_parser_errors_and_keeps_parsing() {
    let command = build_root();
    assert_eq!(
        parsed(&command, &["--count", "many", "--name", "demo"]),
        CommandParseResult::Errors(vec!["--count must be a number, got \"many\"".into()])
    );
}

#[test]
fn appends_builder_errors_after_parse_errors() {
    let mut command: Command<Invocation, ()> = Command::new("strict");
    command.option(string_option("--name")).unwrap();
    command.build(|input| {
        if input.value(&string_option("--name")).is_none() {
            CommandParseResult::Errors(vec!["--name is required".into()])
        } else {
            CommandParseResult::Ok(Invocation {
                command: "strict",
                name: None,
                count: None,
                remaining: Vec::new(),
            })
        }
    });
    assert_eq!(
        parsed(&command, &["--name"]),
        CommandParseResult::Errors(vec!["--name requires a value".into(), "--name is required".into()])
    );
}

#[test]
fn selects_a_subcommand_by_its_first_argument() {
    let mut root = build_root();
    let mut child = Command::new("server");
    child.option(string_option("--listen")).unwrap();
    child.build(|input| {
        CommandParseResult::Ok(Invocation {
            command: "server",
            name: input.value(&string_option("--listen")),
            count: None,
            remaining: input.remaining_args.to_vec(),
        })
    });
    child.action(|_, context: &mut Vec<String>| context.push("server".into()));
    root.command(child, |invocation| invocation).unwrap();

    assert_eq!(
        parsed(&root, &["server", "--listen", "unix:///tmp/pi.sock"]),
        CommandParseResult::Ok(Invocation {
            command: "server",
            name: Some("unix:///tmp/pi.sock".into()),
            count: None,
            remaining: Vec::new(),
        })
    );
    assert_eq!(
        parsed(&root, &["--name", "server"]),
        CommandParseResult::Ok(Invocation {
            command: "pi",
            name: Some("server".into()),
            count: None,
            remaining: Vec::new(),
        })
    );
}

#[test]
fn execute_runs_the_selected_action_with_the_context() {
    let mut root = build_root();
    let mut child = Command::new("client");
    child.build(|_| {
        CommandParseResult::Ok(Invocation {
            command: "client",
            name: None,
            count: None,
            remaining: Vec::new(),
        })
    });
    child.action(|_, context: &mut Vec<String>| context.push("client".into()));
    root.command(child, |invocation| invocation).unwrap();

    let mut context = Vec::new();
    assert_eq!(
        root.execute(&["client".into()], &mut context).unwrap().command,
        "client"
    );
    assert_eq!(
        root.execute(&["--name".into(), "demo".into()], &mut context)
            .unwrap()
            .command,
        "pi"
    );
    assert_eq!(context, vec!["client", "pi:pi"]);
}

#[test]
fn execute_returns_parse_errors_without_running_the_action() {
    let command = build_root();
    let mut context = Vec::new();
    assert_eq!(
        command.execute(&["--count".into(), "many".into()], &mut context),
        Err(CommandExecuteError::Parse(vec![
            "--count must be a number, got \"many\"".into()
        ]))
    );
    assert!(context.is_empty());
}

#[test]
fn multiple_values_are_preserved_in_order() {
    let mut command: Command<Invocation, ()> = Command::new("multi");
    command.option(string_option("--tag")).unwrap();
    command.build(|input| {
        CommandParseResult::Ok(Invocation {
            command: "multi",
            name: input.values(&string_option("--tag")).first().cloned(),
            count: Some(input.values(&string_option("--tag")).len() as i64),
            remaining: Vec::new(),
        })
    });
    assert_eq!(
        parsed(&command, &["--tag", "a", "--tag", "b"]),
        CommandParseResult::Errors(vec!["--tag may only be specified once".into()])
    );
    assert_eq!(
        parsed(&command, &["--tag", "a"]),
        CommandParseResult::Ok(Invocation {
            command: "multi",
            name: Some("a".into()),
            count: Some(1),
            remaining: Vec::new(),
        })
    );
}

#[test]
fn duplicate_option_registration_returns_an_error() {
    let mut command: Command<Invocation, ()> = Command::new("pi");
    command.option(string_option("--name")).unwrap();
    let error = command
        .option(string_option("--name"))
        .err()
        .expect("duplicate option error");
    assert_eq!(error.to_string(), "Option --name is already registered for pi");
}

#[test]
fn duplicate_subcommand_registration_returns_an_error() {
    let mut root: Command<Invocation, ()> = Command::new("pi");
    root.command(Command::<Invocation, ()>::new("server"), |invocation| invocation)
        .unwrap();
    let error = root
        .command(Command::<Invocation, ()>::new("server"), |invocation| invocation)
        .err()
        .expect("duplicate subcommand error");
    assert_eq!(error.to_string(), "Command server is already registered");
}

#[test]
fn parse_without_builder_returns_an_error() {
    let command: Command<Invocation, ()> = Command::new("pi");
    let error = command.parse(&[]).unwrap_err();
    assert_eq!(error.to_string(), "Command pi does not define a builder");
}

#[test]
fn execute_without_action_returns_an_error() {
    let mut command: Command<Invocation, ()> = Command::new("pi");
    command.build(|_| {
        CommandParseResult::Ok(Invocation {
            command: "pi",
            name: None,
            count: None,
            remaining: Vec::new(),
        })
    });
    let error = command.execute(&[], &mut ()).unwrap_err();
    assert_eq!(error.to_string(), "Command pi does not define an action");
}
