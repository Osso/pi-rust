#[path = "../../pi-coding-agent/tests/support/oracle.rs"]
mod oracle;
use pi_agent_core::agent::{Agent, AgentEvent, AgentInitialState, AgentMessage, AgentOptions};
use pi_ai::{AssistantMessage, AssistantMessageEvent, DoneReason, UserContent, UserMessage, UserRole};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::{Arc, Mutex};

fn user(text: &str) -> AgentMessage {
    UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text(text.into()),
        input_source: None,
        provider_native: None,
        timestamp: 0.0,
    }
    .into()
}
fn include_restriction_message(message: &AgentMessage) -> bool {
    match message {
        AgentMessage::Custom(object) => {
            !(object
                .get(&JsString::from("role"))
                .and_then(JsValue::as_string)
                .is_some_and(|role| role == "custom")
                && object
                    .get(&JsString::from("customType"))
                    .and_then(JsValue::as_string)
                    .is_some_and(|kind| kind == "supervisor-status"))
        }
        _ => true,
    }
}

#[test]
fn reference_identity_matches_frozen_same_value_replacement_and_preserved_prefixes() {
    let mut oracle = oracle::TsOracle::spawn("agent-identity-oracle.ts", "packages/agent-core/src/agent.ts");
    for operations in [
        vec![
            "noop",
            "systemPrompt",
            "tools",
            "copyArray",
            "appendStatus",
            "append",
            "truncate",
            "replaceMessages",
            "replaceModel",
        ],
        vec!["reset", "appendOriginalCopy"],
        vec!["replaceModel", "replaceMessages"],
        vec!["append", "truncate", "append", "truncate"],
    ] {
        let expected = oracle.request(&json!({"id":0,"operations":operations}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let agent = Agent::new(AgentOptions {
            initial_state: Some(AgentInitialState {
                messages: Some(vec![user("original")]),
                ..Default::default()
            }),
            ..Default::default()
        })
        .unwrap();
        let baseline = agent.identity_snapshot(include_restriction_message);
        let mut observations = Vec::new();
        for operation in operations {
            match operation {
                "append" => {
                    agent.append_message(user("extra"));
                }
                "appendStatus" => {
                    agent.append_message(AgentMessage::Custom(json!({"role":"custom","customType":"supervisor-status","content":"status","display":true,"timestamp":0}).as_object().unwrap().clone()));
                }
                "truncate" => agent.truncate_messages(1),
                "copyArray" => agent.truncate_messages(agent.messages().len()),
                "replaceMessages" => agent.set_messages(agent.messages()),
                "replaceModel" => agent.set_model(agent.model()),
                "systemPrompt" => agent.set_system_prompt("changed"),
                "tools" => agent.set_tools(vec![]),
                "reset" => agent.reset(),
                "appendOriginalCopy" => agent.append_message(user("original")),
                "noop" => {}
                _ => unreachable!(),
            }
            let current = agent.identity_snapshot(include_restriction_message);
            let model_same = baseline.has_same_model(&current);
            let count = current.message_tokens().len();
            let prefix_same = baseline
                .message_tokens()
                .iter()
                .enumerate()
                .all(|(index, token)| current.message_tokens().get(index) == Some(token));
            observations.push(json!({"modelSame":model_same,"count":count,"prefixSame":prefix_same,"matches":model_same && count==baseline.message_tokens().len() && prefix_same}));
        }
        assert_eq!(JsValue::Array(observations), expected["output"]);
    }
}
#[tokio::test]
async fn earlier_listener_append_does_not_change_dispatched_message_token() {
    let mut oracle = oracle::TsOracle::spawn("agent-identity-oracle.ts", "packages/agent-core/src/agent.ts");
    let expected = oracle.request(&json!({"id":0,"op":"dispatch"}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let agent = Agent::new(AgentOptions {
        stream_fn: Some(Arc::new(|_, _, _| {
            Box::pin(async {
                let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
                sender.push(AssistantMessageEvent::Done {
                    reason: DoneReason::Stop,
                    message: AssistantMessage::empty("unknown", "unknown", "unknown", 0.0),
                });
                Ok(stream)
            })
        })),
        ..Default::default()
    })
    .unwrap();
    assert!(agent.current_message_end_identity().is_err());
    let earlier = agent.clone();
    agent.subscribe(Arc::new(move |event, _| {
        let agent = earlier.clone();
        let emitted = if matches!(
            event,
            AgentEvent::MessageEnd {
                message: AgentMessage::User(_)
            }
        ) {
            Some(agent.current_message_end_identity().unwrap())
        } else {
            None
        };
        Box::pin(async move {
            if let Some(emitted) = emitted {
                assert_eq!(
                    agent.identity_snapshot(|_| true).message_tokens().last(),
                    Some(&emitted)
                );
                agent.append_message(user("inserted by earlier listener"));
            }
            Ok(())
        })
    }));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let subscriber = agent.clone();
    agent.subscribe(Arc::new(move |event,_| {let agent=subscriber.clone();let seen=record.clone();Box::pin(async move {
        if matches!(event,AgentEvent::MessageEnd {message:AgentMessage::User(_)}) {
            let token=agent.current_message_end_identity().unwrap();
            assert!(Agent::new(Default::default()).unwrap().current_message_end_identity().is_err(),"another Agent is outside this dispatch");
            let outside=agent.clone();
            assert!(tokio::spawn(async move {outside.current_message_end_identity()}).await.unwrap().is_err(),"a different task is outside listener dispatch even while it is in progress");
            let current=agent.identity_snapshot(|_| true);
            seen.lock().unwrap().push(json!({"eventStored":current.message_tokens().contains(&token),"eventIsLast":current.message_tokens().last()==Some(&token),"count":current.message_tokens().len()}));
        }
        Ok(())
    })}));
    agent.prompt("prompt").await.unwrap();
    assert!(agent.current_message_end_identity().is_err());
    assert_eq!(JsValue::Array(seen.lock().unwrap().clone()), expected["output"]);
}
