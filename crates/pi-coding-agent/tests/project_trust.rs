//! `core/project-trust.ts` and `cli/project-trust.ts` behavior.

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pi_ai::request::{BoxFuture, CallbackError};
use pi_coding_agent::cli::project_trust::{
    CreateProjectTrustContextOptions, ProjectTrustStartupUi, create_project_trust_context,
};
use pi_coding_agent::core::extensions::types::{
    ExtensionMode, ExtensionUiContext, ExtensionUiDialogOptions, LoadExtensionsResult, ProjectTrustContext,
    ProjectTrustEvent, ProjectTrustEventDecision, ProjectTrustEventResult,
};
use pi_coding_agent::core::extensions::{
    Extension, ExtensionApi, ExtensionFactory, ExtensionRuntime, HandlerError, create_extension_runtime,
    load_extension_from_factory,
};
use pi_coding_agent::core::project_trust::{
    AppMode, ResolveProjectTrustedOptions, format_project_trust_prompt, resolve_project_trusted,
};
use pi_coding_agent::core::settings_manager::DefaultProjectTrust;
use pi_coding_agent::core::trust_manager::{ProjectTrustDecision, ProjectTrustStore, TrustError};

fn extension_runtime() -> ExtensionRuntime {
    create_extension_runtime()
}

async fn extension(
    runtime: &ExtensionRuntime,
    cwd: &JsString,
    path: impl Into<JsString>,
    setup: impl FnOnce(&ExtensionApi) + Send + 'static,
) -> Extension {
    let setup = Mutex::new(Some(setup));
    let factory: ExtensionFactory = Arc::new(move |api| {
        let setup = setup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .expect("extension setup runs once");
        setup(api);
        Box::pin(async { Ok(()) })
    });
    load_extension_from_factory(
        factory,
        cwd,
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        runtime.clone(),
        &path.into(),
    )
    .await
    .expect("extension factory succeeds")
}

fn s(path: &Path) -> JsString {
    path.display().to_string().into()
}

fn temp_dir(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pi-project-trust-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A project directory whose `.pi/settings.json` makes trust relevant.
fn trust_requiring_project(name: &str) -> (PathBuf, PathBuf) {
    let temp = temp_dir(name);
    let agent_dir = temp.join("agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let project = temp.join("project");
    fs::create_dir_all(project.join(".pi")).unwrap();
    fs::write(project.join(".pi/settings.json"), "{}").unwrap();
    (agent_dir, project)
}

struct RecordingUi {
    labels: Mutex<Vec<JsString>>,
    selection: Option<JsString>,
}

impl RecordingUi {
    fn selecting(selection: &str) -> Arc<Self> {
        Arc::new(Self {
            labels: Mutex::new(Vec::new()),
            selection: Some(selection.into()),
        })
    }

    fn recording() -> Arc<Self> {
        Arc::new(Self {
            labels: Mutex::new(Vec::new()),
            selection: None,
        })
    }

    fn labels(&self) -> Vec<JsString> {
        self.labels.lock().unwrap().clone()
    }
}

impl ExtensionUiContext for RecordingUi {
    fn select<'a>(
        &'a self,
        _title: &JsString,
        options: &[JsString],
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        let labels = options.to_vec();
        let selection = self.selection.clone();
        Box::pin(async move {
            *self.labels.lock().unwrap() = labels;
            Ok(selection)
        })
    }
}

fn context(cwd: &JsString, has_ui: bool, ui: Arc<dyn ExtensionUiContext>) -> ProjectTrustContext {
    ProjectTrustContext {
        cwd: cwd.to_owned().into(),
        mode: ExtensionMode::Tui,
        has_ui,
        ui,
    }
}

fn options<'a>(
    cwd: &'a JsString,
    store: &'a ProjectTrustStore,
    trust_context: &'a ProjectTrustContext,
) -> ResolveProjectTrustedOptions<'a> {
    ResolveProjectTrustedOptions {
        cwd,
        trust_store: store,
        trust_override: None,
        default_project_trust: None,
        extensions_result: None,
        project_trust_context: trust_context,
        on_extension_error: None,
    }
}

