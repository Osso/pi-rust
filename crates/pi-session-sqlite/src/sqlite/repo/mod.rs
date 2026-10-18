//! Port of `packages/session-backends/sqlite-node/src/sqlite/repo.ts`.
//!
//! The repository surface lives here; row decoding is in [`decode`], the `SessionStorage`
//! implementation in [`storage`], and fork planning/copying in [`fork`].

mod decode;
mod fork;
mod storage;

use std::sync::{Arc, Mutex};

use pi_agent_core::harness::session::{
    Clock, ForkOptions, IdGenerator, Session, SessionError, SessionErrorCode, UuidV7IdGenerator, system_clock,
};
use pi_agent_core::harness::types::{CreateDirOptions, FileError, FileErrorCode, FileSystem};

use crate::error::SqliteError;
use crate::sqlite::branch_cache::{delete_branch_cache, rebuild_branch_cache};
use crate::sqlite::connection::{Database, configure_sqlite_database, open_database, transaction};
use crate::sqlite::migrations::apply_migrations;
use crate::sqlite::path::get_parent_path;
use crate::sqlite::repo::decode::require_session_row;
use crate::sqlite::storage::entries::delete_entry_rows;
use crate::sqlite::storage::facts::delete_fact_rows;
use crate::sqlite::storage::lanes::{create_initial_lane, delete_lane_rows, read_lanes};
use crate::sqlite::storage::records::delete_record_rows;
use crate::sqlite::storage::session_sequences::{create_sequence, delete_sequence};
use crate::sqlite::storage::session_stats::{create_stats, delete_stats};
use crate::sqlite::storage::sessions::{
    NewSessionRow, decode_session_metadata, delete_session_row, insert_session_row, read_session_rows, session_exists,
};
use crate::sqlite::storage::writer_leases::{
    WriterLease, acquire_writer_lease, delete_writer_lease, release_writer_lease,
};
use crate::sqlite::types::{SqliteSessionCreateOptions, SqliteSessionListOptions, SqliteSessionMetadata};

pub use storage::SqliteSessionStorage;

/// Writer lease timing.
#[derive(Debug, Clone, Default)]
pub struct SqliteWriterLeaseOptions {
    /// Time without a successful heartbeat before another writer may take over. Default: 30 seconds.
    pub ttl_ms: Option<i64>,
    /// Idle heartbeat cadence. Default: 10 seconds. Must be less than `ttl_ms`.
    pub heartbeat_interval_ms: Option<i64>,
}

/// Invalid lease timing, the Rust mapping of the source `RangeError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteWriterLeaseOptionsError {
    TtlNotPositive,
    HeartbeatNotPositiveOrNotLessThanTtl,
}

impl std::fmt::Display for SqliteWriterLeaseOptionsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TtlNotPositive => formatter.write_str("writerLease.ttlMs must be positive"),
            Self::HeartbeatNotPositiveOrNotLessThanTtl => {
                formatter.write_str("writerLease.heartbeatIntervalMs must be positive and less than ttlMs")
            }
        }
    }
}

impl std::error::Error for SqliteWriterLeaseOptionsError {}

/// Repository options.
#[derive(Clone)]
pub struct SqliteSessionRepositoryOptions {
    pub env: Arc<dyn FileSystem>,
    pub database_path: String,
    pub writer_lease: Option<SqliteWriterLeaseOptions>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolvedWriterLeaseOptions {
    pub(crate) ttl_ms: i64,
    pub(crate) heartbeat_interval_ms: i64,
}

fn resolve_writer_lease_options(
    options: Option<&SqliteWriterLeaseOptions>,
) -> Result<ResolvedWriterLeaseOptions, SqliteWriterLeaseOptionsError> {
    let ttl_ms = options.and_then(|options| options.ttl_ms).unwrap_or(30_000);
    let heartbeat_interval_ms = options
        .and_then(|options| options.heartbeat_interval_ms)
        .unwrap_or(10_000);
    if ttl_ms <= 0 {
        return Err(SqliteWriterLeaseOptionsError::TtlNotPositive);
    }
    if heartbeat_interval_ms <= 0 || heartbeat_interval_ms >= ttl_ms {
        return Err(SqliteWriterLeaseOptionsError::HeartbeatNotPositiveOrNotLessThanTtl);
    }
    Ok(ResolvedWriterLeaseOptions {
        ttl_ms,
        heartbeat_interval_ms,
    })
}

fn active_writer_error(session_id: &str) -> SessionError {
    SessionError::new(
        SessionErrorCode::Storage,
        format!("SQLite session {session_id} already has an active writer"),
    )
}

pub(crate) fn claim_writer_lease(
    db: &rusqlite::Connection,
    session_id: &str,
    options: ResolvedWriterLeaseOptions,
    now: i64,
) -> Result<WriterLease, SqliteError> {
    let owner_id = pi_ai::utils::uuid::uuidv7();
    match acquire_writer_lease(db, session_id, &owner_id, now, now + options.ttl_ms)? {
        Some(lease) => Ok(lease),
        None => Err(SqliteError::Session(active_writer_error(session_id))),
    }
}

fn result_or_throw<T>(result: Result<T, FileError>, message: &str) -> Result<T, SessionError> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let code = if error.code == FileErrorCode::NotFound {
                SessionErrorCode::NotFound
            } else {
                SessionErrorCode::Storage
            };
            Err(SessionError::new(code, format!("{message}: {}", error.message)))
        }
    }
}

