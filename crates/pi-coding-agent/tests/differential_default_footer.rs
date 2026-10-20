//! Differential tests for the bundled `default-footer` extension.

#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use std::sync::{Arc, Mutex};

use bundled::{Harness, HarnessOptions};
use oracle::TsOracle;
use pi_agent_core::harness::ThinkingLevel;
use pi_ai::types::{Model, ModelCost, ModelCostRates};
use pi_coding_agent::bundled_extensions::default_footer::{
    DefaultFooterAgentLifecycleCounts, DefaultFooterComponentInput, DefaultFooterSelectedAgent,
    create_default_footer_component,
};
use pi_coding_agent::core::extensions::create_extension_runtime;
use pi_coding_agent::core::extensions::types::{ContextUsage, ExtensionUiComponent};
use pi_coding_agent::core::footer_data_provider::{
    ContextUsage as FooterContextUsage, FooterDataProvider, FooterSessionOverride,
};
use pi_coding_agent::core::multi_agent_store::AgentLifecycle;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::modes::interactive::theme::current_theme_if_initialized;
use serde_json::{Value, json};

const SOURCE: &str = "packages/coding-agent/extensions/default-footer/src/index.ts";

fn assistant_message(input: f64, output: f64, cost: f64) -> pi_agent_core::AgentMessage {
    let mut message = pi_ai::types::AssistantMessage::empty("openai-responses", "openai", "test-model", 1);
    message.usage.input = input;
    message.usage.output = output;
    message.usage.cost.total = cost;
    pi_agent_core::AgentMessage::Assistant(Box::new(message))
}

fn model(id: &str, provider: &str, context_window: f64, reasoning: bool) -> Model {
    Model {
        id: id.to_owned(),
        name: id.to_owned(),
        api: "openai-responses".to_owned(),
        provider: provider.to_owned(),
        base_url: "https://example.test".to_owned(),
        reasoning,
        thinking_level_map: None,
        input: vec![pi_ai::types::Modality::Text],
        cost: ModelCost {
            rates: ModelCostRates {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
            },
            tiers: None,
        },
        context_window,
        auto_compaction_threshold: None,
        max_tokens: 1000.0,
        sampling_params: None,
        headers: None,
        compat: None,
    }
}

fn git_fixture_dir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "pi-default-footer-diff-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(dir.join(".git")).expect("git dir");
    std::fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    dir
}

fn non_git_dir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "pi-default-footer-diff-nogit-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).expect("dir");
    dir
}

