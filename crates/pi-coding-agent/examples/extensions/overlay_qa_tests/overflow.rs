use super::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_tui::utils::{truncate_to_width, truncate_to_width_with};
use tokio::io::AsyncReadExt;
#[derive(Default)]
struct Output {
    lines: Vec<JsString>,
    scroll: usize,
    finished: bool,
    exited: bool,
    disposed: bool,
}
pub(super) struct Process {
    output: Rc<RefCell<Output>>,
    pid: u32,
    readers: Vec<tokio::task::AbortHandle>,
}
impl Process {
    pub(super) fn start(model: &Rc<Model>) -> Self {
        let mut child = tokio::process::Command::new("bash")
            .env_clear()
            .envs(pi_js_compat::process_env::vars())
            .args(["-c", include_str!("overflow.sh.txt")])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("overlay overflow: spawn bash");
        let pid = child.id().expect("spawned child pid");
        let output = Rc::new(RefCell::new(Output::default()));
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let readers = vec![
            read(stdout, false, output.clone(), model.theme.clone(), model.tui.clone()),
            read(stderr, true, output.clone(), model.theme.clone(), model.tui.clone()),
        ];
        let abort_readers = readers.iter().map(tokio::task::JoinHandle::abort_handle).collect();
        let state = output.clone();
        let tui = model.tui.clone();
        tokio::task::spawn_local(async move {
            child.wait().await.expect("overlay overflow: reap subprocess");
            state.borrow_mut().exited = true;
            for reader in readers {
                match reader.await {
                    Ok(()) => {}
                    Err(error) if error.is_cancelled() && state.borrow().disposed => {}
                    Err(error) => panic!("overlay overflow: join output reader: {error}"),
                }
            }
            if !state.borrow().disposed {
                state.borrow_mut().finished = true;
                tui.request_render();
            }
        });
        Self {
            output,
            pid,
            readers: abort_readers,
        }
    }
    fn kill(&self) {
        if self.output.borrow().exited {
            return;
        }
        if let Some(pid) = rustix::process::Pid::from_raw(self.pid as i32) {
            match rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => panic!("overlay overflow: terminate child: {error}"),
            }
        }
    }
    pub(super) fn dispose(&mut self) {
        self.output.borrow_mut().disposed = true;
        self.kill();
        for reader in self.readers.drain(..) {
            reader.abort();
        }
    }
    pub(super) fn input(&self, data: &JsString) {
        let mut state = self.output.borrow_mut();
        if matches_key(data, "up") {
            state.scroll = state.scroll.saturating_sub(1);
        } else if matches_key(data, "down") {
            state.scroll = (state.scroll + 1).min(state.lines.len().saturating_sub(15));
        }
    }
    pub(super) fn render(&self, m: &Model, width: usize) -> Vec<JsString> {
        let state = self.output.borrow();
        let inner = width.saturating_sub(2).max(1);
        let border = |s: &str| m.theme.fg(ThemeColor::Border, s);
        let title = truncate_to_width(format!(" Streaming Output ({} lines) ", state.lines.len()), inner);
        let mut result = vec![js_format!(
            "{}{}{}",
            border("╭"),
            m.theme.fg(ThemeColor::Accent, &title),
            border(&format!("{}╮", "─".repeat(inner.saturating_sub(visible_width(&title)))))
        )];
        let remaining = state.lines.len().saturating_sub(15 + state.scroll);
        let scroll = if state.scroll > 0 || remaining > 0 {
            m.theme
                .fg(ThemeColor::Dim, format!(" ↑{} | ↓{remaining}", state.scroll))
        } else {
            JsString::default()
        };
        let row = |s: &JsString| {
            js_format!(
                "{}{}{}",
                border("│"),
                truncate_to_width_with(s, inner, "...", true),
                border("│")
            )
        };
        result.push(row(&scroll));
        let visible = state.lines.iter().skip(state.scroll).take(15).collect::<Vec<_>>();
        result.extend(visible.iter().map(|line| row(&js_format!(" {}", line))));
        result.extend((visible.len()..15).map(|_| row(&JsString::default())));
        let status = if state.finished {
            m.theme.fg(ThemeColor::Success, "✓ Done")
        } else {
            m.theme.fg(ThemeColor::Warning, "● Running")
        };
        result.push(row(&js_format!(
            " {} {}",
            status,
            m.theme.fg(ThemeColor::Dim, "| ↑↓ scroll | Esc close")
        )));
        result.push(border(&format!("╰{}╯", "─".repeat(inner))));
        result
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.dispose();
    }
}
fn read(
    mut reader: impl tokio::io::AsyncRead + Unpin + 'static,
    stderr: bool,
    output: Rc<RefCell<Output>>,
    theme: Arc<Theme>,
    tui: ExtensionUiTui,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_local(async move {
        let mut buffer = [0; 8192];
        loop {
            let length = reader
                .read(&mut buffer)
                .await
                .expect("overlay overflow: read process output");
            if length == 0 {
                break;
            }
            if output.borrow().disposed {
                continue;
            }
            let text = String::from_utf8_lossy(&buffer[..length]);
            let mut state = output.borrow_mut();
            if stderr {
                state
                    .lines
                    .push(theme.fg(ThemeColor::Error, pi_js_compat::string::js_trim(&text)));
            } else {
                state
                    .lines
                    .extend(text.split('\n').filter(|line| !line.is_empty()).map(JsString::from));
                state.scroll = state.lines.len().saturating_sub(15);
            }
            drop(state);
            tui.request_render();
        }
    })
}
