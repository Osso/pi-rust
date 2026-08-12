use pi_protocol::{
    AbortCommand, AttachCommand, Command, CommandError, CreateCommand, CreateCommandInput, DetachCommand, ListCommand,
    ModelRef, PromptCommand, SetModelCommand, SetThinkingCommand, SteerCommand, ThinkingLevel,
};

fn model_ref() -> ModelRef {
    ModelRef::new("openai".to_owned(), "gpt-5".to_owned()).expect("valid model reference")
}

fn create_input() -> CreateCommandInput {
    CreateCommandInput {
        cwd: None,
        name: None,
        model: None,
        thinking_level: None,
    }
}

#[test]
fn command_variants_report_exact_protocol_literals() {
    let model = model_ref();
    let commands = [
        (Command::List(ListCommand::new()), "list"),
        (
            Command::Create(CreateCommand::new(create_input()).expect("valid create command")),
            "create",
        ),
        (
            Command::Attach(AttachCommand::new("session".to_owned()).expect("valid attach command")),
            "attach",
        ),
        (
            Command::Detach(DetachCommand::new("session".to_owned()).expect("valid detach command")),
            "detach",
        ),
        (
            Command::Prompt(PromptCommand::new("session".to_owned(), "text".to_owned()).expect("valid prompt command")),
            "prompt",
        ),
        (
            Command::Steer(SteerCommand::new("session".to_owned(), "text".to_owned()).expect("valid steer command")),
            "steer",
        ),
        (
            Command::Abort(AbortCommand::new("session".to_owned()).expect("valid abort command")),
            "abort",
        ),
        (
            Command::SetModel(
                SetModelCommand::new("session".to_owned(), model.clone()).expect("valid set-model command"),
            ),
            "set_model",
        ),
        (
            Command::SetThinking(
                SetThinkingCommand::new("session".to_owned(), ThinkingLevel::High).expect("valid set-thinking command"),
            ),
            "set_thinking",
        ),
    ];

    for (command, literal) in commands {
        assert_eq!(command.as_str(), literal);
    }
}

#[test]
fn create_command_preserves_omitted_and_present_options() {
    let omitted = CreateCommand::new(create_input()).expect("optional fields may be omitted");
    assert_eq!(omitted.cwd(), None);
    assert_eq!(omitted.name(), None);
    assert_eq!(omitted.model(), None);
    assert_eq!(omitted.thinking_level(), None);

    let model = model_ref();
    let present = CreateCommand::new(CreateCommandInput {
        cwd: Some(" \tworkspace\0".to_owned()),
        name: Some(String::new()),
        model: Some(model.clone()),
        thinking_level: Some(ThinkingLevel::XHigh),
    })
    .expect("present options must be preserved");

    assert_eq!(present.cwd(), Some(" \tworkspace\0"));
    assert_eq!(present.name(), Some(""));
    assert_eq!(present.model(), Some(&model));
    assert_eq!(present.thinking_level(), Some(ThinkingLevel::XHigh));
}

#[test]
fn create_command_rejects_empty_cwd() {
    let error = CreateCommand::new(CreateCommandInput {
        cwd: Some(String::new()),
        ..create_input()
    })
    .expect_err("empty cwd must be rejected");

    assert_eq!(error, CommandError::EmptyCwd);
}

#[test]
fn session_commands_preserve_ids_and_reject_empty_ids() {
    let attach = AttachCommand::new("session\0é".to_owned()).expect("valid attach command");
    assert_eq!(attach.session_id(), "session\0é");

    let detach = DetachCommand::new("session".to_owned()).expect("valid detach command");
    assert_eq!(detach.session_id(), "session");

    let abort = AbortCommand::new("session".to_owned()).expect("valid abort command");
    assert_eq!(abort.session_id(), "session");

    assert_eq!(AttachCommand::new(String::new()), Err(CommandError::EmptySessionId));
    assert_eq!(DetachCommand::new(String::new()), Err(CommandError::EmptySessionId));
    assert_eq!(AbortCommand::new(String::new()), Err(CommandError::EmptySessionId));
}

#[test]
fn prompt_and_steer_preserve_unrestricted_text_and_reject_empty_ids() {
    let prompt = PromptCommand::new("session".to_owned(), "\né\0".to_owned()).expect("valid prompt command");
    assert_eq!(prompt.session_id(), "session");
    assert_eq!(prompt.text(), "\né\0");

    let steer = SteerCommand::new("session".to_owned(), String::new()).expect("empty steer text is valid");
    assert_eq!(steer.session_id(), "session");
    assert_eq!(steer.text(), "");

    assert_eq!(
        PromptCommand::new(String::new(), "text".to_owned()),
        Err(CommandError::EmptySessionId)
    );
    assert_eq!(
        SteerCommand::new(String::new(), "text".to_owned()),
        Err(CommandError::EmptySessionId)
    );
}

#[test]
fn set_model_preserves_model_and_rejects_empty_id() {
    let model = model_ref();
    let command = SetModelCommand::new("session".to_owned(), model.clone()).expect("valid set-model command");

    assert_eq!(command.session_id(), "session");
    assert_eq!(command.model(), &model);
    assert_eq!(
        SetModelCommand::new(String::new(), model),
        Err(CommandError::EmptySessionId)
    );
}

#[test]
fn set_thinking_preserves_level_and_rejects_empty_id() {
    let command =
        SetThinkingCommand::new("session".to_owned(), ThinkingLevel::Minimal).expect("valid set-thinking command");

    assert_eq!(command.session_id(), "session");
    assert_eq!(command.thinking_level(), ThinkingLevel::Minimal);
    assert_eq!(
        SetThinkingCommand::new(String::new(), ThinkingLevel::Minimal),
        Err(CommandError::EmptySessionId)
    );
}

#[test]
fn command_errors_name_the_invalid_fields() {
    assert_eq!(
        CommandError::EmptyCwd.to_string(),
        "Command field cwd must not be empty"
    );
    assert_eq!(
        CommandError::EmptySessionId.to_string(),
        "Command field sessionId must not be empty"
    );
}
