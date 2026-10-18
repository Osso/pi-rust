//! Port of `packages/agent/src/search/index.ts`.

use crate::harness::session::types::EntryType;

pub use crate::search::scanning::{
    ScanReadableQuery, ScanningReadable, ScanningReadableOptions, ScanningReadableSource, ScanningSearchTextProjector,
    ScanningSessionSearch, ScanningSessionSearchHit, ScanningSessionSearchOptions, SearchError, SessionSearchCandidate,
    create_scanning_session_search, scan_readable_entries, scanning_entries,
};

/// Search filters.
#[derive(Debug, Clone, Default)]
pub struct SessionSearchOptions {
    /// Restrict results to specific canonical entry types.
    pub entry_types: Option<Vec<EntryType>>,
    /// Maximum number of hits to return.
    pub limit: Option<usize>,
    /// Cancellation token, e.g. search-as-you-type.
    pub signal: Option<tokio_util::sync::CancellationToken>,
}

/// Logical identity of one hit.
pub trait SessionSearchHit {
    fn session_id(&self) -> &str;
    fn entry_id(&self) -> &str;
}

/// Session search contract.
pub trait SessionSearch<H: SessionSearchHit> {
    fn search<'a>(
        &'a self,
        text: &'a str,
        options: SessionSearchOptions,
    ) -> pi_ai::request::BoxFuture<'a, Result<Vec<H>, SearchError>>;
}
