//! Behavioral tests for the `main.ts` startup helpers.

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::providers::faux::{FauxTokenSize, RegisterFauxProviderOptions, faux_provider};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, Usage, UserContent, UserMessage,
    UserRole,
};
use pi_coding_agent::cli::args::{ListModels, Mode, parse_args};
use pi_coding_agent::core::ResourceLoader;
use pi_coding_agent::core::agent_session_runtime::CreateAgentSessionRuntimeOptions;
use pi_coding_agent::core::agent_session_services::AgentSessionRuntimeDiagnostic;
use pi_coding_agent::core::auth_guidance::format_no_models_available_message;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::model_resolver::ScopedModel;
use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::project_trust::AppMode;
use pi_coding_agent::core::resource_loader::{DefaultResourceLoader, DefaultResourceLoaderOptions, InlineExtension};
use pi_coding_agent::core::sdk::NoTools;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::settings_manager::{DefaultProjectTrust, SettingsManager, SettingsManagerCreateOptions};
use pi_coding_agent::core::trust_manager::ProjectTrustStore;
use pi_coding_agent::main_runner::{
    DEBUG_REPL_GAP, FIRST_PARTY_EXTENSION_NAMES, FirstPartyOrchestration, MainExit, ResolvedSession,
    RuntimeFactoryOptions, build_session_options, collect_settings_diagnostics, create_first_party_extension_factories,
    create_first_party_multi_agent_store, create_runtime_factory, create_session_manager, debug_repl_gap_provider,
    fork_flag_conflict, is_plain_runtime_metadata_command, is_truthy_env_flag, prepare_initial_message,
    resolve_app_mode, resolve_cli_paths, resolve_session_path, run_cli_action_or_report_error,
    run_interactive_action_or_report_terminal_error, session_belongs_to_project, session_id_flag_conflict,
    to_print_output_mode, validate_fork_flags, validate_session_id_flags,
};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn user_message(text: &str, timestamp: i64) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text(JsString::from(text.to_owned())),
        input_source: None,
        provider_native: None,
        timestamp: timestamp as f64,
    })
}

fn create_session(cwd: &str, session_dir: &str, text: &str) -> SessionManager {
    let mut session =
        SessionManager::create(&JsString::from(cwd), Some(&JsString::from(session_dir)), None).expect("session");
    session.append_message(user_message(text, 1)).expect("append message");
    session
        .append_message(AgentMessage::Assistant(Box::new(AssistantMessage {
            role: AssistantRole::Tag,
            content: vec![AssistantContent::Text(TextContent::new("ok"))],
            api: "anthropic-messages".to_owned(),
            provider: "anthropic".to_owned(),
            model: "claude-test".to_owned(),
            response_model: None,
            response_id: None,
            image_generation_result: None,
            diagnostics: None,
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            error_message: None,
            timestamp: 2.0,
        })))
        .expect("append assistant message");
    session
}

fn settings_manager(cwd: &str, agent_dir: &Path) -> SettingsManager {
    SettingsManager::create(
        &JsString::from(cwd),
        Some(&JsString::from(agent_dir.to_str().expect("UTF-8 temp path"))),
        SettingsManagerCreateOptions {
            project_trusted: Some(false),
        },
    )
    .expect("settings manager")
}

async fn model_registry() -> (ModelRegistry, pi_ai::types::Model) {
    let handle = faux_provider(RegisterFauxProviderOptions {
        api: Some("faux".to_owned()),
        token_size: Some(FauxTokenSize {
            min: Some(1.0),
            max: Some(1.0),
        }),
        ..Default::default()
    });
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::from_iter([(
        "faux".to_owned(),
        serde_json::json!({"type":"api_key","key":"faux-key"}),
    )])))
    .unwrap();
    scripted_registry::register_scripted_provider(&runtime, handle.provider.clone());
    let model = handle.get_model(None).expect("faux model");
    (runtime, model)
}

#[test]
fn truthy_env_flags_match_javascript_lowercasing() {
    assert!(!is_truthy_env_flag(None));
    assert!(!is_truthy_env_flag(Some("")));
    assert!(!is_truthy_env_flag(Some("0")));
    assert!(!is_truthy_env_flag(Some("no")));
    assert!(!is_truthy_env_flag(Some(" true ")));
    assert!(is_truthy_env_flag(Some("1")));
    assert!(is_truthy_env_flag(Some("true")));
    assert!(is_truthy_env_flag(Some("TRUE")));
    assert!(is_truthy_env_flag(Some("Yes")));
}

