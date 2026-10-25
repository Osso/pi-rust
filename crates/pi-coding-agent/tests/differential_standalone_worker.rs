use crate::shared_oracle as oracle;
use pi_coding_agent::core::agent_session::{
    MultiAgentRuntimeRole, StandaloneWorkerOptions, assert_standalone_worker_options,
};
use pi_coding_agent::core::tool_capabilities::STANDALONE_DISABLED_TOOL_NAMES;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};

#[test]
fn standalone_partial_config_validation_and_capability_order_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn(
        "standalone-worker-oracle.ts",
        "packages/coding-agent/src/core/agent-session.ts",
    );
    let mut cases = vec![json!({}), json!({"persisted":true})];
    for field in [
        "multiAgentStore",
        "multiAgentExecutionCapability",
        "multiAgentRequiresAgentId",
    ] {
        let mut options = json!({});
        options[field] = json!(true);
        cases.push(json!({"options":options}));
    }
    for field in ["multiAgentAgentId", "multiAgentParentSessionId"] {
        for value in ["", "child"] {
            let mut options = json!({});
            options[field] = json!(value);
            cases.push(json!({"options":options}));
        }
    }
    for role in ["standalone", "orchestrator", "child", "observer"] {
        cases.push(json!({"options":{"multiAgentRuntimeRole":role}}));
    }
    cases.push(json!({"persisted":true,"options":{"multiAgentStore":true}}));
    for no_supervisor in [None, Some(false), Some(true)] {
        for (index, case) in cases.iter().enumerate() {
            let mut request = case.clone();
            request["id"] = json!(index);
            if request["options"].is_null() {
                request["options"] = json!({});
            }
            if let Some(no_supervisor) = no_supervisor {
                request["options"]["noSupervisor"] = json!(no_supervisor);
            }
            let expected = oracle.request(&request);
            let options = &request["options"];
            let role =
                options["multiAgentRuntimeRole"]
                    .as_string()
                    .map(|role| match role.to_utf8().unwrap().as_str() {
                        "standalone" => MultiAgentRuntimeRole::Standalone,
                        "orchestrator" => MultiAgentRuntimeRole::Orchestrator,
                        "child" => MultiAgentRuntimeRole::Child,
                        "observer" => MultiAgentRuntimeRole::Observer,
                        _ => unreachable!(),
                    });
            let native = StandaloneWorkerOptions {
                no_supervisor: no_supervisor.unwrap_or(false),
                session_is_persisted: request["persisted"] == JsValue::Bool(true),
                has_multi_agent_store: options["multiAgentStore"] == JsValue::Bool(true),
                multi_agent_runtime_role: role,
                has_execution_capability: options["multiAgentExecutionCapability"] == JsValue::Bool(true),
                multi_agent_agent_id: options["multiAgentAgentId"].as_string(),
                multi_agent_parent_session_id: options["multiAgentParentSessionId"].as_string(),
                multi_agent_requires_agent_id: options["multiAgentRequiresAgentId"] == JsValue::Bool(true),
            };
            match assert_standalone_worker_options(&native) {
                Ok(()) => {
                    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
                    assert_eq!(
                        expected["output"]["disabled"],
                        JsValue::Array(
                            STANDALONE_DISABLED_TOOL_NAMES
                                .iter()
                                .map(|name| JsValue::String(JsString::from(*name)))
                                .collect()
                        )
                    );
                }
                Err(error) => {
                    assert_eq!(expected["ok"], JsValue::Bool(false));
                    assert_eq!(&expected["error"]["message"], &JsValue::String(error.message().clone()));
                }
            }
        }
    }
}
