//! Execution half of `package-manager-cli.ts`: package and config commands.

use pi_js_compat::js_json::JsString;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pi_ai::request::BoxFuture;
use pi_coding_agent::config::{InstallContext, SelfUpdateCommand};
use pi_coding_agent::core::extensions::create_extension_runtime;
use pi_coding_agent::core::extensions::types::LoadExtensionsResult;
use pi_coding_agent::core::package_manager::{ConfiguredPackage, InstalledScope};
use pi_coding_agent::core::project_trust::AppMode;
use pi_coding_agent::core::settings_manager::SettingsManager;
use pi_coding_agent::core::trust_manager::{ProjectTrustDecision, ProjectTrustStore};
use pi_coding_agent::package_manager_cli::{
    CommandSettings, ConfigWriteScope, CreateCommandSettingsOptions, DefaultPackageCommandEffects, LocalBoxFuture,
    PackageCommandEffects, PackageProgressSink, ProjectTrustExtensionsLoader, SelectConfigRequest, command_app_mode,
    create_command_settings_manager, execute_config_command, execute_package_command, format_package_list,
    get_self_update_plan, pnpm_self_update_metadata_hint_lines, report_project_trust_warnings_lines, run_self_update,
    self_update_fallback_line, self_update_note_lines, self_update_plan, self_update_unavailable_lines,
};
use pi_coding_agent::utils::chalk::Chalk;
use pi_coding_agent::utils::version_check::LatestPiRelease;

/// The source `VERSION` constant the TypeScript tests interpolate.
fn current_version() -> String {
    pi_coding_agent::config::package_config().version.clone()
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-pkg-cmd-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn agent_dir(name: &str) -> PathBuf {
    let dir = temp_dir(name).join("agent");
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct NoExtensionsLoader;

impl ProjectTrustExtensionsLoader for NoExtensionsLoader {
    fn load<'a>(
        &'a self,
        _cwd: &'a JsString,
        _agent_dir: &'a JsString,
        _settings_manager: &'a Arc<Mutex<SettingsManager>>,
        _extension_factories: &'a [pi_coding_agent::core::resource_loader::InlineExtension],
    ) -> BoxFuture<'a, Result<Option<LoadExtensionsResult>, JsString>> {
        Box::pin(async { Ok(None) })
    }
}

struct ErroringExtensionsLoader {
    path: String,
}

impl ProjectTrustExtensionsLoader for ErroringExtensionsLoader {
    fn load<'a>(
        &'a self,
        _cwd: &'a JsString,
        _agent_dir: &'a JsString,
        _settings_manager: &'a Arc<Mutex<SettingsManager>>,
        _extension_factories: &'a [pi_coding_agent::core::resource_loader::InlineExtension],
    ) -> BoxFuture<'a, Result<Option<LoadExtensionsResult>, JsString>> {
        let path = self.path.clone();
        Box::pin(async move {
            Ok(Some(LoadExtensionsResult {
                extensions: Vec::new(),
                errors: vec![pi_coding_agent::core::extensions::types::ExtensionLoadError {
                    path: path.into(),
                    error: "boom".to_owned().into(),
                }],
                runtime: create_extension_runtime(),
            }))
        })
    }
}

/// Install context that resolves to the `bun` self-update method without
/// touching the host: a Bun runtime under `<home>/.bun/install/global/...`.
fn bun_install_context(root: &Path) -> InstallContext {
    let home = root.join("home");
    let package_dir = home.join(".bun/install/global/node_modules/@earendil-works/pi-coding-agent");
    fs::create_dir_all(&package_dir).unwrap();
    InstallContext {
        package_dir,
        exec_path: root.join("pi").display().to_string(),
        entrypoint: Some(root.join("pi.js").display().to_string()),
        is_bun_binary: false,
        is_bun_runtime: true,
        platform: "linux".to_owned(),
        home,
        path_env: Some(String::new()),
        env: BTreeMap::new(),
    }
}

fn bun_binary_install_context(root: &Path) -> InstallContext {
    InstallContext {
        package_dir: root.to_path_buf(),
        exec_path: root.join("pi").display().to_string(),
        entrypoint: None,
        is_bun_binary: true,
        is_bun_runtime: false,
        platform: "linux".to_owned(),
        home: root.join("home"),
        path_env: Some(String::new()),
        env: BTreeMap::new(),
    }
}

