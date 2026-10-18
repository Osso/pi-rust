use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use pi_protocol::{EventEnvelope, ModelMetadata, ServerEvent, ServerMessage, ServerSnapshot};

use crate::server::ServerCore;
use crate::types::ServiceResult;

/// Publishes serialized server snapshots to ready connections.
pub(crate) struct ServerSnapshotPublisher {
    core: Weak<ServerCore>,
    revision: AtomicU64,
    queue: tokio::sync::Mutex<()>,
}

impl ServerSnapshotPublisher {
    pub(crate) fn new(core: Weak<ServerCore>) -> Self {
        Self {
            core,
            revision: AtomicU64::new(0),
            queue: tokio::sync::Mutex::new(()),
        }
    }

    pub(crate) fn current_revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    pub(crate) fn get(
        &self,
        models: Option<Vec<ModelMetadata>>,
    ) -> crate::types::BoxFuture<'_, ServiceResult<ServerSnapshot>> {
        Box::pin(async move {
            let revision = self.current_revision();
            let core = self.core.upgrade().ok_or_else(dropped_error)?;
            let sessions = core.sessions.list_metadata().await?;
            let models = match models {
                Some(models) => models,
                None => core.service.list_models().await?,
            };
            ServerSnapshot::new(core.id.clone(), revision, sessions, models)
                .map_err(|error| Box::new(error) as crate::types::ServiceError)
        })
    }

    /// Queues one snapshot broadcast. Revisions are serialized by the publisher queue.
    pub(crate) fn broadcast(self: &Arc<Self>) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let result = publisher.perform_broadcast().await;
            if let Err(error) = result
                && let Some(core) = publisher.core.upgrade()
            {
                core.report_error(error.as_ref());
            }
        });
    }

    async fn perform_broadcast(&self) -> ServiceResult<()> {
        let _guard = self.queue.lock().await;
        let core = self.core.upgrade().ok_or_else(dropped_error)?;
        let ready_connections: Vec<_> = core
            .connections
            .lock()
            .expect("connection map mutex poisoned")
            .values()
            .filter(|connection| {
                connection.stage() == crate::connection::ConnectionStage::Ready
                    && !connection.disconnected.load(Ordering::SeqCst)
            })
            .cloned()
            .collect();
        if ready_connections.is_empty() || core.is_closing() {
            return Ok(());
        }
        let revision = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        let models = core.service.list_models().await?;
        let snapshot = self.get(Some(models)).await?;
        let snapshot = ServerSnapshot::new(
            snapshot.server_id().to_string(),
            revision,
            snapshot.sessions().to_vec(),
            snapshot.models().to_vec(),
        )
        .map_err(|error| Box::new(error) as crate::types::ServiceError)?;
        let envelope = EventEnvelope::new(ServerEvent::ServerSnapshot(snapshot));
        for connection in ready_connections {
            core.send_message(connection, ServerMessage::Event(envelope.clone()))
                .await;
        }
        Ok(())
    }
}

pub(crate) fn dropped_error() -> crate::types::ServiceError {
    Box::new(std::io::Error::other("PiServer was dropped"))
}
