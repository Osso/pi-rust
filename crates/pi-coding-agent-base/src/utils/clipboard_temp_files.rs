use indexmap::IndexSet;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

/// Tracks clipboard temporary files and removes them after they are submitted
/// in text or when the tracker is cleared.
#[derive(Debug, Default)]
pub struct ClipboardTempFileTracker {
    /// `Set<string>`: insertion-ordered JS path strings.
    tracked_files: IndexSet<JsString>,
}

impl ClipboardTempFileTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn track(&mut self, file_path: JsString) {
        self.tracked_files.insert(file_path);
    }

    /// Remove tracked files whose path appears in `text` (`text.includes(filePath)`).
    pub fn cleanup_referenced_in(&mut self, text: &JsString) {
        let referenced = self
            .tracked_files
            .iter()
            .filter(|file_path| text.contains(*file_path))
            .cloned()
            .collect::<Vec<_>>();

        for file_path in referenced {
            remove_file(&file_path);
            self.tracked_files.shift_remove(&file_path);
        }
    }

    pub fn cleanup_all(&mut self) {
        for file_path in std::mem::take(&mut self.tracked_files) {
            remove_file(&file_path);
        }
    }
}

/// Temp cleanup is best-effort: the file may already be gone.
fn remove_file(file_path: &JsString) {
    let _ = std::fs::remove_file(encode_node_utf8(file_path));
}
