#[path = "support/model_selector_modifier_rejection.rs"]
mod modifier_rejection;
use crate::shared_oracle as oracle;

use pi_coding_agent::{
    core::{auth_storage::AuthStorage, model_registry::ModelRegistry},
    modes::interactive::{
        components::model_selector::{ModelSelectorComponent, ModelSelectorUi},
        theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance},
    },
};
use pi_tui::Component;
use serde_json::{Value, json};
use std::{cell::Cell, process::Command, rc::Rc, sync::Arc};

#[test]
fn vertex_homedir_throw_reaches_model_selector_catch() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("passwd-failure.so");
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/passwd_failure.c");
    let output = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(&library)
        .arg(source)
        .arg("-ldl")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let path = directory.path().join("models.json");
    std::fs::write(&path, "{}").unwrap();
    let mut source = oracle::TsOracle::spawn_with_environment(
        "model-selector-nss-oracle.mts",
        "packages/coding-agent/src",
        &[("LD_PRELOAD", library.as_os_str().to_owned())],
    );
    let expected: Value = source.request(&json!({"path":path,"colors":REQUIRED_COLORS}));
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "model_selector_nss_child", "--nocapture"])
        .env_remove("HOME")
        .env_remove("GOOGLE_APPLICATION_CREDENTIALS")
        .env_remove("GOOGLE_CLOUD_API_KEY")
        .env("LD_PRELOAD", &library)
        .env("PI_TEST_FAIL_PASSWD", "1")
        .env("PI_TEST_MODELS_PATH", &path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let actual: Value =
        serde_json::from_str(stdout.lines().find_map(|line| line.strip_prefix("RESULT:")).unwrap()).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual["renders"], 1);
    assert!(
        actual["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row.as_str().unwrap().contains("uv_os_homedir returned EIO"))
    );
}

struct Ui(Cell<usize>);
impl ModelSelectorUi for Ui {
    fn request_render(&self) {
        self.0.set(self.0.get() + 1);
    }
}
#[tokio::test(flavor = "current_thread")]
#[ignore = "isolated HOME/passwd child"]
async fn model_selector_nss_child() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let colors: Vec<_> = REQUIRED_COLORS
                .iter()
                .map(|key| ((*key).into(), ColorValue::Index(0.0)))
                .collect();
            set_theme_instance(Arc::new(
                Theme::new(&colors, &[], ColorMode::Color256, None, None).unwrap(),
            ));
            let registry = ModelRegistry::in_memory(AuthStorage::in_memory([] as [(&str, Value); 0])).unwrap();
            let ui = Rc::new(Ui(Cell::new(0)));
            let selector = ModelSelectorComponent::new(
                ui.clone(),
                None,
                registry,
                vec![],
                Rc::new(|_| {}),
                Rc::new(|| {}),
                None,
            );
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            println!(
                "RESULT:{}",
                json!({"renders":ui.0.get(),"rows":selector.borrow_mut().render(160)})
            );
        })
        .await;
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
