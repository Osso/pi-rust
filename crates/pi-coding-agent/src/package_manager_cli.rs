//! Package command argv parsing, help text, and validation output
//! (`package-manager-cli.ts`).
//!
//! The command actions themselves need the settings manager, resource loader,
//! model runtime, self-update runner, and the config TUI. This module ports the
//! observable argument/validation layer and returns a typed [`PackageAction`] for
//! the runtime to execute; see the traceability doc for the gap list.

use crate::config;
use crate::utils::chalk::Chalk;

/// `PackageCommand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageCommand {
    Install,
    Remove,
    Update,
    List,
}

impl PackageCommand {
    fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Remove => "remove",
            Self::Update => "update",
            Self::List => "list",
        }
    }
}

/// `UpdateTarget`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateTarget {
    All,
    SelfUpdate,
    Extensions { source: Option<String> },
    Models,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCommandOptions {
    pub command: PackageCommand,
    pub source: Option<String>,
    pub update_target: Option<UpdateTarget>,
    pub show_extensions_skipped_note: bool,
    pub local: bool,
    pub force: bool,
    pub project_trust_override: Option<bool>,
    pub help: bool,
    pub invalid_option: Option<String>,
    pub invalid_argument: Option<String>,
    pub missing_option_value: Option<String>,
    pub conflicting_options: Option<String>,
}

/// Parsed command handed to the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageAction {
    Install {
        source: String,
        local: bool,
    },
    Remove {
        source: String,
        local: bool,
    },
    List,
    Update {
        target: UpdateTarget,
        force: bool,
        show_extensions_skipped_note: bool,
    },
    Config {
        local: bool,
    },
}

/// Result of the ported portion of `handlePackageCommand`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    pub exit_code: Option<i32>,
    pub action: Option<PackageAction>,
    pub project_trust_override: Option<bool>,
}

fn app_name() -> String {
    config::package_config().app_name.clone()
}

fn config_dir_name() -> String {
    config::package_config().config_dir_name.clone()
}

/// `getPackageCommandUsage`.
pub fn package_command_usage(command: PackageCommand) -> String {
    let app_name = app_name();
    match command {
        PackageCommand::Install => {
            format!("{app_name} install <source> [-l] [--approve|--no-approve]")
        }
        PackageCommand::Remove => {
            format!("{app_name} remove <source> [-l] [--approve|--no-approve]")
        }
        PackageCommand::Update => format!(
            "{app_name} update [source|self|pi] [--self|--extensions|--models|--all] [--extension <source>] [--approve|--no-approve] [--force]"
        ),
        PackageCommand::List => format!("{app_name} list [--approve|--no-approve]"),
    }
}

fn config_command_usage() -> String {
    format!("{} config [-l] [--approve|--no-approve]", app_name())
}

/// `printConfigCommandHelp` text, including the trailing newline from `console.log`.
pub fn config_command_help() -> String {
    let chalk = Chalk::stdout();
    let text = format!(
        "{}
  {}

Open the resource configuration TUI to enable or disable package resources.
Without -l, starts in global settings (~/{}/agent/settings.json).
Press Tab in the TUI to switch between global and project-local modes.

Options:
  -l, --local       Edit project overrides ({}/settings.json)
  -a, --approve     Trust project-local files for this command with -l
  -na, --no-approve Ignore project-local files for this command with -l
",
        chalk.bold("Usage:"),
        config_command_usage(),
        config_dir_name(),
        config_dir_name(),
    );
    text
}

/// `printPackageCommandHelp` text, including the trailing newline from `console.log`.
pub fn package_command_help(command: PackageCommand) -> String {
    let chalk = Chalk::stdout();
    let app_name = app_name();
    let config_dir_name = config_dir_name();
    let body = match command {
        PackageCommand::Install => format!(
            "{}

Install a package and add it to settings.

Options:
  -l, --local       Install project-locally ({config_dir_name}/settings.json)
  -a, --approve     Trust project-local files for this command
  -na, --no-approve Ignore project-local files for this command

Examples:
  {app_name} install npm:@foo/bar
  {app_name} install git:github.com/user/repo
  {app_name} install git:git@github.com:user/repo
  {app_name} install https://github.com/user/repo
  {app_name} install ssh://git@github.com/user/repo
  {app_name} install ./local/path
",
            package_command_usage(command)
        ),
        PackageCommand::Remove => format!(
            "{}

Remove a package and its source from settings.
Alias: {app_name} uninstall <source> [-l]

Options:
  -l, --local       Remove from project settings ({config_dir_name}/settings.json)
  -a, --approve     Trust project-local files for this command
  -na, --no-approve Ignore project-local files for this command

Examples:
  {app_name} remove npm:@foo/bar
  {app_name} uninstall npm:@foo/bar
",
            package_command_usage(command)
        ),
        PackageCommand::Update => format!(
            "{}

Update pi, installed packages, or model catalogs.

Options:
  --self                  Update pi only (default when no target is given)
  --extensions            Update installed packages only
  --models                Refresh model catalogs only
  --all                   Update pi and installed packages
  --extension <source>    Update one package only
  -a, --approve           Trust project-local files for this command
  -na, --no-approve       Ignore project-local files for this command
  --force                 Reinstall pi even if the current version is latest

Short forms:
  {app_name} update                Update pi only
  {app_name} update --all          Update pi and all extensions
  {app_name} update --models       Refresh model catalogs only
  {app_name} update <source>       Update one package
  {app_name} update pi             Update pi only (self works as alias to pi)
",
            package_command_usage(command)
        ),
        PackageCommand::List => format!(
            "{}

List installed packages from user and project settings.

Options:
  -a, --approve      Trust project-local files for this command
  -na, --no-approve  Ignore project-local files for this command
",
            package_command_usage(command)
        ),
    };
    format!("{}\n  {body}", chalk.bold("Usage:"))
}

