//! Package and config command handling (`package-manager-cli.ts`).
//!
//! Argument parsing, help text, and validation output are synchronous. The
//! execution half (`executePackageCommand` / `executeConfigCommand`) runs over
//! [`PackageCommandEffects`], which carries the environment-bound operations:
//! settings creation, package-manager calls, model catalog refresh, self-update
//! release lookup and execution, and the config TUI. See the traceability doc
//! for the blocked concrete pieces.

use std::future::Future;
use std::io::IsTerminal;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use pi_ai::request::{BoxFuture, CallbackError};

use pi_js_compat::js_json::JsString;

use crate::config;
use crate::core::extensions::types::LoadExtensionsResult;
use crate::core::package_manager::{
    ConfiguredPackage, DefaultPackageManager, PackageSettingsStore, ProgressEvent, ProgressType, ResolvedPaths,
    SettingsManagerStore,
};
use crate::core::project_trust::{AppMode, ResolveProjectTrustedOptions, resolve_project_trusted};
use crate::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use crate::core::trust_manager::{ProjectTrustStore, has_trust_requiring_project_resources};
use crate::utils::chalk::Chalk;
use crate::utils::version_check::{
    LatestPiRelease, LatestPiReleaseOptions, get_latest_pi_release, is_newer_package_version,
};

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
    Extensions { source: Option<JsString> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCommandOptions {
    pub command: PackageCommand,
    pub source: Option<JsString>,
    pub update_target: Option<UpdateTarget>,
    pub show_extensions_skipped_note: bool,
    pub local: bool,
    pub force: bool,
    pub project_trust_override: Option<bool>,
    pub help: bool,
    pub invalid_option: Option<JsString>,
    pub invalid_argument: Option<JsString>,
    pub missing_option_value: Option<JsString>,
    pub conflicting_options: Option<JsString>,
}

/// Parsed command handed to the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageAction {
    Install {
        source: JsString,
        local: bool,
    },
    Remove {
        source: JsString,
        local: bool,
    },
    List,
    Update {
        target: UpdateTarget,
        force: bool,
        show_extensions_skipped_note: bool,
    },
    Config,
}

/// Result of the ported portion of `handlePackageCommand`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    pub stdout: Vec<JsString>,
    pub stderr: Vec<JsString>,
    pub exit_code: Option<i32>,
    pub action: Option<PackageAction>,
    pub project_trust_override: Option<bool>,
}

fn app_name() -> JsString {
    config::package_config().app_name.clone().into()
}

fn config_dir_name() -> JsString {
    config::package_config().config_dir_name.clone().into()
}

/// `getPackageCommandUsage`.
pub fn package_command_usage(command: PackageCommand) -> JsString {
    let app_name = app_name();
    match command {
        PackageCommand::Install => {
            pi_js_compat::js_format!("{} install <source> [-l] [--approve|--no-approve]", app_name)
        }
        PackageCommand::Remove => {
            pi_js_compat::js_format!("{} remove <source> [-l] [--approve|--no-approve]", app_name)
        }
        PackageCommand::Update => pi_js_compat::js_format!(
            "{} update [source|self|pi] [--self|--extensions|--all] [--extension <source>] [--approve|--no-approve] [--force]",
            app_name
        ),
        PackageCommand::List => pi_js_compat::js_format!("{} list [--approve|--no-approve]", app_name),
    }
}

/// `printPackageCommandHelp` text, including the trailing newline from `console.log`.
pub fn package_command_help(command: PackageCommand) -> JsString {
    let chalk = Chalk::stdout();
    let app_name = app_name();
    let config_dir_name = config_dir_name();
    let body = match command {
        PackageCommand::Install => pi_js_compat::js_format!(
            "{}

Install a package and add it to settings.

Options:
  -l, --local       Install project-locally ({}/settings.json)
  -a, --approve     Trust project-local files for this command
  -na, --no-approve Ignore project-local files for this command

Examples:
  {} install npm:@foo/bar
  {} install git:github.com/user/repo
  {} install git:git@github.com:user/repo
  {} install https://github.com/user/repo
  {} install ssh://git@github.com/user/repo
  {} install ./local/path
",
            package_command_usage(command),
            config_dir_name,
            app_name,
            app_name,
            app_name,
            app_name,
            app_name,
            app_name
        ),
        PackageCommand::Remove => pi_js_compat::js_format!(
            "{}

Remove a package and its source from settings.
Alias: {} uninstall <source> [-l]

Options:
  -l, --local       Remove from project settings ({}/settings.json)
  -a, --approve     Trust project-local files for this command
  -na, --no-approve Ignore project-local files for this command

Examples:
  {} remove npm:@foo/bar
  {} uninstall npm:@foo/bar
",
            package_command_usage(command),
            app_name,
            config_dir_name,
            app_name,
            app_name
        ),
        PackageCommand::Update => pi_js_compat::js_format!(
            "{}

Update pi and installed packages.

Options:
  --self                  Update pi only (default when no target is given)
  --extensions            Update installed packages only
  --all                   Update pi and installed packages
  --extension <source>    Update one package only
  -a, --approve           Trust project-local files for this command
  -na, --no-approve       Ignore project-local files for this command
  --force                 Reinstall pi even if the current version is latest

Short forms:
  {} update                Update pi only
  {} update --all          Update pi and all extensions
  {} update <source>       Update one package
  {} update pi             Update pi only (self works as alias to pi)
",
            package_command_usage(command),
            app_name,
            app_name,
            app_name,
            app_name
        ),
        PackageCommand::List => pi_js_compat::js_format!(
            "{}

List installed packages from user and project settings.

Options:
  -a, --approve      Trust project-local files for this command
  -na, --no-approve  Ignore project-local files for this command
",
            package_command_usage(command)
        ),
    };
    chalk.bold("Usage:") + "\n  " + body
}