struct FakeEffects {
    cwd: JsString,
    agent_dir: JsString,
    context: InstallContext,
    entrypoint: Option<JsString>,
    loader: Arc<dyn ProjectTrustExtensionsLoader>,
    release: Result<LatestPiRelease, JsString>,
    self_update: Result<(), JsString>,
    refresh: Result<(), pi_js_compat::js_json::JsString>,
    install: Result<(), JsString>,
    remove: Result<bool, JsString>,
    update: Result<(), JsString>,
    packages: Vec<ConfiguredPackage>,
    progress_messages: Vec<JsString>,
    config_requests: Mutex<Vec<(ConfigWriteScope, bool)>>,
    calls: Mutex<Vec<JsString>>,
}

impl FakeEffects {
    fn new(cwd: &Path, agent_dir: &Path, context: InstallContext) -> Self {
        Self {
            cwd: cwd.display().to_string().into(),
            agent_dir: agent_dir.display().to_string().into(),
            entrypoint: context.entrypoint.clone().map(JsString::from),
            context,
            loader: Arc::new(NoExtensionsLoader),
            release: Ok(LatestPiRelease {
                version: "9.9.9".to_owned().into(),
                package_name: None,
                note: None,
            }),
            self_update: Ok(()),
            refresh: Ok(()),
            install: Ok(()),
            remove: Ok(true),
            update: Ok(()),
            packages: Vec::new(),
            progress_messages: Vec::new(),
            config_requests: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<JsString> {
        self.calls.lock().unwrap().clone()
    }
}

impl PackageCommandEffects for FakeEffects {
    fn cwd(&self) -> JsString {
        self.cwd.clone()
    }

    fn agent_dir(&self) -> JsString {
        self.agent_dir.clone()
    }

    fn stdout_columns(&self) -> Option<usize> {
        Some(60)
    }

    fn entrypoint(&self) -> Option<JsString> {
        self.entrypoint.clone()
    }

    fn install_context(&self) -> InstallContext {
        self.context.clone()
    }

    fn create_settings<'a>(
        &'a self,
        options: CreateCommandSettingsOptions,
    ) -> LocalBoxFuture<'a, Result<CommandSettings, JsString>> {
        Box::pin(async move { create_command_settings_manager(options, self.loader.as_ref()).await })
    }

    fn refresh_model_catalogs<'a>(
        &'a self,
        _agent_dir: &'a JsString,
    ) -> LocalBoxFuture<'a, Result<(), pi_js_compat::js_json::JsString>> {
        self.calls.lock().unwrap().push("refresh".into());
        let result = self.refresh.clone();
        Box::pin(async move { result })
    }

    fn latest_release<'a>(&'a self) -> LocalBoxFuture<'a, Result<LatestPiRelease, JsString>> {
        self.calls.lock().unwrap().push("latest_release".into());
        let result = self.release.clone();
        Box::pin(async move { result })
    }

    fn run_self_update<'a>(&'a self, command: &'a SelfUpdateCommand) -> LocalBoxFuture<'a, Result<(), JsString>> {
        self.calls
            .lock()
            .unwrap()
            .push(pi_js_compat::js_format!("self_update:{}", &command.display));
        let result = self.self_update.clone();
        Box::pin(async move { result })
    }

    fn select_config<'a>(&'a self, request: SelectConfigRequest) -> LocalBoxFuture<'a, Result<(), JsString>> {
        self.config_requests
            .lock()
            .unwrap()
            .push((request.write_scope, request.project_mode_available));
        Box::pin(async { Ok(()) })
    }

    fn install_packages<'a>(
        &'a self,
        _settings: &'a Arc<pi_coding_agent::core::package_manager::SettingsManagerStore>,
        source: &'a JsString,
        local: bool,
        progress: PackageProgressSink,
    ) -> LocalBoxFuture<'a, Result<(), JsString>> {
        self.calls
            .lock()
            .unwrap()
            .push(pi_js_compat::js_format!("install:{0}:{1}", source, local.to_string()));
        for message in &self.progress_messages {
            progress(message.clone().into());
        }
        let result = self.install.clone();
        Box::pin(async move { result })
    }

    fn remove_packages<'a>(
        &'a self,
        _settings: &'a Arc<pi_coding_agent::core::package_manager::SettingsManagerStore>,
        source: &'a JsString,
        local: bool,
        progress: PackageProgressSink,
    ) -> LocalBoxFuture<'a, Result<bool, JsString>> {
        self.calls
            .lock()
            .unwrap()
            .push(pi_js_compat::js_format!("remove:{0}:{1}", source, local.to_string()));
        for message in &self.progress_messages {
            progress(message.clone().into());
        }
        let result = self.remove.clone();
        Box::pin(async move { result })
    }

    fn list_packages(
        &self,
        _settings: &Arc<pi_coding_agent::core::package_manager::SettingsManagerStore>,
    ) -> Vec<ConfiguredPackage> {
        self.packages.clone()
    }

    fn update_packages<'a>(
        &'a self,
        _settings: &'a Arc<pi_coding_agent::core::package_manager::SettingsManagerStore>,
        source: Option<&'a JsString>,
        progress: PackageProgressSink,
    ) -> LocalBoxFuture<'a, Result<(), JsString>> {
        self.calls.lock().unwrap().push(pi_js_compat::js_format!(
            "update:{}",
            source.cloned().unwrap_or_else(|| "<all>".into())
        ));
        for message in &self.progress_messages {
            progress(message.clone().into());
        }
        let result = self.update.clone();
        Box::pin(async move { result })
    }
}