/// `parsePackageCommand`.
pub fn parse_package_command(args: &[String]) -> Option<PackageCommandOptions> {
    let (raw_command, rest) = args.split_first()?;
    let command = match raw_command.as_str() {
        "uninstall" => PackageCommand::Remove,
        "install" => PackageCommand::Install,
        "remove" => PackageCommand::Remove,
        "update" => PackageCommand::Update,
        "list" => PackageCommand::List,
        _ => return None,
    };

    let mut options = PackageCommandOptions {
        command,
        source: None,
        update_target: None,
        show_extensions_skipped_note: false,
        local: false,
        force: false,
        project_trust_override: None,
        help: false,
        invalid_option: None,
        invalid_argument: None,
        missing_option_value: None,
        conflicting_options: None,
    };
    let mut self_flag = false;
    let mut extensions_flag = false;
    let mut models_flag = false;
    let mut all_flag = false;
    let mut extension_flag_source: Option<String> = None;

    let mut index = 0;
    while index < rest.len() {
        let arg = rest[index].as_str();
        if arg == "-h" || arg == "--help" {
            options.help = true;
        } else if arg == "-l" || arg == "--local" {
            if matches!(command, PackageCommand::Install | PackageCommand::Remove) {
                options.local = true;
            } else {
                set_once(&mut options.invalid_option, arg);
            }
        } else if arg == "--self" {
            if command == PackageCommand::Update {
                self_flag = true;
            } else {
                set_once(&mut options.invalid_option, arg);
            }
        } else if arg == "--extensions" {
            if command == PackageCommand::Update {
                extensions_flag = true;
            } else {
                set_once(&mut options.invalid_option, arg);
            }
        } else if arg == "--models" {
            if command == PackageCommand::Update {
                models_flag = true;
            } else {
                set_once(&mut options.invalid_option, arg);
            }
        } else if arg == "--all" {
            if command == PackageCommand::Update {
                all_flag = true;
            } else {
                set_once(&mut options.invalid_option, arg);
            }
        } else if arg == "--approve" || arg == "-a" {
            options.project_trust_override = Some(true);
        } else if arg == "--no-approve" || arg == "-na" {
            options.project_trust_override = Some(false);
        } else if arg == "--force" {
            if command == PackageCommand::Update {
                options.force = true;
            } else {
                set_once(&mut options.invalid_option, arg);
            }
        } else if arg == "--extension" {
            if command != PackageCommand::Update {
                set_once(&mut options.invalid_option, arg);
            } else {
                let value = rest.get(index + 1);
                if value.is_none_or(|value| value.starts_with('-')) {
                    set_once(&mut options.missing_option_value, arg);
                } else if extension_flag_source.is_some() {
                    set_once(
                        &mut options.conflicting_options,
                        "--extension can only be provided once",
                    );
                    index += 1;
                } else {
                    extension_flag_source = value.cloned();
                    index += 1;
                }
            }
        } else if arg.starts_with('-') {
            set_once(&mut options.invalid_option, arg);
        } else if options.source.is_none() {
            options.source = Some(arg.to_owned());
        } else {
            set_once(&mut options.invalid_argument, arg);
        }
        index += 1;
    }

    if command == PackageCommand::Update {
        resolve_update_target(
            &mut options,
            UpdateFlags {
                self_flag,
                extensions_flag,
                models_flag,
                all_flag,
                extension_flag_source,
            },
        );
    }
    Some(options)
}

struct UpdateFlags {
    self_flag: bool,
    extensions_flag: bool,
    models_flag: bool,
    all_flag: bool,
    extension_flag_source: Option<String>,
}

