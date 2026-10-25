use pi_coding_agent::core::slash_commands::builtin_slash_commands;

#[test]
fn builtin_commands_keep_source_order_without_argument_hints() {
    let commands = builtin_slash_commands();
    let names: Vec<&str> = commands.iter().map(|command| command.name).collect();

    assert_eq!(names.first(), Some(&"settings"));
    assert_eq!(names.last(), Some(&"quit"));
    assert_eq!(names.len(), 28);

    let model = commands.iter().find(|command| command.name == "model").unwrap();
    assert_eq!(model.argument_hint, None);
    let login = commands.iter().find(|command| command.name == "login").unwrap();
    assert_eq!(login.argument_hint, None);
    let settings = commands.iter().find(|command| command.name == "settings").unwrap();
    assert_eq!(settings.argument_hint, None);
}

#[test]
fn quit_uses_the_package_app_name() {
    let commands = builtin_slash_commands();
    let quit = commands.iter().find(|command| command.name == "quit").unwrap();

    assert_eq!(quit.description, "Quit pi");
}
