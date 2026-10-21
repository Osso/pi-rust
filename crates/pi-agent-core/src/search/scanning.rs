//! Port of `packages/agent/src/search/scanning.ts`.

use std::collections::HashSet;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

use pi_ai::request::BoxFuture;
use pi_ai::types::JsonObject;

use crate::harness::session::facade::Session;
use crate::harness::session::types::{
    Entry, EntryCursor, EntryOrder, EntryQuery, EntryType, SessionError, SessionMetadataValue,
};
use crate::search::index::{SessionSearch, SessionSearchHit, SessionSearchOptions};

/// One entry projected into searchable text.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSearchCandidate {
    pub entry_id: String,
    pub seq: i64,
    pub entry_type: EntryType,
    pub timestamp: i64,
    pub text: String,
    pub fields: Option<JsonObject>,
}

/// Minimal readable surface used by scanning search.
pub trait ScanningReadable<M: SessionMetadataValue>: Send + Sync {
    fn metadata<'a>(&'a self) -> BoxFuture<'a, Result<M, SessionError>>;
    fn entries<'a>(&'a self, query: EntryQuery) -> BoxFuture<'a, Result<Vec<Entry>, SessionError>>;
    fn label<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<String>, SessionError>>;
}

impl<M: SessionMetadataValue> ScanningReadable<M> for Session<M> {
    fn metadata<'a>(&'a self) -> BoxFuture<'a, Result<M, SessionError>> {
        Box::pin(async move { self.get_metadata().await })
    }

    fn entries<'a>(&'a self, query: EntryQuery) -> BoxFuture<'a, Result<Vec<Entry>, SessionError>> {
        Box::pin(async move { self.find_entries(query).await })
    }

    fn label<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<String>, SessionError>> {
        Box::pin(async move { self.get_label(id).await })
    }
}

/// Dynamic source of readable sessions.
pub type ScanningReadableSource<M, O> =
    Arc<dyn Fn(Option<O>) -> BoxFuture<'static, Result<Vec<Arc<dyn ScanningReadable<M>>>, SessionError>> + Send + Sync>;

/// Fixed or dynamic readable source.
pub enum ScanningSource<M: SessionMetadataValue, O> {
    Readables(Vec<Arc<dyn ScanningReadable<M>>>),
    Dynamic(ScanningReadableSource<M, O>),
}

/// Text projector for one entry.
pub type ScanningSearchTextProjector<M> = Arc<dyn Fn(&M, &Entry, Option<&str>) -> String + Send + Sync>;

/// Readable scan options.
#[derive(Clone)]
pub struct ScanningReadableOptions<M: SessionMetadataValue> {
    pub project_text: Option<ScanningSearchTextProjector<M>>,
    pub page_size: Option<usize>,
}

impl<M: SessionMetadataValue> Default for ScanningReadableOptions<M> {
    fn default() -> Self {
        Self {
            project_text: None,
            page_size: None,
        }
    }
}

/// Matcher for one candidate.
pub type ScanningMatcher<M> = Arc<dyn Fn(&str, &SessionSearchCandidate, &M) -> bool + Send + Sync>;

/// Hit factory for one candidate.
pub type ScanningHitFactory<M, H> = Arc<dyn Fn(&M, &SessionSearchCandidate) -> H + Send + Sync>;

/// Maps the normalized query to source-specific options.
pub type ScanningSourceOptionsMapper<O> = Arc<dyn Fn(&str, &SessionSearchOptions) -> Option<O> + Send + Sync>;

/// Hit returned by the default scanning search.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanningSessionSearchHit {
    pub session_id: String,
    pub entry_id: String,
    pub timestamp: i64,
    pub snippet: String,
}

impl SessionSearchHit for ScanningSessionSearchHit {
    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn entry_id(&self) -> &str {
        &self.entry_id
    }
}

/// Scanning search options.
pub struct ScanningSessionSearchOptions<M: SessionMetadataValue, O, H> {
    pub readable: ScanningReadableOptions<M>,
    pub source_options: Option<ScanningSourceOptionsMapper<O>>,
    pub matcher: Option<ScanningMatcher<M>>,
    pub create_hit: Option<ScanningHitFactory<M, H>>,
}

impl<M: SessionMetadataValue, O, H> Default for ScanningSessionSearchOptions<M, O, H> {
    fn default() -> Self {
        Self {
            readable: ScanningReadableOptions::default(),
            source_options: None,
            matcher: None,
            create_hit: None,
        }
    }
}

/// Scanning search failures.
#[derive(Debug)]
pub enum SearchError {
    Session(SessionError),
    Aborted,
    DuplicateSessionId(String),
}

impl Display for SearchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(error) => Display::fmt(error, formatter),
            Self::Aborted => formatter.write_str("The operation was aborted"),
            Self::DuplicateSessionId(id) => write!(formatter, "Duplicate sessionId: {id}"),
        }
    }
}

impl Error for SearchError {}

impl From<SessionError> for SearchError {
    fn from(error: SessionError) -> Self {
        Self::Session(error)
    }
}