fn resolve_update_target(options: &mut PackageCommandOptions, flags: UpdateFlags) {
    if flags.all_flag
        && (flags.self_flag || flags.extensions_flag || flags.models_flag || flags.extension_flag_source.is_some())
    {
        set_once(
            &mut options.conflicting_options,
            "--all cannot be combined with --self, --extensions, --models, or --extension",
        );
    }
    if flags.all_flag && options.source.is_some() {
        set_once(
            &mut options.conflicting_options,
            "--all cannot be combined with a positional source",
        );
    }

    options.update_target = if flags.models_flag {
        if flags.self_flag || flags.extensions_flag || flags.all_flag || flags.extension_flag_source.is_some() {
            set_once(
                &mut options.conflicting_options,
                "--models cannot be combined with --self, --extensions, --all, or --extension",
            );
        }
        if options.source.is_some() {
            set_once(
                &mut options.conflicting_options,
                "--models cannot be combined with a positional source",
            );
        }
        Some(UpdateTarget::Models)
    } else if let Some(extension_source) = flags.extension_flag_source {
        if flags.self_flag || flags.extensions_flag || flags.all_flag {
            set_once(
                &mut options.conflicting_options,
                "--extension cannot be combined with --self, --extensions, or --all",
            );
        }
        if options.source.is_some() {
            set_once(
                &mut options.conflicting_options,
                "--extension cannot be combined with a positional source",
            );
        }
        Some(UpdateTarget::Extensions {
            source: Some(extension_source),
        })
    } else if let Some(source) = &options.source {
        if source == "self" || source == "pi" {
            Some(if flags.extensions_flag {
                UpdateTarget::All
            } else {
                UpdateTarget::SelfUpdate
            })
        } else {
            if flags.extensions_flag || flags.self_flag || flags.all_flag {
                set_once(
                    &mut options.conflicting_options,
                    "positional update targets cannot be combined with --self, --extensions, or --all",
                );
            }
            Some(UpdateTarget::Extensions {
                source: Some(source.clone()),
            })
        }
    } else if flags.all_flag || (flags.self_flag && flags.extensions_flag) {
        Some(UpdateTarget::All)
    } else if flags.self_flag {
        Some(UpdateTarget::SelfUpdate)
    } else if flags.extensions_flag {
        Some(UpdateTarget::Extensions { source: None })
    } else {
        options.show_extensions_skipped_note = true;
        Some(UpdateTarget::SelfUpdate)
    };
}

fn set_once(slot: &mut Option<String>, value: &str) {
    if slot.is_none() {
        *slot = Some(value.to_owned());
    }
}

pub fn update_target_includes_self(target: &UpdateTarget) -> bool {
    matches!(target, UpdateTarget::All | UpdateTarget::SelfUpdate)
}

pub fn update_target_includes_extensions(target: &UpdateTarget) -> bool {
    matches!(target, UpdateTarget::All | UpdateTarget::Extensions { .. })
}

/// Ported validation/output half of `handlePackageCommand`.
///
/// Returns `None` when `args` is not a package command. Help and validation
/// failures return an outcome with no action; valid commands return the action
/// for the runtime to execute.
pub fn handle_package_command(args: &[String]) -> Option<CommandOutcome> {
    let options = parse_package_command(args)?;
    let mut outcome = CommandOutcome {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit_code: None,
        action: None,
        project_trust_override: options.project_trust_override,
    };
    if options.help {
        outcome.stdout.push(package_command_help(options.command));
        return Some(outcome);
    }
    let chalk_stderr = Chalk::stderr();
    let usage = package_command_usage(options.command);

    if let Some(invalid_option) = &options.invalid_option {
        outcome.stderr.push(chalk_stderr.red(&format!(
            "Unknown option {invalid_option} for \"{}\".",
            options.command.as_str()
        )));
        outcome
            .stderr
            .push(chalk_stderr.dim(&format!("Use \"{} --help\" or \"{usage}\".", app_name())));
        outcome.exit_code = Some(1);
        return Some(outcome);
    }
    if let Some(missing_option_value) = &options.missing_option_value {
        outcome
            .stderr
            .push(chalk_stderr.red(&format!("Missing value for {missing_option_value}.")));
        outcome.stderr.push(chalk_stderr.dim(&format!("Usage: {usage}")));
        outcome.exit_code = Some(1);
        return Some(outcome);
    }
    if let Some(invalid_argument) = &options.invalid_argument {
        outcome
            .stderr
            .push(chalk_stderr.red(&format!("Unexpected argument {invalid_argument}.")));
        outcome.stderr.push(chalk_stderr.dim(&format!("Usage: {usage}")));
        outcome.exit_code = Some(1);
        return Some(outcome);
    }
    if let Some(conflicting_options) = &options.conflicting_options {
        outcome.stderr.push(chalk_stderr.red(conflicting_options));
        outcome.stderr.push(chalk_stderr.dim(&format!("Usage: {usage}")));
        outcome.exit_code = Some(1);
        return Some(outcome);
    }

    let source = options.source.clone();
    if matches!(options.command, PackageCommand::Install | PackageCommand::Remove) && source.is_none() {
        outcome
            .stderr
            .push(chalk_stderr.red(&format!("Missing {} source.", options.command.as_str())));
        outcome.stderr.push(chalk_stderr.dim(&format!("Usage: {usage}")));
        outcome.exit_code = Some(1);
        return Some(outcome);
    }

    outcome.action = Some(match options.command {
        PackageCommand::Install => PackageAction::Install {
            source: source.unwrap_or_default(),
            local: options.local,
        },
        PackageCommand::Remove => PackageAction::Remove {
            source: source.unwrap_or_default(),
            local: options.local,
        },
        PackageCommand::List => PackageAction::List,
        PackageCommand::Update => PackageAction::Update {
            target: options.update_target.unwrap_or(UpdateTarget::SelfUpdate),
            force: options.force,
            show_extensions_skipped_note: options.show_extensions_skipped_note,
        },
    });
    Some(outcome)
}