async fn rust_results(cases: &[Value]) -> Vec<Value> {
    let mut results = Vec::new();
    for case in cases {
        let usage = case.get("usage").cloned().unwrap_or_else(|| json!({}));
        let mut session = SessionManager::in_memory(Some("/tmp/project"), None).expect("session");
        session
            .append_message(assistant_message(
                usage.get("input").and_then(Value::as_f64).unwrap_or(0.0),
                usage.get("output").and_then(Value::as_f64).unwrap_or(0.0),
                usage.get("cost").and_then(Value::as_f64).unwrap_or(0.0),
            ))
            .expect("assistant");
        if let Some(name) = case.get("sessionName").and_then(Value::as_str) {
            session.set_session_name(Some(name)).expect("session name");
        }

        let context_usage = case.get("contextUsage").and_then(|usage| {
            Some(ContextUsage {
                tokens: usage.get("tokens").and_then(Value::as_f64),
                context_window: usage.get("contextWindow").and_then(Value::as_f64)?,
                percent: usage.get("percent").and_then(Value::as_f64),
            })
        });
        let model_value = case.get("model").and_then(|value| value.as_object()).map(|value| {
            model(
                value.get("id").and_then(Value::as_str).unwrap_or("test-model"),
                value.get("provider").and_then(Value::as_str).unwrap_or("test"),
                value.get("contextWindow").and_then(Value::as_f64).unwrap_or(272_000.0),
                value.get("reasoning").and_then(Value::as_bool).unwrap_or(false),
            )
        });
        let thinking_level = match case.get("thinkingLevel").and_then(Value::as_str) {
            Some("high") => ThinkingLevel::High,
            _ => ThinkingLevel::Off,
        };
        let runtime = create_extension_runtime();
        let mut options = HarnessOptions::new("/tmp/project");
        options.session_manager = Arc::new(Mutex::new(session));
        options.model = model_value;
        options.thinking_level = thinking_level;
        options.context_usage = context_usage;
        let harness = Harness::new(Vec::new(), runtime, options).await;

        let footer_data = Arc::new(FooterDataProvider::new(
            if case.get("gitBranch").and_then(Value::as_str).is_some() {
                git_fixture_dir()
            } else {
                non_git_dir()
            },
        ));
        footer_data.set_available_provider_count(
            case.get("availableProviderCount").and_then(Value::as_u64).unwrap_or(1) as usize,
        );
        if let Some(statuses) = case.get("extensionStatuses").and_then(Value::as_object) {
            for (key, text) in statuses {
                footer_data.set_extension_status(key, text.as_str());
            }
        }
        if let Some(override_value) = case.get("override") {
            let mut override_session = SessionManager::in_memory(Some("/tmp/child"), None).expect("child session");
            let override_usage = override_value.get("usage").cloned().unwrap_or_else(|| json!({}));
            override_session
                .append_message(assistant_message(
                    override_usage.get("input").and_then(Value::as_f64).unwrap_or(0.0),
                    override_usage.get("output").and_then(Value::as_f64).unwrap_or(0.0),
                    override_usage.get("cost").and_then(Value::as_f64).unwrap_or(0.0),
                ))
                .expect("child assistant");
            if let Some(name) = override_value.get("sessionName").and_then(Value::as_str) {
                override_session
                    .set_session_name(Some(name))
                    .expect("child session name");
            }
            let override_model = override_value.get("model").and_then(|value| {
                Some(model(
                    value.get("id").and_then(Value::as_str)?,
                    value.get("provider").and_then(Value::as_str)?,
                    value.get("contextWindow").and_then(Value::as_f64)?,
                    value.get("reasoning").and_then(Value::as_bool).unwrap_or(false),
                ))
            });
            let override_context = override_value.get("contextUsage").and_then(|usage| {
                Some(FooterContextUsage {
                    tokens: usage.get("tokens").and_then(Value::as_f64),
                    context_window: usage.get("contextWindow").and_then(Value::as_f64)?,
                    percent: usage.get("percent").and_then(Value::as_f64),
                })
            });
            footer_data.set_session_override(FooterSessionOverride {
                cwd: override_value
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or("/tmp/child")
                    .to_owned(),
                session_manager: Some(Arc::new(Mutex::new(override_session))),
                model: override_model,
                thinking_level: override_value
                    .get("thinkingLevel")
                    .and_then(Value::as_str)
                    .unwrap_or("off")
                    .to_owned(),
                context_usage: override_context,
            });
        }
        let counts = case.get("agentCounts").and_then(|counts| {
            Some(DefaultFooterAgentLifecycleCounts {
                running: counts.get("running").and_then(Value::as_i64)?,
                waiting_for_input: counts.get("waitingForInput").and_then(Value::as_i64)?,
                steering_pending: counts.get("steeringPending").and_then(Value::as_i64)?,
            })
        });
        let selected_agent = case.get("selectedAgent").and_then(|agent| {
            let lifecycle = match agent.get("lifecycle").and_then(Value::as_str)? {
                "running" => AgentLifecycle::Running,
                "waiting_for_input" => AgentLifecycle::WaitingForInput,
                "steering_pending" => AgentLifecycle::SteeringPending,
                "cancelling" => AgentLifecycle::Cancelling,
                "completed" => AgentLifecycle::Completed,
                "failed" => AgentLifecycle::Failed,
                _ => AgentLifecycle::Aborted,
            };
            Some(DefaultFooterSelectedAgent {
                display_name: agent.get("displayName").and_then(Value::as_str)?.to_owned(),
                lifecycle,
                slot_index: agent.get("slotIndex").and_then(Value::as_i64),
            })
        });
        let component = create_default_footer_component(DefaultFooterComponentInput {
            ctx: harness.context(),
            footer_data,
            get_agent_counts: Some(Arc::new(move || counts)),
            get_selected_agent: Some(Arc::new(move || selected_agent.clone())),
            theme: Arc::clone(&current_theme_if_initialized().expect("theme")),
        });
        let width = case.get("width").and_then(Value::as_u64).unwrap_or(160) as usize;
        results.push(json!({
            "name": case.get("name").cloned().unwrap_or(Value::Null),
            "lines": component
                .render(width)
                .into_iter()
                .map(|line| pi_coding_agent::utils::ansi::strip_ansi(&line))
                .collect::<Vec<_>>(),
        }));
    }
    results
}

