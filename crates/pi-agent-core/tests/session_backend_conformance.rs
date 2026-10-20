use pi_agent_core::harness::session::testing::{
    ConformanceResult, SessionBackendFixture, SuppressedConformanceError, create_session_backend_conformance,
};
use pi_agent_core::harness::session::{InMemorySessionRepo, SessionCreateOptions};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Fixture {
    repo: InMemorySessionRepo,
    disposed: Arc<AtomicUsize>,
    fail_disposal: bool,
}
impl SessionBackendFixture for Fixture {
    type Repository = InMemorySessionRepo;
    fn repository(&self) -> &Self::Repository {
        &self.repo
    }
    async fn dispose(&mut self) -> ConformanceResult<()> {
        for metadata in self.repo.list().await? {
            self.repo.delete(&metadata).await?;
        }
        self.disposed.fetch_add(1, Ordering::SeqCst);
        if self.fail_disposal {
            return Err(std::io::Error::other("dispose failed").into());
        }
        Ok(())
    }
}

#[tokio::test]
async fn downstream_factory_runs_every_applicable_case_with_fresh_owned_fixtures() {
    let created = Arc::new(AtomicUsize::new(0));
    let disposed = Arc::new(AtomicUsize::new(0));
    let cases = create_session_backend_conformance({
        let created = created.clone();
        let disposed = disposed.clone();
        move || {
            let created = created.clone();
            let disposed = disposed.clone();
            async move {
                created.fetch_add(1, Ordering::SeqCst);
                Ok(Fixture {
                    repo: InMemorySessionRepo::new(),
                    disposed,
                    fail_disposal: false,
                })
            }
        }
    });
    assert_eq!(cases.len(), 28);
    assert_eq!(created.load(Ordering::SeqCst), 0, "factory must be lazy");
    for case in &cases {
        for _ in 0..2 {
            case.run()
                .await
                .unwrap_or_else(|error| panic!("{} / {}: {error}", case.group, case.name));
        }
    }
    assert_eq!(created.load(Ordering::SeqCst), 56);
    assert_eq!(disposed.load(Ordering::SeqCst), 56);
}

#[tokio::test]
async fn factory_failure_is_returned_without_disposal() {
    let cases = create_session_backend_conformance(|| async {
        Err::<Fixture, _>(std::io::Error::other("fixture failed").into())
    });
    assert_eq!(cases[0].run().await.unwrap_err().to_string(), "fixture failed");
}

#[tokio::test]
async fn case_failure_still_disposes_and_cleanup_failure_preserves_both_errors() {
    for fail_disposal in [false, true] {
        let disposed = Arc::new(AtomicUsize::new(0));
        let cases = create_session_backend_conformance({
            let disposed = disposed.clone();
            move || {
                let disposed = disposed.clone();
                async move {
                    let repo = InMemorySessionRepo::new();
                    repo.create(SessionCreateOptions {
                        id: Some("session".into()),
                        ..Default::default()
                    })
                    .await?;
                    Ok(Fixture {
                        repo,
                        disposed,
                        fail_disposal,
                    })
                }
            }
        });
        let error = cases[0].run().await.unwrap_err();
        assert_eq!(disposed.load(Ordering::SeqCst), 1);
        if fail_disposal {
            let both = error
                .downcast_ref::<SuppressedConformanceError>()
                .expect("both failures retained");
            assert_eq!(both.error.to_string(), "dispose failed");
            assert!(both.suppressed.to_string().contains("Session already exists"));
            assert_eq!(
                both.suppressed
                    .downcast_ref::<pi_agent_core::harness::session::SessionError>()
                    .expect("backend error preserved")
                    .code(),
                pi_agent_core::harness::session::SessionErrorCode::AlreadyExists
            );
        } else {
            assert!(error.to_string().contains("Session already exists"));
            assert_eq!(
                error
                    .downcast_ref::<pi_agent_core::harness::session::SessionError>()
                    .expect("backend error preserved")
                    .code(),
                pi_agent_core::harness::session::SessionErrorCode::AlreadyExists
            );
        }
    }
}

#[tokio::test]
async fn assertion_failure_is_returned_after_disposal() {
    let disposed = Arc::new(AtomicUsize::new(0));
    let cases = create_session_backend_conformance({
        let disposed = disposed.clone();
        move || {
            let disposed = disposed.clone();
            async move {
                let repo = InMemorySessionRepo::new();
                repo.create(SessionCreateOptions {
                    id: Some("unexpected-extra-session".into()),
                    ..Default::default()
                })
                .await?;
                Ok(Fixture {
                    repo,
                    disposed,
                    fail_disposal: false,
                })
            }
        }
    });
    let case = cases
        .iter()
        .find(|case| case.name == "creates lists and opens sessions")
        .unwrap();
    let error = case.run().await.unwrap_err();
    assert!(error.to_string().contains("assertion"));
    assert_eq!(disposed.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn disposal_failure_after_success_is_returned() {
    let disposed = Arc::new(AtomicUsize::new(0));
    let cases = create_session_backend_conformance(move || {
        let disposed = disposed.clone();
        async move {
            Ok(Fixture {
                repo: InMemorySessionRepo::new(),
                disposed,
                fail_disposal: true,
            })
        }
    });
    assert_eq!(cases[0].run().await.unwrap_err().to_string(), "dispose failed");
}