/// `parsePackageCommand`.
pub fn parse_package_command(args: &[JsString]) -> Option<PackageCommandOptions> {
    let (raw_command, rest) = args.split_first()?;
    let command_tag = match raw_command.to_utf8() {
        Ok(tag) => tag,
        Err(_) => return None,
    };
    let command = match command_tag.as_str() {
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
    let mut all_flag = false;
    let mut extension_flag_source: Option<JsString> = None;

    let mut index = 0;
    while index < rest.len() {
        let arg = &rest[index];
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
                if value.is_none_or(|value| value.is_empty() || value.starts_with('-')) {
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
        } else if options.source.as_ref().is_none_or(JsString::is_empty) {
            options.source = Some(arg.to_owned());
        } else {
            set_once(&mut options.invalid_argument, arg);
        }
        index += 1;
    }

    options.source = options.source.filter(|source| !source.is_empty());
    if command == PackageCommand::Update {
        resolve_update_target(
            &mut options,
            UpdateFlags {
                self_flag,
                extensions_flag,
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
    all_flag: bool,
    extension_flag_source: Option<JsString>,
}

fn resolve_update_target(options: &mut PackageCommandOptions, flags: UpdateFlags) {
    if flags.all_flag && (flags.self_flag || flags.extensions_flag || flags.extension_flag_source.is_some()) {
        set_once(
            &mut options.conflicting_options,
            "--all cannot be combined with --self, --extensions, or --extension",
        );
    }
    if flags.all_flag && options.source.is_some() {
        set_once(
            &mut options.conflicting_options,
            "--all cannot be combined with a positional source",
        );
    }

    options.update_target = if let Some(extension_source) = flags.extension_flag_source {
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

fn set_once(slot: &mut Option<JsString>, value: impl Into<JsString>) {
    if slot.is_none() {
        *slot = Some(value.into());
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
pub fn handle_package_command(args: &[JsString]) -> Option<CommandOutcome> {
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
        outcome.stderr.push(chalk_stderr.red(&pi_js_compat::js_format!(
            "Unknown option {} for \"{}\".",
            invalid_option,
            options.command.as_str()
        )));
        outcome.stderr.push(chalk_stderr.dim(&pi_js_compat::js_format!(
            "Use \"{} --help\" or \"{}\".",
            app_name(),
            usage
        )));
        outcome.exit_code = Some(1);
        return Some(outcome);
    }
    if let Some(missing_option_value) = &options.missing_option_value {
        outcome
            .stderr
            .push(chalk_stderr.red(&pi_js_compat::js_format!("Missing value for {}.", missing_option_value)));
        outcome
            .stderr
            .push(chalk_stderr.dim(&pi_js_compat::js_format!("Usage: {}", usage)));
        outcome.exit_code = Some(1);
        return Some(outcome);
    }
    if let Some(invalid_argument) = &options.invalid_argument {
        outcome
            .stderr
            .push(chalk_stderr.red(&pi_js_compat::js_format!("Unexpected argument {}.", invalid_argument)));
        outcome
            .stderr
            .push(chalk_stderr.dim(&pi_js_compat::js_format!("Usage: {}", usage)));
        outcome.exit_code = Some(1);
        return Some(outcome);
    }
    if let Some(conflicting_options) = &options.conflicting_options {
        outcome.stderr.push(chalk_stderr.red(conflicting_options));
        outcome
            .stderr
            .push(chalk_stderr.dim(&pi_js_compat::js_format!("Usage: {}", usage)));
        outcome.exit_code = Some(1);
        return Some(outcome);
    }

    let source = options.source.clone();
    if matches!(options.command, PackageCommand::Install | PackageCommand::Remove) && source.is_none() {
        outcome.stderr.push(chalk_stderr.red(&pi_js_compat::js_format!(
            "Missing {} source.",
            options.command.as_str()
        )));
        outcome
            .stderr
            .push(chalk_stderr.dim(&pi_js_compat::js_format!("Usage: {}", usage)));
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

/// `handleConfigCommand`: only trust overrides are parsed; all other arguments are ignored.
pub fn handle_config_command(args: &[JsString]) -> Option<CommandOutcome> {
    if args.first().is_none_or(|command| command != "config") {
        return None;
    }
    let project_trust_override = args.iter().fold(None, |previous, arg| {
        if arg == "--approve" || arg == "-a" {
            Some(true)
        } else if arg == "--no-approve" || arg == "-na" {
            Some(false)
        } else {
            previous
        }
    });
    Some(CommandOutcome {
        stdout: Vec::new(),
        stderr: Vec::new(),
        exit_code: None,
        action: Some(PackageAction::Config),
        project_trust_override,
    })
}

/// `list` output text for configured packages.
pub fn format_package_list(packages: &[crate::core::package_manager::ConfiguredPackage]) -> JsString {
    let chalk = Chalk::stdout();
    if packages.is_empty() {
        return chalk.dim("No packages installed.");
    }
    let mut lines: Vec<JsString> = Vec::new();
    let user_packages: Vec<_> = packages
        .iter()
        .filter(|package| package.scope == crate::core::package_manager::InstalledScope::User)
        .collect();
    let project_packages: Vec<_> = packages
        .iter()
        .filter(|package| package.scope == crate::core::package_manager::InstalledScope::Project)
        .collect();

    let format_package = |package: &crate::core::package_manager::ConfiguredPackage, lines: &mut Vec<JsString>| {
        let display = if package.filtered {
            pi_js_compat::js_format!("{} (filtered)", package.source)
        } else {
            package.source.clone()
        };
        lines.push(pi_js_compat::js_format!("  {}", display).into());
        if let Some(installed_path) = &package.installed_path {
            lines.push(chalk.dim(&pi_js_compat::js_format!("    {}", installed_path)));
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
            lines.push(JsString::default());
        }
        lines.push(chalk.bold("Project packages:"));
        for package in &project_packages {
            format_package(package, &mut lines);
        }
    }
    JsString::join(lines, "\n")
}

/// `reportProjectTrustWarnings`.
pub fn report_project_trust_warnings(warnings: &[JsString]) -> Vec<JsString> {
    let chalk = Chalk::stderr();
    warnings
        .iter()
        .map(|warning| chalk.yellow(&pi_js_compat::js_format!("Warning: {}", warning)))
        .collect()
}

/// `CommandSettingsResult`.
pub struct CommandSettings {
    pub store: Arc<SettingsManagerStore>,
    pub project_trust_warnings: Vec<JsString>,
}

/// Options for `createCommandSettingsManager`.
pub struct CreateCommandSettingsOptions {
    pub cwd: JsString,
    pub agent_dir: JsString,
    pub project_trust_override: Option<bool>,
    pub use_saved_project_trust_only: bool,
    /// Inline extension factories loaded before the trust prompt resolves.
    pub extension_factories: Vec<crate::core::resource_loader::InlineExtension>,
    /// `showStartupSelector`/`showStartupInput` adapter; `None` when the startup
    /// UI is unavailable, which resolves the prompt as dismissed.
    pub startup_ui: Option<Arc<dyn crate::cli::project_trust::ProjectTrustStartupUi>>,
}

/// `DefaultResourceLoader.loadProjectTrustExtensions` for the command paths.
pub trait ProjectTrustExtensionsLoader: Send + Sync {
    fn load<'a>(
        &'a self,
        cwd: &'a JsString,
        agent_dir: &'a JsString,
        settings_manager: &'a Arc<std::sync::Mutex<SettingsManager>>,
        extension_factories: &'a [crate::core::resource_loader::InlineExtension],
    ) -> BoxFuture<'a, Result<Option<LoadExtensionsResult>, CallbackError>>;
}

/// Concrete loader backed by [`DefaultResourceLoader`].
///
/// Shares the command path's settings manager so forcing untrusted project
/// settings during the bootstrap pass is visible to the trust resolution.
#[derive(Clone, Default)]
pub struct DefaultProjectTrustExtensionsLoader {
    pub extension_registry: Option<Arc<crate::core::extensions::loader::ExtensionRegistry>>,
}

impl ProjectTrustExtensionsLoader for DefaultProjectTrustExtensionsLoader {
    fn load<'a>(
        &'a self,
        cwd: &'a JsString,
        agent_dir: &'a JsString,
        settings_manager: &'a Arc<std::sync::Mutex<SettingsManager>>,
        extension_factories: &'a [crate::core::resource_loader::InlineExtension],
    ) -> BoxFuture<'a, Result<Option<LoadExtensionsResult>, CallbackError>> {
        Box::pin(async move {
            let loader = crate::core::resource_loader::DefaultResourceLoader::new(
                crate::core::resource_loader::DefaultResourceLoaderOptions {
                    cwd: cwd.to_owned(),
                    agent_dir: agent_dir.to_owned(),
                    settings_manager: Some(Arc::clone(settings_manager)),
                    extension_registry: self.extension_registry.clone(),
                    extension_factories: extension_factories.to_vec(),
                    ..Default::default()
                },
            )?;
            loader
                .load_project_trust_extensions()
                .await
                .map(Some)
                .map_err(CallbackError::from)
        })
    }
}

/// `getCommandAppMode`.
pub fn command_app_mode(stdin_is_tty: bool, stdout_is_tty: bool) -> AppMode {
    if stdin_is_tty && stdout_is_tty {
        AppMode::Interactive
    } else {
        AppMode::Print
    }
}

fn process_command_app_mode() -> AppMode {
    command_app_mode(std::io::stdin().is_terminal(), std::io::stdout().is_terminal())
}

/// `createCommandSettingsManager`.
pub async fn create_command_settings_manager(
    options: CreateCommandSettingsOptions,
    extensions_loader: &dyn ProjectTrustExtensionsLoader,
) -> Result<CommandSettings, CallbackError> {
    let settings_manager = Arc::new(std::sync::Mutex::new(
        SettingsManager::create(
            &options.cwd,
            Some(&options.agent_dir),
            SettingsManagerCreateOptions {
                project_trusted: Some(false),
            },
        )
        .map_err(|error| error.message().clone())?,
    ));
    let project_trust_warnings: Arc<Mutex<Vec<JsString>>> = Arc::new(Mutex::new(Vec::new()));
    let trust_store = ProjectTrustStore::new(&options.agent_dir).map_err(|error| error.message().clone())?;

    if options.use_saved_project_trust_only {
        let saved_project_trusted = trust_store.get(&options.cwd).map_err(|error| error.message().clone())?
            == crate::core::trust_manager::ProjectTrustDecision::Trusted;
        settings_manager
            .lock()
            .unwrap()
            .set_project_trusted(options.project_trust_override.unwrap_or(saved_project_trusted));
        return Ok(CommandSettings {
            store: Arc::new(SettingsManagerStore::from_shared(settings_manager)),
            project_trust_warnings: project_trust_warnings.lock().unwrap().clone(),
        });
    }

    let app_mode = process_command_app_mode();
    let extensions_result = if options.project_trust_override.is_none()
        && has_trust_requiring_project_resources(&options.cwd).map_err(|error| error.message().clone())?
    {
        extensions_loader
            .load(
                &options.cwd,
                &options.agent_dir,
                &settings_manager,
                &options.extension_factories,
            )
            .await?
    } else {
        None
    };
    if let Some(result) = &extensions_result {
        for error in &result.errors {
            project_trust_warnings.lock().unwrap().push(pi_js_compat::js_format!(
                "Failed to load extension \"{}\": {}",
                error.path,
                error.error.clone()
            ));
        }
    }

    let trust_context = crate::cli::project_trust::create_project_trust_context(
        crate::cli::project_trust::CreateProjectTrustContextOptions {
            cwd: options.cwd.clone(),
            mode: app_mode,
            has_ui: app_mode == AppMode::Interactive,
            startup_ui: options.startup_ui.or_else(|| {
                Some(Arc::new(crate::cli::project_trust::TerminalProjectTrustStartupUi {
                    settings: settings_manager.lock().unwrap().get_merged_settings(),
                }))
            }),
        },
    );
    let default_project_trust = settings_manager.lock().unwrap().get_default_project_trust();
    let warnings_for_trust = Arc::clone(&project_trust_warnings);
    let on_extension_error = move |message: JsString| {
        warnings_for_trust.lock().unwrap().push(message);
    };
    let project_trusted = resolve_project_trusted(ResolveProjectTrustedOptions {
        cwd: &options.cwd,
        trust_store: &trust_store,
        trust_override: options.project_trust_override,
        default_project_trust: Some(default_project_trust),
        extensions_result: extensions_result.as_ref(),
        project_trust_context: &trust_context,
        on_extension_error: Some(&on_extension_error),
    })
    .await
    .map_err(|error| error.message().clone())?;
    settings_manager.lock().unwrap().set_project_trusted(project_trusted);
    Ok(CommandSettings {
        store: Arc::new(SettingsManagerStore::from_shared(settings_manager)),
        project_trust_warnings: project_trust_warnings.lock().unwrap().clone(),
    })
}

/// `reportSettingsErrors`.
pub fn report_settings_errors(store: &Arc<SettingsManagerStore>, context: &str) -> Vec<JsString> {
    let errors = store.with(|manager| manager.drain_errors());
    let chalk = Chalk::stderr();
    let mut lines = Vec::new();
    for error in errors {
        lines.push(chalk.yellow(
            JsString::from(pi_js_compat::js_format!(
                "Warning ({}, {} settings): ",
                context,
                error.scope.as_str()
            )) + error.message,
        ));
    }
    lines
}

/// `reportProjectTrustWarnings`.
pub fn report_project_trust_warnings_lines(warnings: &[JsString]) -> Vec<JsString> {
    let chalk = Chalk::stderr();
    warnings
        .iter()
        .map(|warning| chalk.yellow(&pi_js_compat::js_format!("Warning: {}", warning)))
        .collect()
}

/// `SelfUpdatePlan`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfUpdatePlan {
    pub package_name: JsString,
    pub install_spec: JsString,
    pub version: JsString,
    pub should_run: bool,
    pub note: Option<JsString>,
}

/// Decision half of `getSelfUpdatePlan`; the second value is the
/// "already up to date" line printed when no update runs.
pub fn self_update_plan(
    force: bool,
    release: &LatestPiRelease,
    current_version: &JsString,
) -> (SelfUpdatePlan, Option<JsString>) {
    let package_name = release
        .package_name
        .clone()
        .unwrap_or_else(|| config::package_config().package_name.clone().into());
    let install_spec = pi_js_compat::js_format!("{}@{}", package_name, release.version);
    if force
        || package_name != config::package_config().package_name
        || is_newer_package_version(&release.version, current_version)
    {
        return (
            SelfUpdatePlan {
                package_name: package_name.into(),
                install_spec,
                version: release.version.clone().into(),
                should_run: true,
                note: release.note.clone(),
            },
            None,
        );
    }
    let chalk = Chalk::stdout();
    (
        SelfUpdatePlan {
            package_name: package_name.into(),
            install_spec,
            version: release.version.clone().into(),
            should_run: false,
            note: None,
        },
        Some(chalk.green(&pi_js_compat::js_format!(
            "{} is already up to date (v{})",
            config::package_config().app_name,
            current_version
        ))),
    )
}

/// `getSelfUpdatePlan` including the release lookup and its error mapping.
pub async fn get_self_update_plan(
    fetch: &dyn pi_ai::request::Fetch,
    current_version: &JsString,
    force: bool,
) -> Result<(SelfUpdatePlan, Option<JsString>), JsString> {
    let release = fetch_latest_release(fetch, current_version)
        .await
        .map_err(|error| error.message)?;
    Ok(self_update_plan(force, &release, current_version))
}

async fn fetch_latest_release(
    fetch: &dyn pi_ai::request::Fetch,
    current_version: &JsString,
) -> Result<LatestPiRelease, CallbackError> {
    match get_latest_pi_release(fetch, current_version, &LatestPiReleaseOptions::default()).await {
        Ok(Some(release)) => Ok(release),
        Ok(None) => Err(CallbackError::new(pi_js_compat::js_format!(
            "Could not determine latest {} version.",
            config::package_config().app_name
        ))),
        Err(error) => {
            let message = error.error_or_string_message()?;
            Err(CallbackError::new(pi_js_compat::js_format!(
                "Could not determine latest {} version: {}",
                config::package_config().app_name,
                message
            )))
        }
    }
}

/// `printSelfUpdateUnavailable` stderr lines.
pub fn self_update_unavailable_lines(
    context: &config::InstallContext,
    npm_command: Option<&[JsString]>,
    target: &config::SelfUpdatePackageTarget,
    entrypoint: Option<&JsString>,
) -> Result<Vec<JsString>, JsString> {
    let package_name = JsString::from(config::package_config().package_name.clone());
    let mut lines = vec![
        pi_js_compat::js_format!(
            "error: {} cannot self-update this installation.",
            config::package_config().app_name
        )
        .into(),
        config::get_self_update_unavailable_instruction(context, &package_name, npm_command, Some(target))
            .map_err(|error| error.message().clone())?,
    ];
    if let Some(entrypoint) = entrypoint {
        lines.push(JsString::default());
        lines.push(pi_js_compat::js_format!("Location of pi executable: {}", entrypoint).into());
    }
    Ok(lines)
}

/// `printSelfUpdateFallback` stderr line.
pub fn self_update_fallback_line(command: &config::SelfUpdateCommand) -> JsString {
    Chalk::stderr().dim(&pi_js_compat::js_format!(
        "If this keeps failing, run this command yourself: {}",
        command.display
    ))
}

/// `printPnpmSelfUpdateMetadataHint` stderr lines.
pub fn pnpm_self_update_metadata_hint_lines() -> Vec<JsString> {
    let chalk = Chalk::stderr();
    vec![
        chalk.yellow("If pnpm reports missing package versions, its cached registry metadata may be stale."),
        chalk.yellow(&pi_js_compat::js_format!(
            "Run `pnpm store prune` and retry `{} update --self`.",
            config::package_config().app_name
        )),
    ]
}

/// `printSelfUpdateNote` stdout lines, including the surrounding blank lines.
pub fn self_update_note_lines(note: &JsString, columns: Option<usize>) -> Vec<JsString> {
    let trimmed_note = note.trim();
    if trimmed_note.is_empty() {
        return Vec::new();
    }
    let width = columns.unwrap_or(80).max(20);
    let chalk = Chalk::stdout();
    let mut lines = vec![JsString::default(), chalk.bold(&chalk.yellow("Update note"))];
    let mut markdown = pi_tui::Markdown::new(
        trimmed_note,
        0,
        0,
        Box::new(SelfUpdateNoteMarkdownTheme),
        None,
        pi_tui::MarkdownOptions::default(),
    );
    let rendered: Vec<JsString> = pi_tui::Component::render(&mut markdown, width)
        .into_iter()
        .map(|line| JsString::from(line.trim_end()))
        .collect();
    lines.extend(rendered);
    lines.push(JsString::default());
    lines
}

/// `SELF_UPDATE_NOTE_MARKDOWN_THEME`.
struct SelfUpdateNoteMarkdownTheme;

impl pi_tui::MarkdownTheme for SelfUpdateNoteMarkdownTheme {
    fn heading(&self, text: &JsString) -> JsString {
        let chalk = Chalk::stdout();
        chalk.bold(&chalk.yellow(text))
    }

    fn link(&self, text: &JsString) -> JsString {
        Chalk::stdout().cyan(text)
    }

    fn link_url(&self, text: &JsString) -> JsString {
        Chalk::stdout().dim(text)
    }

    fn code(&self, text: &JsString) -> JsString {
        Chalk::stdout().yellow(text)
    }

    fn code_block(&self, text: &JsString) -> JsString {
        Chalk::stdout().dim(text)
    }

    fn code_block_border(&self, text: &JsString) -> JsString {
        Chalk::stdout().dim(text)
    }

    fn quote(&self, text: &JsString) -> JsString {
        Chalk::stdout().dim(text)
    }

    fn quote_border(&self, text: &JsString) -> JsString {
        Chalk::stdout().dim(text)
    }

    fn hr(&self, text: &JsString) -> JsString {
        Chalk::stdout().dim(text)
    }

    fn list_bullet(&self, text: &JsString) -> JsString {
        Chalk::stdout().yellow(text)
    }

    fn bold(&self, text: &JsString) -> JsString {
        Chalk::stdout().bold(text)
    }

    fn italic(&self, text: &JsString) -> JsString {
        Chalk::stdout().italic(text)
    }

    fn strikethrough(&self, text: &JsString) -> JsString {
        Chalk::stdout().strikethrough(text)
    }

    fn underline(&self, text: &JsString) -> JsString {
        Chalk::stdout().underline(text)
    }
}

/// `runSelfUpdate`: one inherited-stdio child per step, in order.
pub async fn run_self_update(command: &config::SelfUpdateCommand) -> Result<(), JsString> {
    let steps: Vec<config::SelfUpdateCommandStep> = command.steps.clone().unwrap_or_else(|| {
        vec![config::SelfUpdateCommandStep {
            command: command.command.clone(),
            args: command.args.clone(),
            display: command.display.clone(),
        }]
    });
    for step in steps {
        let status = tokio::process::Command::new(pi_js_compat::js_json::encode_node_utf8(&step.command))
            .args(step.args.iter().map(pi_js_compat::js_json::encode_node_utf8))
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .await
            .map_err(|error| crate::utils::child_process::node_spawn_error_message("spawn", &step.command, &error))?;
        if status.success() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(signal) = status.signal() {
                return Err(pi_js_compat::js_format!(
                    "{} terminated by signal {}",
                    step.display,
                    crate::utils::child_process::signal_name(signal)
                ));
            }
        }
        return Err(pi_js_compat::js_format!(
            "{} exited with code {}",
            step.display,
            status
                .code()
                .map_or_else(|| JsString::from("unknown"), |code| code.to_string().into())
        ));
    }
    Ok(())
}

/// `selectConfig` request handed to the config TUI.
pub struct SelectConfigRequest {
    pub resolved_paths: ResolvedPaths,
    pub settings: Arc<SettingsManagerStore>,
    pub cwd: JsString,
    pub agent_dir: JsString,
}

/// Progress sink: one line per `start` event.
pub type PackageProgressSink = Arc<dyn Fn(JsString) + Send + Sync>;

/// Non-`Send` future used by effects that wrap `DefaultPackageManager::update`
/// (the package manager's own fan-out futures are not `Send`).
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Environment-bound operations behind the package and config commands.
pub trait PackageCommandEffects {
    fn cwd(&self) -> JsString;
    fn agent_dir(&self) -> JsString;
    fn stdout_columns(&self) -> Option<usize>;
    fn entrypoint(&self) -> Option<JsString>;
    fn install_context(&self) -> config::InstallContext;
    /// Inline factories loaded before the project-trust prompt resolves.
    fn extension_factories(&self) -> Vec<crate::core::resource_loader::InlineExtension> {
        Vec::new()
    }

    fn create_settings<'a>(
        &'a self,
        options: CreateCommandSettingsOptions,
    ) -> LocalBoxFuture<'a, Result<CommandSettings, CallbackError>>;
    fn latest_release<'a>(&'a self) -> LocalBoxFuture<'a, Result<LatestPiRelease, JsString>>;
    fn run_self_update<'a>(
        &'a self,
        command: &'a config::SelfUpdateCommand,
    ) -> LocalBoxFuture<'a, Result<(), JsString>>;
    fn select_config<'a>(&'a self, request: SelectConfigRequest) -> LocalBoxFuture<'a, Result<(), JsString>>;

    fn install_packages<'a>(
        &'a self,
        settings: &'a Arc<SettingsManagerStore>,
        source: &'a JsString,
        local: bool,
        progress: PackageProgressSink,
    ) -> LocalBoxFuture<'a, Result<(), JsString>>;
    fn remove_packages<'a>(
        &'a self,
        settings: &'a Arc<SettingsManagerStore>,
        source: &'a JsString,
        local: bool,
        progress: PackageProgressSink,
    ) -> LocalBoxFuture<'a, Result<bool, JsString>>;
    fn list_packages(&self, settings: &Arc<SettingsManagerStore>) -> Result<Vec<ConfiguredPackage>, JsString>;
    fn update_packages<'a>(
        &'a self,
        settings: &'a Arc<SettingsManagerStore>,
        source: Option<&'a JsString>,
        progress: PackageProgressSink,
    ) -> LocalBoxFuture<'a, Result<(), JsString>>;
}

