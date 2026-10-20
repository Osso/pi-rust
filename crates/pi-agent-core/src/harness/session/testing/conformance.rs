//! Native runner-neutral conformance factory and resource scope.
use super::{cases, types::*};
use crate::harness::session::{
    ForkOptions, InMemorySessionRepo, Session, SessionCreateOptions, SessionError, SessionMetadata,
};
use pi_ai::request::BoxFuture;
use std::{
    future::{Future, poll_fn},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    task::Poll,
};

/// Returns the 28 representable source cases in source order. The two non-JSON
/// payload cases are typed exclusions, not successful no-op tests.
pub fn create_session_backend_conformance<F: SessionBackendFixtureFactory>(
    factory: F,
) -> Vec<SessionBackendConformanceCase<F>> {
    let factory = Arc::new(factory);
    vec![
        SessionBackendConformanceCase {
            group: "entries and lanes",
            name: "assigns parents and one sequence across every mutation",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::assigns_parents_and_one_sequence_across_every_mutation(repo)),
        },
        SessionBackendConformanceCase {
            group: "records and log",
            name: "commits records and lane moves as separate mutations",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::commits_records_and_lane_moves_as_separate_mutations(repo)),
        },
        SessionBackendConformanceCase {
            group: "entries and lanes",
            name: "rejects duplicate ids without changing state",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::rejects_duplicate_ids_without_changing_state(repo)),
        },
        SessionBackendConformanceCase {
            group: "entries and lanes",
            name: "isolates lanes while sharing the tree",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::isolates_lanes_while_sharing_the_tree(repo)),
        },
        SessionBackendConformanceCase {
            group: "queries and facts",
            name: "rejects invalid queries before empty reads",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::rejects_invalid_queries_before_empty_reads(repo)),
        },
        SessionBackendConformanceCase {
            group: "queries and facts",
            name: "supports bounded filtered and cursor-based queries",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::supports_bounded_filtered_and_cursor_based_queries(repo)),
        },
        SessionBackendConformanceCase {
            group: "records and log",
            name: "keeps lane names permanent with their recovery records",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::keeps_lane_names_permanent_with_their_recovery_records(repo)),
        },
        SessionBackendConformanceCase {
            group: "records and log",
            name: "persists queue cancellation without consuming its target",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::persists_queue_cancellation_without_consuming_its_target(repo)),
        },
        SessionBackendConformanceCase {
            group: "records and log",
            name: "filters records by lane type run sequence and order",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::filters_records_by_lane_type_run_sequence_and_order(repo)),
        },
        SessionBackendConformanceCase {
            group: "records and log",
            name: "filters operation starts by operation kind",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::filters_operation_starts_by_operation_kind(repo)),
        },
        SessionBackendConformanceCase {
            group: "records and log",
            name: "tracks and enforces one open operation per lane",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::tracks_and_enforces_one_open_operation_per_lane(repo)),
        },
        SessionBackendConformanceCase {
            group: "records and log",
            name: "does not let an earlier finish close a later start",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::does_not_let_an_earlier_finish_close_a_later_start(repo)),
        },
        SessionBackendConformanceCase {
            group: "records and log",
            name: "scopes open operations by lane and limit",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::scopes_open_operations_by_lane_and_limit(repo)),
        },
        SessionBackendConformanceCase {
            group: "validation and immutability",
            name: "returns immutable open-operation records",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::returns_immutable_open_operation_records(repo)),
        },
        SessionBackendConformanceCase {
            group: "queries and facts",
            name: "keeps latest-value facts and computes ledger statistics across lanes",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::keeps_latest_value_facts_and_computes_ledger_statistics_across_lanes(repo)),
        },
        SessionBackendConformanceCase {
            group: "queries and facts",
            name: "clears session names durably",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::clears_session_names_durably(repo)),
        },
        SessionBackendConformanceCase {
            group: "validation and immutability",
            name: "returns immutable copies from reads",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::returns_immutable_copies_from_reads(repo)),
        },
        SessionBackendConformanceCase {
            group: "entries and lanes",
            name: "validates lane lifecycle and targets",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::validates_lane_lifecycle_and_targets(repo)),
        },
        SessionBackendConformanceCase {
            group: "entries and lanes",
            name: "binds lane views without caching leaves",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::binds_lane_views_without_caching_leaves(repo)),
        },
        SessionBackendConformanceCase {
            group: "entries and lanes",
            name: "appends provisioned entries with their existing ids",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::appends_provisioned_entries_with_their_existing_ids(repo)),
        },
        SessionBackendConformanceCase {
            group: "entries and lanes",
            name: "persists tool-result termination decisions",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::persists_tool_result_termination_decisions(repo)),
        },
        SessionBackendConformanceCase {
            group: "entries and lanes",
            name: "linearizes concurrent writes across two lanes",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::linearizes_concurrent_writes_across_two_lanes(repo)),
        },
        SessionBackendConformanceCase {
            group: "repository and forks",
            name: "creates lists and opens sessions",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::creates_lists_and_opens_sessions(repo)),
        },
        SessionBackendConformanceCase {
            group: "repository and forks",
            name: "deletes sessions idempotently",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::deletes_sessions_idempotently(repo)),
        },
        SessionBackendConformanceCase {
            group: "repository and forks",
            name: "forks one branch with selected facts and no records",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::forks_one_branch_with_selected_facts_and_no_records(repo)),
        },
        SessionBackendConformanceCase {
            group: "repository and forks",
            name: "forks a complete tree with lanes and facts",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::forks_a_complete_tree_with_lanes_and_facts(repo)),
        },
        SessionBackendConformanceCase {
            group: "repository and forks",
            name: "forks before an entry without modifying the source",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::forks_before_an_entry_without_modifying_the_source(repo)),
        },
        SessionBackendConformanceCase {
            group: "repository and forks",
            name: "validates the default fork target",
            factory: Arc::clone(&factory),
            test: |repo| Box::pin(cases::validates_the_default_fork_target(repo)),
        },
    ]
}