#[test]
fn prompt_text_matches_the_typescript_template() {
    assert_eq!(
        format_project_trust_prompt(&JsString::from("/work/project")),
        "Trust project folder?\n/work/project\n\nThis allows pi to load .pi settings and resources, install missing project packages, and execute project extensions."
    );
}

#[tokio::test]
async fn trust_override_short_circuits_every_other_check() {
    let (agent_dir, project) = trust_requiring_project("override");
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::selecting("Do not trust");
    let project_path = s(&project);
    let ctx = context(&project_path, true, ui.clone());

    let mut requested = options(&project_path, &store, &ctx);
    requested.trust_override = Some(true);
    assert!(resolve_project_trusted(requested).await.unwrap());

    let mut requested = options(&project_path, &store, &ctx);
    requested.trust_override = Some(false);
    assert!(!resolve_project_trusted(requested).await.unwrap());
    assert!(ui.labels().is_empty());
}

#[tokio::test]
async fn projects_without_trust_requiring_resources_are_trusted() {
    let temp = temp_dir("no-resources");
    let agent_dir = temp.join("agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let project = temp.join("plain");
    fs::create_dir_all(&project).unwrap();
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::recording();
    let project_path = s(&project);
    let ctx = context(&project_path, true, ui.clone());

    assert!(
        resolve_project_trusted(options(&project_path, &store, &ctx))
            .await
            .unwrap()
    );
    assert!(ui.labels().is_empty());
}

#[tokio::test]
async fn stored_decisions_win_over_the_default_setting() {
    let (agent_dir, project) = trust_requiring_project("stored");
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::recording();
    let project_path = s(&project);
    let ctx = context(&project_path, true, ui.clone());

    store.set(&project_path, ProjectTrustDecision::Untrusted).unwrap();
    let mut requested = options(&project_path, &store, &ctx);
    requested.default_project_trust = Some(DefaultProjectTrust::Always);
    assert!(!resolve_project_trusted(requested).await.unwrap());

    store.set(&project_path, ProjectTrustDecision::Trusted).unwrap();
    let mut requested = options(&project_path, &store, &ctx);
    requested.default_project_trust = Some(DefaultProjectTrust::Never);
    assert!(resolve_project_trusted(requested).await.unwrap());
    assert!(ui.labels().is_empty());
}

#[tokio::test]
async fn default_setting_applies_when_the_store_has_no_decision() {
    let (agent_dir, project) = trust_requiring_project("defaults");
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::recording();
    let project_path = s(&project);
    let ctx = context(&project_path, true, ui.clone());

    let mut requested = options(&project_path, &store, &ctx);
    requested.default_project_trust = Some(DefaultProjectTrust::Always);
    assert!(resolve_project_trusted(requested).await.unwrap());

    let mut requested = options(&project_path, &store, &ctx);
    requested.default_project_trust = Some(DefaultProjectTrust::Never);
    assert!(!resolve_project_trusted(requested).await.unwrap());
    assert!(ui.labels().is_empty());
}

#[tokio::test]
async fn ask_without_ui_is_untrusted() {
    let (agent_dir, project) = trust_requiring_project("no-ui");
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::selecting("Trust");
    let project_path = s(&project);
    let ctx = context(&project_path, false, ui.clone());

    assert!(
        !resolve_project_trusted(options(&project_path, &store, &ctx))
            .await
            .unwrap()
    );
    assert!(ui.labels().is_empty());
    assert_eq!(store.get(&project_path).unwrap(), ProjectTrustDecision::Unknown);
}

#[tokio::test]
async fn ask_prompts_with_every_option_and_persists_the_choice() {
    let (agent_dir, project) = trust_requiring_project("prompt");
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::selecting("Do not trust");
    let project_path = s(&project);
    let ctx = context(&project_path, true, ui.clone());

    assert!(
        !resolve_project_trusted(options(&project_path, &store, &ctx))
            .await
            .unwrap()
    );
    assert_eq!(
        ui.labels(),
        vec![
            JsString::from("Trust"),
            pi_js_compat::js_format!("Trust parent folder ({})", s(project.parent().unwrap())),
            JsString::from("Trust (this session only)"),
            JsString::from("Do not trust"),
            JsString::from("Do not trust (this session only)"),
        ]
    );
    assert_eq!(store.get(&project_path).unwrap(), ProjectTrustDecision::Untrusted);
}

#[tokio::test]
async fn session_only_choices_do_not_touch_the_store() {
    let (agent_dir, project) = trust_requiring_project("session-only");
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::selecting("Trust (this session only)");
    let project_path = s(&project);
    let ctx = context(&project_path, true, ui.clone());

    assert!(
        resolve_project_trusted(options(&project_path, &store, &ctx))
            .await
            .unwrap()
    );
    assert_eq!(store.get(&project_path).unwrap(), ProjectTrustDecision::Unknown);
}

#[tokio::test]
async fn dismissing_the_prompt_is_untrusted() {
    let (agent_dir, project) = trust_requiring_project("dismiss");
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::recording();
    let project_path = s(&project);
    let ctx = context(&project_path, true, ui.clone());

    assert!(
        !resolve_project_trusted(options(&project_path, &store, &ctx))
            .await
            .unwrap()
    );
    assert_eq!(ui.labels().len(), 5);
}

#[tokio::test]
async fn an_unknown_selection_is_untrusted_without_writing() {
    let (agent_dir, project) = trust_requiring_project("unknown-selection");
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::selecting("something else");
    let project_path = s(&project);
    let ctx = context(&project_path, true, ui.clone());

    assert!(
        !resolve_project_trusted(options(&project_path, &store, &ctx))
            .await
            .unwrap()
    );
    assert_eq!(store.get(&project_path).unwrap(), ProjectTrustDecision::Unknown);
}

async fn extensions_result(
    project: &Path,
    decisions: &[(&str, ProjectTrustEventDecision, Option<bool>)],
) -> LoadExtensionsResult {
    let runtime = extension_runtime();
    let mut extensions = Vec::new();
    for (name, decision, remember) in decisions {
        let decision = *decision;
        let remember = *remember;
        let extension = extension(
            &runtime,
            &s(project),
            &pi_js_compat::js_format!("{}/{}.ts", s(project), name),
            move |api| {
                api.on_project_trust(move |_event: ProjectTrustEvent, _ctx: ProjectTrustContext| {
                    Box::pin(async move {
                        Ok(ProjectTrustEventResult {
                            trusted: decision,
                            remember,
                        })
                    }) as BoxFuture<'static, Result<ProjectTrustEventResult, HandlerError>>
                })
                .unwrap();
            },
        )
        .await;
        extensions.push(extension);
    }
    LoadExtensionsResult {
        extensions,
        errors: Vec::new(),
        runtime,
    }
}

#[tokio::test]
async fn extension_decisions_win_and_remember_persists_them() {
    let (agent_dir, project) = trust_requiring_project("extension");
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::recording();
    let project_path = s(&project);
    let ctx = context(&project_path, true, ui.clone());
    let loaded = extensions_result(
        &project,
        &[
            ("a", ProjectTrustEventDecision::Undecided, None),
            ("b", ProjectTrustEventDecision::Yes, Some(true)),
        ],
    )
    .await;

    let mut requested = options(&project_path, &store, &ctx);
    requested.extensions_result = Some(&loaded);
    assert!(resolve_project_trusted(requested).await.unwrap());
    assert!(ui.labels().is_empty());
    assert_eq!(store.get(&project_path).unwrap(), ProjectTrustDecision::Trusted);
}

#[tokio::test]
async fn extension_decision_without_remember_does_not_write() {
    let (agent_dir, project) = trust_requiring_project("extension-no-remember");
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::recording();
    let project_path = s(&project);
    let ctx = context(&project_path, true, ui.clone());
    let loaded = extensions_result(&project, &[("a", ProjectTrustEventDecision::No, None)]).await;

    let mut requested = options(&project_path, &store, &ctx);
    requested.extensions_result = Some(&loaded);
    assert!(!resolve_project_trusted(requested).await.unwrap());
    assert_eq!(store.get(&project_path).unwrap(), ProjectTrustDecision::Unknown);
}

#[tokio::test]
async fn extension_errors_are_reported_and_resolution_continues() {
    let (agent_dir, project) = trust_requiring_project("extension-error");
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::recording();
    let project_path = s(&project);
    let ctx = context(&project_path, true, ui.clone());
    let runtime = extension_runtime();
    let path = pi_js_compat::js_format!("{}/broken.ts", s(&project));
    let extension = extension(&runtime, &s(&project), &path, |api| {
        api.on_project_trust(|_event: ProjectTrustEvent, _ctx: ProjectTrustContext| {
            Box::pin(async { Err(HandlerError::new("boom")) })
                as BoxFuture<'static, Result<ProjectTrustEventResult, HandlerError>>
        })
        .unwrap();
    })
    .await;
    let loaded = LoadExtensionsResult {
        extensions: vec![extension],
        errors: Vec::new(),
        runtime,
    };

    let reported = Mutex::new(Vec::new());
    let report = |message: JsString| reported.lock().unwrap().push(message);
    let mut requested = options(&project_path, &store, &ctx);
    requested.extensions_result = Some(&loaded);
    requested.on_extension_error = Some(&report);

    assert!(!resolve_project_trusted(requested).await.unwrap());
    assert_eq!(
        *reported.lock().unwrap(),
        vec![pi_js_compat::js_format!(
            "Extension \"{}\" project_trust error: boom",
            path
        )]
    );
    assert_eq!(ui.labels().len(), 5);
}

#[tokio::test]
async fn unreadable_trust_store_errors_instead_of_silently_untrusting() {
    let (agent_dir, project) = trust_requiring_project("bad-store");
    fs::write(agent_dir.join("trust.json"), "{not json").unwrap();
    let store = ProjectTrustStore::new(&JsString::from(&s(&agent_dir))).unwrap();
    let ui = RecordingUi::selecting("Trust");
    let project_path = s(&project);
    let ctx = context(&project_path, true, ui.clone());

    let error: TrustError = resolve_project_trusted(options(&project_path, &store, &ctx))
        .await
        .expect_err("corrupt trust store must surface");
    assert!(error.to_string().contains("trust store"), "unexpected error: {error}");
    assert!(ui.labels().is_empty());
}

struct RecordingStartupUi {
    selections: Mutex<Vec<(JsString, Vec<JsString>)>>,
    inputs: Mutex<Vec<(JsString, Option<JsString>)>>,
    selection: Option<JsString>,
    input: Option<JsString>,
}

impl RecordingStartupUi {
    fn new(selection: Option<&str>, input: Option<&str>) -> Arc<Self> {
        Arc::new(Self {
            selections: Mutex::new(Vec::new()),
            inputs: Mutex::new(Vec::new()),
            selection: selection.map(JsString::from),
            input: input.map(JsString::from),
        })
    }
}

impl ProjectTrustStartupUi for RecordingStartupUi {
    fn select<'a>(
        &'a self,
        title: &'a JsString,
        options: &'a [JsString],
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        self.selections
            .lock()
            .unwrap()
            .push((title.to_owned(), options.to_vec()));
        let selection = self.selection.clone();
        Box::pin(async move { Ok(selection) })
    }

    fn input<'a>(
        &'a self,
        title: &'a JsString,
        placeholder: Option<&'a JsString>,
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        self.inputs
            .lock()
            .unwrap()
            .push((title.to_owned(), placeholder.cloned()));
        let value = self.input.clone();
        Box::pin(async move { Ok(value) })
    }
}

