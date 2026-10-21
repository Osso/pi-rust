//! Supervisor request wake socket (`supervisor/request-wake.ts`).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::utils::private_paths::{create_private_directory, remove_socket_file_if_present, set_private_socket_mode};

pub fn get_supervisor_request_socket_path(control_db_path: &str) -> String {
    format!("{control_db_path}.supervisor.sock")
}

/// Fire-and-forget wake ping; a missing or refused socket is ignored, as in the source.
pub fn notify_supervisor_request(control_db_path: &str) {
    let _ = std::os::unix::net::UnixStream::connect(get_supervisor_request_socket_path(control_db_path));
}

pub struct SupervisorRequestWakeServer {
    socket_path: PathBuf,
    generation: watch::Sender<u64>,
    generation_rx: watch::Receiver<u64>,
    accept_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl SupervisorRequestWakeServer {
    pub fn new(control_db_path: &str) -> Self {
        let (generation, generation_rx) = watch::channel(0);
        Self {
            socket_path: PathBuf::from(get_supervisor_request_socket_path(control_db_path)),
            generation,
            generation_rx,
            accept_task: Mutex::new(None),
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub async fn start(&self) -> Result<(), String> {
        if self.accept_task().is_some() {
            return Ok(());
        }
        assert_unix_socket_support()?;
        if let Some(directory) = self.socket_path.parent() {
            create_private_directory(directory)?;
        }
        let listener = listen_with_stale_socket_recovery(&self.socket_path).await?;
        set_private_socket_mode(&self.socket_path)?;
        let generation = self.generation.clone();
        let task = tokio::spawn(async move {
            while let Ok((stream, _address)) = listener.accept().await {
                generation.send_modify(|value| *value += 1);
                drop(stream);
            }
        });
        *self.accept_task() = Some(task);
        Ok(())
    }

    pub fn current_generation(&self) -> u64 {
        *self.generation_rx.borrow()
    }

    pub async fn wait_for_wake_after(&self, observed_generation: u64, signal: &CancellationToken) {
        let mut receiver = self.generation_rx.clone();
        receiver.borrow_and_update();
        if signal.is_cancelled() || self.current_generation() > observed_generation {
            return;
        }
        tokio::select! {
            _ = receiver.changed() => {}
            () = signal.cancelled() => {}
        }
    }

    pub async fn close(&self) -> Result<(), String> {
        self.generation.send_modify(|_| {});
        if let Some(task) = self.accept_task().take() {
            task.abort();
        }
        remove_socket_file_if_present(&self.socket_path);
        Ok(())
    }

    fn accept_task(&self) -> std::sync::MutexGuard<'_, Option<tokio::task::JoinHandle<()>>> {
        self.accept_task
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Drop for SupervisorRequestWakeServer {
    fn drop(&mut self) {
        self.generation.send_modify(|_| {});
        if let Some(task) = self
            .accept_task
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            task.abort();
        }
        remove_socket_file_if_present(&self.socket_path);
    }
}

fn assert_unix_socket_support() -> Result<(), String> {
    if cfg!(windows) {
        return Err("Supervisor request wake requires Unix domain sockets".to_owned());
    }
    Ok(())
}

async fn listen_with_stale_socket_recovery(socket_path: &Path) -> Result<UnixListener, String> {
    match UnixListener::bind(socket_path) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            if socket_accepts_connections(socket_path).await? {
                return Err(format!(
                    "Supervisor request socket is already active: {}",
                    socket_path.display()
                ));
            }
            remove_socket_file_if_present(socket_path);
            UnixListener::bind(socket_path).map_err(|error| error.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

async fn socket_accepts_connections(socket_path: &Path) -> Result<bool, String> {
    match UnixStream::connect(socket_path).await {
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
        Err(error) => Err(error.to_string()),
    }
}