/// `executePackageCommand`: the execution half of `handlePackageCommand`.
///
/// `Err` carries an exception that escapes the TypeScript function (settings
/// creation and trust-store failures).
pub async fn execute_package_command(
    args: &[JsString],
    effects: &dyn PackageCommandEffects,
) -> Result<Option<CommandOutcome>, CallbackError> {
    let Some(mut outcome) = handle_package_command(args) else {
        return Ok(None);
    };
    let Some(action) = outcome.action.take() else {
        return Ok(Some(outcome));
    };
    let chalk_out = Chalk::stdout();
    let chalk_err = Chalk::stderr();

    let cwd = effects.cwd();
    let agent_dir = effects.agent_dir();
    let writes_project_package_config = matches!(action, PackageAction::Install { .. } | PackageAction::Remove { .. })
        && matches!(
            action,
            PackageAction::Install { local: true, .. } | PackageAction::Remove { local: true, .. }
        );
    let settings = effects
        .create_settings(CreateCommandSettingsOptions {
            cwd: cwd.clone(),
            agent_dir: agent_dir.clone(),
            project_trust_override: outcome.project_trust_override,
            use_saved_project_trust_only: matches!(action, PackageAction::Update { .. }),
            extension_factories: effects.extension_factories(),
            startup_ui: None,
        })
        .await?;
    outcome
        .stderr
        .extend(report_project_trust_warnings_lines(&settings.project_trust_warnings));
    let trusted = settings.store.with(|manager| manager.is_project_trusted());
    if !trusted && writes_project_package_config {
        outcome
            .stderr
            .push(chalk_err.red("Project is not trusted. Use --approve to modify local package config."));
        outcome.exit_code = Some(1);
        return Ok(Some(outcome));
    }
    outcome
        .stderr
        .extend(report_settings_errors(&settings.store, "package command"));
    let npm_command: Option<Vec<JsString>> = settings.store.with(|manager| {
        manager
            .get_global_settings()
            .get("npmCommand")
            .and_then(|value| pi_js_compat::js_json::from_value(value).ok())
    });

    match action {
        PackageAction::Install { source, local } => {
            let (progress, buffer) = progress_buffer();
            let result = effects
                .install_packages(&settings.store, &source, local, progress)
                .await;
            outcome.stdout.extend(drain_progress(&buffer));
            match result {
                Ok(()) => outcome
                    .stdout
                    .push(chalk_out.green(&pi_js_compat::js_format!("Installed {}", source))),
                Err(message) => package_error(&mut outcome, &message),
            }
        }
        PackageAction::Remove { source, local } => {
            let (progress, buffer) = progress_buffer();
            let result = effects.remove_packages(&settings.store, &source, local, progress).await;
            outcome.stdout.extend(drain_progress(&buffer));
            match result {
                Ok(true) => outcome
                    .stdout
                    .push(chalk_out.green(&pi_js_compat::js_format!("Removed {}", source))),
                Ok(false) => {
                    outcome
                        .stderr
                        .push(chalk_err.red(&pi_js_compat::js_format!("No matching package found for {}", source)));
                    outcome.exit_code = Some(1);
                }
                Err(message) => package_error(&mut outcome, &message),
            }
        }
        PackageAction::List => match effects.list_packages(&settings.store) {
            Ok(packages) => outcome.stdout.push(format_package_list(&packages)),
            Err(message) => package_error(&mut outcome, &message),
        },
        PackageAction::Update {
            target,
            force,
            show_extensions_skipped_note,
        } => {
            if show_extensions_skipped_note {
                outcome.stdout.push(chalk_out.dim(&pi_js_compat::js_format!(
                    "Extensions are skipped. Run {} update --extensions to update extensions.",
                    config::package_config().app_name
                )));
            }
            if update_target_includes_extensions(&target) {
                let update_source = match &target {
                    UpdateTarget::Extensions { source } => source.clone(),
                    _ => None,
                };
                let (progress, buffer) = progress_buffer();
                let result = effects
                    .update_packages(&settings.store, update_source.as_ref(), progress)
                    .await;
                outcome.stdout.extend(drain_progress(&buffer));
                match result {
                    Ok(()) => match &update_source {
                        Some(source) => outcome
                            .stdout
                            .push(chalk_out.green(&pi_js_compat::js_format!("Updated {}", source))),
                        None => outcome.stdout.push(chalk_out.green("Updated packages")),
                    },
                    Err(message) => {
                        package_error(&mut outcome, &message);
                        return Ok(Some(outcome));
                    }
                }
            }
            if update_target_includes_self(&target)
                && let Err(message) =
                    run_self_update_target(&mut outcome, effects, &agent_dir, force, npm_command.as_deref()).await
            {
                package_error(&mut outcome, &message);
            }
        }
        PackageAction::Config => {}
    }
    Ok(Some(outcome))
}