fn args(values: &[&str]) -> Vec<JsString> {
    values.iter().map(|value| JsString::from(*value)).collect()
}

#[test]
fn command_app_mode_requires_both_streams_to_be_terminals() {
    assert_eq!(command_app_mode(true, true), AppMode::Interactive);
    assert_eq!(command_app_mode(true, false), AppMode::Print);
    assert_eq!(command_app_mode(false, true), AppMode::Print);
    assert_eq!(command_app_mode(false, false), AppMode::Print);
}

#[tokio::test]
async fn install_prints_progress_then_the_installed_line() {
    let root = temp_dir("install");
    let mut effects = FakeEffects::new(&root, &agent_dir("install"), bun_install_context(&root));
    effects.progress_messages = vec!["Installing npm:@foo/bar".to_owned().into()];

    let outcome = execute_package_command(&args(&["install", "npm:@foo/bar", "-l", "--approve"]), &effects)
        .await
        .unwrap()
        .expect("install is handled");

    let chalk = Chalk::stdout();
    assert_eq!(
        outcome.stdout,
        vec![
            chalk.dim("Installing npm:@foo/bar"),
            chalk.green("Installed npm:@foo/bar"),
        ]
    );
    assert!(outcome.stderr.is_empty());
    assert_eq!(outcome.exit_code, None);
    assert_eq!(effects.calls(), vec!["install:npm:@foo/bar:true"]);
}

#[tokio::test]
async fn install_failure_prints_the_error_and_sets_exit_code() {
    let root = temp_dir("install-fail");
    let mut effects = FakeEffects::new(&root, &agent_dir("install-fail"), bun_install_context(&root));
    effects.install = Err("network down".to_owned().into());

    let outcome = execute_package_command(&args(&["install", "npm:x", "--approve"]), &effects)
        .await
        .unwrap()
        .expect("install is handled");

    assert!(outcome.stdout.is_empty());
    assert_eq!(outcome.stderr, vec![Chalk::stderr().red("Error: network down")]);
    assert_eq!(outcome.exit_code, Some(1));
}

#[tokio::test]
async fn local_writes_require_trust() {
    let root = temp_dir("trust");
    let project = root.join("project");
    fs::create_dir_all(project.join(".pi")).unwrap();
    fs::write(project.join(".pi/settings.json"), "{}").unwrap();
    let effects = FakeEffects::new(&project, &agent_dir("trust"), bun_install_context(&root));

    let outcome = execute_package_command(&args(&["install", "npm:x", "-l", "--no-approve"]), &effects)
        .await
        .unwrap()
        .expect("install is handled");

    assert_eq!(
        outcome.stderr,
        vec![Chalk::stderr().red("Project is not trusted. Use --approve to modify local package config.")]
    );
    assert_eq!(outcome.exit_code, Some(1));
    assert!(effects.calls().is_empty());
}

#[tokio::test]
async fn approved_local_install_runs_even_with_project_resources() {
    let root = temp_dir("approve");
    let project = root.join("project");
    fs::create_dir_all(project.join(".pi")).unwrap();
    fs::write(project.join(".pi/settings.json"), "{}").unwrap();
    let effects = FakeEffects::new(&project, &agent_dir("approve"), bun_install_context(&root));

    let outcome = execute_package_command(&args(&["install", "npm:x", "-l", "--approve"]), &effects)
        .await
        .unwrap()
        .expect("install is handled");

    assert_eq!(outcome.exit_code, None);
    assert_eq!(effects.calls(), vec!["install:npm:x:true"]);
}

