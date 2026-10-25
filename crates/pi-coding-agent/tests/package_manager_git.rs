//! Git install/update behavior at the real git boundary.

#[path = "support/env_probe.rs"]
mod env_probe;
use crate::shared_executable_script as executable_script;

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::package_manager::{
    DefaultPackageManager, PackageSettingsSnapshot, PackageSettingsStore, PackageSource,
};
use tempfile::TempDir;

#[derive(Default)]
struct FakeSettings {
    global: Mutex<PackageSettingsSnapshot>,
    trusted: bool,
    npm_command: Option<Vec<JsString>>,
}

impl PackageSettingsStore for FakeSettings {
    fn global_settings(&self) -> PackageSettingsSnapshot {
        self.global.lock().unwrap().clone()
    }

    fn project_settings(&self) -> PackageSettingsSnapshot {
        PackageSettingsSnapshot::default()
    }

    fn set_packages(&self, packages: Vec<PackageSource>) {
        self.global.lock().unwrap().packages = packages;
    }

    fn set_project_packages(&self, _packages: Vec<PackageSource>) {}

    fn is_project_trusted(&self) -> bool {
        self.trusted
    }

    fn get_npm_command(&self) -> Option<Vec<JsString>> {
        self.npm_command.clone()
    }
}

fn git(args: &[&str], cwd: &Path) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("git must run");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git(&["init", "--initial-branch=main"], dir);
    git(&["config", "user.email", "test@example.com"], dir);
    git(&["config", "user.name", "Test"], dir);
}

fn commit(dir: &Path, file: &str, contents: &str, message: &str) {
    fs::write(dir.join(file), contents).unwrap();
    git(&["add", file], dir);
    git(&["commit", "-m", message], dir);
}

struct Fixture {
    temp: TempDir,
    agent_dir: PathBuf,
    cwd: PathBuf,
    remote: PathBuf,
    installed: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        Self::new_in(temp)
    }

    fn new_in(temp: TempDir) -> Self {
        let agent_dir = temp.path().join("agent");
        let cwd = temp.path().join("project");
        let remote = temp.path().join("remote");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&cwd).unwrap();
        init_repo(&remote);
        commit(&remote, "ext.ts", "v1", "v1");

        let installed = agent_dir.join("git/github.com/test/extension");
        fs::create_dir_all(installed.parent().unwrap()).unwrap();
        git(
            &["clone", remote.to_str().unwrap(), installed.to_str().unwrap()],
            temp.path(),
        );
        git(&["config", "user.email", "test@example.com"], &installed);
        git(&["config", "user.name", "Test"], &installed);
        Self {
            temp,
            agent_dir,
            cwd,
            remote,
            installed,
        }
    }

    fn manager(&self, package: &str) -> DefaultPackageManager {
        self.manager_with(package, None)
    }

    fn manager_with(&self, package: &str, npm_command: Option<Vec<JsString>>) -> DefaultPackageManager {
        DefaultPackageManager::new(
            &JsString::from(self.cwd.to_str().unwrap()),
            &JsString::from(self.agent_dir.to_str().unwrap()),
            Arc::new(FakeSettings {
                global: Mutex::new(PackageSettingsSnapshot {
                    packages: vec![PackageSource::Name(package.to_owned().into())],
                    ..PackageSettingsSnapshot::default()
                }),
                trusted: false,
                npm_command,
            }),
        )
        .unwrap()
    }

    fn write_fake_npm(&self, body: &str) -> String {
        let dir = self.temp.path().join("bin");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("npm");
        executable_script::write_executable_script(&path, body);
        path.to_string_lossy().into_owned()
    }
}

fn block<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

const SOURCE: &str = "git:github.com/test/extension";

#[test]
fn up_to_date_update_keeps_untracked_files_and_skips_reset() {
    let fixture = Fixture::new();
    fs::write(fixture.installed.join("scratch.txt"), "keep").unwrap();

    block(fixture.manager(SOURCE).update(None)).unwrap();

    assert!(fixture.installed.join("scratch.txt").exists());
    assert_eq!(fs::read_to_string(fixture.installed.join("ext.ts")).unwrap(), "v1");
}

#[test]
fn update_resets_to_the_remote_head_and_cleans_untracked_files() {
    let fixture = Fixture::new();
    fs::write(fixture.installed.join("scratch.txt"), "drop").unwrap();
    commit(&fixture.remote, "ext.ts", "v2", "v2");

    block(fixture.manager(SOURCE).update(None)).unwrap();

    assert_eq!(fs::read_to_string(fixture.installed.join("ext.ts")).unwrap(), "v2");
    assert!(!fixture.installed.join("scratch.txt").exists());
    assert!(
        !fixture
            .installed
            .parent()
            .unwrap()
            .join(".extension.pi-update-incomplete")
            .exists()
    );
}