fn progress_buffer() -> (PackageProgressSink, Arc<Mutex<Vec<JsString>>>) {
    let lines: Arc<Mutex<Vec<JsString>>> = Arc::new(Mutex::new(Vec::new()));
    let sink_lines = Arc::clone(&lines);
    let chalk = Chalk::stdout();
    let sink: PackageProgressSink = Arc::new(move |message: JsString| {
        sink_lines.lock().unwrap().push(chalk.dim(&message));
    });
    (sink, lines)
}

fn drain_progress(lines: &Arc<Mutex<Vec<JsString>>>) -> Vec<JsString> {
    lines.lock().unwrap().drain(..).collect()
}

fn package_error(outcome: &mut CommandOutcome, message: &JsString) {
    outcome
        .stderr
        .push(Chalk::stderr().red(&pi_js_compat::js_format!("Error: {}", message)));
    outcome.exit_code = Some(1);
}

async fn run_self_update_target(
    outcome: &mut CommandOutcome,
    effects: &dyn PackageCommandEffects,
    _agent_dir: &JsString,
    force: bool,
    npm_command: Option<&[JsString]>,
) -> Result<(), JsString> {
    let chalk_out = Chalk::stdout();
    let chalk_err = Chalk::stderr();
    let current_version = config::package_config().version.clone();
    let release = match effects.latest_release().await {
        Ok(release) => release,
        Err(message) => {
            outcome
                .stderr
                .push(chalk_err.red(&pi_js_compat::js_format!("Error: {}", message)));
            outcome.exit_code = Some(1);
            return Ok(());
        }
    };
    let (plan, up_to_date_line) = self_update_plan(force, &release, &JsString::from(&current_version));
    if let Some(line) = up_to_date_line {
        outcome.stdout.push(line);
        return Ok(());
    }
    if !plan.should_run {
        return Ok(());
    }
    let install_context = effects.install_context();
    let install_method = config::detect_install_method(&install_context);
    if install_context.platform == "win32"
        && install_method != config::InstallMethod::Npm
        && install_method != config::InstallMethod::Pnpm
    {
        outcome.stderr.push(chalk_err.red(&pi_js_compat::js_format!(
            "{} self-update on Windows is only supported for npm and pnpm installs.",
            config::package_config().app_name
        )));
        outcome.stderr.push(chalk_err.dim(&pi_js_compat::js_format!(
            "Detected install method: {}. Update {} manually.",
            install_method.as_str(),
            config::package_config().app_name
        )));
        outcome.exit_code = Some(1);
        return Ok(());
    }
    let package_name = config::package_config().package_name.clone();
    let target = config::SelfUpdatePackageTarget {
        package_name: plan.package_name.clone(),
        install_spec: Some(plan.install_spec.clone()),
    };
    let Some(command) = config::get_self_update_command(
        &install_context,
        &package_name.clone().into(),
        npm_command,
        Some(&target),
    )
    .map_err(|error| error.message().clone())?
    else {
        outcome.stderr.extend(self_update_unavailable_lines(
            &install_context,
            npm_command,
            &target,
            effects.entrypoint().as_ref(),
        )?);
        outcome.exit_code = Some(1);
        return Ok(());
    };
    if let Some(note) = &plan.note {
        outcome
            .stdout
            .extend(self_update_note_lines(note, effects.stdout_columns()));
    }
    if install_method == config::InstallMethod::Npm {
        prepare_windows_npm_self_update(&install_context).map_err(|error| error.message().clone())?;
    }
    outcome.stdout.push(chalk_out.dim(&pi_js_compat::js_format!(
        "Updating {} with {}...",
        config::package_config().app_name,
        command.display
    )));
    match effects.run_self_update(&command).await {
        Ok(()) => outcome.stdout.push(chalk_out.green(&pi_js_compat::js_format!(
            "Updated {} from {} to {}",
            config::package_config().app_name,
            current_version,
            plan.version
        ))),
        Err(message) => {
            outcome
                .stderr
                .push(chalk_err.red(&pi_js_compat::js_format!("Error: {}", message)));
            if install_method == config::InstallMethod::Pnpm {
                outcome.stderr.extend(pnpm_self_update_metadata_hint_lines());
            }
            outcome.stderr.push(self_update_fallback_line(&command));
            outcome.exit_code = Some(1);
        }
    }
    Ok(())
}

