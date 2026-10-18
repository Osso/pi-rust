use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Tracks clipboard temporary files and removes them after they are submitted
/// in text or when the tracker is cleared.
#[derive(Debug, Default)]
pub struct ClipboardTempFileTracker {
    tracked_files: HashSet<PathBuf>,
}

impl ClipboardTempFileTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn track(&mut self, file_path: PathBuf) {
        self.tracked_files.insert(file_path);
    }

    /// Remove tracked files whose path appears in `text`.
    pub fn cleanup_referenced_in(&mut self, text: &str) {
        let referenced = self
            .tracked_files
            .iter()
            .filter(|file_path| text.contains(file_path.to_string_lossy().as_ref()))
            .cloned()
            .collect::<Vec<_>>();

        for file_path in referenced {
            remove_file(&file_path);
            self.tracked_files.remove(&file_path);
        }
    }

    pub fn cleanup_all(&mut self) {
        for file_path in std::mem::take(&mut self.tracked_files) {
            remove_file(&file_path);
        }
    }
}

fn remove_file(file_path: &Path) {
    let _ = std::fs::remove_file(file_path);
}
