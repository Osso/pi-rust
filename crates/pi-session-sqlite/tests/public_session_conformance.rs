//! Downstream consumer of pi-agent-core's public factory; core never depends on SQLite.
mod support;

use pi_agent_core::harness::session::testing::{
    ConformanceResult, SessionBackendFixture, SessionBackendRepository, create_session_backend_conformance,
};
use pi_agent_core::harness::session::{ForkOptions, Session, SessionCreateOptions, SessionError};
use pi_ai::request::BoxFuture;
use pi_session_sqlite::{SqliteSessionCreateOptions, SqliteSessionListOptions, SqliteSessionMetadata};
use support::Fixture;

impl SessionBackendRepository for Fixture {
    type Metadata = SqliteSessionMetadata;
    fn create(&self, options: SessionCreateOptions) -> BoxFuture<'_, Result<Session<Self::Metadata>, SessionError>> {
        Box::pin(self.repo.create(SqliteSessionCreateOptions {
            id: options.id,
            parent_session_id: options.parent_session_id,
            cwd: self.cwd(),
            ..Default::default()
        }))
    }
    fn open<'a>(
        &'a self,
        metadata: &'a Self::Metadata,
    ) -> BoxFuture<'a, Result<Session<Self::Metadata>, SessionError>> {
        Box::pin(self.repo.open(metadata))
    }
    fn list(&self) -> BoxFuture<'_, Result<Vec<Self::Metadata>, SessionError>> {
        Box::pin(self.repo.list(SqliteSessionListOptions::default()))
    }
    fn delete<'a>(&'a self, metadata: &'a Self::Metadata) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(self.repo.delete(metadata))
    }
    fn fork<'a>(
        &'a self,
        source: &'a Self::Metadata,
        options: ForkOptions,
        create: SessionCreateOptions,
    ) -> BoxFuture<'a, Result<Session<Self::Metadata>, SessionError>> {
        Box::pin(self.repo.fork(
            source,
            options,
            SqliteSessionCreateOptions {
                id: create.id,
                parent_session_id: create.parent_session_id,
                cwd: self.cwd(),
                ..Default::default()
            },
        ))
    }
}

impl SessionBackendFixture for Fixture {
    type Repository = Self;
    fn repository(&self) -> &Self {
        self
    }
    async fn dispose(&mut self) -> ConformanceResult<()> {
        self.repo.close().await?;
        Ok(())
    }
}

#[tokio::test]
async fn downstream_sqlite_runs_all_applicable_public_cases() {
    let cases = create_session_backend_conformance(|| async { Ok(Fixture::new()) });
    assert_eq!(cases.len(), 28);
    for case in cases {
        case.run()
            .await
            .unwrap_or_else(|error| panic!("{} / {}: {error}", case.group, case.name));
    }
}
