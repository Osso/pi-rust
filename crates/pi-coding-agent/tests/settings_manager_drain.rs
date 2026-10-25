//! Shared production-lock persistence boundary; all files are disposable sandbox fixtures.
use pi_js_compat::js_json::JsString;
use std::fs;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use pi_coding_agent::core::settings_manager::{
    FileSettingsStorage, PackageSource, SettingsFailure, SettingsManager, SettingsManagerCreateOptions, SettingsScope,
    SettingsStorage, SharedSettingsManager, lock_settings_manager,
};
use serde_json::Value;
use tempfile::TempDir;

const COMPLETION_TIMEOUT: Duration = Duration::from_secs(5);

struct ObservedStorage {
    files: FileSettingsStorage,
    writes: mpsc::Sender<(SettingsScope, Result<String, String>)>,
    finished: mpsc::Sender<()>,
    fail_on_write: bool,
}

impl SettingsStorage for ObservedStorage {
    fn with_lock(
        &mut self,
        scope: SettingsScope,
        apply: &mut dyn FnMut(Option<&str>) -> Option<String>,
    ) -> Result<(), SettingsFailure> {
        let mut output = None;
        let mut injected = false;
        let result = self.files.with_lock(scope, &mut |current| {
            let next = apply(current);
            if next.is_some() && std::mem::take(&mut self.fail_on_write) {
                injected = true;
                return None;
            }
            output = next.clone();
            next
        });
        if injected {
            return Err(SettingsFailure::new("injected settings write failure"));
        }
        let event = match (&result, output) {
            (Err(error), _) => Some(Err(error.to_string())),
            (Ok(()), Some(text)) => Some(Ok(text)),
            (Ok(()), None) => None,
        };
        if let Some(event) = event {
            self.writes.send((scope, event)).unwrap();
        }
        result
    }
}

impl Drop for ObservedStorage {
    fn drop(&mut self) {
        let _ = self.finished.send(());
    }
}

struct Fixture {
    _temp: TempDir,
    project: std::path::PathBuf,
    agent: std::path::PathBuf,
    writes: mpsc::Receiver<(SettingsScope, Result<String, String>)>,
    finished: mpsc::Receiver<()>,
}

impl Fixture {
    fn new() -> (Self, SharedSettingsManager) {
        Self::with_failing_write(false)
    }

    fn with_failing_write(fail_on_write: bool) -> (Self, SharedSettingsManager) {
        let temp = TempDir::new().unwrap();
        let project = temp.path().join("project");
        let agent = temp.path().join("agent");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&agent).unwrap();
        let (writes_tx, writes) = mpsc::channel();
        let (finished_tx, finished) = mpsc::channel();
        let files = FileSettingsStorage::new(
            &JsString::from(project.to_str().unwrap()),
            &JsString::from(agent.to_str().unwrap()),
        )
        .unwrap();
        let manager = SettingsManager::from_storage(
            Box::new(ObservedStorage {
                files,
                writes: writes_tx,
                finished: finished_tx,
                fail_on_write,
            }),
            SettingsManagerCreateOptions::default(),
        );
        (
            Self {
                _temp: temp,
                project,
                agent,
                writes,
                finished,
            },
            Arc::new(Mutex::new(manager)),
        )
    }

    fn write(&self) -> (SettingsScope, Value) {
        let (scope, result) = self
            .writes
            .recv_timeout(COMPLETION_TIMEOUT)
            .expect("automatic persistence completed");
        (scope, serde_json::from_str(&result.unwrap()).unwrap())
    }

    fn finish(&self, manager: SharedSettingsManager) {
        drop(manager);
        self.finished
            .recv_timeout(COMPLETION_TIMEOUT)
            .expect("scheduled owner released after draining");
    }

    fn reconstructed(&self) -> SettingsManager {
        SettingsManager::create(
            &JsString::from(self.project.to_str().unwrap()),
            Some(&JsString::from(self.agent.to_string_lossy().as_ref())),
            Default::default(),
        )
        .unwrap()
    }
}

#[test]
fn shared_setter_drains_after_guard_release_without_flush_or_runtime() {
    let (fixture, manager) = Fixture::new();
    let mut guard = lock_settings_manager(&manager);
    guard.set_default_model("persisted-model");
    assert!(
        fixture.writes.try_recv().is_err(),
        "write must not run inside setter guard"
    );
    drop(guard);
    // The scheduled task must retain ownership even after the initiating owner disappears.
    drop(manager);
    let (scope, snapshot) = fixture.write();
    assert_eq!(scope, SettingsScope::Global);
    assert_eq!(snapshot["defaultModel"], "persisted-model");
    fixture.finished.recv_timeout(COMPLETION_TIMEOUT).unwrap();
    let disk: Value = serde_json::from_str(&fs::read_to_string(fixture.agent.join("settings.json")).unwrap()).unwrap();
    assert_eq!(disk["defaultModel"], "persisted-model");
    assert_eq!(
        fixture.reconstructed().get_default_model(),
        Some(&JsString::from("persisted-model"))
    );
}