#[test]
fn app_mode_follows_mode_print_and_tty_state() {
    let rpc = parse_args(&args(&["--mode", "rpc"]));
    assert_eq!(resolve_app_mode(&rpc, true, true), AppMode::Rpc);

    let json = parse_args(&args(&["--mode", "json"]));
    assert_eq!(resolve_app_mode(&json, true, true), AppMode::Json);

    let print = parse_args(&args(&["--print", "hello"]));
    assert_eq!(resolve_app_mode(&print, true, true), AppMode::Print);

    let plain = parse_args(&args(&["hello"]));
    assert_eq!(resolve_app_mode(&plain, false, true), AppMode::Print);
    assert_eq!(resolve_app_mode(&plain, true, false), AppMode::Print);
    assert_eq!(resolve_app_mode(&plain, true, true), AppMode::Interactive);

    assert_eq!(to_print_output_mode(AppMode::Json), Mode::Json);
    assert_eq!(to_print_output_mode(AppMode::Print), Mode::Text);
}

#[test]
fn plain_runtime_metadata_commands_skip_stdout_takeover() {
    for values in [
        vec!["--help"],
        vec!["--list-models"],
        vec!["--list-models", "sonnet"],
        vec!["tools"],
        vec!["extensions"],
    ] {
        assert!(
            is_plain_runtime_metadata_command(&parse_args(&args(&values))),
            "{values:?}"
        );
    }
    assert!(!is_plain_runtime_metadata_command(&parse_args(&args(&[
        "--help", "--mode", "json"
    ]))));
    assert!(!is_plain_runtime_metadata_command(&parse_args(&args(&[
        "--print", "--help"
    ]))));
    assert!(!is_plain_runtime_metadata_command(&parse_args(&args(&["hello"]))));
}

#[test]
fn fork_conflicts_list_flags_in_source_order() {
    let parsed = parse_args(&args(&[
        "--fork",
        "abc",
        "--session",
        "def",
        "--continue",
        "--resume",
        "--no-session",
    ]));
    assert_eq!(
        fork_flag_conflict(&parsed).as_deref(),
        Some("Error: --fork cannot be combined with --session, --continue, --resume, --no-session")
    );
    assert_eq!(fork_flag_conflict(&parse_args(&args(&["--fork", "abc"]))), None);
    assert_eq!(fork_flag_conflict(&parse_args(&args(&["--continue"]))), None);
    assert_eq!(validate_fork_flags(&parsed), Err(MainExit(1)));
}

#[test]
fn session_id_conflicts_and_validation_match_the_source() {
    let parsed = parse_args(&args(&["--session-id", "abc", "--session", "def"]));
    assert_eq!(
        session_id_flag_conflict(&parsed).as_deref(),
        Some("Error: --session-id cannot be combined with --session")
    );
    assert_eq!(validate_session_id_flags(&parsed), Err(MainExit(1)));

    let invalid = parse_args(&args(&["--session-id", "-bad"]));
    assert_eq!(session_id_flag_conflict(&invalid), None);
    assert_eq!(validate_session_id_flags(&invalid), Err(MainExit(1)));

    let valid = parse_args(&args(&["--session-id", "good.id_1"]));
    assert_eq!(validate_session_id_flags(&valid), Ok(()));
}

#[test]
fn settings_diagnostics_prefix_scope_and_context() {
    let root = tempfile::tempdir().expect("temp dir");
    let agent_dir = root.path().join("agent");
    std::fs::create_dir_all(&agent_dir).expect("agent dir");
    std::fs::write(agent_dir.join("settings.json"), "{ invalid json").expect("settings");
    let mut manager = settings_manager("/tmp", &agent_dir);

    let diagnostics = collect_settings_diagnostics(&mut manager, "startup session lookup");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].diagnostic_type, "warning");
    assert!(
        diagnostics[0]
            .message
            .starts_with("(startup session lookup, global settings) ")
    );
    assert!(collect_settings_diagnostics(&mut manager, "startup session lookup").is_empty());
}