#[test]
fn pinned_source_checks_out_the_configured_ref() {
    let fixture = Fixture::new();
    commit(&fixture.remote, "ext.ts", "v2", "v2");
    git(&["tag", "v1", "HEAD~1"], &fixture.remote);

    block(fixture.manager("git:github.com/test/extension@v1").update(None)).unwrap();

    assert_eq!(fs::read_to_string(fixture.installed.join("ext.ts")).unwrap(), "v1");
}

#[test]
fn missing_git_dependencies_run_npm_install() {
    let fixture = Fixture::new();
    fs::write(
        fixture.installed.join("package.json"),
        r#"{"name":"ext","dependencies":{"left-pad":"1.0.0"}}"#,
    )
    .unwrap();
    let log = fixture.temp.path().join("npm.log");
    let npm = fixture.write_fake_npm(&format!(
        "#!/bin/sh\nprintf '%s|%s\\n' \"$PWD\" \"$*\" >> \"{}\"\n",
        log.display()
    ));

    block(
        fixture
            .manager_with(SOURCE, Some(vec![JsString::from(&npm)]))
            .update(None),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(&log).unwrap().trim(),
        format!("{}|install", fixture.installed.to_string_lossy())
    );
}

#[test]
fn git_dependencies_install_without_dev_dependencies_by_default() {
    if !env_probe::in_probe() {
        let root = TempDir::new().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let npm = bin.join("npm");
        executable_script::write_executable_script(
            &npm,
            "#!/bin/sh\nprintf '%s|%s\\n' \"$PWD\" \"$*\" >> \"$PACKAGE_TEST_ROOT/npm.log\"\n",
        );
        let path = format!(
            "{}:{}",
            bin.to_string_lossy(),
            std::env::var("PATH").unwrap_or_default()
        );
        env_probe::run_probe(
            "git_dependencies_install_without_dev_dependencies_by_default_probe",
            &[
                ("PACKAGE_TEST_ROOT", root.path().as_os_str().to_owned()),
                ("PATH", path.into()),
            ],
        );
    }
}

#[test]
#[ignore]
fn git_dependencies_install_without_dev_dependencies_by_default_probe() {
    if !env_probe::in_probe() {
        return;
    }
    let root = PathBuf::from(std::env::var("PACKAGE_TEST_ROOT").unwrap());
    fs::create_dir_all(root.join("fixture")).unwrap();
    let fixture = Fixture::new_in(TempDir::new_in(root.join("fixture")).unwrap());
    fs::write(
        fixture.installed.join("package.json"),
        r#"{"name":"ext","dependencies":{"left-pad":"1.0.0"}}"#,
    )
    .unwrap();
    block(fixture.manager(SOURCE).update(None)).unwrap();
    let log = fs::read_to_string(root.join("npm.log")).unwrap();
    assert_eq!(
        log.trim(),
        format!("{}|install --omit=dev", fixture.installed.to_string_lossy())
    );
}

#[test]
fn remove_deletes_the_checkout_and_prunes_empty_parents() {
    let fixture = Fixture::new();
    block(fixture.manager(SOURCE).remove(&JsString::from(SOURCE), false)).unwrap();

    assert!(!fixture.installed.exists());
    assert!(!fixture.agent_dir.join("git/github.com").exists());
    assert!(fixture.agent_dir.join("git").exists());
}

#[test]
fn offline_mode_skips_missing_package_installs() {
    if !env_probe::in_probe() {
        env_probe::run_probe(
            "offline_mode_skips_missing_package_installs_probe",
            &[("PI_OFFLINE", "1".into())],
        );
        return;
    }
    let fixture = Fixture::new();
    let manager = fixture.manager("npm:missing-package");
    let resolved = block(manager.resolve(None)).unwrap();
    assert!(resolved.extensions.is_empty());
}

#[test]
#[ignore]
fn offline_mode_skips_missing_package_installs_probe() {
    if !env_probe::in_probe() {
        return;
    }
    let fixture = Fixture::new();
    let manager = fixture.manager("npm:missing-package");
    let resolved = block(manager.resolve(None)).unwrap();
    assert!(resolved.extensions.is_empty());
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
