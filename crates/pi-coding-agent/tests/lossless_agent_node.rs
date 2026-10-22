//! Native AgentNode/store regression; unexecuted while library compilation is blocked.
use pi_coding_agent::core::multi_agent_store::{AgentNode, MultiAgentStore, MultiAgentStoreOptions};
use pi_js_compat::js_json;

#[test]
fn agent_node_and_store_round_trip_raw_type_policy_and_extra_keys() {
    let expected = js_json::parse_lossless(r#"{"id":"agent-\ud800","parentId":"parent-\udfff","displayName":"display-\ud801","agentType":"type-\ud802","lifecycle":"running","revision":1,"createdAt":"now","updatedAt":"now","cwd":"/fixture","permission":{"policy":"policy-\ud803","inheritedFrom":"parent-\udfff","narrowed":false},"extra-\ud800":{"value":"raw-\udfff"},"extra-�":"distinct","suppressTerminalNotification":true}"#).unwrap();
    let agent: AgentNode = js_json::from_value(&expected).unwrap();
    assert_eq!(js_json::to_value(&agent).unwrap(), expected);
    let store = MultiAgentStore::new(MultiAgentStoreOptions::default());
    store.publish_lifecycle_coordinator_snapshot(&agent).unwrap();
    let stored = store.get_agent(&agent.id).unwrap();
    assert_eq!(js_json::to_value(&stored).unwrap(), expected);
}
