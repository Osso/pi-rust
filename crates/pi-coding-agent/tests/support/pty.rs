//! Minimal pty harness for process-boundary crash tests.
//!
//! `main_interactive.rs` runs the binary through `script(1)`, which makes the
//! pty the child's controlling terminal: closing the master delivers `SIGHUP`.
//! The crash tests need a pty *without* a controlling terminal so that closing
//! the master only makes later terminal I/O fail (`EIO`/`EPIPE`) without
//! signalling the child. `openpty` gives the child a tty on stdin/stdout/stderr
//! while it stays in the test's session.

use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A child process attached to a pty the test owns.
pub struct PtyChild {
    master: Option<File>,
    child: Child,
    output: Arc<Mutex<String>>,
    reader: Option<std::thread::JoinHandle<()>>,
}

/// Spawn `binary` with the pty slave as stdin/stdout/stderr.
pub fn spawn(binary: &Path, args: &[&str], environment: &[(String, String)], cwd: &Path) -> PtyChild {
    let winsize = nix::pty::Winsize {
        ws_row: 40,
        ws_col: 120,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let openpty = nix::pty::openpty(Some(&winsize), None).expect("openpty must allocate a pty");
    let master = File::from(openpty.master);
    let slave = File::from(openpty.slave);
    let mut command = Command::new(binary);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::from(slave.try_clone().expect("clone slave for stdin")))
        .stdout(Stdio::from(slave.try_clone().expect("clone slave for stdout")))
        .stderr(Stdio::from(slave.try_clone().expect("clone slave for stderr")));
    for (name, value) in environment {
        command.env(name, value);
    }
    let child = command.spawn().expect("interactive child must start");
    drop(slave);

    let output = Arc::new(Mutex::new(String::new()));
    let mut reader_master = master.try_clone().expect("clone pty master");
    let buffer = Arc::clone(&output);
    let reader = std::thread::spawn(move || {
        let mut bytes = [0_u8; 4096];
        loop {
            match reader_master.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => buffer
                    .lock()
                    .expect("pty buffer")
                    .push_str(&String::from_utf8_lossy(&bytes[..count])),
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    });

    PtyChild {
        master: Some(master),
        child,
        output,
        reader: Some(reader),
    }
}

impl PtyChild {
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn output(&self) -> String {
        self.output.lock().expect("pty buffer").clone()
    }

    pub fn wait_for(&self, marker: &str, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.output.lock().expect("pty buffer").contains(marker) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!(
            "marker {marker:?} not rendered; output:\n{}",
            self.output.lock().expect("pty buffer")
        );
    }

    pub fn write_input(&mut self, data: &str) {
        let master = self.master.as_mut().expect("pty master is open");
        master.write_all(data.as_bytes()).expect("pty input");
        master.flush().expect("pty input flush");
    }

    /// Close the master so later terminal reads/writes fail.
    pub fn close_master(&mut self) {
        self.master = None;
    }

    pub fn wait(&mut self, timeout: Duration) -> i32 {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().expect("child status") {
                if let Some(reader) = self.reader.take() {
                    reader.join().expect("pty reader");
                }
                return status.code().unwrap_or(-1);
            }
            if Instant::now() >= deadline {
                self.kill();
                panic!(
                    "interactive process did not exit within {timeout:?}; output:\n{}",
                    self.output()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn kill(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
        if let Some(reader) = self.reader.take() {
            reader.join().expect("pty reader");
        }
    }
}

impl Drop for PtyChild {
    fn drop(&mut self) {
        if self.reader.is_some() {
            self.kill();
        }
    }
}