#[tokio::test]
async fn remove_reports_missing_packages() {
    let root = temp_dir("remove-missing");
    let mut effects = FakeEffects::new(&root, &agent_dir("remove-missing"), bun_install_context(&root));
    effects.remove = Ok(false);

    let outcome = execute_package_command(&args(&["remove", "npm:x", "--approve"]), &effects)
        .await
        .unwrap()
        .expect("remove is handled");

    assert_eq!(
        outcome.stderr,
        vec![Chalk::stderr().red("No matching package found for npm:x")]
    );
    assert_eq!(outcome.exit_code, Some(1));
}

#[tokio::test]
async fn remove_success_prints_the_removed_line() {
    let root = temp_dir("remove");
    let effects = FakeEffects::new(&root, &agent_dir("remove"), bun_install_context(&root));

    let outcome = execute_package_command(&args(&["uninstall", "npm:x", "--approve"]), &effects)
        .await
        .unwrap()
        .expect("uninstall is handled");

    assert_eq!(outcome.stdout, vec![Chalk::stdout().green("Removed npm:x")]);
    assert_eq!(effects.calls(), vec!["remove:npm:x:false"]);
}

#[tokio::test]
async fn list_prints_the_configured_package_table() {
    let root = temp_dir("list");
    let mut effects = FakeEffects::new(&root, &agent_dir("list"), bun_install_context(&root));
    effects.packages = vec![
        ConfiguredPackage {
            source: "npm:@foo/bar".to_owned().into(),
            scope: InstalledScope::User,
            filtered: false,
            installed_path: Some("/agent/node_modules/@foo/bar".to_owned().into()),
        },
        ConfiguredPackage {
            source: "git:github.com/x/y".to_owned().into(),
            scope: InstalledScope::Project,
            filtered: true,
            installed_path: None,
        },
    ];

    let outcome = execute_package_command(&args(&["list"]), &effects)
        .await
        .unwrap()
        .expect("list is handled");

    assert_eq!(outcome.stdout, vec![format_package_list(&effects.packages)]);
}

