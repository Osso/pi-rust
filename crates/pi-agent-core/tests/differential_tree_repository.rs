use crate::shared_operation as operation;
use crate::shared_oracle as oracle;

use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::session::*;
use pi_js_compat::js_json::{self, JsString, JsValue, parse_lossless, to_value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const ISO: &str = "2023-11-14T22:13:20.000Z";
const SOURCE: &str = "packages/agent-core/src/harness/session/session.ts";

trait FixtureRepo {
    type Metadata: SessionMetadataValue + serde::Serialize;
    async fn create(&self, id: Option<&str>) -> Result<Session<Self::Metadata>, SessionError>;
    async fn list(&self, cwd: Option<&JsString>) -> Result<Vec<Self::Metadata>, SessionError>;
    async fn open(&self, metadata: &Self::Metadata) -> Result<Session<Self::Metadata>, SessionError>;
    async fn fork(
        &self,
        metadata: &Self::Metadata,
        options: SessionForkOptions,
    ) -> Result<Session<Self::Metadata>, SessionError>;
    async fn delete(&self, metadata: &Self::Metadata) -> Result<(), SessionError>;
}

impl FixtureRepo for InMemorySessionRepo {
    type Metadata = SessionMetadata;
    async fn create(&self, id: Option<&str>) -> Result<Session<Self::Metadata>, SessionError> {
        self.create(SessionCreateOptions {
            id: id.map(JsString::from),
        })
        .await
    }
    async fn list(&self, _: Option<&JsString>) -> Result<Vec<Self::Metadata>, SessionError> {
        self.list().await
    }
    async fn open(&self, metadata: &Self::Metadata) -> Result<Session<Self::Metadata>, SessionError> {
        self.open(metadata).await
    }
    async fn fork(
        &self,
        metadata: &Self::Metadata,
        options: SessionForkOptions,
    ) -> Result<Session<Self::Metadata>, SessionError> {
        self.fork(metadata, options).await
    }
    async fn delete(&self, metadata: &Self::Metadata) -> Result<(), SessionError> {
        self.delete(metadata).await
    }
}

struct JsonlFixture {
    repo: JsonlSessionRepo,
    cwd: JsString,
}
impl FixtureRepo for JsonlFixture {
    type Metadata = JsonlSessionMetadata;
    async fn create(&self, id: Option<&str>) -> Result<Session<Self::Metadata>, SessionError> {
        self.repo
            .create(JsonlSessionCreateOptions {
                id: id.map(JsString::from),
                cwd: self.cwd.clone(),
                parent_session_path: None,
            })
            .await
            .map_err(operation::returned_error)
    }
    async fn list(&self, cwd: Option<&JsString>) -> Result<Vec<Self::Metadata>, SessionError> {
        self.repo
            .list(JsonlSessionListOptions { cwd: cwd.cloned() })
            .await
            .map_err(operation::returned_error)
    }
    async fn open(&self, metadata: &Self::Metadata) -> Result<Session<Self::Metadata>, SessionError> {
        self.repo.open(metadata).await.map_err(operation::returned_error)
    }
    async fn fork(
        &self,
        metadata: &Self::Metadata,
        options: SessionForkOptions,
    ) -> Result<Session<Self::Metadata>, SessionError> {
        self.repo
            .fork(
                metadata,
                JsonlSessionForkOptions {
                    create: JsonlSessionCreateOptions {
                        cwd: self.cwd.clone(),
                        parent_session_path: None,
                        id: options.id,
                    },
                    entry_id: options.entry_id,
                    position: options.position,
                },
            )
            .await
            .map_err(operation::returned_error)
    }
    async fn delete(&self, metadata: &Self::Metadata) -> Result<(), SessionError> {
        self.repo.delete(metadata).await.map_err(operation::returned_error)
    }
}
fn error_value(error: SessionError) -> JsValue {
    pi_js_compat::js_json!({"name":"SessionError","code":error.code.as_str(),"message":error.message})
}
async fn snapshot<M: SessionMetadataValue + serde::Serialize>(session: &Session<M>) -> JsValue {
    pi_js_compat::js_json!({"metadata":to_value(&session.get_metadata().await.unwrap()).unwrap(),"entries":session.get_entries().await.unwrap(),"leaf":session.get_leaf_id().await.unwrap()})
}
fn fixture_entries() -> Vec<SessionTreeEntry> {
    let raw = parse_lossless(r#"[{"type":"message","id":"root","parentId":null,"timestamp":"t","message":{"role":"user","content":"root","timestamp":1}},{"type":"message","id":"child","parentId":"root","timestamp":"t","message":{"role":"user","content":"child","timestamp":2}},{"type":"custom","id":"custom","parentId":"child","timestamp":"t","customType":"marker","data":{"raw":"\ud800"}}]"#).unwrap();
    raw.as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(|entry| SessionTreeEntry::from_value(entry).unwrap())
        .collect()
}
async fn exercise(repo: &impl FixtureRepo, cwd: &JsString, automatic_ids: bool) -> JsValue {
    let empty = repo.list(None).await.unwrap();
    let source = repo.create((!automatic_ids).then_some("source")).await.unwrap();
    for entry in fixture_entries() {
        source.get_storage().append_entry(entry).await.unwrap();
    }
    let metadata = source.get_metadata().await.unwrap();
    let original = snapshot(&source).await;
    let mut forks = Vec::new();
    for (id, entry_id, position) in [
        ("all", None, ForkPosition::Before),
        ("before", Some("child"), ForkPosition::Before),
        ("at", Some("custom"), ForkPosition::At),
        ("invalid", Some("custom"), ForkPosition::Before),
        ("missing", Some("missing"), ForkPosition::At),
        ("empty", Some(""), ForkPosition::Before),
    ] {
        let result = repo
            .fork(
                &metadata,
                SessionForkOptions {
                    id: (!automatic_ids).then(|| id.into()),
                    entry_id: entry_id.map(JsString::from),
                    position,
                },
            )
            .await;
        forks.push(match result {
            Ok(session) => snapshot(&session).await,
            Err(error) => pi_js_compat::js_json!({"error":error_value(error)}),
        });
    }
    let reopened = snapshot(&repo.open(&metadata).await.unwrap()).await;
    let listed = repo.list(None).await.unwrap();
    let filtered = repo.list(Some(cwd)).await.unwrap();
    repo.delete(&metadata).await.unwrap();
    repo.delete(&metadata).await.unwrap();
    let missing = error_value(repo.open(&metadata).await.expect_err("deleted session must fail open"));
    let after_delete = repo.list(None).await.unwrap();
    let replacement = repo.create((!automatic_ids).then_some("all")).await.unwrap();
    let replaced = snapshot(&replacement).await;
    let after_replace = repo.list(None).await.unwrap();
    pi_js_compat::js_json!({"empty":empty,"original":original,"forks":forks,"reopened":reopened,"listed":listed,"filtered":filtered,"missing":missing,"afterDelete":after_delete,"replaced":replaced,"afterReplace":after_replace})
}

fn sample_ids() -> Vec<JsString> {
    let mut oracle = oracle::TsOracle::spawn("tree-session-oracle.ts", SOURCE);
    let wire = js_json::stringify_lossless(&pi_js_compat::js_json!({"kind":"uuidSamples","count":64}));
    let response = oracle.request(&serde_json::json!({"wire":wire}));
    let value = parse_lossless(response["wire"].as_str().unwrap()).unwrap();
    value["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_string().unwrap().clone())
        .collect()
}

#[tokio::test]
async fn memory_and_jsonl_repositories_match_frozen_orchestration() {
    let samples = sample_ids();
    for (mode, automatic_ids) in [("memory", false), ("jsonl", false), ("memory", true), ("jsonl", true)] {
        let root = tempfile::tempdir().unwrap();
        let root_string: JsString = root.path().to_str().unwrap().into();
        let cwd = root_string.clone() + "/project:a";
        let clock: Clock = Arc::new(|| ISO.into());
        let ids: IdGenerator = if automatic_ids {
            let queue = Arc::new(Mutex::new(VecDeque::from(samples.clone())));
            Arc::new(move || {
                queue
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("fixture has enough frozen UUID samples")
            })
        } else {
            Arc::new(|| panic!("all repository IDs are explicit in this fixture"))
        };
        let actual = if mode == "memory" {
            exercise(&InMemorySessionRepo::with_runtime(clock, ids), &cwd, automatic_ids).await
        } else {
            let repo = JsonlSessionRepo::with_runtime(
                JsonlSessionRepoOptions {
                    fs: Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
                        cwd: root_string.clone(),
                        ..Default::default()
                    })),
                    sessions_root: root_string.clone() + "/sessions",
                },
                clock,
                ids,
            );
            exercise(&JsonlFixture { repo, cwd: cwd.clone() }, &cwd, automatic_ids).await
        };
        // Both runtimes must see the same paths, but not each other's fixture files.
        std::fs::remove_dir_all(root.path()).unwrap();
        std::fs::create_dir(root.path()).unwrap();
        let mut ts = oracle::TsOracle::spawn("tree-session-oracle.ts", SOURCE);
        let request = pi_js_compat::js_json!({"kind":"repository","mode":mode,"root":root_string,"cwd":cwd,"entries":fixture_entries(),"autoIds":automatic_ids});
        let response = ts.request(&serde_json::json!({"wire":js_json::stringify_lossless(&request)}));
        let expected = parse_lossless(response["wire"].as_str().unwrap()).unwrap();
        let actual = pi_js_compat::js_json!({"value":actual});
        if actual != expected {
            eprintln!("ACTUAL={}", js_json::stringify_lossless(&actual));
            eprintln!("EXPECTED={}", js_json::stringify_lossless(&expected));
        }
        assert_eq!(actual, expected, "repository {mode} automatic_ids={automatic_ids}");
    }
}
