use crate::shared_oracle as oracle;
use pi_agent_core::harness::compaction::collect_entries_for_branch_summary;
use pi_agent_core::harness::session::*;
use pi_ai::request::BoxFuture;
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue},
};
use std::sync::Arc;

struct MissingEntryLookup(InMemorySessionStorage);
impl SessionStorage<SessionMetadata> for MissingEntryLookup {
    fn get_metadata(&self) -> BoxFuture<'_, Result<SessionMetadata, SessionError>> {
        self.0.get_metadata()
    }
    fn get_leaf_id(&self) -> BoxFuture<'_, Result<Option<JsString>, SessionError>> {
        self.0.get_leaf_id()
    }
    fn create_entry_id(&self) -> BoxFuture<'_, Result<JsString, SessionError>> {
        self.0.create_entry_id()
    }
    fn set_leaf_id<'a>(&'a self, id: Option<&'a JsString>) -> BoxFuture<'a, Result<(), SessionFailure>> {
        self.0.set_leaf_id(id)
    }
    fn append_entry(&self, entry: SessionTreeEntry) -> BoxFuture<'_, Result<(), SessionFailure>> {
        self.0.append_entry(entry)
    }
    fn get_entry<'a>(&'a self, _id: &'a JsString) -> BoxFuture<'a, Result<Option<SessionTreeEntry>, SessionError>> {
        Box::pin(async { Ok(None) })
    }
    fn find_entries<'a>(&'a self, kind: &'a JsString) -> BoxFuture<'a, Result<Vec<SessionTreeEntry>, SessionError>> {
        self.0.find_entries(kind)
    }
    fn get_label<'a>(&'a self, id: &'a JsString) -> BoxFuture<'a, Result<Option<JsString>, SessionError>> {
        self.0.get_label(id)
    }
    fn get_path_to_root<'a>(
        &'a self,
        id: Option<&'a JsString>,
    ) -> BoxFuture<'a, Result<Vec<SessionTreeEntry>, SessionError>> {
        self.0.get_path_to_root(id)
    }
    fn get_entries(&self) -> BoxFuture<'_, Result<Vec<SessionTreeEntry>, SessionError>> {
        self.0.get_entries()
    }
}
fn entry(id: &str, parent: Option<&str>) -> JsValue {
    json!({"type":"custom","id":id,"parentId":parent,"timestamp":"t","customType":"fixture"})
}

#[tokio::test]
async fn branch_paths_missing_corrupt_disconnected_and_disappearing_entries_match_frozen() {
    let base = vec![
        entry("root", None),
        entry("old", Some("root")),
        entry("target", Some("root")),
    ];
    let mut orphan = base.clone();
    orphan.push(entry("orphan", Some("ghost\u{d7ff}")));
    let disconnected = vec![
        entry("a", None),
        entry("old", Some("a")),
        entry("b", None),
        entry("target", Some("b")),
    ];
    let cases = [
        (base.clone(), None, "missing", false),
        (base.clone(), Some(""), "missing", false),
        (base.clone(), Some("missing"), "target", false),
        (base.clone(), Some("old"), "missing", false),
        (orphan.clone(), Some("orphan"), "target", false),
        (orphan, Some("old"), "orphan", false),
        (disconnected, Some("old"), "target", false),
        (base.clone(), Some("old"), "old", false),
        (base.clone(), Some("old"), "root", false),
        (base.clone(), Some("root"), "old", false),
        (base, Some("old"), "target", true),
    ];
    let mut oracle = oracle::TsOracle::spawn(
        "branch-summary-path-oracle.ts",
        "packages/agent-core/src/harness/compaction/branch-summarization.ts",
    );
    for (entries, old, target, missing_lookup) in cases {
        let input = json!({"entries":entries,"old":old,"target":target,"missingLookup":missing_lookup});
        let expected = oracle.request(&input);
        let entries = entries
            .into_iter()
            .map(|entry| SessionTreeEntry::from_value(entry).unwrap())
            .collect();
        let memory = InMemorySessionStorage::new(InMemorySessionStorageOptions {
            entries,
            metadata: Some(SessionMetadata {
                id: "fixture".into(),
                created_at: "t".into(),
            }),
        })
        .unwrap();
        let storage: Arc<dyn SessionStorage<SessionMetadata>> = if missing_lookup {
            Arc::new(MissingEntryLookup(memory))
        } else {
            Arc::new(memory)
        };
        let session = Session::new(storage);
        let old = old.map(JsString::from);
        let actual = match collect_entries_for_branch_summary(&session, old.as_ref(), &target.into()).await {
            Ok(result) => {
                json!({"ok":true,"value":{"entries":result.entries,"commonAncestorId":result.common_ancestor_id}})
            }
            Err(error) => {
                json!({"ok":false,"error":{"name":"SessionError","code":error.code.as_str(),"message":error.message}})
            }
        };
        assert_eq!(
            actual, expected,
            "old={old:?} target={target} missing_lookup={missing_lookup}"
        );
    }
}