#[test]
fn rapid_shared_snapshots_preserve_fifo_across_scopes() {
    let (fixture, manager) = Fixture::new();
    {
        let mut guard = lock_settings_manager(&manager);
        guard.set_default_model("first");
        guard
            .set_project_packages(vec![PackageSource::Name("npm:project".into())])
            .unwrap();
        guard.set_default_model("second");
    }
    let first = fixture.write();
    let project = fixture.write();
    let second = fixture.write();
    assert_eq!(first.0, SettingsScope::Global);
    assert_eq!(first.1["defaultModel"], "first");
    assert_eq!(project.0, SettingsScope::Project);
    assert_eq!(project.1["packages"][0], "npm:project");
    assert_eq!(second.0, SettingsScope::Global);
    assert_eq!(second.1["defaultModel"], "second");
    // A completed burst must not leave scheduling latched and strand the next mutation.
    lock_settings_manager(&manager).set_default_model("third");
    assert_eq!(fixture.write().1["defaultModel"], "third");
    fixture.finish(manager);
    assert!(fixture.writes.try_recv().is_err());
    assert_eq!(
        fixture.reconstructed().get_default_model(),
        Some(&JsString::from("third"))
    );
}

#[test]
fn automatic_write_failure_is_recorded_and_later_scope_still_drains() {
    let (fixture, manager) = Fixture::new();
    // A directory cannot be read/written as settings JSON; fail real file storage after loading.
    fs::create_dir(fixture.agent.join("settings.json")).unwrap();
    {
        let mut guard = lock_settings_manager(&manager);
        guard.set_default_model("cannot-write");
        guard
            .set_project_packages(vec![PackageSource::Name("npm:survives".into())])
            .unwrap();
    }
    let (scope, error) = fixture.writes.recv_timeout(COMPLETION_TIMEOUT).unwrap();
    assert_eq!(scope, SettingsScope::Global);
    assert!(error.is_err());
    assert_eq!(fixture.write().1["packages"][0], "npm:survives");
    let errors = lock_settings_manager(&manager).drain_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].scope, SettingsScope::Global);
    assert!(!errors[0].message.is_empty());
    assert!(lock_settings_manager(&manager).drain_errors().is_empty());
    fixture.finish(manager);
}

#[test]
fn failing_write_is_recorded_without_dropping_later_snapshots_or_latching_drain() {
    let (fixture, manager) = Fixture::with_failing_write(true);
    {
        let mut guard = lock_settings_manager(&manager);
        guard.set_default_model("panics");
        guard
            .set_project_packages(vec![PackageSource::Name("npm:survives-panic".into())])
            .unwrap();
        guard.set_default_model("second");
    }
    let project = fixture.write();
    assert_eq!(project.0, SettingsScope::Project);
    assert_eq!(project.1["packages"][0], "npm:survives-panic");
    assert_eq!(fixture.write().1["defaultModel"], "second");
    let errors = lock_settings_manager(&manager).drain_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].scope, SettingsScope::Global);
    assert!(errors[0].message.contains("injected settings write failure"));
    assert!(lock_settings_manager(&manager).drain_errors().is_empty());
    lock_settings_manager(&manager).set_default_model("third");
    assert_eq!(fixture.write().1["defaultModel"], "third");
    fixture.finish(manager);
    assert!(fixture.writes.try_recv().is_err());
    let restored = fixture.reconstructed();
    assert_eq!(restored.get_default_model(), Some(&JsString::from("third")));
    assert_eq!(
        restored.get_project_settings()["packages"][0],
        pi_js_compat::js_json::JsValue::String("npm:survives-panic".into())
    );
}

#[test]
fn synchronous_flush_barrier_does_not_duplicate_scheduled_writes() {
    let (fixture, manager) = Fixture::new();
    lock_settings_manager(&manager).set_default_model("first");
    {
        let mut guard = lock_settings_manager(&manager);
        guard.set_default_model("second");
        guard.flush();
        let disk: Value =
            serde_json::from_str(&fs::read_to_string(fixture.agent.join("settings.json")).unwrap()).unwrap();
        assert_eq!(disk["defaultModel"], "second");
    }
    fixture.finish(manager);
    assert_eq!(fixture.write().1["defaultModel"], "first");
    assert_eq!(fixture.write().1["defaultModel"], "second");
    assert!(
        fixture.writes.try_recv().is_err(),
        "scheduled task must not replay flushed snapshots"
    );
}

#[test]
fn automatic_drain_rechecks_project_trust_before_write() {
    let (fixture, manager) = Fixture::new();
    {
        let mut guard = lock_settings_manager(&manager);
        guard
            .set_project_packages(vec![PackageSource::Name("npm:blocked".into())])
            .unwrap();
        guard.set_project_trusted(false);
        guard.set_default_model("global-still-writes");
    }
    assert_eq!(fixture.write().1["defaultModel"], "global-still-writes");
    let errors = lock_settings_manager(&manager).drain_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].scope, SettingsScope::Project);
    fixture.finish(manager);
    assert!(!fixture.project.join(".pi/settings.json").exists());
    assert!(fixture.writes.try_recv().is_err());
}
