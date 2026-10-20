//! Public runner-independent session backend fixture and case contracts.

use super::super::{ForkOptions, Session, SessionCreateOptions, SessionError, SessionMetadataValue};
use pi_ai::request::BoxFuture;
use serde::{Serialize, de::DeserializeOwned};
use std::{error::Error, fmt::Debug, future::Future, sync::Arc};

pub type ConformanceError = Box<dyn Error + Send + Sync>;
pub type ConformanceResult<T> = Result<T, ConformanceError>;

/// Adapts repository operations without implementing or replacing backend storage.
/// Metadata remains backend-native; serialization exposes the common metadata fields.
pub trait SessionBackendRepository: Send + Sync {
    type Metadata: SessionMetadataValue + Serialize + DeserializeOwned + Debug + PartialEq;
    fn create(&self, options: SessionCreateOptions) -> BoxFuture<'_, Result<Session<Self::Metadata>, SessionError>>;
    fn open<'a>(&'a self, metadata: &'a Self::Metadata)
    -> BoxFuture<'a, Result<Session<Self::Metadata>, SessionError>>;
    fn list(&self) -> BoxFuture<'_, Result<Vec<Self::Metadata>, SessionError>>;
    fn delete<'a>(&'a self, metadata: &'a Self::Metadata) -> BoxFuture<'a, Result<(), SessionError>>;
    fn fork<'a>(
        &'a self,
        source: &'a Self::Metadata,
        options: ForkOptions,
        create: SessionCreateOptions,
    ) -> BoxFuture<'a, Result<Session<Self::Metadata>, SessionError>>;
}

/// A fresh backend owned by one case. Disposal runs after success or case failure.
/// As with any async resource scope, callers must await `run` to completion.
pub trait SessionBackendFixture: Send {
    type Repository: SessionBackendRepository;
    fn repository(&self) -> &Self::Repository;
    fn dispose(&mut self) -> impl Future<Output = ConformanceResult<()>> + Send;
}

/// Creates one isolated fixture. Closures returning async blocks implement this contract.
pub trait SessionBackendFixtureFactory: Send + Sync {
    type Fixture: SessionBackendFixture;
    fn create(&self) -> impl Future<Output = ConformanceResult<Self::Fixture>> + Send;
}

impl<F, Fut, T> SessionBackendFixtureFactory for F
where
    F: Fn() -> Fut + Send + Sync,
    Fut: Future<Output = ConformanceResult<T>> + Send,
    T: SessionBackendFixture,
{
    type Fixture = T;
    fn create(&self) -> impl Future<Output = ConformanceResult<T>> + Send {
        self()
    }
}

pub(super) type ConformanceTest<R> = for<'a> fn(&'a R) -> BoxFuture<'a, ConformanceResult<()>>;

/// A named case usable from any Rust test runner; constructing cases creates no fixtures.
pub struct SessionBackendConformanceCase<F: SessionBackendFixtureFactory> {
    pub group: &'static str,
    pub name: &'static str,
    pub(super) factory: Arc<F>,
    pub(super) test: ConformanceTest<<F::Fixture as SessionBackendFixture>::Repository>,
}

/// Mirrors `await using` when both the case and its disposal fail.
#[derive(Debug)]
pub struct SuppressedConformanceError {
    pub error: ConformanceError,
    pub suppressed: ConformanceError,
}
impl std::fmt::Display for SuppressedConformanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (suppressed: {})", self.error, self.suppressed)
    }
}
impl Error for SuppressedConformanceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.error.as_ref())
    }
}