fn cli_context(
    cwd: &JsString,
    mode: AppMode,
    has_ui: bool,
    startup_ui: Option<Arc<RecordingStartupUi>>,
) -> ProjectTrustContext {
    create_project_trust_context(CreateProjectTrustContextOptions {
        cwd: cwd.to_owned().into(),
        mode,
        has_ui,
        startup_ui: startup_ui.map(|ui| ui as Arc<dyn ProjectTrustStartupUi>),
    })
}

#[tokio::test]
async fn cli_context_maps_app_modes_to_extension_modes() {
    let startup = RecordingStartupUi::new(None, None);
    let cases = [
        (AppMode::Interactive, ExtensionMode::Tui),
        (AppMode::Print, ExtensionMode::Print),
        (AppMode::Json, ExtensionMode::Json),
        (AppMode::Rpc, ExtensionMode::Rpc),
    ];
    for (mode, expected) in cases {
        let ctx = cli_context(&JsString::from("/work"), mode, true, Some(startup.clone()));
        assert_eq!(ctx.cwd, "/work");
        assert_eq!(ctx.mode, expected);
        assert!(ctx.has_ui);
    }
}

#[tokio::test]
async fn cli_context_selects_through_the_startup_ui_in_interactive_mode() {
    let startup = RecordingStartupUi::new(Some("Yes"), Some("typed"));
    let ctx = cli_context(
        &JsString::from("/work"),
        AppMode::Interactive,
        true,
        Some(startup.clone()),
    );

    let selected = ctx
        .ui
        .select(
            &JsString::from("Pick"),
            &["Yes".to_owned().into(), "No".to_owned().into()],
            None,
        )
        .await
        .unwrap();
    assert_eq!(selected.as_ref(), Some(&JsString::from("Yes")));
    assert_eq!(
        *startup.selections.lock().unwrap(),
        vec![(
            JsString::from("Pick"),
            vec![JsString::from("Yes"), JsString::from("No")]
        )]
    );

    assert_eq!(
        ctx.ui
            .input(&JsString::from("Name"), Some(&JsString::from("placeholder")), None)
            .await
            .unwrap(),
        (Some("typed".to_owned())).map(JsString::from)
    );
    assert_eq!(
        *startup.inputs.lock().unwrap(),
        vec![(JsString::from("Name"), Some(JsString::from("placeholder")))]
    );
}

