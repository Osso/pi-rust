//! Port of `packages/coding-agent/src/utils/windows-self-update.ts`.
//!
//! The TypeScript module enumerates native addons from `process.report.getReport().sharedObjects`.
//! A native Rust binary loads no addons, so the enumeration is injected; the default implementation
//! reads `/proc/self/maps` on Linux and returns nothing elsewhere.

use std::path::{Path, PathBuf};

use crate::utils::paths::get_cwd_relative_path;

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
            .is_some_and(|name| name.to_string_lossy().to_lowercase() == "node_modules")
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

/// Shared objects loaded from the package directory, compared case-insensitively like the source.
pub fn loaded_shared_objects_in_package_dir(package_dir: &Path, shared_objects: &[String]) -> Vec<PathBuf> {
    let root = resolve(package_dir).to_string_lossy().to_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut loaded = Vec::new();
    for value in shared_objects {
        let file_path = resolve(Path::new(value));
        let comparison_path = file_path.to_string_lossy().to_lowercase();
        let inside = matches!(get_cwd_relative_path(&comparison_path, &root), Ok(Some(_)));
        if !inside || !seen.insert(comparison_path) {
            continue;
        }
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
/// running process keeps working. Filesystem failures propagate like the source's synchronous throws.
pub fn quarantine_windows_native_dependencies(package_dir: &Path, shared_objects: &[String]) -> std::io::Result<()> {
    let resolved_package_dir = resolve(package_dir);
    let Some(quarantine_root) = get_quarantine_root(&resolved_package_dir) else {
        return Ok(());
    };
    let loaded_files = loaded_shared_objects_in_package_dir(&resolved_package_dir, shared_objects);
    if loaded_files.is_empty() {
        return Ok(());
    }
    let unique = format!(
        "{}-{}-{}",
        pi_ai::request::now_millis(),
        std::process::id(),
        pi_ai::utils::uuid::random_uuid()
    );
    let quarantine_run_dir = quarantine_root.join(unique);
    for loaded_file in loaded_files {
        if !loaded_file.exists() {
            continue;
        }
        let quarantine_path = quarantine_run_dir.join(relative_to(&resolved_package_dir, &loaded_file));
        if let Some(parent) = quarantine_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&loaded_file, &quarantine_path)?;
        std::fs::copy(&quarantine_path, &loaded_file)?;
    }
    Ok(())
}

/// `path.relative(base, path)` for a path the case-insensitive containment check placed under `base`.
fn relative_to(base: &Path, path: &Path) -> PathBuf {
    let base_depth = base.components().count();
    path.components().skip(base_depth).collect()
}