/// `prepareWindowsNpmSelfUpdate`: no-op outside Windows.
fn prepare_windows_npm_self_update(context: &config::InstallContext) -> Result<(), config::ConfigError> {
    if context.platform != "win32" {
        return Ok(());
    }
    let package_dir = std::path::PathBuf::from(pi_js_compat::js_json::encode_node_utf8(&config::get_package_dir()?));
    crate::utils::windows_self_update::cleanup_windows_self_update_quarantine(&package_dir);
    let _ = crate::utils::windows_self_update::quarantine_windows_native_dependencies(&package_dir, &[]);
    Ok(())
}

/// `executeConfigCommand`: the execution half of `handleConfigCommand`.
pub async fn execute_config_command(
    args: &[JsString],
    effects: &dyn PackageCommandEffects,
) -> Result<Option<CommandOutcome>, CallbackError> {
    let Some(mut outcome) = handle_config_command(args) else {
        return Ok(None);
    };
    let action = outcome.action.take();
    let Some(PackageAction::Config) = action else {
        return Ok(Some(outcome));
    };
    let cwd = effects.cwd();
    let agent_dir = effects.agent_dir();
    let settings = effects
        .create_settings(CreateCommandSettingsOptions {
            cwd: cwd.clone(),
            agent_dir: agent_dir.clone(),
            project_trust_override: outcome.project_trust_override,
            use_saved_project_trust_only: false,
            extension_factories: effects.extension_factories(),
            startup_ui: None,
        })
        .await?;
    outcome
        .stderr
        .extend(report_project_trust_warnings_lines(&settings.project_trust_warnings));
    outcome
        .stderr
        .extend(report_settings_errors(&settings.store, "config command"));

    let package_manager = DefaultPackageManager::new(
        &cwd,
        &agent_dir,
        Arc::clone(&settings.store) as Arc<dyn PackageSettingsStore>,
    )
    .map_err(|error| error.message().clone())?;
    let resolved_paths = package_manager
        .resolve(None)
        .await
        .map_err(|error| error.message().clone())?;

    effects
        .select_config(SelectConfigRequest {
            resolved_paths,
            settings: Arc::clone(&settings.store),
            cwd,
            agent_dir,
        })
        .await?;
    outcome.exit_code = Some(0);
    Ok(Some(outcome))
}