#[tokio::test]
async fn cli_context_confirm_uses_yes_no_labels() {
    let startup = RecordingStartupUi::new(Some("Yes"), None);
    let ctx = cli_context(
        &JsString::from("/work"),
        AppMode::Interactive,
        true,
        Some(startup.clone()),
    );
    assert!(
        ctx.ui
            .confirm(&JsString::from("Title"), &JsString::from("Body"), None)
            .await
            .unwrap()
    );
    assert_eq!(
        *startup.selections.lock().unwrap(),
        vec![(
            JsString::from("Title\nBody"),
            vec![JsString::from("Yes"), JsString::from("No")]
        )]
    );

    let startup = RecordingStartupUi::new(Some("No"), None);
    let ctx = cli_context(&JsString::from("/work"), AppMode::Interactive, true, Some(startup));
    assert!(
        !ctx.ui
            .confirm(&JsString::from("Title"), &JsString::from("Body"), None)
            .await
            .unwrap()
    );

    let startup = RecordingStartupUi::new(None, None);
    let ctx = cli_context(&JsString::from("/work"), AppMode::Interactive, true, Some(startup));
    assert!(
        !ctx.ui
            .confirm(&JsString::from("Title"), &JsString::from("Body"), None)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn cli_context_without_ui_never_prompts() {
    let startup = RecordingStartupUi::new(Some("Yes"), Some("typed"));
    let ctx = cli_context(
        &JsString::from("/work"),
        AppMode::Interactive,
        false,
        Some(startup.clone()),
    );
    assert_eq!(
        ctx.ui
            .select(&JsString::from("Pick"), &["Yes".to_owned().into()], None)
            .await
            .unwrap(),
        None
    );
    assert!(
        !ctx.ui
            .confirm(&JsString::from("Title"), &JsString::from("Body"), None)
            .await
            .unwrap()
    );
    assert_eq!(ctx.ui.input(&JsString::from("Name"), None, None).await.unwrap(), None);
    assert!(startup.selections.lock().unwrap().is_empty());
    assert!(startup.inputs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn cli_context_outside_interactive_mode_never_prompts() {
    for mode in [AppMode::Print, AppMode::Json, AppMode::Rpc] {
        let startup = RecordingStartupUi::new(Some("Yes"), Some("typed"));
        let ctx = cli_context(&JsString::from("/work"), mode, true, Some(startup.clone()));
        assert_eq!(
            ctx.ui
                .select(&JsString::from("Pick"), &["Yes".to_owned().into()], None)
                .await
                .unwrap(),
            None
        );
        assert!(
            !ctx.ui
                .confirm(&JsString::from("Title"), &JsString::from("Body"), None)
                .await
                .unwrap()
        );
        assert_eq!(ctx.ui.input(&JsString::from("Name"), None, None).await.unwrap(), None);
        assert!(startup.selections.lock().unwrap().is_empty());
        assert!(startup.inputs.lock().unwrap().is_empty());
    }
}