type StorageRegistry = Arc<Mutex<Vec<Arc<SqliteSessionStorage>>>>;

/// SQLite session repository.
pub struct SqliteSessionRepository {
    env: Arc<dyn FileSystem>,
    database_path_input: String,
    lease_options: ResolvedWriterLeaseOptions,
    database_path: Mutex<Option<String>>,
    database: Mutex<Option<Arc<Database>>>,
    active_storages: StorageRegistry,
    operations: tokio::sync::Mutex<()>,
    clock: Clock,
    id_generator: Arc<dyn IdGenerator>,
}

impl SqliteSessionRepository {
    pub fn new(options: SqliteSessionRepositoryOptions) -> Result<Self, SqliteWriterLeaseOptionsError> {
        Self::with_clock_and_id_generator(options, system_clock(), Arc::new(UuidV7IdGenerator))
    }

    pub fn with_clock(
        options: SqliteSessionRepositoryOptions,
        clock: Clock,
    ) -> Result<Self, SqliteWriterLeaseOptionsError> {
        Self::with_clock_and_id_generator(options, clock, Arc::new(UuidV7IdGenerator))
    }

    pub fn with_clock_and_id_generator(
        options: SqliteSessionRepositoryOptions,
        clock: Clock,
        id_generator: Arc<dyn IdGenerator>,
    ) -> Result<Self, SqliteWriterLeaseOptionsError> {
        let lease_options = resolve_writer_lease_options(options.writer_lease.as_ref())?;
        Ok(Self {
            env: options.env,
            database_path_input: options.database_path,
            lease_options,
            database_path: Mutex::new(None),
            database: Mutex::new(None),
            active_storages: Arc::new(Mutex::new(Vec::new())),
            operations: tokio::sync::Mutex::new(()),
            clock,
            id_generator,
        })
    }

    async fn release_storages_for_session(&self, session_id: &str) {
        let storages: Vec<Arc<SqliteSessionStorage>> = self
            .active_storages
            .lock()
            .expect("active storages mutex")
            .iter()
            .filter(|storage| storage.is_for_session(session_id))
            .cloned()
            .collect();
        for storage in storages {
            let _ = storage.release().await;
        }
    }

    fn build_storage(
        &self,
        db: Arc<Database>,
        metadata: SqliteSessionMetadata,
        lease: WriterLease,
    ) -> Arc<SqliteSessionStorage> {
        let registry = Arc::clone(&self.active_storages);
        let clock = Arc::clone(&self.clock);
        let lease_options = self.lease_options;
        let storage = Arc::new_cyclic(|weak| {
            let weak = weak.clone();
            SqliteSessionStorage::new(
                db,
                metadata,
                lease,
                lease_options,
                clock,
                Box::new(move || {
                    if let Some(storage) = weak.upgrade() {
                        registry
                            .lock()
                            .expect("active storages mutex")
                            .retain(|candidate| !Arc::ptr_eq(candidate, &storage));
                    }
                }),
            )
        });
        self.active_storages
            .lock()
            .expect("active storages mutex")
            .push(Arc::clone(&storage));
        storage.start_heartbeat();
        storage
    }

    fn claim_session(
        &self,
        db: Arc<Database>,
        metadata: &SqliteSessionMetadata,
    ) -> Result<Session<SqliteSessionMetadata>, SessionError> {
        let active = self
            .active_storages
            .lock()
            .expect("active storages mutex")
            .iter()
            .find(|storage| storage.is_for_session(&metadata.id))
            .cloned();
        if let Some(active) = active {
            db.with_connection(|connection| read_lanes(connection, &metadata.id))
                .map_err(SqliteError::into_session)?;
            return Ok(Session::new(active));
        }
        let storage = self.claim_storage(db, metadata)?;
        Ok(Session::new(storage))
    }