/// `DefaultPackageCommandEffects`: the real host wiring.
pub struct DefaultPackageCommandEffects {
    pub cwd: JsString,
    pub agent_dir: JsString,
    pub install_context: config::InstallContext,
    pub entrypoint: Option<JsString>,
    pub extensions_loader: Arc<dyn ProjectTrustExtensionsLoader>,
    pub config_selector: Rc<dyn ConfigSelector>,
    pub fetch: Arc<dyn pi_ai::request::Fetch>,
    pub extension_factories: Vec<crate::core::resource_loader::InlineExtension>,
}

/// `selectConfig` for the real host.
pub trait ConfigSelector {
    fn select<'a>(&'a self, request: SelectConfigRequest) -> LocalBoxFuture<'a, Result<(), JsString>>;
}

impl PackageCommandEffects for DefaultPackageCommandEffects {
    fn cwd(&self) -> JsString {
        self.cwd.clone()
    }

    fn extension_factories(&self) -> Vec<crate::core::resource_loader::InlineExtension> {
        self.extension_factories.clone()
    }

    fn agent_dir(&self) -> JsString {
        self.agent_dir.clone()
    }

    fn stdout_columns(&self) -> Option<usize> {
        pi_tui::terminal::stdout_columns()
    }

    fn entrypoint(&self) -> Option<JsString> {
        self.entrypoint.clone()
    }

