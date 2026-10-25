//! Differential coverage for docs-tree-context, claude-memory-enrich, and claude-memory-session-end.

use crate::shared_executable_script as executable_script;
use crate::shared_oracle as oracle;
use pi_js_compat::js_json::JsString;

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// `process_env` overrides are process-global, so tests that mutate them must
/// not run concurrently inside this binary.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn lock_env() -> tokio::sync::MutexGuard<'static, ()> {
    ENV_LOCK.lock().await
}

use pi_coding_agent::core::event_bus::{EventBus, EventBusController};
use pi_coding_agent::core::extensions::loader::{ExtensionFactory, load_extension_from_factory};
use pi_coding_agent::core::extensions::types::{
    BeforeAgentStartEvent, Extension, ExtensionApi, ExtensionContext, HandlerError, SessionStartEvent,
    SessionStartReason,
};
use pi_coding_agent::core::extensions::{ExtensionRunner, create_extension_runtime};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json as json;
use pi_js_compat::process_env;
use tempfile::TempDir;

async fn model_registry() -> Arc<ModelRegistry> {
    Arc::new(
        ModelRegistry::in_memory(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(
            serde_json::Map::new(),
        ))
        .unwrap(),
    )
}

async fn load(
    cwd: &str,
    session_manager: pi_coding_agent::core::session_manager::SharedSessionManager,
    setup: impl FnOnce(&ExtensionApi) -> Result<(), HandlerError> + Send + 'static,
) -> Harness {
    let runtime = create_extension_runtime();
    let setup = Mutex::new(Some(setup));
    let factory: ExtensionFactory = Arc::new(move |api| {
        let setup = setup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .expect("extension setup runs once");
        let result = setup(api);
        Box::pin(async move { result })
    });
    let bus: Arc<dyn EventBus> = Arc::new(EventBusController::new());
    let extension = load_extension_from_factory(
        factory,
        &JsString::from(cwd),
        bus,
        runtime.clone(),
        &JsString::from("<test>"),
    )
    .await
    .expect("extension factory succeeds");
    let runner = ExtensionRunner::new(
        vec![extension.clone()],
        runtime,
        cwd,
        session_manager,
        model_registry().await,
        None,
    );
    Harness { extension, runner }
}

struct Harness {
    extension: Extension,
    runner: ExtensionRunner,
}

impl Harness {
    fn context(&self) -> ExtensionContext {
        self.runner.create_context()
    }

    fn registrations(&self) -> pi_js_compat::js_json::JsValue {
        let tools: Vec<pi_js_compat::js_json::JsValue> = self
            .extension
            .read()
            .tools
            .values()
            .map(|registered| {
                let tool = &registered.definition;
                json!({
                    "name": tool.name,
                    "label": tool.label,
                    "description": tool.description,
                    "promptSnippet": tool.prompt_snippet,
                    "promptGuidelines": tool.prompt_guidelines,
                    "approvalRequired": tool.approval_required,
                    "parameters": tool.parameters,
                })
            })
            .collect();
        let commands: Vec<pi_js_compat::js_json::JsValue> = self
            .extension
            .read()
            .commands
            .values()
            .map(|command| {
                json!({
                    "name": command.name,
                    "description": command.description,
                    "hasCompletions": command.get_argument_completions.is_some(),
                })
            })
            .collect();
        json!({
            "tools": tools,
            "commands": commands,
            "flags": [],
            "shortcuts": Vec::<String>::new(),
            "handlers": handler_names(&self.extension),
            "toolGates": self.extension.read().tool_gates.len(),
        })
    }
}

fn handler_names(extension: &Extension) -> Vec<&'static str> {
    let handlers = &extension.read().handlers;
    let mut names = Vec::new();
    if !handlers.session_start.is_empty() {
        names.push("session_start");
    }
    if !handlers.session_shutdown.is_empty() {
        names.push("session_shutdown");
    }
    if !handlers.before_agent_start.is_empty() {
        names.push("before_agent_start");
    }
    if !handlers.agent_end.is_empty() {
        names.push("agent_end");
    }
    names
}

fn oracle_for(module: &str) -> oracle::TsOracle {
    oracle::TsOracle::spawn("bundled-extensions-oracle.ts", module)
}

fn write_docs_tree(root: &Path) {
    fs::create_dir_all(root.join("docs/sub")).unwrap();
    fs::write(root.join("docs/a.md"), "a").unwrap();
    fs::write(root.join("docs/sub/b.md"), "b").unwrap();
    fs::create_dir_all(root.join("docs/wiki")).unwrap();
    fs::write(root.join("docs/wiki/ignored.md"), "ignored").unwrap();
    fs::create_dir_all(root.join("docs/openclaw")).unwrap();
    fs::write(root.join("docs/openclaw/x.md"), "x").unwrap();
    fs::write(root.join("docs/openclaw/README.md"), "explicit").unwrap();
    fs::write(root.join("docs/wiki/README.md"), "explicit").unwrap();
    fs::create_dir_all(root.join("docs/claw-specs")).unwrap();
    fs::write(root.join("docs/claw-specs/y.md"), "y").unwrap();
}

