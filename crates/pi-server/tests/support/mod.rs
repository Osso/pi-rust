use std::sync::Arc;

use pi_protocol::{AttachCommand, Command, DetachCommand, ResponseEnvelope, SetThinkingCommand, ThinkingLevel};
use pi_server::testing::{ProtocolTestClient, connect_unix_test_client};
use pi_server::transports::unix::{UnixServerOptions, create_unix_server};
use pi_server::{PiServer, PiServerService};
use tempfile::TempDir;

pub struct ServerFixture {
    _directory: TempDir,
    pub path: String,
    pub server: Arc<PiServer>,
}

impl ServerFixture {
    pub async fn start(service: Arc<dyn PiServerService>) -> Self {
        Self::start_with(service, |_| {}).await
    }

    pub async fn start_with(service: Arc<dyn PiServerService>, configure: impl FnOnce(&mut UnixServerOptions)) -> Self {
        let directory = tempfile::tempdir().expect("temporary directory must be created");
        let path = directory.path().join("server.sock").to_string_lossy().into_owned();
        let mut options = UnixServerOptions {
            path: path.clone(),
            ..UnixServerOptions::default()
        };
        configure(&mut options);
        let server = Arc::new(create_unix_server(service, options).expect("unix server options must be valid"));
        server.start().await.expect("unix server must start");
        Self {
            _directory: directory,
            path,
            server,
        }
    }

    pub async fn connect(&self) -> ProtocolTestClient {
        connect_unix_test_client(&self.path)
            .await
            .expect("test client must connect")
    }

    pub async fn close(&self) {
        self.server.close().await.expect("server must close");
    }
}

pub fn list_command() -> Command {
    Command::List(pi_protocol::ListCommand::new())
}

pub fn attach_command(session_id: &str) -> Command {
    Command::Attach(AttachCommand::new(session_id.to_string()).expect("attach command must be valid"))
}

pub fn detach_command(session_id: &str) -> Command {
    Command::Detach(DetachCommand::new(session_id.to_string()).expect("detach command must be valid"))
}

pub fn set_thinking_command(session_id: &str, thinking_level: ThinkingLevel) -> Command {
    Command::SetThinking(
        SetThinkingCommand::new(session_id.to_string(), thinking_level).expect("set thinking command must be valid"),
    )
}

pub fn success_result(response: &ResponseEnvelope) -> &pi_protocol::CommandResult {
    match response {
        ResponseEnvelope::Success(response) => response.result(),
        ResponseEnvelope::Error(response) => {
            panic!("expected success response, got error: {:?}", response.error())
        }
    }
}
