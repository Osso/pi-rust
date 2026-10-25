//! Fixture package directory and child-process runner for HTML export tests.

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::env_probe;

pub(crate) fn fixture_package_dir() -> TempDir {
    let directory = tempfile::tempdir().expect("package fixture dir");
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    fs::create_dir(directory.path().join("theme")).expect("theme dir");
    let ts_theme_dir = directory.path().join("src/modes/interactive/theme");
    fs::create_dir_all(&ts_theme_dir).expect("ts theme dir");
    for name in ["dark.json", "light.json"] {
        fs::copy(
            manifest.join("theme").join(name),
            directory.path().join("theme").join(name),
        )
        .expect("copy built-in theme");
        fs::copy(manifest.join("theme").join(name), ts_theme_dir.join(name)).expect("copy ts built-in theme");
    }
    let templates = manifest.join("../pi-coding-agent-tools/src/core/export_html/templates");
    let ts_export_dir = directory.path().join("src/core/export-html");
    let native_export_dir = directory.path().join("export-html");
    fs::create_dir_all(ts_export_dir.join("vendor")).expect("ts export dir");
    fs::create_dir_all(native_export_dir.join("vendor")).expect("native export dir");
    for name in ["template.html", "template.css", "template.js"] {
        fs::copy(templates.join(name), ts_export_dir.join(name)).expect("copy export template");
        fs::copy(templates.join(name), native_export_dir.join(name)).expect("copy native export template");
    }
    for name in ["marked.min.js", "highlight.min.js"] {
        fs::copy(
            templates.join("vendor").join(name),
            ts_export_dir.join("vendor").join(name),
        )
        .expect("copy export vendor asset");
        fs::copy(
            templates.join("vendor").join(name),
            native_export_dir.join("vendor").join(name),
        )
        .expect("copy native vendor asset");
    }
    directory
}

pub(crate) fn run_export_probe(test_name: &str, package_dir: &Path) -> std::process::Output {
    let agent_dir = tempfile::tempdir().expect("agent dir").keep();
    let environment = [
        ("PI_PACKAGE_DIR", package_dir.as_os_str().to_owned()),
        ("PI_CODING_AGENT_DIR", agent_dir.as_os_str().to_owned()),
    ];
    let environment: Vec<(&str, OsString)> = environment.iter().map(|(name, value)| (*name, value.clone())).collect();
    env_probe::run_probe(test_name, &environment)
}