async fn start_docs(harness: &Harness) {
    let event = SessionStartEvent {
        reason: SessionStartReason::Startup,
        previous_session_file: None,
    };
    let handler = harness.extension.read().handlers.session_start[0].clone();
    handler.call(event, harness.context()).await.unwrap();
}

fn before_agent_event(system_prompt: &str, prompt: &str) -> BeforeAgentStartEvent {
    BeforeAgentStartEvent {
        prompt: prompt.to_owned().into(),
        images: None,
        system_prompt: system_prompt.to_owned().into(),
        system_prompt_options: Default::default(),
    }
}

async fn run_before_agent(harness: &Harness, system_prompt: &str, prompt: &str) -> pi_js_compat::js_json::JsValue {
    let handler = harness.extension.read().handlers.before_agent_start[0].clone();
    let result = handler
        .call(before_agent_event(system_prompt, prompt), harness.context())
        .await
        .unwrap();
    match result {
        Some(result) => json!(result),
        None => pi_js_compat::js_json::JsValue::Null,
    }
}

#[tokio::test]
async fn docs_tree_context_matches_typescript() {
    let temp = TempDir::new().unwrap();
    write_docs_tree(temp.path());
    let cwd = temp.path().to_str().expect("utf-8 test path").to_owned();

    let mut oracle = oracle_for("packages/coding-agent/extensions/docs-tree-context/src/index.ts");
    let response = oracle.request(&json!({
        "id": 1,
        "extension": "docs-tree-context",
        "steps": [
            {"op": "registrations"},
            {"op": "event", "name": "session_start", "event": {}, "cwd": cwd},
            {"op": "event", "name": "before_agent_start", "event": {"prompt": "hi", "systemPrompt": "BASE\n</user_rules>\nTAIL"}, "cwd": cwd},
            {"op": "event", "name": "before_agent_start", "event": {"prompt": "hi", "systemPrompt": "BASE"}, "cwd": cwd},
            {"op": "event", "name": "before_agent_start", "event": {"prompt": "hi", "systemPrompt": "BASE\n<repo_docs_files>\nold\n</repo_docs_files>"}, "cwd": cwd}
        ]
    }));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    let steps = response["output"]["steps"].as_array().unwrap();

    let harness = load(
        &cwd,
        Arc::new(std::sync::Mutex::new(
            SessionManager::in_memory(Some(&JsString::from(&cwd)), None).unwrap(),
        )),
        pi_coding_agent::bundled_extensions::docs_tree_context::docs_tree_context_extension,
    )
    .await;
    assert_eq!(harness.registrations(), steps[0]["result"]);
    start_docs(&harness).await;
    assert_eq!(
        run_before_agent(&harness, "BASE\n</user_rules>\nTAIL", "hi").await,
        steps[2]["result"]
    );
    assert_eq!(run_before_agent(&harness, "BASE", "hi").await, steps[3]["result"]);
    assert_eq!(
        run_before_agent(&harness, "BASE\n<repo_docs_files>\nold\n</repo_docs_files>", "hi").await,
        steps[4]["result"]
    );

    let empty = TempDir::new().unwrap();
    let empty_cwd = empty.path().to_str().expect("utf-8 test path").to_owned();
    let response = oracle.request(&json!({
        "id": 2,
        "extension": "docs-tree-context",
        "steps": [
            {"op": "event", "name": "session_start", "event": {}, "cwd": empty_cwd},
            {"op": "event", "name": "before_agent_start", "event": {"prompt": "hi", "systemPrompt": "BASE"}, "cwd": empty_cwd}
        ]
    }));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    let steps = response["output"]["steps"].as_array().unwrap();
    let harness = load(
        &empty_cwd,
        Arc::new(std::sync::Mutex::new(
            SessionManager::in_memory(Some(&JsString::from(&empty_cwd)), None).unwrap(),
        )),
        pi_coding_agent::bundled_extensions::docs_tree_context::docs_tree_context_extension,
    )
    .await;
    start_docs(&harness).await;
    assert_eq!(run_before_agent(&harness, "BASE", "hi").await, steps[1]["result"]);
}

fn write_enrich_script(root: &Path, name: &str, body: &str) -> String {
    let path = root.join(name);
    executable_script::write_executable_script(&path, body);
    path.to_str().expect("utf-8 test path").to_owned()
}

