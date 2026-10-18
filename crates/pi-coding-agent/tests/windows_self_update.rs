//! Tests for `utils/windows-self-update.ts` with injected shared-object lists.

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use pi_coding_agent::utils::windows_self_update::{
    cleanup_windows_self_update_quarantine, get_quarantine_root, loaded_shared_objects_in_package_dir,
    process_shared_objects, quarantine_windows_native_dependencies,
};

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-winself-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn quarantine_root_is_the_nearest_node_modules_directory() {
    let root = temp_dir("root");
    let node_modules = root.join("node_modules");
    let package = node_modules.join("@scope/pkg");
    fs::create_dir_all(&package).unwrap();
    assert_eq!(
        get_quarantine_root(&package),
        Some(node_modules.join(".pi-native-quarantine"))
    );
    assert_eq!(get_quarantine_root(&root), None);

    let nested = node_modules.join("@scope/pkg/deep/dir");
    fs::create_dir_all(&nested).unwrap();
    assert_eq!(
        get_quarantine_root(&nested),
        Some(node_modules.join(".pi-native-quarantine"))
    );
}

#[test]
fn loaded_shared_objects_are_filtered_and_deduplicated() {
    let root = temp_dir("shared");
    let package = root.join("node_modules/@scope/pkg");
    fs::create_dir_all(&package).unwrap();
    let inside = package.join("native.node");
    let outside = root.join("elsewhere.node");
    fs::write(&inside, "native").unwrap();
    fs::write(&outside, "other").unwrap();

    let loaded = loaded_shared_objects_in_package_dir(
        &package,
        &[
            inside.display().to_string(),
            inside.display().to_string(),
            outside.display().to_string(),
        ],
    );
    assert_eq!(loaded, [inside]);
}

#[test]
fn loaded_shared_objects_compare_paths_case_insensitively() {
    let root = temp_dir("case");
    let package = root.join("node_modules/pkg");
    fs::create_dir_all(&package).unwrap();
    let upper = root.join("node_modules/PKG/Native.node");
    let lower = root.join("node_modules/pkg/native.node");

    let loaded =
        loaded_shared_objects_in_package_dir(&package, &[upper.display().to_string(), lower.display().to_string()]);

    // The source lowercases both sides; the first spelling of a path is kept.
    assert_eq!(loaded, [upper]);
}

#[test]
fn quarantine_reports_filesystem_failures() {
    let root = temp_dir("failure");
    let node_modules = root.join("node_modules");
    let package = node_modules.join("pkg");
    fs::create_dir_all(&package).unwrap();
    let native = package.join("addon.node");
    fs::write(&native, "native").unwrap();
    // A file where the quarantine directory belongs makes `mkdirSync` throw.
    fs::write(node_modules.join(".pi-native-quarantine"), "blocker").unwrap();

    let result = quarantine_windows_native_dependencies(&package, &[native.display().to_string()]);

    assert!(result.is_err(), "mkdir failure must propagate like the source's throw");
    assert_eq!(fs::read_to_string(&native).unwrap(), "native");
}

#[test]
fn quarantine_moves_native_files_and_restores_a_copy() {
    let root = temp_dir("quarantine");
    let node_modules = root.join("node_modules");
    let package = node_modules.join("@scope/pkg");
    let native_dir = package.join("build/Release");
    fs::create_dir_all(&native_dir).unwrap();
    let native = native_dir.join("addon.node");
    fs::write(&native, "native-binary").unwrap();
    let unrelated = root.join("unrelated.node");
    fs::write(&unrelated, "other").unwrap();

    quarantine_windows_native_dependencies(
        &package,
        &[native.display().to_string(), unrelated.display().to_string()],
    )
    .unwrap();

    // The running process keeps a copy at the original path.
    assert_eq!(fs::read_to_string(&native).unwrap(), "native-binary");
    let quarantine_root = node_modules.join(".pi-native-quarantine");
    let runs: Vec<PathBuf> = fs::read_dir(&quarantine_root)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    assert_eq!(runs.len(), 1);
    let moved = runs[0].join("build/Release/addon.node");
    assert_eq!(fs::read_to_string(&moved).unwrap(), "native-binary");
    assert!(!runs[0].join("unrelated.node").exists(), "outside files are ignored");
}

#[test]
fn cleanup_removes_the_quarantine_root() {
    let root = temp_dir("cleanup");
    let node_modules = root.join("node_modules");
    let package = node_modules.join("pkg");
    fs::create_dir_all(&package).unwrap();
    let quarantine = node_modules.join(".pi-native-quarantine/run/file.node");
    fs::create_dir_all(quarantine.parent().unwrap()).unwrap();
    fs::write(&quarantine, "x").unwrap();

    cleanup_windows_self_update_quarantine(&package);
    assert!(!node_modules.join(".pi-native-quarantine").exists());
}

#[test]
fn process_shared_objects_are_absolute_paths() {
    let shared = process_shared_objects();
    #[cfg(target_os = "linux")]
    assert!(!shared.is_empty(), "the process maps shared objects");
    assert!(shared.iter().all(|path| path.starts_with('/')));
}

#[test]
fn quarantine_without_loaded_files_is_a_noop() {
    let root = temp_dir("noop");
    let node_modules = root.join("node_modules");
    let package = node_modules.join("pkg");
    fs::create_dir_all(&package).unwrap();
    quarantine_windows_native_dependencies(&package, &[]).unwrap();
    assert!(!node_modules.join(".pi-native-quarantine").exists());
}