#[test]
fn session_paths_resolve_locally_globally_and_by_prefix() {
    let root = tempfile::tempdir().expect("temp dir");
    let session_dir = root.path().join("sessions");
    std::fs::create_dir_all(&session_dir).expect("session dir");
    let project_a = root.path().join("project-a");
    let project_b = root.path().join("project-b");
    std::fs::create_dir_all(&project_a).expect("project a");
    std::fs::create_dir_all(&project_b).expect("project b");
    let session_dir = session_dir.display().to_string();
    let project_a = project_a.display().to_string();
    let project_b = project_b.display().to_string();

    let session = create_session(&project_a, &session_dir, "from A");
    let session_id = session.get_session_id().to_owned();
    let session_file = session.get_session_file().expect("session file").to_owned();
    assert_eq!(
        SessionManager::list_all(Some(&JsString::from(&session_dir)), None, None)
            .expect("all")
            .len(),
        1
    );
    let listed = SessionManager::list(
        &JsString::from(&project_a),
        Some(&JsString::from(&session_dir)),
        None,
        None,
    )
    .expect("list");
    assert_eq!(listed.len(), 1, "cwd={project_a} sessions={listed:?}");
    let listed = SessionManager::list(
        &JsString::from(&project_a),
        Some(&JsString::from(&session_dir)),
        None,
        None,
    )
    .expect("list");
    assert_eq!(listed.len(), 1, "cwd={project_a} sessions={listed:?}");

    let resolved = resolve_session_path(
        &session_id.slice(0, 6),
        &JsString::from(&project_a),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("resolved");
    assert_eq!(resolved, ResolvedSession::Local(session_file.clone()));

    let resolved = resolve_session_path(
        &session_id,
        &JsString::from(&project_b),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("resolved");
    assert_eq!(
        resolved,
        ResolvedSession::Global {
            path: session_file.clone(),
            cwd: JsString::from(project_a.clone())
        }
    );

    let resolved = resolve_session_path(
        &session_file,
        &JsString::from(&project_b),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("resolved");
    assert_eq!(resolved, ResolvedSession::Path(session_file.clone()));

    let missing_jsonl = root.path().join("missing.jsonl").display().to_string();
    let resolved = resolve_session_path(
        &JsString::from(&missing_jsonl),
        &JsString::from(&project_b),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("resolved");
    assert_eq!(resolved, ResolvedSession::Path(JsString::from(missing_jsonl)));

    let resolved = resolve_session_path(
        &JsString::from("does-not-exist"),
        &JsString::from(&project_a),
        Some(&JsString::from(&session_dir)),
        None,
    )
    .expect("resolved");
    assert_eq!(resolved, ResolvedSession::NotFound(JsString::from("does-not-exist")));
}

#[test]
fn session_project_membership_uses_the_git_common_directory() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo = root.path().join("repo");
    let sibling = root.path().join("sibling");
    std::fs::create_dir_all(&repo).expect("repo");
    std::fs::create_dir_all(&sibling).expect("sibling");
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["init", "--quiet"])
        .status()
        .expect("git init");
    assert!(status.success());
    let worktree = repo.join("worktree");
    std::fs::create_dir_all(&worktree).expect("worktree");

    let repo = repo.display().to_string();
    let worktree = worktree.display().to_string();
    let sibling = sibling.display().to_string();
    assert!(session_belongs_to_project(
        &JsString::from(&repo),
        &JsString::from(&repo)
    ));
    assert!(session_belongs_to_project(
        &JsString::from(&worktree),
        &JsString::from(&repo)
    ));
    assert!(!session_belongs_to_project(
        &JsString::from(&sibling),
        &JsString::from(&repo)
    ));
}

#[test]
fn cli_paths_resolve_local_entries_only() {
    let root = tempfile::tempdir().expect("temp dir");
    let cwd = root.path().display().to_string();
    let resolved = resolve_cli_paths(
        &JsString::from(&cwd),
        Some(&[
            "./skills".to_owned(),
            "npm:package".to_owned(),
            "/absolute/path".to_owned(),
        ]),
    )
    .expect("paths")
    .expect("some");
    assert_eq!(
        resolved,
        vec![
            Path::new(&cwd).join("skills").display().to_string(),
            "npm:package".to_owned(),
            "/absolute/path".to_owned(),
        ]
    );
    assert_eq!(resolve_cli_paths(&JsString::from(&cwd), None).expect("paths"), None);
}

#[tokio::test]
async fn initial_message_concatenates_stdin_and_the_first_cli_message() {
    let mut parsed = parse_args(&args(&["hello", "second"]));
    let result = prepare_initial_message(&mut parsed, true, Some("piped\n"))
        .await
        .expect("initial message");
    assert_eq!(result.initial_message.as_ref(), Some(&JsString::from("piped\nhello")));
    assert_eq!(parsed.messages, vec!["second".to_owned()]);

    let mut parsed = parse_args(&args(&[]));
    let result = prepare_initial_message(&mut parsed, true, None)
        .await
        .expect("initial message");
    assert_eq!(result.initial_message, None);
}

#[tokio::test]
async fn build_session_options_prefers_cli_model_and_explicit_thinking() {
    let (runtime, model) = model_registry().await;
    let root = tempfile::tempdir().expect("temp dir");
    let settings = settings_manager("/tmp", root.path());

    let parsed = parse_args(&args(&["--model", &format!("{}:high", model.id)]));
    let built = build_session_options(&parsed, &[], false, &runtime, &settings).unwrap();
    assert!(built.diagnostics.is_empty());
    assert_eq!(
        built.options.model.as_ref().map(|model| model.id.as_str()),
        Some(model.id.as_str())
    );
    assert_eq!(built.options.thinking_level, Some(ThinkingLevel::High));
    assert!(built.cli_thinking_from_model);

    let parsed = parse_args(&args(&["--model", &model.id, "--thinking", "off"]));
    let built = build_session_options(&parsed, &[], false, &runtime, &settings).unwrap();
    assert_eq!(built.options.thinking_level, Some(ThinkingLevel::Off));
    assert!(!built.cli_thinking_from_model);
}

#[tokio::test]
async fn build_session_options_uses_scoped_models_and_tool_flags() {
    let (runtime, model) = model_registry().await;
    let root = tempfile::tempdir().expect("temp dir");
    let mut settings = settings_manager("/tmp", root.path());
    settings.set_default_provider(&model.provider);
    settings.set_default_model(&model.id);
    let scoped = vec![ScopedModel {
        model: model.clone(),
        thinking_level: Some(ThinkingLevel::Medium),
    }];

    let parsed = parse_args(&args(&[
        "--no-builtin-tools",
        "--tools",
        "read,write",
        "--exclude-tools",
        "bash",
    ]));
    let built = build_session_options(&parsed, &scoped, false, &runtime, &settings).unwrap();
    assert_eq!(
        built.options.model.as_ref().map(|model| model.id.as_str()),
        Some(model.id.as_str())
    );
    assert_eq!(built.options.thinking_level, Some(ThinkingLevel::Medium));
    assert_eq!(built.options.no_tools, Some(NoTools::Builtin));
    assert_eq!(built.options.tools, Some(vec!["read".to_owned(), "write".to_owned()]));
    assert_eq!(built.options.exclude_tools, Some(vec!["bash".to_owned()]));
    assert_eq!(built.options.scoped_models.len(), 1);

    let built = build_session_options(&parsed, &scoped, true, &runtime, &settings).unwrap();
    assert!(built.options.model.is_none());
}

#[tokio::test]
async fn session_argument_opens_other_project_without_fork_or_prompt() {
    let root = tempfile::tempdir().unwrap();
    let project_a = root.path().join("a");
    let project_b = root.path().join("b");
    std::fs::create_dir_all(&project_a).unwrap();
    std::fs::create_dir_all(&project_b).unwrap();
    let sessions = JsString::from(root.path().join("sessions").to_str().unwrap());
    let cwd_a = JsString::from(project_a.to_str().unwrap());
    let cwd_b = JsString::from(project_b.to_str().unwrap());
    let original = create_session(&cwd_a.to_utf8().unwrap(), &sessions.to_utf8().unwrap(), "original");
    let parsed = parse_args(&args(&["--session", &original.get_session_id().to_utf8().unwrap()]));
    let settings = SettingsManager::in_memory(Default::default(), Default::default());
    let opened = create_session_manager(&parsed, &cwd_b, Some(&sessions), &settings, None)
        .await
        .unwrap();
    assert_eq!(opened.get_session_id(), original.get_session_id());
    assert_eq!(opened.get_session_file(), original.get_session_file());
    assert_eq!(opened.get_cwd(), &cwd_a);
}

#[test]
fn compressed_transcript_argument_is_a_path_even_when_missing() {
    let root = tempfile::tempdir().unwrap();
    let cwd = JsString::from(root.path().to_str().unwrap());
    let resolved = resolve_session_path(&"saved.jsonl.zst".into(), &cwd, None, None).unwrap();
    let expected = JsString::from(root.path().join("saved.jsonl.zst").to_str().unwrap());
    assert_eq!(resolved, ResolvedSession::Path(expected));
}

#[tokio::test]
async fn session_argument_restores_archive_before_open() {
    let root = tempfile::tempdir().unwrap();
    let cwd = JsString::from(root.path().to_str().unwrap());
    let sessions = JsString::from(root.path().join("sessions").to_str().unwrap());
    let original = create_session(&cwd.to_utf8().unwrap(), &sessions.to_utf8().unwrap(), "archived");
    let path = original.get_session_file().unwrap();
    let db = JsString::from(root.path().join("control.sqlite").to_str().unwrap());
    let archived = pi_coding_agent::core::session_archive_storage::archive_session_file(path).unwrap();
    let parsed = parse_args(&args(&["--session", &archived.to_utf8().unwrap()]));
    let settings = SettingsManager::in_memory(Default::default(), Default::default());
    let opened = create_session_manager(&parsed, &cwd, Some(&sessions), &settings, Some(&db))
        .await
        .unwrap();
    assert_eq!(opened.get_session_id(), original.get_session_id());
    assert_eq!(opened.get_session_file(), Some(path));
    assert!(std::path::Path::new(&path.to_utf8().unwrap()).exists());
}

#[tokio::test]
async fn create_session_manager_honors_help_in_memory_and_session_ids() {
    let root = tempfile::tempdir().expect("temp dir");
    let agent_dir = root.path().join("agent");
    std::fs::create_dir_all(&agent_dir).expect("agent dir");
    let session_dir = root.path().join("sessions");
    std::fs::create_dir_all(&session_dir).expect("session dir");
    let cwd = root.path().display().to_string();
    let session_dir = session_dir.display().to_string();
    let settings = settings_manager(&cwd, &agent_dir);

    let parsed = parse_args(&args(&["--help"]));
    let manager = create_session_manager(
        &parsed,
        &JsString::from(&cwd),
        Some(&JsString::from(&session_dir)),
        &settings,
        None,
    )
    .await
    .expect("manager");
    assert!(manager.get_session_file().is_none());

    let parsed = parse_args(&args(&["--session-id", "chosen-id"]));
    let manager = create_session_manager(
        &parsed,
        &JsString::from(&cwd),
        Some(&JsString::from(&session_dir)),
        &settings,
        None,
    )
    .await
    .expect("manager");
    assert_eq!(manager.get_session_id(), "chosen-id");
    assert!(manager.get_session_file().is_some());

    let parsed = parse_args(&args(&["--fork", "missing-session"]));
    assert_eq!(
        create_session_manager(
            &parsed,
            &JsString::from(&cwd),
            Some(&JsString::from(&session_dir)),
            &settings,
            None
        )
        .await
        .err(),
        Some(pi_coding_agent::main_runner::MainFailure::Exit(MainExit(1)))
    );
}

#[tokio::test]
async fn cli_action_wrappers_map_errors_to_exit_codes() {
    assert_eq!(run_cli_action_or_report_error(|| async { Ok(()) }).await, 0);
    assert_eq!(
        run_cli_action_or_report_error(|| async { Err(JsString::from("boom")) }).await,
        1
    );

    let outcome = run_interactive_action_or_report_terminal_error(|| async { Ok(()) }, None)
        .await
        .expect("ok");
    assert!(outcome.ok);
    assert_eq!(outcome.exit_code, None);

    let outcome = run_interactive_action_or_report_terminal_error(
        || async { Err(pi_ai::request::CallbackError::new("SetRawMode failed with errno: 5")) },
        None,
    )
    .await
    .expect("terminal error");
    assert!(!outcome.ok);
    assert_eq!(outcome.exit_code, Some(1));

    let error = run_interactive_action_or_report_terminal_error(
        || async { Err(pi_ai::request::CallbackError::new("other failure")) },
        None,
    )
    .await;
    assert_eq!(error.unwrap_err().message, "other failure");

    let original = pi_ai::request::CallbackError::from(pi_coding_agent::config::ConfigError::from(
        pi_js_compat::node_os::homedir_system_error(-5, "EIO", "i/o error"),
    ));
    let supplied = original.clone();
    let returned = run_interactive_action_or_report_terminal_error(|| async { Err(supplied) }, None)
        .await
        .unwrap_err();
    assert_eq!(
        returned, original,
        "non-terminal native errors must be rethrown unchanged"
    );
}

#[test]
fn list_models_parse_variants_are_preserved() {
    assert_eq!(parse_args(&args(&["--list-models"])).list_models, Some(ListModels::All));
    assert_eq!(
        parse_args(&args(&["--list-models", "sonnet"])).list_models,
        Some(ListModels::Search("sonnet".to_owned()))
    );
}

#[test]
fn diagnostics_are_serialized_with_the_ts_type_field() {
    let diagnostic = AgentSessionRuntimeDiagnostic {
        diagnostic_type: "error".to_owned(),
        message: JsString::from("broken"),
    };
    assert_eq!(
        serde_json::to_string(&diagnostic).expect("json"),
        r#"{"type":"error","message":"broken"}"#
    );
}

#[test]
fn first_party_factory_list_partitions_the_source_names() {
    let store = create_first_party_multi_agent_store();
    let handles = pi_coding_agent::bundled_extensions::agents_core::runtime::create_multi_agent_runtime_handles();
    let fetch = pi_ai::http::default_fetch();
    let factories = create_first_party_extension_factories(
        Some(FirstPartyOrchestration {
            store,
            runtime_handles: handles,
            select_agent_view: Arc::new(Mutex::new(None)),
        }),
        fetch,
        JsString::from("/tmp/pi-test-agent"),
        std::sync::Arc::new(Vec::new),
    );
    let registered: Vec<JsString> = factories
        .iter()
        .map(|factory| match factory {
            InlineExtension::Factory {
                extension_path: Some(path),
                ..
            } => path
                .strip_prefix("<first-party:")
                .and_then(|path| path.strip_suffix(">"))
                .expect("first-party path"),
            _ => panic!("first-party factories carry an extension path"),
        })
        .collect();
    assert_eq!(
        registered,
        FIRST_PARTY_EXTENSION_NAMES
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn first_party_factories_load_and_register_agent_tools() {
    let root = tempfile::tempdir().expect("temp dir");
    let cwd = root.path().display().to_string();
    let store = create_first_party_multi_agent_store();
    let handles = pi_coding_agent::bundled_extensions::agents_core::runtime::create_multi_agent_runtime_handles();
    let fetch = pi_ai::http::default_fetch();
    let factories = create_first_party_extension_factories(
        Some(FirstPartyOrchestration {
            store,
            runtime_handles: handles,
            select_agent_view: Arc::new(Mutex::new(None)),
        }),
        fetch,
        JsString::from("/tmp/pi-test-agent"),
        std::sync::Arc::new(Vec::new),
    );
    let loader = DefaultResourceLoader::new(DefaultResourceLoaderOptions {
        cwd: JsString::from(cwd.clone()),
        agent_dir: JsString::from(cwd),
        extension_factories: factories,
        no_skills: true,
        no_prompt_templates: true,
        no_themes: true,
        no_context_files: true,
        ..Default::default()
    })
    .expect("loader");
    loader.reload_with_options(None).await.expect("reload");
    let result = loader.get_extensions();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    let paths: Vec<JsString> = result
        .extensions
        .iter()
        .map(|extension| extension.read().path.clone())
        .collect();
    let expected: Vec<JsString> = FIRST_PARTY_EXTENSION_NAMES
        .iter()
        .map(|name| JsString::from(format!("<first-party:{name}>")))
        .collect();
    assert_eq!(paths, expected);
    let core = result
        .extensions
        .iter()
        .find(|extension| extension.read().path == "<first-party:agents-core>")
        .expect("agents-core");
    assert!(core.read().tools.contains_key("spawn_agent"));
    assert!(!core.read().tools.contains_key("send_agent_message"));
    let viewer = result
        .extensions
        .iter()
        .find(|extension| extension.read().path == "<first-party:agent-viewer>")
        .expect("agent-viewer");
    assert!(viewer.read().tools.contains_key("agent_viewer"));
    let mailbox = result
        .extensions
        .iter()
        .find(|extension| extension.read().path == "<first-party:agents-mailbox>")
        .expect("agents-mailbox");
    assert!(mailbox.read().tools.contains_key("send_agent_message"));
    assert!(mailbox.read().tools.contains_key("contact_parent"));
}

#[tokio::test]
async fn runtime_factory_creates_services_and_reports_missing_models() {
    let root = tempfile::tempdir().expect("temp dir");
    let cwd = root.path().join("project");
    let agent_dir = root.path().join("agent");
    std::fs::create_dir_all(&cwd).expect("project");
    std::fs::create_dir_all(&agent_dir).expect("agent");
    std::fs::create_dir_all(cwd.join(".pi")).expect("project config");
    std::fs::write(cwd.join(".pi/settings.json"), "{}").expect("project settings");
    let cwd_string = cwd.display().to_string();
    let agent_dir_string = agent_dir.display().to_string();

    let parsed = parse_args(&args(&["--no-session"]));
    let session_manager = SessionManager::in_memory(Some(&JsString::from(&cwd_string)), None).expect("session manager");
    let trust_store = Arc::new(ProjectTrustStore::new(&JsString::from(&agent_dir_string)).expect("trust store"));
    let project_trust_by_cwd = Arc::new(Mutex::new(HashMap::new()));
    let factory = create_runtime_factory(RuntimeFactoryOptions {
        parsed,
        agent_dir: JsString::from(agent_dir_string.clone()),
        startup_default_project_trust: DefaultProjectTrust::Ask,
        startup_ui: None,
        trust_store,
        trust_prompt_mode: AppMode::Print,
        app_mode: AppMode::Print,
        project_trust_by_cwd: Arc::clone(&project_trust_by_cwd),
        additional_extension_paths: Vec::new(),
        additional_skill_paths: Vec::new(),
        additional_prompt_template_paths: Vec::new(),
        additional_theme_paths: Vec::new(),
        extension_factories: Vec::new(),
        orchestration: Some(FirstPartyOrchestration {
            store: Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default())),
            runtime_handles:
                pi_coding_agent::bundled_extensions::agents_core::runtime::create_multi_agent_runtime_handles(),
            select_agent_view: Arc::new(Mutex::new(None)),
        }),
    });
    let result = factory(CreateAgentSessionRuntimeOptions {
        project_trust_context: None,
        no_supervisor: None,
        cwd: JsString::from(cwd_string.clone()),
        agent_dir: JsString::from(agent_dir_string),
        session_manager,
        session_start_event: None,
    })
    .await
    .expect("runtime");

    assert_eq!(
        result.model_fallback_message.as_ref(),
        Some(format_no_models_available_message().unwrap()).as_ref()
    );
    assert_eq!(result.services.cwd, cwd_string);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.session.messages().is_empty());
    assert_eq!(
        project_trust_by_cwd.lock().unwrap().get(&JsString::from(&cwd_string)),
        Some(&false)
    );
}

// Frozen main.ts:1156-1163 selects the supplied runtime context before the
// default context. The nested TS main factory isn't independently loadable;
// exercise the real native factory/loader/extension event boundary here.
#[tokio::test]
async fn runtime_factory_uses_the_supplied_project_trust_context() {
    use pi_coding_agent::core::extensions::types::{
        ExtensionMode, NoOpUiContext, ProjectTrustContext, ProjectTrustEvent, ProjectTrustEventDecision,
        ProjectTrustEventResult,
    };
    for provided in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let cwd = root.path().join("project");
        let agent = root.path().join("agent");
        std::fs::create_dir_all(cwd.join(".pi")).unwrap();
        std::fs::create_dir(&agent).unwrap();
        std::fs::write(cwd.join(".pi/settings.json"), "{}").unwrap();
        let cwd = JsString::from(cwd.to_str().unwrap());
        let agent = JsString::from(agent.to_str().unwrap());
        let observed = Arc::new(Mutex::new(None));
        let captured = observed.clone();
        let extension = InlineExtension::Named {
            name: "trust-probe".into(),
            hidden: None,
            factory: Arc::new(move |api| {
                let captured = captured.clone();
                api.on_project_trust(move |_event: ProjectTrustEvent, context: ProjectTrustContext| {
                    *captured.lock().unwrap() =
                        Some(serde_json::json!({"cwd":context.cwd,"mode":context.mode,"hasUI":context.has_ui}));
                    Box::pin(async {
                        Ok(ProjectTrustEventResult {
                            trusted: ProjectTrustEventDecision::No,
                            remember: Some(false),
                        })
                    })
                        as pi_coding_agent::core::extensions::BoxFuture<
                            'static,
                            Result<ProjectTrustEventResult, pi_coding_agent::core::extensions::HandlerError>,
                        >
                })
                .unwrap();
                Box::pin(async { Ok(()) })
            }),
        };
        let factory = create_runtime_factory(RuntimeFactoryOptions {
            parsed: parse_args(&args(&["--no-session"])),
            agent_dir: agent.clone(),
            startup_default_project_trust: DefaultProjectTrust::Ask,
            startup_ui: None,
            trust_store: Arc::new(ProjectTrustStore::new(&agent).unwrap()),
            trust_prompt_mode: AppMode::Print,
            app_mode: AppMode::Print,
            project_trust_by_cwd: Arc::new(Mutex::new(HashMap::new())),
            additional_extension_paths: Vec::new(),
            additional_skill_paths: Vec::new(),
            additional_prompt_template_paths: Vec::new(),
            additional_theme_paths: Vec::new(),
            extension_factories: vec![extension],
            orchestration: None,
        });
        let context = provided.then(|| ProjectTrustContext {
            cwd: "provided-cwd".into(),
            mode: ExtensionMode::Rpc,
            has_ui: true,
            ui: Arc::new(NoOpUiContext),
        });
        let result = factory(CreateAgentSessionRuntimeOptions {
            project_trust_context: context,
            no_supervisor: None,
            cwd: cwd.clone(),
            agent_dir: agent,
            session_manager: SessionManager::in_memory(Some(&cwd), None).unwrap(),
            session_start_event: Some(pi_coding_agent::core::extensions::types::SessionStartEvent {
                reason: pi_coding_agent::core::extensions::types::SessionStartReason::Resume,
                previous_session_file: None,
            }),
        })
        .await
        .unwrap();
        let expected = if provided {
            serde_json::json!({"cwd":"provided-cwd","mode":"rpc","hasUI":true})
        } else {
            serde_json::json!({"cwd":cwd,"mode":"print","hasUI":false})
        };
        assert_eq!(*observed.lock().unwrap(), Some(expected), "provided {provided}");
        result.session.dispose();
    }
}

