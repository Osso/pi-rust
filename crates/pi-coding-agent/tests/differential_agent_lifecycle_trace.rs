//! Frozen whole viewer/trace over the same isolated persisted DB and JSONL files.
use crate::shared_oracle as oracle;
use crate::shared_tool_context as tool_context;
use pi_coding_agent::bundled_extensions::agents_core::runtime::{
    MultiAgentExtensionOptions, register_agent_viewer_tools,
};
use pi_coding_agent::core::extensions::{create_extension_runtime, load_extension_from_factory};
use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::{js_json, js_json as json};
use std::sync::Arc;

#[tokio::test]
async fn persisted_viewer_trace_matches_frozen_descendants_transcripts_ownership_and_outbox() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().expect("utf-8 test path");
    let db = get_control_db_path(pi_js_compat::node_fs::decode_node_path((dir.path()).as_os_str()))
        .to_utf8()
        .expect("fixture control path is scalar");
    let parent = dir.path().join("parent.jsonl");
    let child = dir.path().join("child.jsonl");
    let time = "2026-10-21T00:00:00.000Z";
    let entries = [
        json!({"type":"session","version":3,"id":"store-session","timestamp":time,"cwd":root}),
        json!({"type":"custom","id":"start","parentId":null,"timestamp":time,"customType":"agent_start","data":{"agentId":"a","childSessionId":"child-session","lifecycle":"running","transcriptPath":child}}),
        json!({"type":"custom","id":"complete","parentId":"start","timestamp":time,"customType":"agent_complete","data":{"agentId":"a","lifecycle":"completed"}}),
    ];
    std::fs::write(
        &parent,
        entries
            .iter()
            .map(js_json::stringify_lossless)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();
    let entries = [
        json!({"type":"session","version":3,"id":"child-session","timestamp":time,"cwd":root}),
        json!({"type":"message","id":"end","parentId":null,"timestamp":time,"message":{"role":"toolResult","toolCallId":"end-call","toolName":"end_turn","content":[],"details":{"reason":"completed slice"},"isError":false,"timestamp":0}}),
        json!({"type":"message","id":"failed-end","parentId":"end","timestamp":time,"message":{"role":"toolResult","toolCallId":"failed-call","toolName":"end_turn","content":[],"isError":true,"timestamp":0}}),
    ];
    std::fs::write(
        &child,
        entries
            .iter()
            .map(js_json::stringify_lossless)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();
    write_session_metadata(&db,&js_json::from_value(&json!({"sessionPath":parent,"id":"store-session","cwd":root,"createdAt":time,"modifiedAt":time,"messageCount":0,"firstMessage":""})).unwrap()).unwrap();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    let mut agents = Vec::new();
    for (id, parent_id, detached) in [
        ("a", None, false),
        ("b", Some("a"), true),
        ("c", Some("b"), false),
        ("unrelated", None, false),
    ] {
        let mut agent = pi_coding_agent::bundled_extensions::agents_core::shared::empty_agent(id);
        agent.display_name = id.into();
        agent.agent_type = "implement".into();
        agent.parent_id = parent_id.map(JsString::from);
        agent.detached = Some(detached);
        agent.created_at = time.into();
        agent.updated_at = time.into();
        agent.revision = 7;
        if id == "a" {
            agent.transcript = Some(pi_coding_agent::core::multi_agent_store::AgentTranscriptMetadata {
                session_id: "child-session".into(),
                path: Some(child.to_str().expect("utf-8 test path").into()),
            });
        }
        let value = js_json::to_value(&agent).unwrap();
        bootstrap_multi_agent_agent(
            &db,
            parent.to_str().expect("utf-8 test path"),
            &JsString::from(id),
            &value,
        )
        .unwrap();
        store.publish_lifecycle_coordinator_snapshot(&agent).unwrap();
        agents.push(value);
    }
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute("INSERT INTO multi_agent_runtime_owners (session_path,agent_id,process_identity,owner_session_id,owner_agent_id) VALUES (?,'a',?,'owner-session',NULL)",rusqlite::params![parent.to_str().expect("utf-8 test path"),r#"{"pid":123,"startTimeTicks":456,"incarnation":"test"}"#]).unwrap();
    conn.execute("INSERT INTO multi_agent_terminal_outbox(session_path,agent_id,terminal_revision,event_kind,status,attempt_count,last_error,updated_at) VALUES (?,'b',7,'completed','pending',2,'delivery failed',?)",rusqlite::params![parent.to_str().expect("utf-8 test path"),time]).unwrap();
    let options = MultiAgentExtensionOptions {
        store: Some(store),
        ..Default::default()
    };
    let extension = load_extension_from_factory(
        Arc::new(move |pi| {
            let result = register_agent_viewer_tools(pi, &options);
            Box::pin(async move { result })
        }),
        &JsString::from(root),
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        create_extension_runtime(),
        &JsString::from("<viewer>"),
    )
    .await
    .unwrap();
    let tool = extension.read().tools.get("agent_viewer").unwrap().definition.clone();
    let context = tool_context::TestContextBuilder::new(root, tool_context::in_memory_session(root, "viewer-session"))
        .control_db_path(&db)
        .build()
        .await
        .1;
    let mut oracle = oracle::TsOracle::spawn("agent-lifecycle-viewer-oracle.ts", "packages/coding-agent");
    for params in [
        json!({"agentId":"a","trace":true}),
        json!({"agentId":"a","trace":true,"storeSessionId":"store-session"}),
        json!({"agentId":"a","trace":true,"storeSessionId":"store-session","sessionId":"wrong"}),
        json!({"agentId":"missing","trace":true,"storeSessionId":"store-session"}),
    ] {
        let expected = oracle.request(&json!({"agents":agents,"db":db,"params":params}));
        let result = (tool.execute)("view".into(), json!(params.clone()), None, None, Some(context.clone()))
            .await
            .unwrap();
        let actual = json!({"content":result.content,"details":result.details,"isError":result.is_error});
        assert_eq!(actual, expected, "{params:?}");
    }
    for ownership in [
        Some(r#"{"pid":100.0,"startTimeTicks":1e21}"#),
        Some(r#"{"pid":-0,"startTimeTicks":4.5}"#),
        Some(r#"{"pid":1e400,"startTimeTicks":1}"#),
        Some(r#"{"pid":1,"startTimeTicks":-1e400}"#),
        None,
    ] {
        if let Some(process) = ownership {
            conn.execute(
                "UPDATE multi_agent_runtime_owners SET process_identity = ? WHERE agent_id = 'a'",
                [process],
            )
            .unwrap();
        } else {
            conn.execute("DELETE FROM multi_agent_runtime_owners WHERE agent_id = 'a'", [])
                .unwrap();
        }
        let params = json!({"agentId":"a","trace":true,"storeSessionId":"store-session"});
        let expected = oracle.request(&json!({"agents":agents,"db":db,"params":params}));
        let result = (tool.execute)("view".into(), json!(params), None, None, Some(context.clone()))
            .await
            .unwrap();
        assert_eq!(
            json!({"content":result.content,"details":result.details,"isError":result.is_error}),
            expected,
            "ownership {ownership:?}"
        );
    }
}
