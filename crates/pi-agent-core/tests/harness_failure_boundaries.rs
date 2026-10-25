#[path = "support/session_fault_fs.rs"]
mod fault_fs;
use crate::shared_oracle as oracle;
use fault_fs::{FaultFs, FaultRule, fault_message};
use pi_agent_core::AgentError;
use pi_agent_core::harness::types::NodeRejection;
use pi_agent_core::harness::{session::*, *};
use pi_ai::models::{CreateModelsOptions, MutableModels, create_models};
use pi_ai::providers::faux::{RegisterFauxProviderOptions, faux_provider};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue},
};
use std::error::Error;
use std::sync::Arc;

fn entries() -> Vec<JsValue> {
    [ ("u", None, "first"), ("a", Some("u"), "answer"), ("v", Some("a"), "second"), ("b", Some("v"), "answer") ]
        .into_iter().map(|(id, parent, text)| json!({"type":"message","id":id,"parentId":parent,"timestamp":"now","message":{"role":"user","content":text,"timestamp":0}})).collect()
}
fn error_value(error: &(dyn Error + 'static)) -> JsValue {
    if let Some(error) = error.downcast_ref::<NodeRejection>() {
        return error.error.value();
    }
    let mut record = if let Some(error) = error.downcast_ref::<AgentHarnessError>() {
        let code = match error.code {
            AgentHarnessErrorCode::Unknown => "unknown",
            AgentHarnessErrorCode::Auth => "auth",
            AgentHarnessErrorCode::Hook => "hook",
            AgentHarnessErrorCode::Session => "session",
            AgentHarnessErrorCode::Compaction => "compaction",
            AgentHarnessErrorCode::BranchSummary => "branch_summary",
            _ => panic!("unexpected code"),
        };
        json!({"name":error.name,"code":code,"message":error.message})
    } else if let Some(error) = error.downcast_ref::<HarnessAggregateError>() {
        json!({"name":"AggregateError","message":error.message,"errors":error.errors.iter().map(|error|error_value(error.as_ref())).collect::<Vec<_>>()})
    } else if let Some(error) = error.downcast_ref::<SessionError>() {
        json!({"name":"SessionError","code":error.code.as_str(),"message":error.message})
    } else if let Some(error) = error.downcast_ref::<FileError>() {
        json!({"name":error.name,"code":error.code.as_str(),"message":error.message})
    } else if let Some(error) = error.downcast_ref::<AgentError>() {
        json!({"name":"Error","message":error.message()})
    } else {
        panic!("unexpected error type: {error:?}")
    };
    if let Some(cause) = error.source() {
        record["cause"] = error_value(cause);
    }
    record
}
async fn run_case(action: &str, rejected: bool, root: &std::path::Path) -> JsValue {
    let fs = Arc::new(FaultFs::new(root.to_str().unwrap().into()));
    let storage = Arc::new(
        JsonlSessionStorage::create(
            fs.clone(),
            root.join("native.jsonl").to_str().unwrap().into(),
            JsonlSessionStorageCreateOptions {
                cwd: "/fixture".into(),
                session_id: "fixture".into(),
                parent_session_path: None,
            },
        )
        .await
        .unwrap(),
    );
    for entry in entries() {
        storage
            .append_entry(SessionTreeEntry::from_value(entry).unwrap())
            .await
            .unwrap();
    }
    let models = create_models(CreateModelsOptions::default());
    let faux = faux_provider(RegisterFauxProviderOptions {
        api: Some("fixture-api".into()),
        provider: Some("fixture".into()),
        ..Default::default()
    });
    models.set_provider(faux.provider.clone());
    let harness = AgentHarness::new(AgentHarnessOptions::new(
        Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
            cwd: root.to_str().unwrap().into(),
            ..Default::default()
        })),
        Session::new(storage),
        Arc::new(models),
        faux.get_model(None).unwrap(),
    ))
    .unwrap();
    if action == "aggregate" {
        let _unsubscribe_request = harness.on(
            HarnessHookKind::BeforeProviderRequest,
            Arc::new(|_| Box::pin(async { Err(AgentHarnessError::new(AgentHarnessErrorCode::Auth, fault_message())) })),
        );
        let _unsubscribe_events = harness.subscribe(Arc::new(|event, _| {
            Box::pin(async move {
                if matches!(
                    event,
                    HarnessEvent::Agent(pi_agent_core::AgentEvent::MessageStart {
                        message: pi_agent_core::AgentMessage::Assistant(_)
                    })
                ) {
                    Err(AgentHarnessError::new(AgentHarnessErrorCode::Hook, "report"))
                } else {
                    Ok(())
                }
            })
        }));
    } else {
        fs.arm(Some(FaultRule {
            method: "appendFile",
            nth: 1,
            after: false,
            rejected,
            code: FileErrorCode::PermissionDenied,
        }));
        let _unsubscribe_compact = harness.on(
            HarnessHookKind::SessionBeforeCompact,
            Arc::new(|_| {
                Box::pin(async {
                    Ok(Some(HarnessHookResult::SessionBeforeCompact {
                        cancel: false,
                        compaction: Some(HarnessCompactResult {
                            summary: "provided".into(),
                            first_kept_entry_id: "v".into(),
                            tokens_before: 12.,
                            details: None,
                            duration_ms: None,
                        }),
                    }))
                })
            }),
        );
    }
    let result = match action {
        "compact" => harness.compact(None).await.map(|_| ()),
        "navigate" => harness.navigate_tree(JsString::from("a"), None).await.map(|_| ()),
        "aggregate" => harness.prompt("prompt".into(), None).await.map(|_| ()),
        _ => panic!("unknown action"),
    };
    match result {
        Ok(()) => json!({"ok":true}),
        Err(error) => json!({"ok":false,"error":error_value(&error)}),
    }
}
#[tokio::test]
async fn harness_preserves_native_failure_fallbacks_and_original_aggregate_causes() {
    let mut oracle = oracle::TsOracle::spawn(
        "harness-failure-oracle.ts",
        "packages/agent-core/src/harness/agent-harness.ts",
    );
    let mut failures = Vec::new();
    for (action, rejected) in [
        ("compact", false),
        ("compact", true),
        ("navigate", false),
        ("navigate", true),
        ("aggregate", false),
    ] {
        let root = tempfile::tempdir().unwrap();
        let expected = oracle.request(&json!({"root":root.path(),"action":action,"entries":entries(),"message":fault_message(),"rejected":rejected}));
        let actual = run_case(action, rejected, root.path()).await;
        if actual != expected {
            failures.push(format!(
                "action={action} rejected={rejected}\nactual={}\nexpected={}",
                json::stringify_lossless(&actual),
                json::stringify_lossless(&expected)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