    fn install_context(&self) -> config::InstallContext {
        self.install_context.clone()
    }

    fn create_settings<'a>(
        &'a self,
        options: CreateCommandSettingsOptions,
    ) -> LocalBoxFuture<'a, Result<CommandSettings, CallbackError>> {
        Box::pin(async move { create_command_settings_manager(options, self.extensions_loader.as_ref()).await })
    }

    fn latest_release<'a>(&'a self) -> LocalBoxFuture<'a, Result<LatestPiRelease, JsString>> {
        Box::pin(async move {
            fetch_latest_release(self.fetch.as_ref(), &config::package_config().version)
                .await
                .map_err(|error| error.message)
        })
    }

    fn run_self_update<'a>(
        &'a self,
        command: &'a config::SelfUpdateCommand,
    ) -> LocalBoxFuture<'a, Result<(), JsString>> {
        Box::pin(run_self_update(command))
    }

    fn select_config<'a>(&'a self, request: SelectConfigRequest) -> LocalBoxFuture<'a, Result<(), JsString>> {
        Box::pin(async move { self.config_selector.select(request).await })
    }

    fn install_packages<'a>(
        &'a self,
        settings: &'a Arc<SettingsManagerStore>,
        source: &'a JsString,
        local: bool,
        progress: PackageProgressSink,
    ) -> LocalBoxFuture<'a, Result<(), JsString>> {
        Box::pin(async move {
            let manager = self.package_manager(settings, progress)?;
            manager
                .install_and_persist(source, local)
                .await
                .map_err(|error| error.message().clone())
        })
    }

    fn remove_packages<'a>(
        &'a self,
        settings: &'a Arc<SettingsManagerStore>,
        source: &'a JsString,
        local: bool,
        progress: PackageProgressSink,
    ) -> LocalBoxFuture<'a, Result<bool, JsString>> {
        Box::pin(async move {
            let manager = self.package_manager(settings, progress)?;
            manager
                .remove_and_persist(source, local)
                .await
                .map_err(|error| error.message().clone())
        })
    }

    fn list_packages(&self, settings: &Arc<SettingsManagerStore>) -> Result<Vec<ConfiguredPackage>, JsString> {
        DefaultPackageManager::new(
            &self.cwd,
            &self.agent_dir,
            Arc::clone(settings) as Arc<dyn PackageSettingsStore>,
        )
        .map_err(|error| error.message().clone())?
        .list_configured_packages()
        .map_err(|error| error.message().clone())
    }

    fn update_packages<'a>(
        &'a self,
        settings: &'a Arc<SettingsManagerStore>,
        source: Option<&'a JsString>,
        progress: PackageProgressSink,
    ) -> LocalBoxFuture<'a, Result<(), JsString>> {
        Box::pin(async move {
            let manager = self.package_manager(settings, progress)?;
            manager.update(source).await.map_err(|error| error.message().clone())
        })
    }
}

impl DefaultPackageCommandEffects {
    fn package_manager(
        &self,
        settings: &Arc<SettingsManagerStore>,
        progress: PackageProgressSink,
    ) -> Result<DefaultPackageManager, JsString> {
        let manager = DefaultPackageManager::new(
            &self.cwd,
            &self.agent_dir,
            Arc::clone(settings) as Arc<dyn PackageSettingsStore>,
        )
        .map_err(|error| error.message().clone())?;
        manager.set_progress_callback(Some(Arc::new(move |event: &ProgressEvent| {
            if event.event_type == ProgressType::Start
                && let Some(message) = &event.message
            {
                progress(message.clone());
            }
            Ok(())
        })));
        Ok(manager)
    }
}
