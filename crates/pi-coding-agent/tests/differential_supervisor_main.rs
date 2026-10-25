#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::{
    core::{
        auth_storage::AuthStorage, model_registry::ModelRegistry, resource_loader::StaticResourceLoader,
        session_manager::SessionManager,
    },
    supervisor::main::{CreateSupervisorSessionOptions, create_supervisor_session, create_supervisor_settings_manager},
};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue},
};
use std::sync::{Arc, Mutex};
const MODULE: &str = "packages/coding-agent/src/supervisor/main.ts";

#[test]
fn opening_supervisor_session_prepares_and_removes_stale_files_like_frozen() {
    fn populate(root: &std::path::Path) {
        let sessions = root.join("supervisor-sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        for name in [
            "supervisor.jsonl",
            "2026-01-01_supervisor.jsonl",
            "2026-02-01_supervisor.jsonl",
        ] {
            std::fs::write(sessions.join(name), format!("{}\n",pi_js_compat::js_json::stringify_lossless(&json!({
                "type":"session","version":3,"id":"supervisor","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/fixture/kb",
            })))).unwrap();
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let frozen = directory.path().join("source");
    let native = directory.path().join("native");
    populate(&frozen);
    populate(&native);
    let mut source = oracle::TsOracle::spawn("coding-agent-supervisor-main-oracle.ts", MODULE);
    let expected = source.request(&json!({
        "id":2,"operation":"openSession","agentDir":frozen,"kbDir":"/fixture/kb","database":directory.path().join("source.sqlite"),
    }));
    assert_eq!(expected["ok"], json!(true));
    let session = pi_coding_agent::supervisor::main::open_supervisor_session(
        &native.to_str().unwrap().into(),
        &"/fixture/kb".into(),
        &directory.path().join("native.sqlite").to_str().unwrap().into(),
    )
    .unwrap();
    let mut files = std::fs::read_dir(native.join("supervisor-sessions"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    files.sort();
    let file = session.get_session_file().unwrap().to_utf8().unwrap();
    let actual = json!({"files":files,"file":std::path::Path::new(&file).file_name().unwrap().to_str().unwrap(),"id":session.get_session_id(),"cwd":session.get_cwd()});
    assert_eq!(actual, expected["output"]);
    assert_eq!(actual["files"], json!(["2026-02-01_supervisor.jsonl"]));
}

#[tokio::test]
async fn authenticated_supervisor_model_selection_matches_frozen_requirements() {
    for (providers, selected) in [
        (vec![], None),
        (vec!["openai-codex-gc"], Some("openai-codex-gc")),
        (
            vec!["openai-codex-team", "openai-codex-gc", "openai-codex"],
            Some("openai-codex"),
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let agent_dir = JsString::from(directory.path().to_str().unwrap());
        let mut records = pi_js_compat::js_json::JsObject::new();
        for provider in providers {
            records.insert(provider.into(), json!({"type":"api_key","key":"fixture"}));
        }
        let models = ["openai-codex", "openai-codex-gc", "openai-codex-team"]
            .into_iter()
            .map(|provider| {
                (
                    JsString::from(provider),
                    json!({
                        "baseUrl":"https://example.invalid", "api":"openai-codex-responses",
                        "models":[{"id":"gpt-6.1-sol","name":"Sol","api":"openai-codex-responses","reasoning":true,
                            "input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},
                            "contextWindow":200000,"maxTokens":8192}],
                    }),
                )
            })
            .collect::<pi_js_compat::js_json::JsObject>();
        std::fs::write(
            directory.path().join("models.json"),
            pi_js_compat::js_json::stringify_lossless(&JsValue::object([("providers", JsValue::Object(models))])),
        )
        .unwrap();
        let auth_path = directory.path().join("auth.json");
        std::fs::write(
            &auth_path,
            pi_js_compat::js_json::stringify_lossless(&JsValue::Object(records)),
        )
        .unwrap();
        let mut source = oracle::TsOracle::spawn("coding-agent-supervisor-main-oracle.ts", MODULE);
        let expected = source.request(&json!({"id":1,"operation":"modelRequirements","agentDir":agent_dir}));
        assert_eq!(expected["ok"], json!(true));
        assert_eq!(expected["output"]["available"], json!(selected.is_some()));
        let registry = ModelRegistry::create(
            AuthStorage::create(Some(&auth_path.to_str().unwrap().into())).unwrap(),
            Some(directory.path().join("models.json").to_str().unwrap().into()),
        )
        .unwrap();
        let result = create_supervisor_session(CreateSupervisorSessionOptions {
            agent_dir: agent_dir.clone(),
            kb_dir: agent_dir.clone(),
            model_registry: registry,
            resource_loader: StaticResourceLoader::new(),
            session_manager: SessionManager::in_memory(Some(&agent_dir), None).unwrap(),
            settings_manager: Arc::new(Mutex::new(create_supervisor_settings_manager())),
        })
        .await;
        match selected {
            Some(provider) => {
                let session = result.unwrap().session;
                assert_eq!(session.model().provider, provider);
                assert_eq!(session.model().id, "gpt-6.1-sol");
                assert_eq!(expected["output"]["error"], JsValue::Null);
            }
            None => assert_eq!(
                result.err().expect("missing authentication must reject"),
                expected["output"]["error"].as_string().unwrap().clone()
            ),
        }
    }
}