    fn claim_storage(
        &self,
        db: Arc<Database>,
        metadata: &SqliteSessionMetadata,
    ) -> Result<Arc<SqliteSessionStorage>, SessionError> {
        let path = metadata.path.clone();
        let lease_options = self.lease_options;
        let now = (self.clock)();
        let (row, lease) = db
            .with_connection(|connection| {
                require_session_row(connection, &metadata.id)?;
                transaction(connection, |connection| {
                    let lease = claim_writer_lease(connection, &metadata.id, lease_options, now)?;
                    let row = require_session_row(connection, &metadata.id)?;
                    read_lanes(connection, &metadata.id)?;
                    Ok((row, lease))
                })
            })
            .map_err(SqliteError::into_session)?;
        let metadata = decode_session_metadata(&row, &path).map_err(SqliteError::into_session)?;
        Ok(self.build_storage(db, metadata, lease))
    }

    pub async fn create(
        &self,
        options: SqliteSessionCreateOptions,
    ) -> Result<Session<SqliteSessionMetadata>, SessionError> {
        let _guard = self.operations.lock().await;
        let db = self.get_database().await?;
        let path = self.get_database_path().await?;
        let id = options.id.clone().unwrap_or_else(|| self.id_generator.next());
        ensure_session_id_unused(&db, &id)?;
        let created_at = (self.clock)();
        let lease = db
            .with_connection(|connection| insert_new_session(connection, &options, &id, created_at, self.lease_options))
            .map_err(SqliteError::into_session)?;
        let metadata = require_session_metadata(&db, &path, &id)?;
        Ok(Session::new(self.build_storage(db, metadata, lease)))
    }

    pub async fn open(&self, metadata: &SqliteSessionMetadata) -> Result<Session<SqliteSessionMetadata>, SessionError> {
        let _guard = self.operations.lock().await;
        let db = self.get_database().await?;
        self.claim_session(db, metadata)
    }

    /// Rebuilds the private branch cache from canonical entry parent links.
    pub async fn repair_branch_cache(&self, metadata: &SqliteSessionMetadata) -> Result<(), SessionError> {
        let _guard = self.operations.lock().await;
        self.release_storages_for_session(&metadata.id).await;
        let db = self.get_database().await?;
        let lease_options = self.lease_options;
        let now = (self.clock)();
        db.with_connection(|connection| {
            transaction(connection, |connection| {
                let lease = claim_writer_lease(connection, &metadata.id, lease_options, now)?;
                require_session_row(connection, &metadata.id)?;
                rebuild_branch_cache(connection, &metadata.id)?;
                release_writer_lease(connection, &metadata.id, &lease)
            })
        })
        .map_err(SqliteError::into_session)
    }

    /// Reads the session catalog without acquiring writer leases.
    pub async fn list(&self, options: SqliteSessionListOptions) -> Result<Vec<SqliteSessionMetadata>, SessionError> {
        let _guard = self.operations.lock().await;
        let path = self.get_database_path().await?;
        let exists = result_or_throw(
            self.env.exists(&path, None).await,
            &format!("Failed to check database {path}"),
        )?;
        if !exists {
            return Ok(Vec::new());
        }
        let db = self.get_database().await?;
        db.with_connection(|connection| {
            let rows = read_session_rows(connection, options.cwd.as_deref())?;
            rows.iter().map(|row| decode_session_metadata(row, &path)).collect()
        })
        .map_err(SqliteError::into_session)
    }

    pub async fn delete(&self, metadata: &SqliteSessionMetadata) -> Result<(), SessionError> {
        let _guard = self.operations.lock().await;
        self.release_storages_for_session(&metadata.id).await;
        let db = self.get_database().await?;
        let lease_options = self.lease_options;
        let now = (self.clock)();
        db.with_connection(|connection| {
            transaction(connection, |connection| {
                if !session_exists(connection, &metadata.id)? {
                    delete_writer_lease(connection, &metadata.id)?;
                    return Ok(());
                }
                claim_writer_lease(connection, &metadata.id, lease_options, now)?;
                delete_branch_cache(connection, &metadata.id)?;
                delete_fact_rows(connection, &metadata.id)?;
                delete_lane_rows(connection, &metadata.id)?;
                delete_record_rows(connection, &metadata.id)?;
                delete_entry_rows(connection, &metadata.id)?;
                delete_writer_lease(connection, &metadata.id)?;
                delete_stats(connection, &metadata.id)?;
                delete_sequence(connection, &metadata.id)?;
                delete_session_row(connection, &metadata.id)
            })
        })
        .map_err(SqliteError::into_session)
    }

