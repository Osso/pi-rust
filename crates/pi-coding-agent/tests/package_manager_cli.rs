//! Behavioral tests for `package-manager-cli.ts` parsing and list output.

use pi_coding_agent::core::package_manager::{ConfiguredPackage, InstalledScope};
use pi_coding_agent::package_manager_cli::{
    PackageAction, PackageCommand, UpdateTarget, format_package_list, handle_config_command, handle_package_command,
    package_command_usage, parse_package_command, update_target_includes_extensions, update_target_includes_self,
};
use pi_js_compat::js_json::JsString;

fn args(values: &[&str]) -> Vec<JsString> {
    values.iter().map(|value| JsString::from(*value)).collect()
}

#[test]
fn parses_install_and_remove_flags() {
    let options = parse_package_command(&args(&["install", "npm:x", "-l", "--approve"])).unwrap();
    assert_eq!(options.command, PackageCommand::Install);
    assert_eq!(options.source.as_ref(), Some(&JsString::from("npm:x")));
    assert!(options.local);
    assert_eq!(options.project_trust_override, Some(true));

    let options = parse_package_command(&args(&["uninstall", "./local", "-na"])).unwrap();
    assert_eq!(options.command, PackageCommand::Remove);
    assert_eq!(options.project_trust_override, Some(false));
    assert!(!options.local);
}

#[test]
fn update_target_matrix_matches_the_source() {
    let cases: &[(&[&str], UpdateTarget, bool)] = &[
        (&["update"], UpdateTarget::SelfUpdate, true),
        (&["update", "--self"], UpdateTarget::SelfUpdate, false),
        (
            &["update", "--extensions"],
            UpdateTarget::Extensions { source: None },
            false,
        ),
        (&["update", "--all"], UpdateTarget::All, false),
        (&["update", "--self", "--extensions"], UpdateTarget::All, false),
        (&["update", "self"], UpdateTarget::SelfUpdate, false),
        (&["update", "pi"], UpdateTarget::SelfUpdate, false),
        (&["update", "self", "--extensions"], UpdateTarget::All, false),
        (
            &["update", "npm:pkg"],
            UpdateTarget::Extensions {
                source: Some("npm:pkg".to_owned().into()),
            },
            false,
        ),
        (
            &["update", "--extension", "npm:pkg"],
            UpdateTarget::Extensions {
                source: Some("npm:pkg".to_owned().into()),
            },
            false,
        ),
        (&["update", "--force"], UpdateTarget::SelfUpdate, true),
    ];
    for (input, expected_target, expected_note) in cases {
        let options = parse_package_command(&args(input)).unwrap();
        assert_eq!(options.update_target.as_ref(), Some(expected_target), "{input:?}");
        assert_eq!(options.show_extensions_skipped_note, *expected_note, "{input:?}");
        assert_eq!(options.conflicting_options, None, "{input:?}");
    }
}

#[test]
fn update_conflicts_are_reported_once() {
    let options = parse_package_command(&args(&["update", "--all", "--self", "--models"])).unwrap();
    assert_eq!(
        options.conflicting_options.as_ref(),
        Some(&JsString::from(
            "--all cannot be combined with --self, --extensions, or --extension"
        ))
    );
    let options = parse_package_command(&args(&["update", "--extension", "a", "--extension", "b"])).unwrap();
    assert_eq!(
        options.conflicting_options.as_ref(),
        Some(&JsString::from("--extension can only be provided once"))
    );
}

#[test]
fn help_wins_over_validation_errors() {
    let outcome = handle_package_command(&args(&["install", "--help", "--bogus"])).unwrap();
    assert_eq!(outcome.exit_code, None);
    assert!(outcome.stdout[0].contains("Install a package and add it to settings."));
    assert!(outcome.stderr.is_empty());
}

#[test]
fn validation_errors_set_exit_code_one() {
    let outcome = handle_package_command(&args(&["install"])).unwrap();
    assert_eq!(outcome.exit_code, Some(1));
    assert_eq!(outcome.stderr[0], "Missing install source.");
    assert_eq!(
        outcome.stderr[1],
        pi_js_compat::js_format!("Usage: {}", package_command_usage(PackageCommand::Install))
    );

    let outcome = handle_package_command(&args(&["update", "--bogus"])).unwrap();
    assert_eq!(outcome.stderr[0], r#"Unknown option --bogus for "update"."#);
    assert_eq!(outcome.exit_code, Some(1));
}

#[test]
fn valid_commands_return_actions() {
    let outcome = handle_package_command(&args(&["install", "npm:x", "-l"])).unwrap();
    assert_eq!(
        outcome.action,
        Some(PackageAction::Install {
            source: "npm:x".to_owned().into(),
            local: true,
        })
    );
    let outcome = handle_config_command(&args(&["config", "-l"])).unwrap();
    assert_eq!(outcome.action, Some(PackageAction::Config));
}

#[test]
fn config_ignores_non_trust_arguments_and_keeps_last_trust_override() {
    let outcome = handle_config_command(&args(&[
        "config",
        "--help",
        "--bogus",
        "extra",
        "-l",
        "--approve",
        "--no-approve",
    ]))
    .unwrap();
    assert_eq!(outcome.action, Some(PackageAction::Config));
    assert_eq!(outcome.project_trust_override, Some(false));
    assert_eq!(outcome.exit_code, None);
    assert_eq!(outcome.stdout, Vec::<JsString>::new());
    assert_eq!(outcome.stderr, Vec::<JsString>::new());
}

#[test]
fn update_target_predicates() {
    assert!(update_target_includes_self(&UpdateTarget::All));
    assert!(update_target_includes_self(&UpdateTarget::SelfUpdate));
    assert!(update_target_includes_extensions(&UpdateTarget::All));
    assert!(update_target_includes_extensions(&UpdateTarget::Extensions {
        source: None
    }));
    assert!(!update_target_includes_extensions(&UpdateTarget::SelfUpdate));
}

#[test]
fn formats_the_configured_package_list() {
    assert_eq!(format_package_list(&[]), "No packages installed.");
    let packages = vec![
        ConfiguredPackage {
            source: "npm:user".to_owned().into(),
            scope: InstalledScope::User,
            filtered: false,
            installed_path: Some("/agent/npm/node_modules/user".to_owned().into()),
        },
        ConfiguredPackage {
            source: "npm:project".to_owned().into(),
            scope: InstalledScope::Project,
            filtered: true,
            installed_path: None,
        },
    ];
    assert_eq!(
        format_package_list(&packages),
        "User packages:\n  npm:user\n    /agent/npm/node_modules/user\n\nProject packages:\n  npm:project (filtered)"
    );
}