impl<F: SessionBackendFixtureFactory> SessionBackendConformanceCase<F> {
    pub async fn run(&self) -> ConformanceResult<()> {
        let mut fixture = self.factory.create().await?;
        let result = catch_assertion((self.test)(fixture.repository())).await;
        let disposal = fixture.dispose().await;
        match (result, disposal) {
            (Ok(()), disposal) => disposal,
            (Err(error), Ok(())) => Err(error),
            (Err(suppressed), Err(error)) => Err(Box::new(SuppressedConformanceError { error, suppressed })),
        }
    }
}

/// Source assertions throw; native assertions unwind. Catch each poll without
/// spawning a runtime task so cleanup executes before the failure is returned.
async fn catch_assertion(future: impl Future<Output = ConformanceResult<()>>) -> ConformanceResult<()> {
    let mut future = std::pin::pin!(future);
    poll_fn(|cx| match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(cx))) {
        Ok(Poll::Ready(result)) => Poll::Ready(result),
        Ok(Poll::Pending) => Poll::Pending,
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_else(|| "non-string assertion panic".to_owned());
            Poll::Ready(Err(std::io::Error::other(message).into()))
        }
    })
    .await
}

impl SessionBackendRepository for InMemorySessionRepo {
    type Metadata = SessionMetadata;
    fn create(&self, options: SessionCreateOptions) -> BoxFuture<'_, Result<Session<SessionMetadata>, SessionError>> {
        Box::pin(self.create(options))
    }
    fn open<'a>(
        &'a self,
        metadata: &'a SessionMetadata,
    ) -> BoxFuture<'a, Result<Session<SessionMetadata>, SessionError>> {
        Box::pin(self.open(metadata))
    }
    fn list(&self) -> BoxFuture<'_, Result<Vec<SessionMetadata>, SessionError>> {
        Box::pin(self.list())
    }
    fn delete<'a>(&'a self, metadata: &'a SessionMetadata) -> BoxFuture<'a, Result<(), SessionError>> {
        Box::pin(self.delete(metadata))
    }
    fn fork<'a>(
        &'a self,
        source: &'a SessionMetadata,
        options: ForkOptions,
        create: SessionCreateOptions,
    ) -> BoxFuture<'a, Result<Session<SessionMetadata>, SessionError>> {
        Box::pin(self.fork(source, options, create))
    }
}
