//! Supervisor request wake socket (`supervisor/request-wake.ts`).

use pi_js_compat::{
    js_format,
    js_json::{JsString, encode_node_utf8},
};
use std::path::Path;
use std::sync::Mutex;

use pi_js_compat::abort::AbortSignal;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;

use crate::utils::private_paths::{create_private_directory, remove_socket_file_if_present, set_private_socket_mode};

pub fn get_supervisor_request_socket_path(control_db_path: &JsString) -> JsString {
    js_format!("{}.supervisor.sock", control_db_path)
}

/// Fire-and-forget wake ping; a missing or refused socket is ignored, as in the source.
pub fn notify_supervisor_request(control_db_path: &JsString) {
    let _ =
        std::os::unix::net::UnixStream::connect(encode_node_utf8(&get_supervisor_request_socket_path(control_db_path)));
}

pub struct SupervisorRequestWakeServer {
    socket_path: JsString,
    generation: watch::Sender<u64>,
    generation_rx: watch::Receiver<u64>,
    shutdown: Mutex<pi_js_compat::abort::AbortController>,
    accept_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl SupervisorRequestWakeServer {
    pub fn new(control_db_path: &JsString) -> Self {
        let (generation, generation_rx) = watch::channel(0);
        Self {
            socket_path: get_supervisor_request_socket_path(control_db_path),
            generation,
            generation_rx,
            shutdown: Mutex::new(pi_js_compat::abort::AbortController::new()),
            accept_task: Mutex::new(None),
        }
    }

    pub fn socket_path(&self) -> &JsString {
        &self.socket_path
    }

    pub async fn start(&self) -> Result<(), JsString> {
        if self.accept_task().is_some() {
            return Ok(());
        }
        assert_unix_socket_support()?;
        let socket_path = encode_node_utf8(&self.socket_path);
        if let Some(directory) = Path::new(&socket_path).parent() {
            create_private_directory(directory)?;
        }
        let listener = listen_with_stale_socket_recovery(&self.socket_path).await?;
        set_private_socket_mode(Path::new(&encode_node_utf8(&self.socket_path)))?;
        let generation = self.generation.clone();
        let shutdown = self.shutdown().signal();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown.wait() => break,
                    accepted = listener.accept() => {
                        let Ok((stream, _address)) = accepted else { break };
                        generation.send_modify(|value| *value += 1);
                        drop(stream);
                    }
                }
            }
        });
        *self.accept_task() = Some(task);
        Ok(())
    }

    pub fn current_generation(&self) -> u64 {
        *self.generation_rx.borrow()
    }

    pub async fn wait_for_wake_after(&self, observed_generation: u64, signal: &AbortSignal) {
        let mut receiver = self.generation_rx.clone();
        let shutdown = self.shutdown().signal();
        loop {
            if signal.aborted() || *receiver.borrow_and_update() > observed_generation {
                return;
            }
            tokio::select! {
                changed = receiver.changed() => {
                    if changed.is_err() {
                        return;
                    }
                }
                _ = signal.wait() => return,
                _ = shutdown.wait() => return,
            }
        }
    }

    pub async fn close(&self) -> Result<(), JsString> {
        // Resolve only the current waiters and accept loop (request-wake.ts:60-75); later
        // waits observe a fresh signal and stay pending until the next wake.
        {
            let mut controller = self.shutdown();
            controller.abort(None);
            *controller = pi_js_compat::abort::AbortController::new();
        }
        if let Some(task) = self.accept_task().take() {
            task.abort();
        }
        remove_socket_file_if_present(Path::new(&encode_node_utf8(&self.socket_path)));
        Ok(())
    }

    fn shutdown(&self) -> std::sync::MutexGuard<'_, pi_js_compat::abort::AbortController> {
        self.shutdown.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn accept_task(&self) -> std::sync::MutexGuard<'_, Option<tokio::task::JoinHandle<()>>> {
        self.accept_task
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Drop for SupervisorRequestWakeServer {
    fn drop(&mut self) {
        self.shutdown().abort(None);
        if let Some(task) = self
            .accept_task
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            task.abort();
            remove_socket_file_if_present(Path::new(&encode_node_utf8(&self.socket_path)));
        }
    }
}

fn assert_unix_socket_support() -> Result<(), JsString> {
    if cfg!(windows) {
        return Err("Supervisor request wake requires Unix domain sockets".into());
    }
    Ok(())
}

async fn listen_with_stale_socket_recovery(socket_path: &JsString) -> Result<UnixListener, JsString> {
    match UnixListener::bind(encode_node_utf8(socket_path)) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            if socket_accepts_connections(socket_path).await? {
                return Err(js_format!(
                    "Supervisor request socket is already active: {}",
                    socket_path
                ));
            }
            remove_socket_file_if_present(Path::new(&encode_node_utf8(socket_path)));
            UnixListener::bind(encode_node_utf8(socket_path)).map_err(|error| JsString::from(error.to_string()))
        }
        Err(error) => Err(error.to_string().into()),
    }
}

async fn socket_accepts_connections(socket_path: &JsString) -> Result<bool, JsString> {
    match UnixStream::connect(encode_node_utf8(socket_path)).await {
        Ok(stream) => {
            drop(stream);
            Ok(true)
        }
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error.to_string().into()),
    }
}