#[tokio::test]
async fn runtime_factory_keeps_runtime_credentials_across_recreation() {
    let root = tempfile::tempdir().unwrap();
    let cwd = JsString::from(root.path().to_str().unwrap());
    let factory = create_runtime_factory(RuntimeFactoryOptions {
        parsed: parse_args(&args(&["--no-session", "--no-supervisor"])),
        agent_dir: cwd.clone(),
        startup_default_project_trust: DefaultProjectTrust::Never,
        startup_ui: None,
        trust_store: Arc::new(ProjectTrustStore::new(&cwd).unwrap()),
        trust_prompt_mode: AppMode::Print,
        app_mode: AppMode::Print,
        project_trust_by_cwd: Arc::new(Mutex::new(HashMap::new())),
        additional_extension_paths: Vec::new(),
        additional_skill_paths: Vec::new(),
        additional_prompt_template_paths: Vec::new(),
        additional_theme_paths: Vec::new(),
        extension_factories: Vec::new(),
        orchestration: None,
    });
    for iteration in 0..2 {
        let result = factory(CreateAgentSessionRuntimeOptions {
            cwd: cwd.clone(),
            agent_dir: cwd.clone(),
            session_manager: SessionManager::in_memory(Some(&cwd), None).unwrap(),
            no_supervisor: Some(true),
            session_start_event: None,
            project_trust_context: None,
        })
        .await
        .unwrap();
        if iteration == 0 {
            result
                .services
                .auth_storage
                .set_runtime_api_key("native-fixture", &"runtime-secret".into());
        } else {
            assert!(result.services.auth_storage.has_auth("native-fixture").unwrap());
        }
        result.session.dispose();
    }
}

