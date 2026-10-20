#[path = "../../pi-coding-agent/tests/support/oracle.rs"]
mod oracle;

use pi_agent_core::harness::session::InMemorySessionRepo;
use pi_agent_core::harness::session::testing::{
    ConformanceResult, SessionBackendFixture, create_session_backend_conformance,
};
use serde_json::json;

struct Fixture(InMemorySessionRepo);
impl SessionBackendFixture for Fixture {
    type Repository = InMemorySessionRepo;
    fn repository(&self) -> &Self::Repository {
        &self.0
    }
    async fn dispose(&mut self) -> ConformanceResult<()> {
        for metadata in self.0.list().await? {
            self.0.delete(&metadata).await?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn pinned_public_factory_manifest_and_every_applicable_case_match() {
    let mut oracle = oracle::TsOracle::spawn(
        "session-conformance-oracle.ts",
        "packages/agent/src/harness/session/testing/index.ts",
    );
    let cases = create_session_backend_conformance(|| async { Ok(Fixture(InMemorySessionRepo::new())) });
    let mut manifest: Vec<_> = cases
        .iter()
        .map(|case| json!({"group":case.group,"name":case.name}))
        .collect();
    // Compare the complete source manifest. These exact two payload domains have no Rust values.
    manifest.insert(
        21,
        json!({"group":"validation and immutability", "name":"rejects non-JSON entries before storage mutation"}),
    );
    manifest.insert(
        22,
        json!({"group":"validation and immutability", "name":"rejects non-JSON records before storage mutation"}),
    );
    assert_eq!(
        oracle.request(&json!({"op":"manifest"})),
        json!({"cases":manifest,"created":0,"disposed":0})
    );
    for (native_index, case) in cases.iter().enumerate() {
        let source_index = if native_index >= 21 {
            native_index + 2
        } else {
            native_index
        };
        case.run().await.unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert_eq!(
            oracle.request(&json!({"op":"run","index":source_index})),
            json!({
                "group":case.group,"name":case.name,"created":1,"disposed":1,"success":true
            })
        );
    }
    for index in [21, 22] {
        assert_eq!(
            oracle.request(&json!({"op":"run","index":index})),
            json!({
                "group":manifest[index]["group"],"name":manifest[index]["name"],"created":1,"disposed":1,"success":true
            })
        );
    }
}