    pub async fn fork(
        &self,
        source: &SqliteSessionMetadata,
        options: ForkOptions,
        create_options: SqliteSessionCreateOptions,
    ) -> Result<Session<SqliteSessionMetadata>, SessionError> {
        let _guard = self.operations.lock().await;
        let db = self.get_database().await?;
        let path = self.get_database_path().await?;
        let source_metadata = self.read_source_metadata(&db, &path, &source.id)?;
        let id = create_options.id.clone().unwrap_or_else(|| self.id_generator.next());
        ensure_session_id_unused(&db, &id)?;
        let plan = fork::plan_fork(&db, &source.id, &options)?;
        let copy = fork::ForkCopy {
            id: &id,
            created_at: (self.clock)(),
            cwd: &create_options.cwd,
            parent_session_id: create_options
                .parent_session_id
                .clone()
                .or_else(|| Some(source.id.clone())),
            metadata: create_options
                .metadata
                .clone()
                .or_else(|| source_metadata.metadata.clone()),
        };
        let lease = fork::copy_fork(&db, copy, &plan, self.lease_options)?;
        let metadata = require_session_metadata(&db, &path, &id)?;
        Ok(Session::new(self.build_storage(db, metadata, lease)))
    }

    pub async fn close(&self) -> Result<(), SessionError> {
        let _guard = self.operations.lock().await;
        let storages: Vec<Arc<SqliteSessionStorage>> =
            self.active_storages.lock().expect("active storages mutex").clone();
        let mut first_error = None;
        for storage in storages {
            if let Err(error) = storage.release().await
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        let database = self.database.lock().expect("database mutex").take();
        if let Some(database) = database {
            database.close();
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn read_source_metadata(
        &self,
        db: &Database,
        path: &str,
        source_id: &str,
    ) -> Result<SqliteSessionMetadata, SessionError> {
        db.with_connection(|connection| {
            let row = require_session_row(connection, source_id)?;
            decode_session_metadata(&row, path)
        })
        .map_err(SqliteError::into_session)
    }

    async fn get_database_path(&self) -> Result<String, SessionError> {
        if let Some(path) = self.database_path.lock().expect("database path mutex").clone() {
            return Ok(path);
        }
        let path = result_or_throw(
            self.env.absolute_path(&self.database_path_input, None).await,
            &format!(
                "Failed to resolve SQLite sessions database {}",
                self.database_path_input
            ),
        )?;
        *self.database_path.lock().expect("database path mutex") = Some(path.clone());
        Ok(path)
    }

    async fn get_database(&self) -> Result<Arc<Database>, SessionError> {
        if let Some(database) = self.database.lock().expect("database mutex").clone() {
            return Ok(database);
        }
        let path = self.get_database_path().await?;
        let directory = get_parent_path(&path);
        result_or_throw(
            self.env
                .create_dir(&directory, CreateDirOptions { recursive: Some(true) })
                .await,
            &format!("Failed to create SQLite sessions directory {path}"),
        )?;
        let connection = open_database(&path).map_err(SqliteError::into_session)?;
        configure_sqlite_database(&connection)
            .and_then(|()| apply_migrations(&connection))
            .map_err(SqliteError::into_session)?;
        let database = Arc::new(Database::new(connection));
        *self.database.lock().expect("database mutex") = Some(Arc::clone(&database));
        Ok(database)
    }
}

fn insert_new_session(
    connection: &rusqlite::Connection,
    options: &SqliteSessionCreateOptions,
    id: &str,
    created_at: i64,
    lease_options: ResolvedWriterLeaseOptions,
) -> Result<WriterLease, SqliteError> {
    transaction(connection, |connection| {
        insert_session_row(
            connection,
            &NewSessionRow {
                id: id.to_owned(),
                created_at,
                cwd: options.cwd.clone(),
                parent_session_id: options.parent_session_id.clone(),
                metadata: options.metadata.clone(),
            },
        )?;
        create_sequence(connection, id, 1)?;
        create_stats(connection, id, 0)?;
        create_initial_lane(connection, id, "main", None)?;
        claim_writer_lease(connection, id, lease_options, created_at)
    })
}

fn require_session_metadata(db: &Database, path: &str, id: &str) -> Result<SqliteSessionMetadata, SessionError> {
    db.with_connection(|connection| {
        let row = require_session_row(connection, id)?;
        decode_session_metadata(&row, path)
    })
    .map_err(SqliteError::into_session)
}

fn ensure_session_id_unused(db: &Database, id: &str) -> Result<(), SessionError> {
    db.with_connection(|connection| {
        if session_exists(connection, id)? {
            return Err(SqliteError::Session(SessionError::new(
                SessionErrorCode::AlreadyExists,
                format!("Session already exists: {id}"),
            )));
        }
        Ok(())
    })
    .map_err(SqliteError::into_session)
}