#[tokio::test]
async fn differential_default_footer() {
    let cases = json!([
        {
            "name": "full_stats",
            "usage": {"input": 412000, "output": 33000, "cost": 9.389},
            "contextUsage": {"contextWindow": 272000, "percent": 60.9},
            "model": {"id": "test-model", "provider": "test", "contextWindow": 272000, "reasoning": false},
            "gitBranch": "main",
            "agentCounts": {"running": 2, "waitingForInput": 3, "steeringPending": 1},
            "extensionStatuses": {"bwrap": "bwrap ready", "goal": "goal: ship footer", "multi-agent-mode": "multi-agent: proactive"}
        },
        {
            "name": "unknown_context",
            "usage": {"input": 0, "output": 0, "cost": 0},
            "contextUsage": {"contextWindow": 272000},
            "model": {"id": "test-model", "provider": "test", "contextWindow": 272000, "reasoning": false},
            "gitBranch": "main"
        },
        {
            "name": "narrow",
            "usage": {"input": 412000, "output": 33000, "cost": 9.389},
            "contextUsage": {"contextWindow": 272000, "percent": 60.9},
            "model": {"id": "test-model", "provider": "test", "contextWindow": 272000, "reasoning": false},
            "agentCounts": {"running": 2, "waitingForInput": 3, "steeringPending": 1},
            "width": 24
        },
        {
            "name": "selected_agent",
            "usage": {"input": 0, "output": 0, "cost": 0},
            "contextUsage": {"contextWindow": 272000},
            "model": {"id": "reasoner", "provider": "test", "contextWindow": 200000, "reasoning": true},
            "thinkingLevel": "high",
            "selectedAgent": {"displayName": "Second", "lifecycle": "waiting_for_input", "slotIndex": 2}
        },
        {
            "name": "child_override",
            "usage": {"input": 11, "output": 7, "cost": 1},
            "contextUsage": {"contextWindow": 272000, "percent": 60.9},
            "model": {"id": "test-model", "provider": "test", "contextWindow": 272000, "reasoning": false},
            "gitBranch": "main",
            "override": {
                "cwd": "/tmp/child",
                "sessionName": "child",
                "usage": {"input": 23, "output": 19, "cost": 2},
                "model": {"id": "child-model", "provider": "child", "contextWindow": 100000, "reasoning": true},
                "thinkingLevel": "high",
                "contextUsage": {"tokens": 25000, "contextWindow": 100000, "percent": 25}
            }
        },
        {
            "name": "provider_count",
            "usage": {"input": 0, "output": 0, "cost": 0},
            "contextUsage": {"contextWindow": 200000, "percent": 10},
            "model": {"id": "reasoner", "provider": "test", "contextWindow": 200000, "reasoning": true},
            "thinkingLevel": "high",
            "availableProviderCount": 2,
            "gitBranch": "main"
        }
    ]);
    let case_list = cases.as_array().expect("cases").clone();
    let rust = rust_results(&case_list).await;
    let mut oracle = TsOracle::spawn("default-footer-oracle.ts", SOURCE);
    let ts = oracle.request(&json!({"operation": "render", "cases": case_list}));
    assert_eq!(ts["ok"], true, "oracle failed: {ts}");
    assert_eq!(rust, ts["output"].as_array().expect("oracle results").clone());
}