#[tokio::test]
async fn claude_memory_enrich_matches_typescript() {
    let _env = lock_env().await;
    let temp = TempDir::new().unwrap();
    let ok = write_enrich_script(
        temp.path(),
        "ok.sh",
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"hookSpecificOutput\":{\"hookEventName\":\"UserPromptSubmit\",\"additionalContext\":\"  enriched context  \"}}'\n",
    );
    let empty = write_enrich_script(
        temp.path(),
        "empty.sh",
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{}'\n",
    );
    let invalid_shape = write_enrich_script(
        temp.path(),
        "invalid.sh",
        "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{\"hookSpecificOutput\":{\"additionalContext\":1}}'\n",
    );
    let failing = write_enrich_script(
        temp.path(),
        "fail.sh",
        "#!/bin/sh\ncat >/dev/null\necho boom >&2\nexit 3\n",
    );

    let mut oracle = oracle_for("packages/coding-agent/extensions/claude-memory-enrich/src/index.ts");
    let response = oracle.request(&json!({
        "id": 3,
        "extension": "claude-memory-enrich",
        "steps": [
            {"op": "registrations"},
            {"op": "event", "name": "before_agent_start", "event": {"prompt": "  hello  ", "systemPrompt": "BASE"}, "env": {"PI_CLAUDE_MEMORY": ok}},
            {"op": "event", "name": "before_agent_start", "event": {"prompt": "   ", "systemPrompt": "BASE"}, "env": {"PI_CLAUDE_MEMORY": ok}},
            {"op": "event", "name": "before_agent_start", "event": {"prompt": "hello", "systemPrompt": "BASE"}, "env": {"PI_CLAUDE_MEMORY": empty}},
            {"op": "event", "name": "before_agent_start", "event": {"prompt": "hello", "systemPrompt": "BASE"}, "env": {"PI_CLAUDE_MEMORY": invalid_shape}},
            {"op": "event", "name": "before_agent_start", "event": {"prompt": "hello", "systemPrompt": "BASE"}, "env": {"PI_CLAUDE_MEMORY": failing}},
            {"op": "event", "name": "before_agent_start", "event": {"prompt": "hello", "systemPrompt": "BASE\n<claude_memory_enrich>\nold\n</claude_memory_enrich>"}, "env": {"PI_CLAUDE_MEMORY": ok}}
        ]
    }));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    let steps = response["output"]["steps"].as_array().unwrap();

    let harness = load(
        "/tmp",
        Arc::new(std::sync::Mutex::new(
            SessionManager::in_memory(Some(&JsString::from("/tmp")), None).unwrap(),
        )),
        pi_coding_agent::bundled_extensions::claude_memory_enrich::claude_memory_enrich_extension,
    )
    .await;
    assert_eq!(harness.registrations(), steps[0]["result"]);

    for (index, (script, prompt, system_prompt)) in [
        (ok.as_str(), "  hello  ", "BASE"),
        (ok.as_str(), "   ", "BASE"),
        (empty.as_str(), "hello", "BASE"),
        (invalid_shape.as_str(), "hello", "BASE"),
        (failing.as_str(), "hello", "BASE"),
        (
            ok.as_str(),
            "hello",
            "BASE\n<claude_memory_enrich>\nold\n</claude_memory_enrich>",
        ),
    ]
    .iter()
    .enumerate()
    {
        process_env::set("PI_CLAUDE_MEMORY", script);
        assert_eq!(
            run_before_agent(&harness, system_prompt, prompt).await,
            steps[index + 1]["result"],
            "step {index}"
        );
    }
    process_env::clear();
}

#[tokio::test]
async fn claude_memory_session_end_skips_ephemeral_sessions() {
    let _env = lock_env().await;
    let temp = TempDir::new().unwrap();
    process_env::set("XDG_CACHE_HOME", temp.path().to_str().expect("utf-8 test path"));
    let harness = load(
        "/tmp",
        Arc::new(std::sync::Mutex::new(
            SessionManager::in_memory(Some(&JsString::from("/tmp")), None).unwrap(),
        )),
        pi_coding_agent::bundled_extensions::claude_memory_session_end::claude_memory_session_end_extension,
    )
    .await;

    let mut oracle = oracle_for("packages/coding-agent/extensions/claude-memory-session-end/src/index.ts");
    let response = oracle.request(&json!({
        "id": 4,
        "extension": "claude-memory-session-end",
        "steps": [{"op": "registrations"}]
    }));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    assert_eq!(harness.registrations(), response["output"]["steps"][0]["result"]);

    let event = pi_coding_agent::core::extensions::types::SessionShutdownEvent {
        reason: pi_coding_agent::core::extensions::types::SessionShutdownReason::Quit,
        target_session_file: None,
    };
    let handler = harness.extension.read().handlers.session_shutdown[0].clone();
    let ctx: ExtensionContext = harness.context();
    handler.call(event, ctx).await.unwrap();
    process_env::clear();
    assert!(!temp.path().join("claude-memory/pi-index.log").exists());
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