fn default_search_text<M>(_metadata: &M, entry: &Entry, label: Option<&str>) -> String {
    let serialized = serde_json::to_value(entry)
        .map(|value| pi_js_compat::js_json::stringify(&value))
        .unwrap_or_default();
    match label {
        None => serialized,
        Some(label) => format!("{serialized} {label}"),
    }
}

fn normalize_query(text: &str, options: &SessionSearchOptions) -> Option<String> {
    let normalized = text.trim().to_lowercase();
    if normalized.is_empty() || options.limit == Some(0) || options.entry_types.as_ref().is_some_and(Vec::is_empty) {
        return None;
    }
    Some(normalized)
}

fn default_match(query_text: &str, candidate: &SessionSearchCandidate) -> bool {
    candidate.text.to_lowercase().contains(query_text)
}

fn check_aborted(options: &SessionSearchOptions) -> Result<(), SearchError> {
    match &options.signal {
        Some(signal) if signal.is_cancelled() => Err(SearchError::Aborted),
        _ => Ok(()),
    }
}

/// Pagination controls for one readable scan.
#[derive(Debug, Clone, Default)]
pub struct ScanReadableQuery {
    pub after_seq: Option<i64>,
    pub limit: Option<usize>,
    pub entry_types: Option<Vec<EntryType>>,
}

/// Scan one readable session and project matching candidates.
pub async fn scan_readable_entries<M: SessionMetadataValue>(
    readable: &Arc<dyn ScanningReadable<M>>,
    metadata: &M,
    options: &ScanningReadableOptions<M>,
    query: ScanReadableQuery,
) -> Result<Vec<SessionSearchCandidate>, SearchError> {
    let project_text = options
        .project_text
        .clone()
        .unwrap_or_else(|| Arc::new(default_search_text::<M>));
    let page_size = query.limit.or(options.page_size).unwrap_or(100);
    let entry_types = query.entry_types;
    scan_all_pages(
        readable,
        metadata,
        &project_text,
        entry_types.as_deref(),
        page_size,
        query.after_seq.unwrap_or(0),
    )
    .await
}

async fn scan_all_pages<M: SessionMetadataValue>(
    readable: &Arc<dyn ScanningReadable<M>>,
    metadata: &M,
    project_text: &ScanningSearchTextProjector<M>,
    entry_types: Option<&[EntryType]>,
    page_size: usize,
    mut after_seq: i64,
) -> Result<Vec<SessionSearchCandidate>, SearchError> {
    let mut candidates = Vec::new();
    loop {
        let (page, next_after, page_len) =
            scan_page(readable, metadata, project_text, entry_types, page_size, after_seq).await?;
        candidates.extend(page);
        after_seq = next_after;
        if page_len < page_size {
            return Ok(candidates);
        }
    }
}

async fn scan_page<M: SessionMetadataValue>(
    readable: &Arc<dyn ScanningReadable<M>>,
    metadata: &M,
    project_text: &ScanningSearchTextProjector<M>,
    entry_types: Option<&[EntryType]>,
    page_size: usize,
    after_seq: i64,
) -> Result<(Vec<SessionSearchCandidate>, i64, usize), SearchError> {
    let entries = readable
        .entries(EntryQuery {
            entry_type: entry_types.filter(|types| types.len() == 1).map(|types| types[0]),
            order: Some(EntryOrder::OldestFirst),
            limit: Some(page_size),
            cursor: Some(EntryCursor { after_seq }),
            custom_type: None,
        })
        .await?;
    let Some(last) = entries.last() else {
        return Ok((Vec::new(), after_seq, 0));
    };
    let mut candidates = Vec::new();
    for entry in &entries {
        if entry_types.is_some_and(|types| !types.contains(&entry.entry_type())) {
            continue;
        }
        candidates.push(project_candidate(readable, metadata, project_text, entry).await?);
    }
    Ok((candidates, last.seq, entries.len()))
}

async fn project_candidate<M: SessionMetadataValue>(
    readable: &Arc<dyn ScanningReadable<M>>,
    metadata: &M,
    project_text: &ScanningSearchTextProjector<M>,
    entry: &Entry,
) -> Result<SessionSearchCandidate, SearchError> {
    let label = readable.label(entry.id()).await?;
    let fields = label.as_ref().map(|label| {
        let mut fields = JsonObject::new();
        fields.insert("label".to_owned(), serde_json::Value::String(label.clone()));
        fields
    });
    Ok(SessionSearchCandidate {
        entry_id: entry.id().to_owned(),
        seq: entry.seq,
        entry_type: entry.entry_type(),
        timestamp: entry.timestamp,
        text: project_text(metadata, entry, label.as_deref()),
        fields,
    })
}

/// Scan every entry of one readable session.
pub async fn scanning_entries<M: SessionMetadataValue>(
    readable: &Arc<dyn ScanningReadable<M>>,
    options: &ScanningReadableOptions<M>,
) -> Result<Vec<SessionSearchCandidate>, SearchError> {
    let metadata = readable.metadata().await?;
    scan_readable_entries(readable, &metadata, options, ScanReadableQuery::default()).await
}