#[tokio::test]
async fn models_target_refreshes_catalogs_and_skips_settings() {
    let root = temp_dir("models");
    let effects = FakeEffects::new(&root, &agent_dir("models"), bun_install_context(&root));

    let outcome = execute_package_command(&args(&["update", "--models"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");

    assert_eq!(outcome.stdout, vec![Chalk::stdout().green("Model catalogs refreshed")]);
    assert_eq!(effects.calls(), vec!["refresh"]);
}

#[tokio::test]
async fn models_refresh_failure_sets_exit_code() {
    let root = temp_dir("models-fail");
    let mut effects = FakeEffects::new(&root, &agent_dir("models-fail"), bun_install_context(&root));
    effects.refresh = Err("Could not refresh model catalogs: x: y".into());

    let outcome = execute_package_command(&args(&["update", "--models"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");

    assert_eq!(
        outcome.stderr,
        vec![Chalk::stderr().red("Error: Could not refresh model catalogs: x: y")]
    );
    assert_eq!(outcome.exit_code, Some(1));
}

#[tokio::test]
async fn default_update_notes_skipped_extensions_and_runs_self_update() {
    let root = temp_dir("self-update");
    let effects = FakeEffects::new(&root, &agent_dir("self-update"), bun_install_context(&root));

    let outcome = execute_package_command(&args(&["update"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");

    let app = pi_coding_agent::config::package_config().app_name.clone();
    let chalk = Chalk::stdout();
    let calls = effects.calls();
    assert_eq!(calls[0], "latest_release");
    assert_eq!(
        calls[1],
        "self_update:bun install -g --ignore-scripts --minimum-release-age=0 @earendil-works/pi-coding-agent@9.9.9"
    );
    assert_eq!(
        outcome.stdout,
        vec![
            chalk.dim(&format!(
                "Extensions are skipped. Run {app} update --extensions to update extensions."
            )),
            chalk.dim(&format!(
                "Updating {app} with bun install -g --ignore-scripts --minimum-release-age=0 @earendil-works/pi-coding-agent@9.9.9..."
            )),
            chalk.green(&format!("Updated {app} from {} to 9.9.9", current_version())),
        ]
    );
    assert_eq!(outcome.exit_code, None);
}

#[tokio::test]
async fn up_to_date_release_prints_the_up_to_date_line() {
    let root = temp_dir("up-to-date");
    let mut effects = FakeEffects::new(&root, &agent_dir("up-to-date"), bun_install_context(&root));
    effects.release = Ok(LatestPiRelease {
        version: current_version().into(),
        package_name: None,
        note: None,
    });

    let outcome = execute_package_command(&args(&["update"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");

    let app = pi_coding_agent::config::package_config().app_name.clone();
    assert_eq!(
        outcome.stdout,
        vec![
            Chalk::stdout().dim(&format!(
                "Extensions are skipped. Run {app} update --extensions to update extensions."
            )),
            Chalk::stdout().green(&format!("{app} is already up to date (v{})", current_version())),
        ]
    );
    assert_eq!(effects.calls(), vec!["latest_release"]);
}

#[tokio::test]
async fn force_runs_the_self_update_even_when_current() {
    let root = temp_dir("force");
    let mut effects = FakeEffects::new(&root, &agent_dir("force"), bun_install_context(&root));
    effects.release = Ok(LatestPiRelease {
        version: current_version().into(),
        package_name: None,
        note: None,
    });

    let outcome = execute_package_command(&args(&["update", "--self", "--force"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");

    let app = pi_coding_agent::config::package_config().app_name.clone();
    assert_eq!(
        outcome.stdout,
        vec![
            Chalk::stdout().dim(&format!(
                "Updating {app} with bun install -g --ignore-scripts --minimum-release-age=0 @earendil-works/pi-coding-agent@{}...",
                current_version()
            )),
            Chalk::stdout().green(&format!("Updated {app} from {0} to {0}", current_version())),
        ]
    );
}

#[tokio::test]
async fn self_update_failure_prints_the_fallback_hint() {
    let root = temp_dir("self-update-fail");
    let mut effects = FakeEffects::new(&root, &agent_dir("self-update-fail"), bun_install_context(&root));
    effects.self_update = Err("bun install -g exited with code 1".to_owned().into());

    let outcome = execute_package_command(&args(&["update", "--self"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");

    let app = pi_coding_agent::config::package_config().app_name.clone();
    assert_eq!(outcome.exit_code, Some(1));
    assert_eq!(
        outcome.stderr,
        vec![
            Chalk::stderr().red("Error: bun install -g exited with code 1"),
            Chalk::stderr().dim(
                "If this keeps failing, run this command yourself: bun install -g --ignore-scripts --minimum-release-age=0 @earendil-works/pi-coding-agent@9.9.9"
            ),
        ]
    );
    assert_eq!(
        outcome.stdout,
        vec![Chalk::stdout().dim(&format!(
            "Updating {app} with bun install -g --ignore-scripts --minimum-release-age=0 @earendil-works/pi-coding-agent@9.9.9..."
        ))]
    );
}

#[tokio::test]
async fn bun_binary_install_reports_the_download_instruction() {
    let root = temp_dir("bun-binary");
    let effects = FakeEffects::new(&root, &agent_dir("bun-binary"), bun_binary_install_context(&root));

    let outcome = execute_package_command(&args(&["update", "--self"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");

    let app = pi_coding_agent::config::package_config().app_name.clone();
    assert_eq!(outcome.exit_code, Some(1));
    assert_eq!(
        outcome.stderr,
        vec![
            format!("error: {app} cannot self-update this installation."),
            "Download from: https://github.com/earendil-works/pi-mono/releases/latest".to_owned(),
        ]
    );
    assert!(effects.calls().iter().all(|call| call == "latest_release"));
}

#[tokio::test]
async fn release_lookup_failure_sets_exit_code() {
    let root = temp_dir("release-fail");
    let mut effects = FakeEffects::new(&root, &agent_dir("release-fail"), bun_install_context(&root));
    effects.release = Err("Could not determine latest pi version.".to_owned().into());

    let outcome = execute_package_command(&args(&["update", "--self"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");

    assert_eq!(
        outcome.stderr,
        vec![Chalk::stderr().red("Error: Could not determine latest pi version.")]
    );
    assert_eq!(outcome.exit_code, Some(1));
}

#[tokio::test]
async fn extension_updates_print_the_updated_line() {
    let root = temp_dir("extensions");
    let effects = FakeEffects::new(&root, &agent_dir("extensions"), bun_install_context(&root));

    let outcome = execute_package_command(&args(&["update", "--extension", "npm:x"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");

    assert_eq!(outcome.stdout, vec![Chalk::stdout().green("Updated npm:x")]);
    assert_eq!(effects.calls(), vec!["update:npm:x"]);
}

#[tokio::test]
async fn all_updates_extensions_then_self() {
    let root = temp_dir("all");
    let effects = FakeEffects::new(&root, &agent_dir("all"), bun_install_context(&root));

    let outcome = execute_package_command(&args(&["update", "--all"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");

    let app = pi_coding_agent::config::package_config().app_name.clone();
    assert_eq!(
        effects.calls(),
        vec![
            "update:<all>",
            "latest_release",
            "self_update:bun install -g --ignore-scripts --minimum-release-age=0 @earendil-works/pi-coding-agent@9.9.9",
        ]
    );
    assert_eq!(
        outcome.stdout,
        vec![
            Chalk::stdout().green("Updated packages"),
            Chalk::stdout().dim(&format!(
                "Updating {app} with bun install -g --ignore-scripts --minimum-release-age=0 @earendil-works/pi-coding-agent@9.9.9..."
            )),
            Chalk::stdout().green(&format!("Updated {app} from {} to 9.9.9", current_version())),
        ]
    );
}

#[tokio::test]
async fn help_and_validation_stay_synchronous() {
    let root = temp_dir("help");
    let effects = FakeEffects::new(&root, &agent_dir("help"), bun_install_context(&root));

    let outcome = execute_package_command(&args(&["update", "--bogus"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");
    assert_eq!(outcome.exit_code, Some(1));
    assert!(effects.calls().is_empty());

    let outcome = execute_package_command(&args(&["update", "--help"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");
    assert!(outcome.stdout[0].contains("Update pi, installed packages, or model catalogs."));

    assert!(
        execute_package_command(&args(&["other"]), &effects)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn saved_project_trust_is_used_for_updates() {
    let root = temp_dir("saved-trust");
    let project = root.join("project");
    fs::create_dir_all(project.join(".pi")).unwrap();
    fs::write(project.join(".pi/settings.json"), "{}").unwrap();
    let agent = agent_dir("saved-trust");
    let store = ProjectTrustStore::new(&JsString::from(&agent.display().to_string())).unwrap();
    store
        .set(
            &JsString::from(&project.display().to_string()),
            ProjectTrustDecision::Trusted,
        )
        .unwrap();
    let mut effects = FakeEffects::new(&project, &agent, bun_install_context(&root));
    effects.release = Ok(LatestPiRelease {
        version: current_version().into(),
        package_name: None,
        note: None,
    });

    let outcome = execute_package_command(&args(&["update", "--self"]), &effects)
        .await
        .unwrap()
        .expect("update is handled");

    // Trusted via the saved store, so the update proceeds to the release check.
    assert_eq!(effects.calls()[0], "latest_release");
    assert_eq!(outcome.exit_code, None);
}

#[tokio::test]
async fn extension_load_errors_become_trust_warnings() {
    let root = temp_dir("loader-error");
    let project = root.join("project");
    fs::create_dir_all(project.join(".pi")).unwrap();
    fs::write(project.join(".pi/settings.json"), "{}").unwrap();
    let agent = agent_dir("loader-error");
    let mut effects = FakeEffects::new(&project, &agent, bun_install_context(&root));
    effects.loader = Arc::new(ErroringExtensionsLoader {
        path: "/ext/broken.ts".to_owned(),
    });

    let outcome = execute_package_command(&args(&["list"]), &effects)
        .await
        .unwrap()
        .expect("list is handled");

    assert!(
        outcome
            .stderr
            .iter()
            .any(|line| line == &Chalk::stderr().yellow("Warning: Failed to load extension \"/ext/broken.ts\": boom")),
        "{:?}",
        outcome.stderr
    );
}

#[tokio::test]
async fn config_command_reports_the_requested_scope() {
    let root = temp_dir("config");
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    let effects = FakeEffects::new(&project, &agent_dir("config"), bun_install_context(&root));

    let outcome = execute_config_command(&args(&["config", "--approve"]), &effects)
        .await
        .unwrap()
        .expect("config is handled");
    assert_eq!(outcome.exit_code, Some(0));
    assert_eq!(
        *effects.config_requests.lock().unwrap(),
        vec![(ConfigWriteScope::Global, true)]
    );

    let outcome = execute_config_command(&args(&["config", "-l", "--approve"]), &effects)
        .await
        .unwrap()
        .expect("config is handled");
    assert_eq!(outcome.exit_code, Some(0));
    assert_eq!(
        *effects.config_requests.lock().unwrap(),
        vec![(ConfigWriteScope::Global, true), (ConfigWriteScope::Project, true)]
    );

    assert!(
        execute_config_command(&args(&["install", "x"]), &effects)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn config_local_requires_trust() {
    let root = temp_dir("config-trust");
    let project = root.join("project");
    fs::create_dir_all(project.join(".pi")).unwrap();
    fs::write(project.join(".pi/settings.json"), "{}").unwrap();
    let effects = FakeEffects::new(&project, &agent_dir("config-trust"), bun_install_context(&root));

    let outcome = execute_config_command(&args(&["config", "-l", "--no-approve"]), &effects)
        .await
        .unwrap()
        .expect("config is handled");

    assert_eq!(outcome.exit_code, Some(1));
    assert_eq!(
        outcome.stderr,
        vec![Chalk::stderr().red("Project is not trusted. Use --approve to modify local resource config.")]
    );
    assert!(effects.config_requests.lock().unwrap().is_empty());
}

#[test]
fn self_update_plan_decides_between_running_and_up_to_date() {
    let release = LatestPiRelease {
        version: "9.9.9".to_owned().into(),
        package_name: None,
        note: Some("note".to_owned().into()),
    };
    let (plan, line) = self_update_plan(false, &release, &JsString::from("0.0.0"));
    assert!(plan.should_run);
    assert_eq!(plan.install_spec, "@earendil-works/pi-coding-agent@9.9.9");
    assert_eq!(plan.note.as_ref(), Some(&JsString::from("note")));
    assert!(line.is_none());

    let (plan, line) = self_update_plan(false, &release, &JsString::from("9.9.9"));
    assert!(!plan.should_run);
    assert!(plan.note.is_none());
    assert_eq!(line, Some(Chalk::stdout().green("pi is already up to date (v9.9.9)")));

    let (plan, line) = self_update_plan(true, &release, &JsString::from("9.9.9"));
    assert!(plan.should_run);
    assert!(line.is_none());

    let renamed = LatestPiRelease {
        version: "1.0.0".to_owned().into(),
        package_name: Some("@other/pi".to_owned().into()),
        note: None,
    };
    let (plan, line) = self_update_plan(false, &renamed, &JsString::from("1.0.0"));
    assert!(plan.should_run);
    assert_eq!(plan.package_name, "@other/pi");
    assert_eq!(plan.install_spec, "@other/pi@1.0.0");
    assert!(line.is_none());
}

#[tokio::test]
async fn get_self_update_plan_reports_lookup_failures() {
    struct FailingFetch;

    impl pi_ai::request::Fetch for FailingFetch {
        fn fetch(
            &self,
            _request: pi_ai::request::HttpRequest,
            _signal: Option<tokio_util::sync::CancellationToken>,
        ) -> BoxFuture<'_, Result<pi_ai::request::HttpResponse, pi_ai::request::CallbackError>> {
            Box::pin(async { Err(pi_ai::request::CallbackError::new("offline")) })
        }
    }

    let error = get_self_update_plan(&FailingFetch, &JsString::from("0.0.0"), false)
        .await
        .expect_err("offline lookup fails");
    assert_eq!(error, "Could not determine latest pi version.");
}

#[test]
fn self_update_note_renders_markdown_between_blank_lines() {
    assert!(self_update_note_lines(&JsString::from("   "), Some(80)).is_empty());
    let lines = self_update_note_lines(&JsString::from("**bold** text"), Some(40));
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(lines[0], "");
    assert!(lines[1].contains("Update note"));
    assert!(lines[2].contains("bold"));
    assert_eq!(lines[3], "");
}

#[test]
fn self_update_helpers_render_the_expected_lines() {
    let command = SelfUpdateCommand {
        command: "bun".to_owned().into(),
        args: vec!["install".to_owned().into()],
        display: "bun install".to_owned().into(),
        steps: None,
    };
    assert_eq!(
        self_update_fallback_line(&command),
        Chalk::stderr().dim("If this keeps failing, run this command yourself: bun install")
    );
    let hints = pnpm_self_update_metadata_hint_lines();
    assert_eq!(hints.len(), 2);
    assert!(hints[0].contains("pnpm reports missing package versions"));
    assert!(hints[1].contains("pnpm store prune"));

    let root = temp_dir("unavailable");
    let context = bun_binary_install_context(&root);
    let target = pi_coding_agent::config::SelfUpdatePackageTarget::new("@earendil-works/pi-coding-agent");
    let lines = self_update_unavailable_lines(&context, None, &target, Some(&JsString::from("/usr/bin/pi"))).unwrap();
    assert_eq!(lines.len(), 4);
    assert!(lines[0].starts_with("error: "));
    assert!(lines[1].starts_with("Download from: "));
    assert_eq!(lines[2], "");
    assert_eq!(lines[3], "Location of pi executable: /usr/bin/pi");
}

#[tokio::test]
async fn run_self_update_reports_exit_codes_and_signals() {
    let success = SelfUpdateCommand {
        command: "sh".to_owned().into(),
        args: vec!["-c".to_owned().into(), "exit 0".to_owned().into()],
        display: "sh -c exit 0".to_owned().into(),
        steps: None,
    };
    assert_eq!(run_self_update(&success).await, Ok(()));

    let failure = SelfUpdateCommand {
        command: "sh".to_owned().into(),
        args: vec!["-c".to_owned().into(), "exit 3".to_owned().into()],
        display: "sh -c exit 3".to_owned().into(),
        steps: None,
    };
    assert_eq!(
        run_self_update(&failure).await,
        Err(JsString::from("sh -c exit 3 exited with code 3"))
    );

    let missing = SelfUpdateCommand {
        command: "/nonexistent/pi-self-update".to_owned().into(),
        args: Vec::new(),
        display: "missing".to_owned().into(),
        steps: None,
    };
    assert!(run_self_update(&missing).await.is_err());
}

#[test]
fn report_project_trust_warnings_are_yellow() {
    assert_eq!(
        report_project_trust_warnings_lines(&["a".to_owned().into()]),
        vec![Chalk::stderr().yellow("Warning: a")]
    );
}

#[test]
fn package_config_effects_struct_exposes_the_host_values() {
    let root = temp_dir("effects");
    let context = bun_install_context(&root);
    struct Selector;
    impl pi_coding_agent::package_manager_cli::ConfigSelector for Selector {
        fn select<'a>(
            &'a self,
            _request: SelectConfigRequest,
        ) -> pi_coding_agent::package_manager_cli::LocalBoxFuture<'a, Result<(), JsString>> {
            Box::pin(async { Ok(()) })
        }
    }
    let effects = DefaultPackageCommandEffects {
        cwd: root.display().to_string().into(),
        agent_dir: root.join("agent").display().to_string().into(),
        install_context: context.clone(),
        entrypoint: (context.entrypoint.clone()).map(JsString::from),
        extensions_loader: Arc::new(NoExtensionsLoader),
        config_selector: std::rc::Rc::new(Selector),
        fetch: pi_ai::http::default_fetch(),
        extension_factories: Vec::new(),
    };
    assert_eq!(effects.cwd(), root.display().to_string());
    assert_eq!(effects.agent_dir(), root.join("agent").display().to_string());
    assert_eq!(effects.stdout_columns(), None);
    assert_eq!(effects.entrypoint(), (context.entrypoint.clone()).map(JsString::from));
    assert_eq!(effects.install_context().package_dir, context.package_dir);
    assert_eq!(effects.install_context().exec_path, context.exec_path);
}

#[test]
fn model_catalog_refresh_outcome_maps_abort_and_provider_errors() {
    let aborted =
        pi_coding_agent::package_manager_cli::model_catalog_refresh_outcome(pi_ai::models::ModelsRefreshResult {
            aborted: true,
            errors: Default::default(),
        });
    assert_eq!(aborted, Err(JsString::from("Model catalog refresh timed out.")));

    let mut errors = indexmap::IndexMap::new();
    errors.insert("openai".to_owned(), pi_ai::request::CallbackError::new("boom"));
    errors.insert("anthropic".to_owned(), pi_ai::request::CallbackError::new("down"));
    let failed =
        pi_coding_agent::package_manager_cli::model_catalog_refresh_outcome(pi_ai::models::ModelsRefreshResult {
            aborted: false,
            errors,
        });
    assert_eq!(
        failed,
        Err(JsString::from(
            "Could not refresh model catalogs: openai: boom; anthropic: down"
        ))
    );

    assert_eq!(
        pi_coding_agent::package_manager_cli::model_catalog_refresh_outcome(
            pi_ai::models::ModelsRefreshResult::default()
        ),
        Ok(())
    );
}