#[test]
fn startup_proxy_child() {
    if std::env::var("PI_MAIN_PROXY_CHILD").as_deref() != Ok("1") {
        return;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let code = runtime
        .block_on(pi_coding_agent::main_runner::main(
            &args(&["--version"]),
            Default::default(),
        ))
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(
        pi_js_compat::process_env::get("HTTP_PROXY").as_deref(),
        Some("http://127.0.0.1:12345")
    );
    assert_eq!(pi_js_compat::process_env::get("HTTPS_PROXY").as_deref(), Some(""));
}

#[test]
fn bootstrap_applies_global_proxy_without_overwriting_existing_environment() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("settings.json"),
        r#"{"httpProxy":"  http://127.0.0.1:12345  "}"#,
    )
    .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "startup_proxy_child", "--nocapture"])
        .env("PI_MAIN_PROXY_CHILD", "1")
        .env("PI_CODING_AGENT_DIR", root.path())
        .env("PI_CODING_AGENT_STATE_DIR", root.path().join("state"))
        .env("HTTPS_PROXY", "")
        .env_remove("HTTP_PROXY")
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8(output.stderr).unwrap());
}

#[tokio::test]
async fn debug_repl_provider_reports_the_documented_gap() {
    let controller = (debug_repl_gap_provider())();
    assert_eq!(
        controller
            .enable(&JsString::from("session-1"))
            .await
            .unwrap_err()
            .message,
        DEBUG_REPL_GAP
    );
    assert_eq!(controller.disable().await.unwrap_err().message, DEBUG_REPL_GAP);
    assert_eq!(DEBUG_REPL_GAP, "Debug REPL is not ported (core/debug-repl.ts)");
}

use crate::shared_scripted_registry as scripted_registry;

// Shared support is compiled once per integration target.
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