fn default_hit<M: SessionMetadataValue>(metadata: &M, candidate: &SessionSearchCandidate) -> ScanningSessionSearchHit {
    ScanningSessionSearchHit {
        session_id: metadata.id().to_owned(),
        entry_id: candidate.entry_id.clone(),
        timestamp: candidate.timestamp,
        snippet: candidate.text.clone(),
    }
}

/// Scanning search over fixed or dynamic readable sources.
pub struct ScanningSessionSearch<M: SessionMetadataValue, O, H> {
    source: ScanningSource<M, O>,
    options: ScanningSessionSearchOptions<M, O, H>,
}

/// Create a scanning search over a fixed or dynamic source.
pub fn create_scanning_session_search<M, O, H>(
    source: ScanningSource<M, O>,
    options: ScanningSessionSearchOptions<M, O, H>,
) -> ScanningSessionSearch<M, O, H>
where
    M: SessionMetadataValue,
    H: SessionSearchHit + Clone + Send + Sync + From<ScanningSessionSearchHit> + 'static,
{
    ScanningSessionSearch { source, options }
}

impl<M, O, H> ScanningSessionSearch<M, O, H>
where
    M: SessionMetadataValue,
    O: Send + 'static,
    H: SessionSearchHit + Clone + Send + Sync + From<ScanningSessionSearchHit> + 'static,
{
    async fn collect_readable_hits(
        &self,
        readable: &Arc<dyn ScanningReadable<M>>,
        normalized_text: &str,
        entry_types: Option<&[EntryType]>,
        search_options: &SessionSearchOptions,
        seen_session_ids: &mut HashSet<String>,
        hits: &mut Vec<H>,
    ) -> Result<bool, SearchError> {
        check_aborted(search_options)?;
        let metadata = readable.metadata().await?;
        if !seen_session_ids.insert(metadata.id().to_owned()) {
            return Err(SearchError::DuplicateSessionId(metadata.id().to_owned()));
        }
        let candidates = scan_readable_entries(
            readable,
            &metadata,
            &self.options.readable,
            ScanReadableQuery {
                entry_types: entry_types.map(<[EntryType]>::to_vec),
                ..ScanReadableQuery::default()
            },
        )
        .await?;
        self.collect_candidate_hits(&candidates, normalized_text, &metadata, search_options, hits)
    }

    fn collect_candidate_hits(
        &self,
        candidates: &[SessionSearchCandidate],
        normalized_text: &str,
        metadata: &M,
        search_options: &SessionSearchOptions,
        hits: &mut Vec<H>,
    ) -> Result<bool, SearchError> {
        for candidate in candidates {
            check_aborted(search_options)?;
            if !self.candidate_matches(normalized_text, candidate, metadata) {
                continue;
            }
            hits.push(self.create_hit(metadata, candidate));
            if search_options.limit.is_some_and(|limit| hits.len() >= limit) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    async fn load_readables(
        &self,
        normalized_text: &str,
        search_options: &SessionSearchOptions,
    ) -> Result<Vec<Arc<dyn ScanningReadable<M>>>, SearchError> {
        let source_options = self
            .options
            .source_options
            .as_ref()
            .and_then(|source_options| source_options(normalized_text, search_options));
        match &self.source {
            ScanningSource::Readables(readables) => Ok(readables.clone()),
            ScanningSource::Dynamic(source) => Ok(source(source_options).await?),
        }
    }

    fn candidate_matches(&self, normalized_text: &str, candidate: &SessionSearchCandidate, metadata: &M) -> bool {
        match &self.options.matcher {
            Some(matcher) => matcher(normalized_text, candidate, metadata),
            None => default_match(normalized_text, candidate),
        }
    }

    fn create_hit(&self, metadata: &M, candidate: &SessionSearchCandidate) -> H {
        match &self.options.create_hit {
            Some(create_hit) => create_hit(metadata, candidate),
            None => H::from(default_hit(metadata, candidate)),
        }
    }
}

impl<M, O, H> SessionSearch<H> for ScanningSessionSearch<M, O, H>
where
    M: SessionMetadataValue,
    O: Send + 'static,
    H: SessionSearchHit + Clone + Send + Sync + From<ScanningSessionSearchHit> + 'static,
{
    fn search<'a>(
        &'a self,
        text: &'a str,
        search_options: SessionSearchOptions,
    ) -> BoxFuture<'a, Result<Vec<H>, SearchError>> {
        Box::pin(async move {
            let Some(normalized_text) = normalize_query(text, &search_options) else {
                return Ok(Vec::new());
            };
            let readables = self.load_readables(&normalized_text, &search_options).await?;
            let mut hits = Vec::new();
            let mut seen_session_ids = HashSet::new();
            for readable in readables {
                let done = self
                    .collect_readable_hits(
                        &readable,
                        &normalized_text,
                        search_options.entry_types.as_deref(),
                        &search_options,
                        &mut seen_session_ids,
                        &mut hits,
                    )
                    .await?;
                if done {
                    return Ok(hits);
                }
            }
            Ok(hits)
        })
    }
}
