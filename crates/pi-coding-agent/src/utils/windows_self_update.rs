//! Port of `packages/coding-agent/src/utils/windows-self-update.ts`.
//!
//! The TypeScript module enumerates native addons from `process.report.getReport().sharedObjects`.
//! A native Rust binary loads no addons, so the enumeration is injected; the default implementation
//! reads `/proc/self/maps` on Linux and returns nothing elsewhere. Quarantine run directories use a
//! UUID (v7 instead of `randomUUID`'s v4); the name is only used for uniqueness.

use std::path::{Path, PathBuf};

const QUARANTINE_DIR_NAME: &str = ".pi-native-quarantine";

fn resolve(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// `getQuarantineRoot`: the nearest `node_modules` directory.
pub fn get_quarantine_root(package_dir: &Path) -> Option<PathBuf> {
    let mut current = resolve(package_dir);
    loop {
        if current
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("node_modules"))
        {
            return Some(current.join(QUARANTINE_DIR_NAME));
        }
        let parent = current.parent().map(Path::to_path_buf)?;
        if parent == current {
            return None;
        }
        current = parent;
    }
}

/// Shared objects loaded from the package directory.
pub fn loaded_shared_objects_in_package_dir(package_dir: &Path, shared_objects: &[String]) -> Vec<PathBuf> {
    let root = resolve(package_dir);
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut loaded = Vec::new();
    for value in shared_objects {
        let file_path = resolve(Path::new(value));
        if !file_path.starts_with(&root) || seen.contains(&file_path) {
            continue;
        }
        seen.push(file_path.clone());
        loaded.push(file_path);
    }
    loaded
}

/// Default shared-object source: `/proc/self/maps` on Linux, nothing elsewhere.
pub fn process_shared_objects() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        let Ok(maps) = std::fs::read_to_string("/proc/self/maps") else {
            return Vec::new();
        };
        let mut paths: Vec<String> = maps
            .lines()
            .filter_map(|line| line.split_whitespace().nth(5))
            .filter(|path| path.starts_with('/'))
            .map(str::to_owned)
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

/// `cleanupWindowsSelfUpdateQuarantine`: best-effort removal of a previous quarantine.
pub fn cleanup_windows_self_update_quarantine(package_dir: &Path) {
    let Some(root) = get_quarantine_root(package_dir) else {
        return;
    };
    let _ = std::fs::remove_dir_all(root);
}

/// `quarantineWindowsNativeDependencies`: move loaded native files aside, then restore copies so the
/// running process keeps working.
pub fn quarantine_windows_native_dependencies(package_dir: &Path, shared_objects: &[String]) {
    let resolved_package_dir = resolve(package_dir);
    let Some(quarantine_root) = get_quarantine_root(&resolved_package_dir) else {
        return;
    };
    let loaded_files = loaded_shared_objects_in_package_dir(&resolved_package_dir, shared_objects);
    if loaded_files.is_empty() {
        return;
    }
    let unique = format!(
        "{}-{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis()),
        std::process::id(),
        pi_ai::utils::uuid::uuidv7()
    );
    let quarantine_run_dir = quarantine_root.join(unique);
    for loaded_file in loaded_files {
        if !loaded_file.exists() {
            continue;
        }
        let Ok(relative) = loaded_file.strip_prefix(&resolved_package_dir) else {
            continue;
        };
        let quarantine_path = quarantine_run_dir.join(relative);
        if let Some(parent) = quarantine_path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            continue;
        }
        if std::fs::rename(&loaded_file, &quarantine_path).is_err() {
            continue;
        }
        let _ = std::fs::copy(&quarantine_path, &loaded_file);
    }
}