/// `handleConfigCommand` argument validation; `None` when the command differs.
pub fn handle_config_command(args: &[String]) -> Option<CommandOutcome> {
    let (command, rest) = args.split_first()?;
    if command != "config" {
        return None;
    }
    let mut outcome = CommandOutcome {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit_code: None,
        action: None,
        project_trust_override: None,
    };
    if rest.iter().any(|arg| arg == "-h" || arg == "--help") {
        outcome.stdout.push(config_command_help());
        return Some(outcome);
    }
    let chalk_stderr = Chalk::stderr();
    let mut local = false;
    let mut project_trust_override = None;
    for arg in rest {
        if arg == "-l" || arg == "--local" {
            local = true;
        } else if arg == "-a" || arg == "--approve" {
            project_trust_override = Some(true);
        } else if arg == "-na" || arg == "--no-approve" {
            project_trust_override = Some(false);
        } else if arg.starts_with('-') {
            outcome
                .stderr
                .push(chalk_stderr.red(&format!("Unknown option {arg} for \"config\".")));
            outcome.stderr.push(chalk_stderr.dim(&format!(
                "Use \"{} --help\" or \"{}\".",
                app_name(),
                config_command_usage()
            )));
            outcome.exit_code = Some(1);
            return Some(outcome);
        } else {
            outcome
                .stderr
                .push(chalk_stderr.red(&format!("Unexpected argument {arg}.")));
            outcome
                .stderr
                .push(chalk_stderr.dim(&format!("Usage: {}", config_command_usage())));
            outcome.exit_code = Some(1);
            return Some(outcome);
        }
    }
    outcome.project_trust_override = project_trust_override;
    outcome.action = Some(PackageAction::Config { local });
    Some(outcome)
}

/// `list` output text for configured packages.
pub fn format_package_list(packages: &[crate::core::package_manager::ConfiguredPackage]) -> String {
    let chalk = Chalk::stdout();
    if packages.is_empty() {
        return format!("{}\n", chalk.dim("No packages installed."));
    }
    let mut lines: Vec<String> = Vec::new();
    let user_packages: Vec<_> = packages
        .iter()
        .filter(|package| package.scope == crate::core::package_manager::InstalledScope::User)
        .collect();
    let project_packages: Vec<_> = packages
        .iter()
        .filter(|package| package.scope == crate::core::package_manager::InstalledScope::Project)
        .collect();

    let format_package = |package: &crate::core::package_manager::ConfiguredPackage, lines: &mut Vec<String>| {
        let display = if package.filtered {
            format!("{} (filtered)", package.source)
        } else {
            package.source.clone()
        };
        lines.push(format!("  {display}"));
        if let Some(installed_path) = &package.installed_path {
            lines.push(chalk.dim(&format!("    {installed_path}")));
        }
    };

    if !user_packages.is_empty() {
        lines.push(chalk.bold("User packages:"));
        for package in &user_packages {
            format_package(package, &mut lines);
        }
    }
    if !project_packages.is_empty() {
        if !user_packages.is_empty() {
            lines.push(String::new());
        }
        lines.push(chalk.bold("Project packages:"));
        for package in &project_packages {
            format_package(package, &mut lines);
        }
    }
    format!("{}\n", lines.join("\n"))
}

/// `reportProjectTrustWarnings`.
pub fn report_project_trust_warnings(warnings: &[String]) -> Vec<String> {
    let chalk = Chalk::stderr();
    warnings
        .iter()
        .map(|warning| chalk.yellow(&format!("Warning: {warning}")))
        .collect()
}
