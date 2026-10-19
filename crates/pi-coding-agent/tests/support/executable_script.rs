//! Executable test-script creation without `ETXTBSY` spawn races.
//!
//! Opening a script for writing in the test process and then exec'ing it from
//! another test thread races with every concurrent `fork`: the forked child
//! inherits the write descriptor until its own `exec`, and `execve` returns
//! `ETXTBSY` while any process still holds the file open for writing. Writing
//! through a short-lived `sh` child keeps the write descriptor out of the test
//! process, so no sibling fork can inherit it.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Write `contents` to `path` and mark it executable (mode `0o755`).
pub fn write_executable_script(path: &Path, contents: &str) {
    let mut writer = Command::new("sh")
        .arg("-c")
        .arg("cat > \"$1\" && chmod 755 \"$1\"")
        .arg("sh")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("script writer must start");
    writer
        .stdin
        .take()
        .expect("script writer stdin must be piped")
        .write_all(contents.as_bytes())
        .expect("script contents must be writable");
    let status = writer.wait().expect("script writer must finish");
    assert!(status.success(), "script writer failed for {}", path.display());
}
