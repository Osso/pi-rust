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
use std::os::fd::AsFd;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A child process attached to a pty the test owns.
pub struct PtyChild {
    master: Option<File>,
    child: Child,
    output: Arc<Mutex<String>>,
    reader_stop: Arc<AtomicBool>,
    reader: Option<std::thread::JoinHandle<()>>,
}

/// How long the reader thread blocks before re-checking its stop flag.
const READER_POLL_INTERVAL_MS: u8 = 20;

/// Spawn `binary` with the pty slave as stdin/stdout/stderr.
pub fn spawn(binary: &Path, args: &[&str], environment: &[(String, String)], cwd: &Path) -> PtyChild {
    let winsize = nix::pty::Winsize {
        ws_row: 40,
        ws_col: 120,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let openpty = nix::pty::openpty(Some(&winsize), None).expect("openpty must allocate a pty");
    // The child must not inherit the master: it would keep the pty open
    // after `close_master`.
    rustix::io::fcntl_setfd(&openpty.master, rustix::io::FdFlags::CLOEXEC).expect("master must close on exec");
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
    let reader_stop = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&reader_stop);
    let reader = std::thread::spawn(move || {
        let mut bytes = [0_u8; 4096];
        while !stop.load(Ordering::Relaxed) {
            let mut fds = [nix::poll::PollFd::new(
                reader_master.as_fd(),
                nix::poll::PollFlags::POLLIN,
            )];
            match nix::poll::poll(&mut fds, nix::poll::PollTimeout::from(READER_POLL_INTERVAL_MS)) {
                Ok(0) => continue,
                Ok(_) => {}
                Err(nix::errno::Errno::EINTR) => continue,
                Err(_) => break,
            }
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
        reader_stop,
        reader: Some(reader),
    }
}

impl PtyChild {
    pub fn termios(&self) -> String {
        let termios =
            rustix::termios::tcgetattr(self.master.as_ref().expect("pty master is open")).expect("pty termios");
        format!("{termios:?}")
    }

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

    /// Close the master so later terminal reads/writes fail. The reader
    /// thread holds a duplicate of the master, so it is stopped first.
    pub fn close_master(&mut self) {
        self.stop_reader();
        self.master = None;
    }

    fn stop_reader(&mut self) {
        self.reader_stop.store(true, Ordering::Relaxed);
        if let Some(reader) = self.reader.take() {
            reader.join().expect("pty reader");
        }
    }

    pub fn wait(&mut self, timeout: Duration) -> i32 {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().expect("child status") {
                self.stop_reader();
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
        self.stop_reader();
    }
}

impl Drop for PtyChild {
    fn drop(&mut self) {
        if self.child.try_wait().is_ok_and(|status| status.is_none()) {
            self.kill();
        }
        self.stop_reader();
    }
}
