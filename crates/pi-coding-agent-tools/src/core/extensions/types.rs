//! Extension contracts (`core/extensions/types.ts`).
//!
//! Rust-native extensions replace the TypeScript jiti-loaded extension files.
//! The event payloads, handler ordering, and result merging match the source;
//! the extension entry point is a Rust factory receiving [`ExtensionApi`].

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use pi_agent_core::agent::types::{AgentMessage, AgentToolResult, AgentToolUpdateCallback};
use pi_agent_core::harness::types::ThinkingLevel;
use pi_agent_core::harness::types::ToolExecutionMode;
use pi_ai::types::{AssistantMessageEvent, ImageContent, Model, TextOrImageContent, ToolCallExecution, Usage};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use pi_tui::keybindings::KeyId;
use pi_tui::tui::ComponentRef;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::detached_job_runner::DetachedJobLifecycleController;
use crate::core::event_bus::EventBus;
use crate::core::exec::ExecResult;
pub use crate::core::footer_data_provider::ExtensionStatus;
use crate::core::footer_data_provider::FooterDataProvider;
use crate::core::model_registry::ModelRegistry;
use crate::core::model_registry::ProviderConfigInput;
use crate::core::model_resolver::ScopedModel;
use crate::core::multi_agent_store::MultiAgentStore;
use crate::core::permissions::presets::{SandboxProfileName, SandboxProfileScope};
use crate::core::session_control_db::RuntimeMailboxMessage;
use crate::core::session_manager::{
    BranchSummaryEntry, CompactionEntry, CustomEntry, SessionEntry, SessionManager, SharedSessionManager,
};
use crate::core::settings_manager::SettingsManager;
use crate::core::slash_commands::SlashCommandInfo;
use crate::core::source_info::SourceInfo;
use crate::core::system_prompt::BuildSystemPromptOptions;
use crate::core::tool_detach_registry::ToolDetachRegistry;
use crate::modes::interactive::theme::{Theme, current_theme};

use super::extension_runtime::ExtensionRuntime;

/// Boxed, sendable future used by every asynchronous extension callback.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub use pi_coding_agent_tool_support::core::extensions::types::HandlerError;

/// Typed handler for one event/result pair.
///
/// Events are passed by value (a cheap `Clone` snapshot); contexts are cheap
/// clones of the runner's shared live state. Handlers return result patches
/// instead of mutating shared state.
pub trait Handler<E, R>: Send + Sync {
    fn call(&self, event: E, ctx: ExtensionContext) -> BoxFuture<'static, Result<Option<R>, HandlerError>>;
}

impl<E, R, F> Handler<E, R> for F
where
    E: Send + 'static,
    R: Send + 'static,
    F: Fn(E, ExtensionContext) -> BoxFuture<'static, Result<Option<R>, HandlerError>> + Send + Sync,
{
    fn call(&self, event: E, ctx: ExtensionContext) -> BoxFuture<'static, Result<Option<R>, HandlerError>> {
        self(event, ctx)
    }
}

impl<E, R, T: Handler<E, R> + ?Sized> Handler<E, R> for Arc<T> {
    fn call(&self, event: E, ctx: ExtensionContext) -> BoxFuture<'static, Result<Option<R>, HandlerError>> {
        (**self).call(event, ctx)
    }
}

pub type HandlerHandle<E, R> = Arc<dyn Handler<E, R> + Send + Sync>;

pub type CompactCompleteCallback = Arc<dyn Fn(&CompactionResult) -> Result<(), CallbackError> + Send + Sync>;
pub type CompactErrorCallback = Arc<dyn Fn(&CallbackError) -> Result<(), CallbackError> + Send + Sync>;
pub type StrCallback = Arc<dyn Fn(&str) + Send + Sync>;
pub type JsStringCallback = Arc<dyn Fn(&JsString) + Send + Sync>;
pub type SessionSetupCallback =
    Arc<dyn Fn(&mut SessionManager) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>;
pub type WithSessionCallback =
    Arc<dyn Fn(ReplacedSessionContext) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>;
pub type RelocateCallback = Arc<dyn Fn(&JsString) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>;
pub type ActivateToolResultRelocationHandler =
    Arc<dyn Fn() -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>;
pub type NewSessionCallback = Arc<
    dyn Fn(Option<NewSessionOptions>) -> BoxFuture<'static, Result<SessionTransitionResult, HandlerError>>
        + Send
        + Sync,
>;
pub type ForkCallback = Arc<
    dyn Fn(JsString, Option<ForkOptions>) -> BoxFuture<'static, Result<SessionTransitionResult, HandlerError>>
        + Send
        + Sync,
>;
pub type NavigateTreeCallback = Arc<
    dyn Fn(JsString, Option<NavigateTreeOptions>) -> BoxFuture<'static, Result<SessionTransitionResult, HandlerError>>
        + Send
        + Sync,
>;
pub type SwitchSessionCallback = Arc<
    dyn Fn(JsString, Option<SwitchSessionOptions>) -> BoxFuture<'static, Result<SessionTransitionResult, HandlerError>>
        + Send
        + Sync,
>;
pub type ReplacedSendMessageCallback = Arc<
    dyn Fn(CustomMessagePayload, Option<SendMessageOptions>) -> BoxFuture<'static, Result<(), HandlerError>>
        + Send
        + Sync,
>;
pub type ReplacedSendUserMessageCallback = Arc<
    dyn Fn(UserMessageContent, Option<SendUserMessageOptions>) -> BoxFuture<'static, Result<(), HandlerError>>
        + Send
        + Sync,
>;
pub type ContextShortcutHandler =
    Arc<dyn Fn(ExtensionContext) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>;
// Frozen extensions/runner.ts:461–473 propagates immediate provider failures.
pub type RegisterProviderCallback = Arc<dyn Fn(&str, &ProviderConfig) -> Result<(), CallbackError> + Send + Sync>;
pub type UnregisterProviderCallback = Arc<dyn Fn(&str) -> Result<(), CallbackError> + Send + Sync>;
pub type RegisterProviderHandler =
    Arc<dyn Fn(&str, &ProviderConfig, Option<&JsString>) -> Result<(), CallbackError> + Send + Sync>;
pub type UnregisterProviderRuntimeCallback =
    Arc<dyn Fn(&str, Option<&JsString>) -> Result<(), CallbackError> + Send + Sync>;

pub trait ApprovalReviewer: Send + Sync {
    fn call(
        &self,
        event: ToolCallEvent,
        ctx: ExtensionContext,
    ) -> BoxFuture<'static, Result<Option<ApprovalReviewerResult>, HandlerError>>;
}

impl<F> ApprovalReviewer for F
where
    F: Fn(ToolCallEvent, ExtensionContext) -> BoxFuture<'static, Result<Option<ApprovalReviewerResult>, HandlerError>>
        + Send
        + Sync,
{
    fn call(
        &self,
        event: ToolCallEvent,
        ctx: ExtensionContext,
    ) -> BoxFuture<'static, Result<Option<ApprovalReviewerResult>, HandlerError>> {
        self(event, ctx)
    }
}

pub trait ToolGate: Send + Sync {
    fn call(
        &self,
        event: ToolCallEvent,
        ctx: ExtensionContext,
    ) -> BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>>;
}

impl<F> ToolGate for F
where
    F: Fn(ToolCallEvent, ExtensionContext) -> BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>>
        + Send
        + Sync,
{
    fn call(
        &self,
        event: ToolCallEvent,
        ctx: ExtensionContext,
    ) -> BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>> {
        self(event, ctx)
    }
}

pub type ApprovalReviewerHandle = Arc<dyn ApprovalReviewer + Send + Sync>;
pub type ToolGateHandle = Arc<dyn ToolGate + Send + Sync>;

// ============================================================================
// UI Context
// ============================================================================

/// `AbortSignal` replacement for dialog dismissal.
#[derive(Debug, Clone, Default)]
pub struct ExtensionUiDialogOptions {
    pub signal: Option<AbortSignal>,
    /// Timeout in milliseconds.
    pub timeout: Option<f64>,
    pub secret: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WidgetPlacement {
    AboveEditor,
    BelowEditor,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionWidgetOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<WidgetPlacement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalInputResult {
    pub consume: Option<bool>,
    pub data: Option<JsString>,
}

/// UI-thread component returned by a thread-safe extension factory.
///
/// Only factories cross [`ExtensionUiContext`]'s thread boundary. Their outputs
/// stay on the UI thread and may own `Rc`-based `pi-tui` editors, matching
/// `question.ts:73–89` and `questionnaire.ts:110–130`.
/// `render` takes `&self`; implementors use interior mutability.
pub trait ExtensionUiComponent {
    /// Optional source `width` property used by `custom({ overlay: true })`.
    fn width(&self) -> Option<pi_tui::tui::SizeValue> {
        None
    }

    /// `None` means the source component has no `focused` property.
    fn focused(&self) -> Option<bool> {
        None
    }

    fn set_focused(&self, _focused: bool) {}

    fn render(&self, width: usize) -> Vec<JsString>;

    fn handle_input(&self, _data: &JsString) {}

    fn invalidate(&self) {}

    fn wants_input(&self) -> bool {
        true
    }

    /// `Component.wantsKeyRelease` (`packages/tui/src/tui.ts:47,1156`).
    fn wants_key_release(&self) -> bool {
        false
    }

    fn dispose(&self) {}

    /// UI-local `custom` onHandle bridge (`interactive-mode.ts:3518–3520`).
    /// Called once by `custom` immediately after mounting, not by raw
    /// `show_overlay`. Unlike the shared
    /// options callback, this hook may retain the handle in `Rc` state.
    fn on_overlay_handle(&self, _handle: Rc<dyn ExtensionUiOverlayHandle>) {}
}

/// Backend behind the [`ExtensionUiTui`] handle passed to factories.
pub trait ExtensionUiTuiBackend: Send + Sync {
    /// Called on the owning UI thread, like the editor factory itself.
    fn create_fixed_cell(&self) -> Rc<dyn pi_tui::components::editor::EditorFixedCell>;
    fn request_render(&self);
    fn request_render_force(&self);
    fn stop(&self) -> Result<(), CallbackError>;
    fn start(&self) -> Result<(), CallbackError>;
    fn terminal_rows(&self) -> usize;
    fn terminal_columns(&self) -> usize;
    /// UI-thread operation; source `tui.ts:439,770–862`.
    fn show_overlay(
        &self,
        component: Box<dyn ExtensionUiComponent>,
        options: Option<ExtensionUiOverlayOptions>,
    ) -> Rc<dyn ExtensionUiOverlayHandle>;
}

/// Thread-safe `TUI` handle passed to extension factories.
#[derive(Clone)]
pub struct ExtensionUiTui {
    backend: Arc<dyn ExtensionUiTuiBackend>,
}

impl ExtensionUiTui {
    pub fn new(backend: Arc<dyn ExtensionUiTuiBackend>) -> Self {
        Self { backend }
    }

    pub fn request_render(&self) {
        self.backend.request_render();
    }

    /// Mount a nested overlay on the owning UI thread (`tui.ts:439,770–862`).
    pub fn show_overlay(
        &self,
        component: Box<dyn ExtensionUiComponent>,
        options: Option<ExtensionUiOverlayOptions>,
    ) -> Rc<dyn ExtensionUiOverlayHandle> {
        self.backend.show_overlay(component, options)
    }

    /// `TUI.requestRender(true)` (`packages/tui/src/tui.ts:998`).
    pub fn request_render_force(&self) {
        self.backend.request_render_force();
    }

    /// Synchronously release the terminal (`packages/tui/src/tui.ts:972`).
    /// Like component factories, lifecycle calls require the owning UI thread.
    pub fn stop(&self) -> Result<(), CallbackError> {
        self.backend.stop()
    }

    /// Reacquire the terminal (`packages/tui/src/tui.ts:912`).
    pub fn start(&self) -> Result<(), CallbackError> {
        self.backend.start()
    }

    pub fn terminal_rows(&self) -> usize {
        self.backend.terminal_rows()
    }

    pub fn terminal_columns(&self) -> usize {
        self.backend.terminal_columns()
    }

    pub fn create_fixed_cell(&self) -> Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        self.backend.create_fixed_cell()
    }
}

impl pi_tui::components::editor::EditorHost for ExtensionUiTui {
    fn terminal_rows(&self) -> usize {
        self.terminal_rows()
    }

    fn request_render(&self) {
        self.request_render();
    }

    fn create_fixed_cell(&self) -> Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        self.create_fixed_cell()
    }
}

/// Arguments passed to `setWidget`/`setHeader`/`custom` component factories.
#[derive(Clone)]
pub struct ExtensionUiFactoryContext {
    pub tui: ExtensionUiTui,
    pub theme: Arc<Theme>,
    pub keybindings: crate::core::keybindings::KeybindingsManager,
}

/// Arguments passed to `setFooter`/`setDefaultFooter` component factories.
#[derive(Clone)]
pub struct ExtensionUiFooterContext {
    pub tui: ExtensionUiTui,
    pub theme: Arc<Theme>,
    pub footer_data: FooterDataProvider,
}

/// Component factory used by `setWidget` and `custom`.
pub type ExtensionUiComponentFactory =
    Arc<dyn Fn(ExtensionUiFactoryContext) -> Box<dyn ExtensionUiComponent> + Send + Sync>;

/// Optional source `setExpanded` function; retained on the UI owner, never sent across threads.
pub type HeaderExpansionHandler = Rc<dyn Fn(bool) -> Result<(), pi_ai::request::CallbackError>>;

/// Header registration preserves the frozen component's optional `setExpanded` shape.
pub struct ExtensionUiHeader {
    pub component: Box<dyn ExtensionUiComponent>,
    pub set_expanded: Option<HeaderExpansionHandler>,
}

pub type ExtensionUiHeaderFactory =
    Arc<dyn Fn(ExtensionUiFactoryContext) -> Result<ExtensionUiHeader, pi_ai::request::CallbackError> + Send + Sync>;

/// Component factory used by `setFooter` and `setDefaultFooter`.
pub type ExtensionUiFooterFactory =
    Arc<dyn Fn(ExtensionUiFooterContext) -> Box<dyn ExtensionUiComponent> + Send + Sync>;

/// `addAutocompleteProvider` wrapper: wraps the active provider on the TUI thread.
pub type ExtensionAutocompleteProviderFactory = Arc<
    dyn Fn(Rc<dyn pi_tui::autocomplete::AutocompleteProvider>) -> Rc<dyn pi_tui::autocomplete::AutocompleteProvider>
        + Send
        + Sync,
>;

/// `setEditorComponent` factory: builds an editor on the TUI thread.
pub type ExtensionEditorFactory = Arc<
    dyn Fn(
            ExtensionUiTui,
            pi_tui::components::editor::EditorTheme,
            crate::core::keybindings::KeybindingsManager,
        ) -> Rc<RefCell<dyn pi_tui::EditorComponent>>
        + Send
        + Sync,
>;

/// UI-local `done` callback (`interactive-mode.ts:3486–3505`).
/// Factory closures cross threads; their created callbacks/components do not.
#[derive(Clone)]
pub struct ExtensionUiDone(Rc<dyn Fn(Option<JsValue>)>);

impl ExtensionUiDone {
    pub fn new(callback: Rc<dyn Fn(Option<JsValue>)>) -> Self {
        Self(callback)
    }

    pub fn call(&self, value: Option<JsValue>) {
        (self.0)(value);
    }
}

/// UI-local factory continuation; only its factory closure crosses threads.
/// Outer errors model synchronous throws; future errors model Promise rejection.
pub type ExtensionUiCustomResult =
    Result<futures_util::future::LocalBoxFuture<'static, Result<Box<dyn ExtensionUiComponent>, JsString>>, JsString>;

/// Custom-UI factories may finish immediately or await UI-local work.
pub type ExtensionUiCustomFactory =
    Arc<dyn Fn(ExtensionUiFactoryContext, ExtensionUiDone) -> ExtensionUiCustomResult + Send + Sync>;

/// Return an already constructed component through the common future path.
pub fn ready_custom_component(component: Box<dyn ExtensionUiComponent>) -> ExtensionUiCustomResult {
    Ok(Box::pin(std::future::ready(Ok(component))))
}

/// UI-thread overlay handle passed to `custom`'s `onHandle` callback.
pub trait ExtensionUiOverlayHandle {
    fn hide(&self);
    fn set_hidden(&self, hidden: bool);
    fn is_hidden(&self) -> bool;
    fn focus(&self);
    fn unfocus(&self);
    /// Source `tui.ts:828–858`: explicit target; `None` clears focus.
    fn unfocus_to(&self, target: Option<pi_tui::tui::ComponentRef>);
    /// Source `tui.ts:861`: whether this overlay's component currently owns focus.
    fn is_focused(&self) -> bool;
}

/// Overlay positioning/sizing options accepted by `custom`.
///
/// Mirrors `OverlayOptions`; `visible` uses a `Send + Sync` callback so the
/// options can cross the extension boundary.
#[derive(Clone, Default)]
pub struct ExtensionUiOverlayOptions {
    pub width: Option<pi_tui::tui::SizeValue>,
    pub min_width: Option<i64>,
    pub max_height: Option<pi_tui::tui::SizeValue>,
    pub anchor: Option<pi_tui::tui::OverlayAnchor>,
    pub offset_x: Option<i64>,
    pub offset_y: Option<i64>,
    pub row: Option<pi_tui::tui::SizeValue>,
    pub col: Option<pi_tui::tui::SizeValue>,
    pub margin: Option<pi_tui::tui::OverlayMargin>,
    pub visible: Option<Arc<dyn Fn(usize, usize) -> bool + Send + Sync>>,
    pub non_capturing: bool,
}

impl From<ExtensionUiOverlayOptions> for pi_tui::tui::OverlayOptions {
    fn from(options: ExtensionUiOverlayOptions) -> Self {
        Self {
            width: options.width,
            min_width: options.min_width,
            max_height: options.max_height,
            anchor: options.anchor,
            offset_x: options.offset_x,
            offset_y: options.offset_y,
            row: options.row,
            col: options.col,
            margin: options.margin,
            visible: options
                .visible
                .map(|visible| Rc::new(move |w, h| visible(w, h)) as Rc<dyn Fn(usize, usize) -> bool>),
            non_capturing: options.non_capturing,
        }
    }
}

/// `custom` options: overlay mode, dynamic overlay options, and handle callback.
#[derive(Clone, Default)]
pub struct ExtensionUiCustomOptions {
    pub overlay: Option<bool>,
    pub overlay_options: Option<Arc<dyn Fn() -> ExtensionUiOverlayOptions + Send + Sync>>,
    pub on_handle: Option<ExtensionUiOnHandle>,
}

/// `onHandle` callback receiving the overlay handle.
pub type ExtensionUiOnHandle = Arc<dyn Fn(Rc<dyn ExtensionUiOverlayHandle>) + Send + Sync>;

pub type TerminalInputHandler = Arc<dyn Fn(&JsString) -> Option<TerminalInputResult> + Send + Sync>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingIndicatorOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<Vec<JsString>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "pi_js_compat::number::serialize_option"
    )]
    pub interval_ms: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifyType {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeInfo {
    pub name: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeSetResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsString>,
}

/// UI surface exposed to extensions. Defaults match the source `noOpUIContext`;
/// the interactive and RPC modes override the methods they support.
pub trait ExtensionUiContext: Send + Sync {
    fn select<'a>(
        &'a self,
        _title: &JsString,
        _options: &[JsString],
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        Box::pin(async { Ok(None) })
    }

    fn confirm<'a>(
        &'a self,
        _title: &JsString,
        _message: &JsString,
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, CallbackError>> {
        Box::pin(async { Ok(false) })
    }

    fn input<'a>(
        &'a self,
        _title: &JsString,
        _placeholder: Option<&JsString>,
        _opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        Box::pin(async { Ok(None) })
    }

    fn notify(&self, _message: &JsString, _notify_type: Option<NotifyType>) -> Result<(), CallbackError> {
        Ok(())
    }

    fn request_render(&self) {}

    fn on_terminal_input(&self, _handler: TerminalInputHandler) -> Box<dyn Fn() + Send + Sync> {
        Box::new(|| {})
    }

    fn set_status(&self, _key: &JsString, _text: Option<ExtensionStatus>) -> Result<(), CallbackError> {
        Ok(())
    }

    fn set_working_message(&self, _message: Option<&JsString>) {}

    fn set_working_visible(&self, _visible: bool) {}

    fn set_working_indicator(&self, _options: Option<&WorkingIndicatorOptions>) {}

    fn set_hidden_thinking_label(&self, _label: Option<&JsString>) {}

    /// String-array widget overload. The component-factory overload is
    /// [`ExtensionUiContext::set_widget_component`].
    fn set_widget(
        &self,
        _key: &JsString,
        _content: Option<&[JsString]>,
        _options: Option<&ExtensionWidgetOptions>,
    ) -> Result<(), CallbackError> {
        Ok(())
    }

    /// Component-factory `setWidget` overload.
    fn set_widget_component(
        &self,
        _key: &JsString,
        _content: Option<ExtensionUiComponentFactory>,
        _options: Option<&ExtensionWidgetOptions>,
    ) {
    }

    /// Set a custom footer component, or `None` to restore the built-in footer.
    fn set_footer(&self, _factory: Option<ExtensionUiFooterFactory>) {}

    /// Set the default footer component. Custom footers override it.
    fn set_default_footer(&self, _factory: Option<ExtensionUiFooterFactory>) {}

    /// Set a custom header component, or `None` to restore the built-in header.
    fn set_header(&self, _factory: Option<ExtensionUiHeaderFactory>) -> Result<(), pi_ai::request::CallbackError> {
        Ok(())
    }

    /// Show a custom component with keyboard focus and return the `done` value.
    fn custom<'a>(
        &'a self,
        _factory: ExtensionUiCustomFactory,
        _options: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
        Box::pin(async { Ok(None) })
    }

    fn set_title(&self, _title: &JsString) -> Result<(), CallbackError> {
        Ok(())
    }

    fn paste_to_editor(&self, _text: &JsString) -> Result<(), CallbackError> {
        Ok(())
    }

    fn set_editor_text(&self, _text: &JsString) -> Result<(), CallbackError> {
        Ok(())
    }

    fn get_editor_text(&self) -> Result<JsString, CallbackError> {
        Ok(JsString::default())
    }

    fn editor<'a>(
        &'a self,
        _title: &JsString,
        _prefill: Option<&JsString>,
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        Box::pin(async { Ok(None) })
    }

    fn get_all_themes(&self) -> Vec<ThemeInfo> {
        Vec::new()
    }

    /// Load a theme by name without switching to it.
    fn get_theme(&self, _name: &JsString) -> Option<Arc<Theme>> {
        None
    }

    fn set_theme(&self, _theme: &JsString) -> ThemeSetResult {
        ThemeSetResult {
            success: false,
            error: Some("UI not available".into()),
        }
    }

    /// `setTheme(theme: Theme)`: install an in-memory theme object.
    fn set_theme_object(&self, _theme: Arc<Theme>) -> ThemeSetResult {
        ThemeSetResult {
            success: false,
            error: Some("UI not available".into()),
        }
    }

    fn get_tools_expanded(&self) -> bool {
        false
    }

    fn set_tools_expanded(&self, _expanded: bool) {}

    /// Wrap the active autocomplete provider (`addAutocompleteProvider`).
    fn add_autocomplete_provider(&self, _factory: ExtensionAutocompleteProviderFactory) {}

    /// Install a custom editor component, or `None` to restore the default.
    fn set_editor_component(&self, _factory: Option<ExtensionEditorFactory>) {}

    fn get_editor_component(&self) -> Option<ExtensionEditorFactory> {
        None
    }

    fn theme(&self) -> Arc<Theme> {
        current_theme()
    }
}

/// `BeforeProviderRequestEventResult` (`unknown` in the source): the handler
/// returns the replacement payload.
pub type BeforeProviderRequestEventResult = Value;

/// `AutocompleteProviderFactory`: wraps the current provider.
///
/// The source factory is `(current: AutocompleteProvider) => AutocompleteProvider`;
/// Rust hands the shared trait object through.
pub type AutocompleteProviderFactory = Arc<
    dyn Fn(Rc<dyn pi_tui::autocomplete::AutocompleteProvider>) -> Rc<dyn pi_tui::autocomplete::AutocompleteProvider>
        + Send
        + Sync,
>;

/// `EditorFactory`: builds a custom editor component.
///
/// The source returns `EditorComponent`; Rust returns a shared trait object.
pub type EditorFactory = Arc<
    dyn Fn(
            ExtensionUiTui,
            pi_tui::components::editor::EditorTheme,
            crate::core::keybindings::KeybindingsManager,
        ) -> Rc<RefCell<dyn pi_tui::EditorComponent>>
        + Send
        + Sync,
>;

/// `SessionMutationTarget`: the session a multi-agent selection points at.
///
/// TypeScript declares a plain object with a getter pair and mutators; Rust
/// models it as a trait so the resolver can hand out a live handle.
pub trait SessionMutationTarget: Send + Sync {
    fn model(&self) -> Option<Model>;
    fn thinking_level(&self) -> pi_agent_core::harness::types::ThinkingLevel;
    /// Applies the model and reports whether the target accepted it, matching
    /// the source `setModel(): boolean` contract.
    fn set_model(&self, model: Model) -> BoxFuture<'static, Result<bool, HandlerError>>;
    fn set_thinking_level(&self, level: pi_agent_core::harness::types::ThinkingLevel) -> Result<(), HandlerError>;
}

/// `ViewedSessionMutationTarget`: adds the registry and scoped models the
/// interactive session viewer exposes.
pub trait ViewedSessionMutationTarget: SessionMutationTarget {
    fn cycle_model(
        &self,
        direction: crate::core::agent_session::CycleDirection,
    ) -> BoxFuture<'static, Result<Option<crate::core::agent_session::ModelCycleResult>, HandlerError>>;
    /// Source `setModel(model, "set", thinkingLevel)` applies effort in the same model selection.
    fn set_model_with_thinking_level(
        &self,
        model: Model,
        thinking_level: Option<pi_agent_core::harness::types::ThinkingLevel>,
    ) -> BoxFuture<'static, Result<bool, HandlerError>>;
    fn model_registry(&self) -> Arc<crate::core::model_registry::ModelRegistry>;
    fn scoped_models(&self) -> Vec<crate::core::model_resolver::ScopedModel>;
}

/// The source `noOpUIContext`.
pub struct NoOpUiContext;

impl ExtensionUiContext for NoOpUiContext {}

// ============================================================================
// Extension Context
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsage {
    /// Estimated context tokens, or `None` when unknown.
    #[serde(serialize_with = "pi_js_compat::number::serialize_option")]
    pub tokens: Option<f64>,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub context_window: f64,
    /// Context usage as percentage of the context window, or `None`.
    #[serde(serialize_with = "pi_js_compat::number::serialize_option")]
    pub percent: Option<f64>,
}

#[derive(Clone, Default)]
pub struct CompactOptions {
    pub custom_instructions: Option<JsString>,
    pub on_complete: Option<CompactCompleteCallback>,
    pub on_error: Option<CompactErrorCallback>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtensionMode {
    Tui,
    Rpc,
    Json,
    Print,
}

/// Model registry surface exposed to extensions.
pub type ExtensionModelRegistry = ModelRegistry;

/// `ScopedModel`: a model plus an optional explicit thinking-level suffix.
pub type ScopedModelView = ScopedModel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTransitionResult {
    pub cancelled: bool,
}

#[derive(Clone)]
pub struct RestartOptions {
    pub notice: Option<JsString>,
    pub process: Option<bool>,
}

/// Restart handler stored on live extension contexts.
pub type RestartHandler =
    Arc<dyn Fn(Option<RestartOptions>) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>;

/// Session control methods only safe in user-initiated commands.
#[derive(Clone, Default)]
pub struct NewSessionOptions {
    pub parent_session: Option<JsString>,
    pub setup: Option<SessionSetupCallback>,
    pub with_session: Option<WithSessionCallback>,
}

#[derive(Clone, Default)]
pub struct ForkOptions {
    pub position: Option<ForkPosition>,
    pub with_session: Option<WithSessionCallback>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForkPosition {
    Before,
    At,
}

#[derive(Clone, Default)]
pub struct NavigateTreeOptions {
    pub summarize: Option<bool>,
    pub custom_instructions: Option<JsString>,
    pub replace_instructions: Option<bool>,
    pub label: Option<JsString>,
}

#[derive(Clone, Default)]
pub struct SwitchSessionOptions {
    pub with_session: Option<WithSessionCallback>,
}

/// Read-only footer data available to extension contexts.
pub type ExtensionFooterDataProvider = FooterDataProvider;

/// Live runner state backing every [`ExtensionContext`] member.
///
/// TypeScript's `createContext()` returns getters that call
/// `runner.assertActive()` and then read current runner fields. Rust contexts
/// hold this shared state and expose the same members as fallible methods.
pub type SetSandboxProfileCallback =
    Arc<dyn Fn(Option<SandboxProfileName>, SandboxProfileScope) -> Result<(), HandlerError> + Send + Sync>;

pub struct ExtensionContextState {
    pub(super) stale_message: Option<String>,
    pub(super) ui_context: Arc<dyn ExtensionUiContext>,
    pub(super) has_ui: bool,
    pub(super) mode: ExtensionMode,
    pub(super) cwd: JsString,
    pub(super) session_manager: SharedSessionManager,
    pub(super) model_registry: Arc<ModelRegistry>,
    pub(super) settings_manager: Option<Arc<Mutex<SettingsManager>>>,
    pub(super) get_model: Arc<dyn Fn() -> Option<Model> + Send + Sync>,
    pub(super) set_model_fn: SetModelHandler,
    pub(super) get_thinking_level_fn: Arc<dyn Fn() -> ThinkingLevel + Send + Sync>,
    pub(super) set_thinking_level_fn: Arc<dyn Fn(ThinkingLevel) -> Result<(), HandlerError> + Send + Sync>,
    pub(super) get_scoped_models: Arc<dyn Fn() -> Vec<ScopedModel> + Send + Sync>,
    pub(super) get_footer_data: Arc<dyn Fn() -> Option<Arc<FooterDataProvider>> + Send + Sync>,
    pub(super) is_idle_fn: Arc<dyn Fn() -> bool + Send + Sync>,
    pub(super) has_active_retry_fn: Arc<dyn Fn() -> bool + Send + Sync>,
    pub(super) is_project_trusted_fn: Arc<dyn Fn() -> bool + Send + Sync>,
    pub(super) get_signal_fn: Arc<dyn Fn() -> Option<AbortSignal> + Send + Sync>,
    pub(super) abort_fn: Arc<dyn Fn() + Send + Sync>,
    pub(super) has_pending_messages_fn: Arc<dyn Fn() -> bool + Send + Sync>,
    pub(super) shutdown_handler: Arc<dyn Fn() + Send + Sync>,
    pub(super) restart_fn: RestartHandler,
    pub(super) get_control_db_path_fn: Arc<dyn Fn() -> Option<JsString> + Send + Sync>,
    pub(super) get_context_usage_fn: Arc<dyn Fn() -> Option<ContextUsage> + Send + Sync>,
    pub(super) get_multi_agent_agent_id_fn: Arc<dyn Fn() -> Option<JsString> + Send + Sync>,
    pub(super) get_multi_agent_parent_session_id_fn: Arc<dyn Fn() -> Option<JsString> + Send + Sync>,
    pub(super) get_no_supervisor_fn: Arc<dyn Fn() -> bool + Send + Sync>,
    pub(super) get_multi_agent_requires_agent_id_fn: Arc<dyn Fn() -> bool + Send + Sync>,
    pub(super) get_multi_agent_store_fn: Arc<dyn Fn() -> Option<Arc<MultiAgentStore>> + Send + Sync>,
    pub(super) get_detached_job_lifecycle_fn:
        Arc<dyn Fn() -> Option<Arc<DetachedJobLifecycleController>> + Send + Sync>,
    pub(super) get_tool_detach_registry_fn: Arc<dyn Fn() -> Option<Arc<ToolDetachRegistry>> + Send + Sync>,
    pub(super) compact_fn: Arc<dyn Fn(Option<CompactOptions>) + Send + Sync>,
    pub(super) get_system_prompt_fn: Arc<dyn Fn() -> JsString + Send + Sync>,
    pub(super) get_system_prompt_options_fn: Option<Arc<dyn Fn() -> BuildSystemPromptOptions + Send + Sync>>,
    pub(super) relocate_handler: RelocateCallback,
    pub(super) prepare_tool_result_relocation_handler: RelocateCallback,
    pub(super) activate_tool_result_relocation_handler: ActivateToolResultRelocationHandler,
    pub(super) pending_tool_result_relocations: IndexMap<String, JsString>,
    pub(super) show_approval_selector_handler: Arc<dyn Fn() -> Result<(), HandlerError> + Send + Sync>,
    pub(super) show_sandbox_selector_handler: Arc<dyn Fn() -> Result<(), HandlerError> + Send + Sync>,
    pub(super) set_sandbox_profile_handler: SetSandboxProfileCallback,
    pub(super) wait_for_idle_fn: Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
    pub(super) new_session_handler: NewSessionCallback,
    pub(super) fork_handler: ForkCallback,
    pub(super) navigate_tree_handler: NavigateTreeCallback,
    pub(super) switch_session_handler: SwitchSessionCallback,
    pub(super) reload_handler: Arc<dyn Fn() -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>,
}

impl ExtensionContextState {
    pub(super) fn new(
        cwd: impl Into<JsString>,
        session_manager: SharedSessionManager,
        model_registry: Arc<ModelRegistry>,
        settings_manager: Option<Arc<Mutex<SettingsManager>>>,
    ) -> Self {
        Self {
            stale_message: None,
            ui_context: Arc::new(NoOpUiContext),
            has_ui: false,
            mode: ExtensionMode::Print,
            cwd: cwd.into(),
            session_manager,
            model_registry,
            settings_manager,
            get_model: Arc::new(|| None),
            set_model_fn: Arc::new(|_| Box::pin(async { Ok(false) })),
            get_thinking_level_fn: Arc::new(|| ThinkingLevel::Off),
            set_thinking_level_fn: Arc::new(|_| Ok(())),
            get_scoped_models: Arc::new(Vec::new),
            get_footer_data: Arc::new(|| None),
            is_idle_fn: Arc::new(|| true),
            has_active_retry_fn: Arc::new(|| false),
            is_project_trusted_fn: Arc::new(|| true),
            get_signal_fn: Arc::new(|| None),
            abort_fn: Arc::new(|| {}),
            has_pending_messages_fn: Arc::new(|| false),
            shutdown_handler: Arc::new(|| {}),
            restart_fn: Arc::new(|_| Box::pin(async { Ok(()) })),
            get_control_db_path_fn: Arc::new(|| None),
            get_context_usage_fn: Arc::new(|| None),
            get_multi_agent_agent_id_fn: Arc::new(|| None),
            get_multi_agent_parent_session_id_fn: Arc::new(|| None),
            get_no_supervisor_fn: Arc::new(|| false),
            get_multi_agent_requires_agent_id_fn: Arc::new(|| false),
            get_multi_agent_store_fn: Arc::new(|| None),
            get_detached_job_lifecycle_fn: Arc::new(|| None),
            get_tool_detach_registry_fn: Arc::new(|| None),
            compact_fn: Arc::new(|_| {}),
            get_system_prompt_fn: Arc::new(JsString::default),
            get_system_prompt_options_fn: None,
            relocate_handler: Arc::new(|_| {
                Box::pin(async {
                    Err(HandlerError::new(
                        "Working-directory changes are not available in this session mode",
                    ))
                })
            }),
            prepare_tool_result_relocation_handler: Arc::new(|_| {
                Box::pin(async {
                    Err(HandlerError::new(
                        "Tool-result working-directory changes are not available in this session mode",
                    ))
                })
            }),
            activate_tool_result_relocation_handler: Arc::new(|| Box::pin(async { Ok(()) })),
            pending_tool_result_relocations: IndexMap::new(),
            show_approval_selector_handler: Arc::new(|| Ok(())),
            show_sandbox_selector_handler: Arc::new(|| Ok(())),
            set_sandbox_profile_handler: Arc::new(|_, _| {
                Err(HandlerError::new("Sandbox profile settings are unavailable"))
            }),
            wait_for_idle_fn: Arc::new(|| Box::pin(async {})),
            new_session_handler: Arc::new(|_| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
            fork_handler: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
            navigate_tree_handler: Arc::new(|_, _| {
                Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })
            }),
            switch_session_handler: Arc::new(|_, _| {
                Box::pin(async {
                    Err(HandlerError::new(
                        "Session resume is not available in this session mode",
                    ))
                })
            }),
            reload_handler: Arc::new(|| Box::pin(async { Ok(()) })),
        }
    }

    pub(super) fn assert_active(&self) -> Result<(), HandlerError> {
        match &self.stale_message {
            Some(message) => Err(HandlerError::new(message.clone())),
            None => Ok(()),
        }
    }

    fn schedule_tool_result_relocation(
        &mut self,
        tool_call_id: &str,
        target_cwd: &JsString,
    ) -> Result<(), HandlerError> {
        if self.pending_tool_result_relocations.contains_key(tool_call_id) {
            return Err(HandlerError::new(format!(
                "Working-directory relocation already scheduled for tool call {tool_call_id}"
            )));
        }
        self.pending_tool_result_relocations
            .insert(tool_call_id.to_owned(), target_cwd.to_owned());
        Ok(())
    }
}

/// Context passed to extension event handlers and tools.
///
/// Every runner-backed member is a method that asserts the runner is still
/// active and reads the current shared state, matching the TypeScript getters.
#[derive(Clone)]
pub struct ExtensionContext {
    pub tool_execution_started_at: Option<f64>,
    pub tool_execution_agent_id: Option<String>,
    state: Arc<Mutex<ExtensionContextState>>,
    system_prompt_override: Option<Arc<dyn Fn() -> JsString + Send + Sync>>,
    viewed_session_target: Option<ViewedSessionMutationTargetResolver>,
}

pub type ViewedSessionMutationTargetResolver =
    Arc<dyn Fn() -> Result<Option<Arc<dyn ViewedSessionMutationTarget>>, HandlerError> + Send + Sync>;

impl ExtensionContext {
    pub(super) fn new(state: Arc<Mutex<ExtensionContextState>>) -> Self {
        Self {
            tool_execution_started_at: None,
            tool_execution_agent_id: None,
            state,
            system_prompt_override: None,
            viewed_session_target: None,
        }
    }

    /// Shadows model and thinking accessors with a viewed child session's live state.
    pub fn with_viewed_session_mutation_target(mut self, resolver: ViewedSessionMutationTargetResolver) -> Self {
        self.viewed_session_target = Some(resolver);
        self
    }

    fn viewed_session_target(&self) -> Result<Option<Arc<dyn ViewedSessionMutationTarget>>, HandlerError> {
        match &self.viewed_session_target {
            Some(resolver) => resolver(),
            None => Ok(None),
        }
    }

    /// Overrides `getSystemPrompt()` for `before_agent_start` chaining.
    pub fn with_system_prompt_override(mut self, getter: Arc<dyn Fn() -> JsString + Send + Sync>) -> Self {
        self.system_prompt_override = Some(getter);
        self
    }

    fn assert_active(&self) -> Result<(), HandlerError> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .assert_active()
    }

    fn read<T>(&self, read: impl FnOnce(&ExtensionContextState) -> T) -> Result<T, HandlerError> {
        let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.assert_active()?;
        Ok(read(&state))
    }

    /// Reads a callback from shared state, then invokes it outside the lock.
    fn callback<T>(
        &self,
        pick: impl FnOnce(&ExtensionContextState) -> Arc<dyn Fn() -> T + Send + Sync>,
    ) -> Result<T, HandlerError> {
        let callback = self.read(pick)?;
        Ok(callback())
    }

    pub fn ui(&self) -> Result<Arc<dyn ExtensionUiContext>, HandlerError> {
        self.read(|state| Arc::clone(&state.ui_context))
    }

    pub fn footer_data(&self) -> Result<Option<Arc<FooterDataProvider>>, HandlerError> {
        self.callback(|state| Arc::clone(&state.get_footer_data))
    }

    pub fn mode(&self) -> Result<ExtensionMode, HandlerError> {
        self.read(|state| state.mode)
    }

    pub fn has_ui(&self) -> Result<bool, HandlerError> {
        self.read(|state| state.has_ui)
    }

    pub fn cwd(&self) -> Result<JsString, HandlerError> {
        self.read(|state| state.cwd.clone())
    }

    /// The same handle the owning `AgentSession` uses, so extension reads and
    /// writes observe live session state (`ctx.sessionManager` in the source).
    pub fn session_manager(&self) -> Result<SharedSessionManager, HandlerError> {
        self.read(|state| Arc::clone(&state.session_manager))
    }

    pub fn model_registry(&self) -> Result<Arc<ModelRegistry>, HandlerError> {
        if let Some(target) = self.viewed_session_target()? {
            return Ok(target.model_registry());
        }
        self.read(|state| Arc::clone(&state.model_registry))
    }

    pub fn settings_manager(&self) -> Result<Option<Arc<Mutex<SettingsManager>>>, HandlerError> {
        self.read(|state| state.settings_manager.clone())
    }

    pub fn control_db_path(&self) -> Result<Option<JsString>, HandlerError> {
        self.callback(|state| Arc::clone(&state.get_control_db_path_fn))
    }

    pub fn multi_agent_agent_id(&self) -> Result<Option<JsString>, HandlerError> {
        self.callback(|state| Arc::clone(&state.get_multi_agent_agent_id_fn))
    }

    pub fn multi_agent_parent_session_id(&self) -> Result<Option<JsString>, HandlerError> {
        self.callback(|state| Arc::clone(&state.get_multi_agent_parent_session_id_fn))
    }

    pub fn no_supervisor(&self) -> Result<bool, HandlerError> {
        self.callback(|state| Arc::clone(&state.get_no_supervisor_fn))
    }

    pub fn multi_agent_requires_agent_id(&self) -> Result<bool, HandlerError> {
        self.callback(|state| Arc::clone(&state.get_multi_agent_requires_agent_id_fn))
    }

    pub fn multi_agent_store(&self) -> Result<Option<Arc<MultiAgentStore>>, HandlerError> {
        self.callback(|state| Arc::clone(&state.get_multi_agent_store_fn))
    }

    pub fn detached_job_lifecycle(&self) -> Result<Option<Arc<DetachedJobLifecycleController>>, HandlerError> {
        self.callback(|state| Arc::clone(&state.get_detached_job_lifecycle_fn))
    }

    pub fn tool_detach_registry(&self) -> Result<Option<Arc<ToolDetachRegistry>>, HandlerError> {
        self.callback(|state| Arc::clone(&state.get_tool_detach_registry_fn))
    }

    pub fn model(&self) -> Result<Option<Model>, HandlerError> {
        if let Some(target) = self.viewed_session_target()? {
            return Ok(target.model());
        }
        self.callback(|state| Arc::clone(&state.get_model))
    }

    pub fn set_model(&self, model: Model) -> BoxFuture<'static, Result<bool, HandlerError>> {
        let target = match self.viewed_session_target() {
            Ok(target) => target,
            Err(error) => return Box::pin(async move { Err(error) }),
        };
        if let Some(target) = target {
            return target.set_model(model);
        }
        let handler = self.read(|state| Arc::clone(&state.set_model_fn));
        match handler {
            Ok(handler) => handler(model),
            Err(error) => Box::pin(async move { Err(error) }),
        }
    }

    pub fn scoped_models(&self) -> Result<Vec<ScopedModel>, HandlerError> {
        if let Some(target) = self.viewed_session_target()? {
            return Ok(target.scoped_models());
        }
        self.callback(|state| Arc::clone(&state.get_scoped_models))
    }

    pub fn thinking_level(&self) -> Result<ThinkingLevel, HandlerError> {
        if let Some(target) = self.viewed_session_target()? {
            return Ok(target.thinking_level());
        }
        self.callback(|state| Arc::clone(&state.get_thinking_level_fn))
    }

    pub fn get_thinking_level(&self) -> Result<ThinkingLevel, HandlerError> {
        self.thinking_level()
    }

    pub fn set_thinking_level(&self, level: ThinkingLevel) -> Result<(), HandlerError> {
        if let Some(target) = self.viewed_session_target()? {
            return target.set_thinking_level(level);
        }
        let setter = self.read(|state| Arc::clone(&state.set_thinking_level_fn))?;
        setter(level)
    }

    pub fn get_scoped_models(&self) -> Result<Vec<ScopedModel>, HandlerError> {
        self.scoped_models()
    }

    pub fn is_idle(&self) -> Result<bool, HandlerError> {
        self.callback(|state| Arc::clone(&state.is_idle_fn))
    }

    pub fn has_active_retry(&self) -> Result<bool, HandlerError> {
        self.callback(|state| Arc::clone(&state.has_active_retry_fn))
    }

    pub fn is_project_trusted(&self) -> Result<bool, HandlerError> {
        self.callback(|state| Arc::clone(&state.is_project_trusted_fn))
    }

    pub fn signal(&self) -> Result<Option<AbortSignal>, HandlerError> {
        self.callback(|state| Arc::clone(&state.get_signal_fn))
    }

    pub fn abort(&self) -> Result<(), HandlerError> {
        self.callback(|state| Arc::clone(&state.abort_fn))
    }

    pub fn has_pending_messages(&self) -> Result<bool, HandlerError> {
        self.callback(|state| Arc::clone(&state.has_pending_messages_fn))
    }

    pub fn shutdown(&self) -> Result<(), HandlerError> {
        self.callback(|state| Arc::clone(&state.shutdown_handler))
    }

    pub fn restart(
        &self,
        options: Option<RestartOptions>,
    ) -> Result<BoxFuture<'static, Result<(), HandlerError>>, HandlerError> {
        let restart = self.read(|state| Arc::clone(&state.restart_fn))?;
        Ok(restart(options))
    }

    pub fn get_context_usage(&self) -> Result<Option<ContextUsage>, HandlerError> {
        self.callback(|state| Arc::clone(&state.get_context_usage_fn))
    }

    pub fn compact(&self, options: Option<CompactOptions>) -> Result<(), HandlerError> {
        let compact = self.read(|state| Arc::clone(&state.compact_fn))?;
        compact(options);
        Ok(())
    }

    pub fn get_system_prompt(&self) -> Result<JsString, HandlerError> {
        self.assert_active()?;
        if let Some(getter) = &self.system_prompt_override {
            return Ok(getter());
        }
        self.callback(|state| Arc::clone(&state.get_system_prompt_fn))
    }

    pub fn relocate(
        &self,
        target_cwd: &JsString,
    ) -> Result<BoxFuture<'static, Result<(), HandlerError>>, HandlerError> {
        let handler = self.read(|state| Arc::clone(&state.relocate_handler))?;
        Ok(handler(target_cwd))
    }

    pub fn relocate_after_tool_result(&self, tool_call_id: &str, target_cwd: &JsString) -> Result<(), HandlerError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.assert_active()?;
        state.schedule_tool_result_relocation(tool_call_id, target_cwd)
    }

    pub fn switch_session(
        &self,
        session_path: JsString,
        options: Option<SwitchSessionOptions>,
    ) -> BoxFuture<'static, Result<SessionTransitionResult, HandlerError>> {
        match self.read(|state| Arc::clone(&state.switch_session_handler)) {
            Ok(handler) => handler(session_path, options),
            Err(error) => Box::pin(async move { Err(error) }),
        }
    }
}

/// `ctx` passed to command handlers.
///
/// Inherits the live [`ExtensionContext`] members and reads command handlers
/// from the same shared runner state on every call.
#[derive(Clone)]
pub struct ExtensionCommandContext {
    pub context: ExtensionContext,
}

impl std::ops::Deref for ExtensionCommandContext {
    type Target = ExtensionContext;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl ExtensionCommandContext {
    pub fn show_approval_selector(&self) -> Result<(), HandlerError> {
        let handler = self
            .context
            .read(|state| Arc::clone(&state.show_approval_selector_handler))?;
        handler()
    }

    pub fn show_sandbox_selector(&self) -> Result<(), HandlerError> {
        let handler = self
            .context
            .read(|state| Arc::clone(&state.show_sandbox_selector_handler))?;
        handler()
    }

    pub fn set_sandbox_profile(
        &self,
        profile: Option<SandboxProfileName>,
        scope: SandboxProfileScope,
    ) -> Result<(), HandlerError> {
        let handler = self
            .context
            .read(|state| Arc::clone(&state.set_sandbox_profile_handler))?;
        handler(profile, scope)
    }

    pub fn wait_for_idle(&self) -> BoxFuture<'static, ()> {
        match self.context.read(|state| Arc::clone(&state.wait_for_idle_fn)) {
            Ok(handler) => handler(),
            Err(_) => Box::pin(async {}),
        }
    }

    pub fn new_session(
        &self,
        options: Option<NewSessionOptions>,
    ) -> BoxFuture<'static, Result<SessionTransitionResult, HandlerError>> {
        match self.context.read(|state| Arc::clone(&state.new_session_handler)) {
            Ok(handler) => handler(options),
            Err(error) => Box::pin(async move { Err(error) }),
        }
    }

    pub fn fork(
        &self,
        entry_id: JsString,
        options: Option<ForkOptions>,
    ) -> BoxFuture<'static, Result<SessionTransitionResult, HandlerError>> {
        match self.context.read(|state| Arc::clone(&state.fork_handler)) {
            Ok(handler) => handler(entry_id, options),
            Err(error) => Box::pin(async move { Err(error) }),
        }
    }

    pub fn navigate_tree(
        &self,
        target_id: JsString,
        options: Option<NavigateTreeOptions>,
    ) -> BoxFuture<'static, Result<SessionTransitionResult, HandlerError>> {
        match self.context.read(|state| Arc::clone(&state.navigate_tree_handler)) {
            Ok(handler) => handler(target_id, options),
            Err(error) => Box::pin(async move { Err(error) }),
        }
    }

    pub fn switch_session(
        &self,
        session_path: JsString,
        options: Option<SwitchSessionOptions>,
    ) -> BoxFuture<'static, Result<SessionTransitionResult, HandlerError>> {
        match self.context.read(|state| Arc::clone(&state.switch_session_handler)) {
            Ok(handler) => handler(session_path, options),
            Err(error) => Box::pin(async move { Err(error) }),
        }
    }

    pub fn reload(&self) -> BoxFuture<'static, Result<(), HandlerError>> {
        match self.context.read(|state| Arc::clone(&state.reload_handler)) {
            Ok(handler) => handler(),
            Err(error) => Box::pin(async move { Err(error) }),
        }
    }

    pub fn get_system_prompt_options(&self) -> Result<BuildSystemPromptOptions, HandlerError> {
        let (custom, cwd) = self
            .context
            .read(|state| (state.get_system_prompt_options_fn.clone(), state.cwd.clone()))?;
        Ok(match custom {
            Some(build) => build(),
            None => BuildSystemPromptOptions {
                cwd,
                ..BuildSystemPromptOptions::default()
            },
        })
    }
}

/// Fresh command-capable context bound to the replacement session.
#[derive(Clone)]
pub struct ReplacedSessionContext {
    pub command: ExtensionCommandContext,
    pub send_message: ReplacedSendMessageCallback,
    pub send_user_message: ReplacedSendUserMessageCallback,
}

impl std::ops::Deref for ReplacedSessionContext {
    type Target = ExtensionCommandContext;

    fn deref(&self) -> &Self::Target {
        &self.command
    }
}

// ============================================================================
// Tool Types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRenderResultOptions {
    pub expanded: bool,
    pub is_partial: bool,
}

/// Schedule a renderer completion after the current synchronous owner turn yields.
pub type ToolRenderCompletionExecutor = Arc<dyn Fn(BoxFuture<'static, ()>) + Send + Sync>;

/// Shared renderer state for one tool row (`ToolRenderContext.state`).
/// The source carries the same mutable object across call and result renders.
#[derive(Clone)]
pub struct ToolRenderState(Arc<std::sync::Mutex<ToolRenderStateData>>);

struct ToolRenderStateData {
    value: JsValue,
    completion_executor: Option<ToolRenderCompletionExecutor>,
    interval: Option<Arc<std::sync::atomic::AtomicBool>>,
}

impl Drop for ToolRenderStateData {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.interval {
            cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

impl Default for ToolRenderState {
    fn default() -> Self {
        Self(Arc::new(std::sync::Mutex::new(ToolRenderStateData {
            value: JsValue::Object(JsObject::new()),
            completion_executor: None,
            interval: None,
        })))
    }
}

impl ToolRenderState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of the current state.
    pub fn get(&self) -> JsValue {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .value
            .clone()
    }

    /// Bind the native owner loop; None has no observable async rerender target.
    pub fn set_completion_executor(&self, executor: Option<ToolRenderCompletionExecutor>) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .completion_executor = executor;
    }

    pub fn completion_executor(&self) -> Option<ToolRenderCompletionExecutor> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .completion_executor
            .clone()
    }

    /// Start one owner-loop interval for this state. Each token identifies one lifetime;
    /// cancellation suppresses queued completions as well as future ticks.
    pub fn start_interval(&self, delay: std::time::Duration, invalidate: Arc<dyn Fn() + Send + Sync>) {
        let mut state = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.interval.is_some() {
            return;
        }
        let Some(executor) = state.completion_executor.clone() else {
            return;
        };
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        state.interval = Some(cancelled.clone());
        spawn_render_interval(delay, cancelled, executor, invalidate);
    }

    /// Stop the current lifetime without invalidating a later restarted interval.
    pub fn stop_interval(&self) {
        let mut state = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cancelled) = state.interval.take() {
            cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Mutates the state in place.
    pub fn update(&self, update: impl FnOnce(&mut JsValue)) {
        let mut state = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        update(&mut state.value);
    }
}

fn spawn_render_interval(
    delay: std::time::Duration,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    executor: ToolRenderCompletionExecutor,
    invalidate: Arc<dyn Fn() + Send + Sync>,
) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(delay);
            if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            let cancelled = cancelled.clone();
            let invalidate = invalidate.clone();
            executor(Box::pin(async move {
                if !cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                    invalidate();
                }
            }));
        }
    });
}

#[derive(Clone)]
pub struct ToolRenderContext {
    pub args: JsValue,
    pub tool_call_id: String,
    pub invalidate: Arc<dyn Fn() + Send + Sync>,
    pub last_component: Option<ComponentRef>,
    /// Component returned by the last `renderCall` (`EditRenderState.callComponent`).
    pub call_component: Option<ComponentRef>,
    pub state: ToolRenderState,
    pub cwd: JsString,
    pub execution_started: bool,
    pub args_complete: bool,
    pub is_partial: bool,
    pub expanded: bool,
    pub show_images: bool,
    pub is_error: bool,
}

pub type PrepareArguments = Arc<dyn Fn(&JsValue) -> Result<JsValue, HandlerError> + Send + Sync>;

pub type PrepareExecution = Arc<
    dyn Fn(
            String,
            Option<AbortSignal>,
            Option<ExtensionContext>,
        ) -> BoxFuture<'static, Result<Option<ToolCallExecution>, HandlerError>>
        + Send
        + Sync,
>;

/// Native registered extensions require the context provided by their runner.
/// Standalone built-in definitions model missing context explicitly instead.
pub fn require_extension_context(context: Option<ExtensionContext>) -> Result<ExtensionContext, HandlerError> {
    context.ok_or_else(|| HandlerError::new("Registered extension tool requires an extension context"))
}

pub type ToolExecute = Arc<
    dyn Fn(
            String,
            JsValue,
            Option<AbortSignal>,
            Option<AgentToolUpdateCallback>,
            Option<ExtensionContext>,
        ) -> BoxFuture<'static, Result<AgentToolResult, HandlerError>>
        + Send
        + Sync,
>;

pub type ToolRenderer =
    Arc<dyn Fn(&JsValue, &Theme, &ToolRenderContext) -> Result<ComponentRef, HandlerError> + Send + Sync>;

pub type ToolResultRenderer = Arc<
    dyn Fn(&AgentToolResult, &ToolRenderResultOptions, &Theme, &ToolRenderContext) -> Result<ComponentRef, HandlerError>
        + Send
        + Sync,
>;

/// Adapts a built-in renderer, which cannot fail, to the fallible renderer type.
pub fn infallible_render_call<F>(renderer: F) -> ToolRenderer
where
    F: Fn(&JsValue, &Theme, &ToolRenderContext) -> ComponentRef + Send + Sync + 'static,
{
    Arc::new(move |args, theme, context| Ok(renderer(args, theme, context)))
}

/// Adapts a built-in result renderer, which cannot fail, to the fallible renderer type.
pub fn infallible_render_result<F>(renderer: F) -> ToolResultRenderer
where
    F: Fn(&AgentToolResult, &ToolRenderResultOptions, &Theme, &ToolRenderContext) -> ComponentRef
        + Send
        + Sync
        + 'static,
{
    Arc::new(move |result, options, theme, context| Ok(renderer(result, options, theme, context)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalKind {
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderShell {
    Default,
    SelfRender,
}

/// Rust method adapter for the executable fields of frozen `ToolDefinition`.
/// Stored extension definitions remain the `ToolDefinition` record below.
pub trait ContextTool<TContext>: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> JsString;
    /// JSON Schema for the tool arguments.
    fn parameters(&self) -> &Value;
    /// Human-readable label for UI display.
    fn label(&self) -> JsString;
    /// Per-tool execution mode override.
    fn execution_mode(&self) -> Option<ToolExecutionMode> {
        None
    }
    /// Compatibility shim for raw tool-call arguments before schema validation.
    fn prepare_arguments(&self, args: JsValue) -> JsValue {
        args
    }
    /// Prepare durable execution metadata before the assistant tool call is persisted.
    fn prepare_execution(
        &self,
        _tool_call_id: String,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'static, Option<pi_ai::types::ToolCallExecution>> {
        Box::pin(async { None })
    }
    /// Execute the tool call with the context resolved for the current turn snapshot.
    fn execute(
        &self,
        tool_call_id: String,
        params: JsValue,
        signal: Option<AbortSignal>,
        on_update: Option<AgentToolUpdateCallback>,
        context: TContext,
        execution: Option<pi_agent_core::AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, pi_agent_core::ToolError>>;
}

#[derive(Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub label: JsString,
    pub description: JsString,
    pub prompt_snippet: Option<JsString>,
    pub prompt_guidelines: Option<Vec<JsString>>,
    pub approval_required: Option<bool>,
    pub approval_kind: Option<ApprovalKind>,
    /// JSON Schema for the tool parameters.
    pub parameters: Value,
    pub render_shell: Option<RenderShell>,
    pub prepare_arguments: Option<PrepareArguments>,
    pub prepare_execution: Option<PrepareExecution>,
    pub execution_mode: Option<ToolExecutionMode>,
    pub execute: ToolExecute,
    pub render_call: Option<ToolRenderer>,
    pub render_result: Option<ToolResultRenderer>,
}

impl ToolDefinition {
    /// Standalone tool definition preserving parameter inference in the source.
    pub fn define(tool: ToolDefinition) -> Self {
        tool
    }
}

// ============================================================================
// Startup / Resource Events
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTrustEvent {
    pub cwd: JsString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectTrustEventDecision {
    Yes,
    No,
    Undecided,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTrustEventResult {
    pub trusted: ProjectTrustEventDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remember: Option<bool>,
}

#[derive(Clone)]
pub struct ProjectTrustContext {
    pub cwd: JsString,
    pub mode: ExtensionMode,
    pub has_ui: bool,
    pub ui: Arc<dyn ExtensionUiContext>,
}

pub trait ProjectTrustHandler: Send + Sync {
    fn call(
        &self,
        event: ProjectTrustEvent,
        ctx: ProjectTrustContext,
    ) -> BoxFuture<'static, Result<ProjectTrustEventResult, HandlerError>>;
}

impl<F> ProjectTrustHandler for F
where
    F: Fn(ProjectTrustEvent, ProjectTrustContext) -> BoxFuture<'static, Result<ProjectTrustEventResult, HandlerError>>
        + Send
        + Sync,
{
    fn call(
        &self,
        event: ProjectTrustEvent,
        ctx: ProjectTrustContext,
    ) -> BoxFuture<'static, Result<ProjectTrustEventResult, HandlerError>> {
        self(event, ctx)
    }
}

pub type ProjectTrustHandlerHandle = Arc<dyn ProjectTrustHandler + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesDiscoverEvent {
    pub cwd: JsString,
    pub reason: ResourcesDiscoverReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourcesDiscoverReason {
    Startup,
    Reload,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesDiscoverResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_paths: Option<Vec<JsString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_paths: Option<Vec<JsString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_paths: Option<Vec<JsString>>,
}

// ============================================================================
// Session Events
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStartReason {
    Startup,
    Reload,
    New,
    Resume,
    Fork,
    Restart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStartEvent {
    pub reason: SessionStartReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_session_file: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoChangedEvent {
    /// `None` when the name is cleared.
    pub name: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBeforeSwitchEvent {
    pub reason: SessionSwitchReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_session_file: Option<JsString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionSwitchReason {
    New,
    Resume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBeforeForkEvent {
    pub entry_id: JsString,
    pub position: ForkPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactionReason {
    Idle,
    Manual,
    Threshold,
    Overflow,
}

pub use crate::core::compaction::{
    CompactionPreparation, CompactionResult, CompactionSourceInfo, CompactionSourceType,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBeforeCompactEvent {
    pub preparation: CompactionPreparation,
    pub branch_entries: Vec<SessionEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<JsString>,
    pub reason: CompactionReason,
    pub will_retry: bool,
    #[serde(skip)]
    pub signal: Option<AbortSignal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionEvent {
    pub preparation: CompactionPreparation,
    pub branch_entries: Vec<SessionEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<JsString>,
    pub reason: CompactionReason,
    pub will_retry: bool,
    #[serde(skip)]
    pub signal: Option<AbortSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCompactionSourceEvent {
    pub reason: CompactionReason,
    pub will_retry: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCompactEvent {
    pub compaction_entry: CompactionEntry,
    pub from_extension: bool,
    pub reason: CompactionReason,
    pub will_retry: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionShutdownReason {
    Quit,
    Reload,
    New,
    Resume,
    Fork,
    Restart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionShutdownEvent {
    pub reason: SessionShutdownReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_session_file: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMailboxEvent {
    pub message: RuntimeMailboxMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreePreparation {
    pub target_id: JsString,
    pub old_leaf_id: Option<JsString>,
    pub common_ancestor_id: Option<JsString>,
    pub entries_to_summarize: Vec<SessionEntry>,
    pub user_wants_summary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace_instructions: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<JsString>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBeforeTreeEvent {
    pub preparation: TreePreparation,
    #[serde(skip)]
    pub signal: Option<AbortSignal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTreeEvent {
    pub new_leaf_id: Option<JsString>,
    pub old_leaf_id: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_entry: Option<BranchSummaryEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_extension: Option<bool>,
}

// ============================================================================
// Agent Events
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEvent {
    pub messages: Vec<AgentMessage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeforeProviderRequestEvent {
    pub payload: JsValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AfterProviderResponseEvent {
    pub status: u16,
    pub headers: IndexMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeforeAgentStartEvent {
    pub prompt: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<ImageContent>>,
    pub system_prompt: JsString,
    pub system_prompt_options: BuildSystemPromptOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStartEvent {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequestStartEvent {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequestEndEvent {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEndSessionContinuation {
    CwdRelocation,
    CodexRestrictionValidation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEndEvent {
    pub messages: Vec<AgentMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_continuation: Option<AgentEndSessionContinuation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartEvent {
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub turn_index: f64,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub timestamp: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnEndEvent {
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub turn_index: f64,
    pub message: AgentMessage,
    pub tool_results: Vec<pi_ai::types::ToolResultMessage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageStartEvent {
    pub message: AgentMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageUpdateEvent {
    pub message: AgentMessage,
    pub assistant_message_event: AssistantMessageEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageEndEvent {
    pub message: AgentMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionStartEvent {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: JsValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionUpdateEvent {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args: JsValue,
    pub partial_result: JsValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutionEndEvent {
    pub tool_call_id: String,
    pub tool_name: String,
    pub result: Value,
    pub is_error: bool,
}

// ============================================================================
// Model Events
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelSelectSource {
    Set,
    Cycle,
    Restore,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelectEvent {
    pub model: Model,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_model: Option<Model>,
    pub source: ModelSelectSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingLevelSelectEvent {
    pub level: ThinkingLevel,
    pub previous_level: ThinkingLevel,
}

// ============================================================================
// User Bash Events
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserBashEvent {
    pub command: JsString,
    pub exclude_from_context: bool,
    pub cwd: JsString,
}

// ============================================================================
// Input Events
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputSource {
    Interactive,
    Rpc,
    Extension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamingBehavior {
    Steer,
    FollowUp,
}

/// Input images with reference identity.
///
/// TypeScript compares `currentImages !== images`, so a handler that returns
/// the same array produces no transform. Handlers preserve identity by
/// cloning the event images (`event.images.clone()`); a freshly built
/// [`InputImages`] is a new reference.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputImages(Arc<Vec<ImageContent>>);

impl InputImages {
    pub fn new(images: Vec<ImageContent>) -> Self {
        Self(Arc::new(images))
    }

    pub fn as_slice(&self) -> &[ImageContent] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<ImageContent> {
        Arc::try_unwrap(self.0).unwrap_or_else(|images| (*images).clone())
    }

    /// Whether both values are the same reference (`===`).
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Serialize for InputImages {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_slice().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for InputImages {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<ImageContent>::deserialize(deserializer).map(Self::new)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputEvent {
    pub text: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<InputImages>,
    pub source: InputSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streaming_behavior: Option<StreamingBehavior>,
}

// ============================================================================
// Tool Events
// ============================================================================

/// Built-in tool names plus custom tool names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToolName {
    Bash,
    Read,
    Edit,
    Write,
    Grep,
    Find,
    Ls,
    Outline,
    Symbol,
    References,
    Custom(String),
}

impl ToolName {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Bash => "bash",
            Self::Read => "read",
            Self::Edit => "edit",
            Self::Write => "write",
            Self::Grep => "grep",
            Self::Find => "find",
            Self::Ls => "ls",
            Self::Outline => "outline",
            Self::Symbol => "symbol",
            Self::References => "references",
            Self::Custom(name) => name,
        }
    }

    pub fn parse(name: &str) -> Self {
        match name {
            "bash" => Self::Bash,
            "read" => Self::Read,
            "edit" => Self::Edit,
            "write" => Self::Write,
            "grep" => Self::Grep,
            "find" => Self::Find,
            "ls" => Self::Ls,
            "outline" => Self::Outline,
            "symbol" => Self::Symbol,
            "references" => Self::References,
            other => Self::Custom(other.to_owned()),
        }
    }
}

impl Serialize for ToolName {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ToolName {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Ok(Self::parse(&name))
    }
}

/// Tool-call arguments are the per-tool JSON object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallEvent {
    pub tool_name: ToolName,
    pub tool_call_id: String,
    pub bypass_permissions: bool,
    #[serde(
        serialize_with = "pi_js_compat::js_json::serialize_object",
        deserialize_with = "pi_js_compat::js_json::deserialize_object"
    )]
    pub input: JsObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultEvent {
    pub tool_name: ToolName,
    pub tool_call_id: String,
    #[serde(
        serialize_with = "pi_js_compat::js_json::serialize_object",
        deserialize_with = "pi_js_compat::js_json::deserialize_object"
    )]
    pub input: JsObject,
    pub content: Vec<TextOrImageContent>,
    pub is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "pi_js_compat::js_json::deserialize_present"
    )]
    pub details: Option<JsValue>,
}

pub fn is_tool_call_event_type(event: &ToolCallEvent, tool_name: &str) -> bool {
    event.tool_name.as_str() == tool_name
}

pub fn is_bash_tool_result(event: &ToolResultEvent) -> bool {
    event.tool_name == ToolName::Bash
}

pub fn is_read_tool_result(event: &ToolResultEvent) -> bool {
    event.tool_name == ToolName::Read
}

pub fn is_edit_tool_result(event: &ToolResultEvent) -> bool {
    event.tool_name == ToolName::Edit
}

pub fn is_write_tool_result(event: &ToolResultEvent) -> bool {
    event.tool_name == ToolName::Write
}

pub fn is_grep_tool_result(event: &ToolResultEvent) -> bool {
    event.tool_name == ToolName::Grep
}

pub fn is_find_tool_result(event: &ToolResultEvent) -> bool {
    event.tool_name == ToolName::Find
}

pub fn is_ls_tool_result(event: &ToolResultEvent) -> bool {
    event.tool_name == ToolName::Ls
}

pub fn is_outline_tool_result(event: &ToolResultEvent) -> bool {
    event.tool_name == ToolName::Outline
}

pub fn is_symbol_tool_result(event: &ToolResultEvent) -> bool {
    event.tool_name == ToolName::Symbol
}

pub fn is_references_tool_result(event: &ToolResultEvent) -> bool {
    event.tool_name == ToolName::References
}

// ============================================================================
// Extension Event Union
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExtensionEvent {
    ProjectTrust(ProjectTrustEvent),
    ResourcesDiscover(ResourcesDiscoverEvent),
    SessionStart(SessionStartEvent),
    SessionInfoChanged(SessionInfoChangedEvent),
    SessionBeforeSwitch(SessionBeforeSwitchEvent),
    SessionBeforeFork(SessionBeforeForkEvent),
    SessionBeforeCompact(SessionBeforeCompactEvent),
    Compaction(CompactionEvent),
    SessionCompactionSource(SessionCompactionSourceEvent),
    SessionCompact(SessionCompactEvent),
    SessionShutdown(SessionShutdownEvent),
    RuntimeMailbox(RuntimeMailboxEvent),
    SessionBeforeTree(SessionBeforeTreeEvent),
    SessionTree(SessionTreeEvent),
    Context(ContextEvent),
    BeforeProviderRequest(BeforeProviderRequestEvent),
    AfterProviderResponse(AfterProviderResponseEvent),
    BeforeAgentStart(BeforeAgentStartEvent),
    AgentStart(AgentStartEvent),
    ModelRequestStart(ModelRequestStartEvent),
    ModelRequestEnd(ModelRequestEndEvent),
    AgentEnd(AgentEndEvent),
    TurnStart(TurnStartEvent),
    TurnEnd(TurnEndEvent),
    MessageStart(MessageStartEvent),
    MessageUpdate(Box<MessageUpdateEvent>),
    MessageEnd(MessageEndEvent),
    ToolExecutionStart(ToolExecutionStartEvent),
    ToolExecutionUpdate(ToolExecutionUpdateEvent),
    ToolExecutionEnd(ToolExecutionEndEvent),
    ModelSelect(Box<ModelSelectEvent>),
    ThinkingLevelSelect(ThinkingLevelSelectEvent),
    UserBash(UserBashEvent),
    Input(InputEvent),
    ToolCall(ToolCallEvent),
    ToolResult(ToolResultEvent),
}

impl ExtensionEvent {
    /// Event name used by handler lookup and error records.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::ProjectTrust(_) => "project_trust",
            Self::ResourcesDiscover(_) => "resources_discover",
            Self::SessionStart(_) => "session_start",
            Self::SessionInfoChanged(_) => "session_info_changed",
            Self::SessionBeforeSwitch(_) => "session_before_switch",
            Self::SessionBeforeFork(_) => "session_before_fork",
            Self::SessionBeforeCompact(_) => "session_before_compact",
            Self::Compaction(_) => "compaction",
            Self::SessionCompactionSource(_) => "session_compaction_source",
            Self::SessionCompact(_) => "session_compact",
            Self::SessionShutdown(_) => "session_shutdown",
            Self::RuntimeMailbox(_) => "runtime_mailbox",
            Self::SessionBeforeTree(_) => "session_before_tree",
            Self::SessionTree(_) => "session_tree",
            Self::Context(_) => "context",
            Self::BeforeProviderRequest(_) => "before_provider_request",
            Self::AfterProviderResponse(_) => "after_provider_response",
            Self::BeforeAgentStart(_) => "before_agent_start",
            Self::AgentStart(_) => "agent_start",
            Self::ModelRequestStart(_) => "model_request_start",
            Self::ModelRequestEnd(_) => "model_request_end",
            Self::AgentEnd(_) => "agent_end",
            Self::TurnStart(_) => "turn_start",
            Self::TurnEnd(_) => "turn_end",
            Self::MessageStart(_) => "message_start",
            Self::MessageUpdate(_) => "message_update",
            Self::MessageEnd(_) => "message_end",
            Self::ToolExecutionStart(_) => "tool_execution_start",
            Self::ToolExecutionUpdate(_) => "tool_execution_update",
            Self::ToolExecutionEnd(_) => "tool_execution_end",
            Self::ModelSelect(_) => "model_select",
            Self::ThinkingLevelSelect(_) => "thinking_level_select",
            Self::UserBash(_) => "user_bash",
            Self::Input(_) => "input",
            Self::ToolCall(_) => "tool_call",
            Self::ToolResult(_) => "tool_result",
        }
    }
}

// ============================================================================
// Event Results
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMailboxEventResult {
    pub handled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEventResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<AgentMessage>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallEventResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<bool>,
    /// Rust-native input patch; the source mutates `event.input` in place.
    #[serde(skip)]
    pub input: Option<JsObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminate: Option<bool>,
}

#[derive(Clone, Default)]
pub struct UserBashEventResult {
    /// Custom operations used by `AgentSession::execute_bash` (`BashOperations`).
    pub operations: Option<Arc<dyn crate::core::tools::bash::BashOperations>>,
    /// Full replacement: the extension handled execution and supplies the result.
    pub result: Option<JsValue>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultEventResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<TextOrImageContent>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "pi_js_compat::js_json::deserialize_present"
    )]
    pub details: Option<JsValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageEndEventResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<AgentMessage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomMessagePayload {
    pub custom_type: JsString,
    pub content: CustomMessageContent,
    pub display: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "pi_js_compat::js_json::deserialize_present"
    )]
    pub details: Option<JsValue>,
}

pub use pi_agent_core::harness::messages::CustomMessageContent;

pub use pi_agent_core::harness::messages::CustomMessageContent as UserMessageContent;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_turn: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deliver_as: Option<MessageDelivery>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendUserMessageOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deliver_as: Option<UserMessageDelivery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expand_prompt_templates: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MessageDelivery {
    Steer,
    FollowUp,
    NextTurn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UserMessageDelivery {
    Steer,
    FollowUp,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeforeAgentStartEventResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<CustomMessagePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<JsString>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBeforeSwitchResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBeforeForkResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip_conversation_restore: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBeforeCompactResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionEventResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compaction: Option<CompactionResult>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCompactionSourceResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<CompactionSourceInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTreeSummary {
    pub summary: JsString,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "pi_js_compat::js_json::deserialize_present"
    )]
    pub details: Option<JsValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionBeforeTreeResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<SessionTreeSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace_instructions: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ApprovalReviewerResult {
    Allow {
        #[serde(
            skip_serializing_if = "Option::is_none",
            serialize_with = "serialize_optional_js_object",
            deserialize_with = "deserialize_optional_js_object",
            default
        )]
        updated_input: Option<JsObject>,
    },
    Ask {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<JsString>,
    },
    Deny {
        reason: JsString,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum InputEventResult {
    Continue,
    Transform {
        text: JsString,
        #[serde(skip_serializing_if = "Option::is_none")]
        images: Option<InputImages>,
    },
    Handled,
}

fn serialize_optional_js_object<S: serde::Serializer>(
    value: &Option<JsObject>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value
        .as_ref()
        .map(|object| JsValue::Object(object.clone()))
        .serialize(serializer)
}

fn deserialize_optional_js_object<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<JsObject>, D::Error> {
    match Option::<JsValue>::deserialize(deserializer)? {
        None => Ok(None),
        Some(JsValue::Object(object)) => Ok(Some(object)),
        Some(_) => Err(serde::de::Error::custom("expected object")),
    }
}

// ============================================================================
// Message and Entry Rendering
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRenderOptions {
    pub expanded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownTransformContext {
    pub message_type: MarkdownMessageType,
    pub is_streaming: bool,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub available_width: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkdownMessageType {
    User,
    Assistant,
    AssistantThinking,
}

#[derive(Clone)]
pub struct EntryRenderOptions {
    pub expanded: bool,
    pub session_id: JsString,
    pub request_render: Rc<dyn Fn() -> Result<bool, CallbackError>>,
    pub register_cleanup: Rc<dyn Fn(Rc<dyn Fn()>)>,
}

pub type MarkdownTransformer = Arc<dyn Fn(&str, &MarkdownTransformContext) -> String + Send + Sync>;

pub type MessageRenderer =
    Arc<dyn Fn(&CustomMessageValue, &MessageRenderOptions, &Theme) -> Option<ComponentRef> + Send + Sync>;

pub type EntryRenderer = Arc<dyn Fn(&CustomEntry, &EntryRenderOptions, &Theme) -> Option<ComponentRef> + Send + Sync>;

/// Custom message shape passed to renderers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomMessageValue {
    pub custom_type: JsString,
    pub content: CustomMessageContent,
    pub display: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "pi_js_compat::js_json::deserialize_present"
    )]
    pub details: Option<JsValue>,
    pub timestamp: f64,
}

// ============================================================================
// Command Registration
// ============================================================================

pub type CommandHandler =
    Arc<dyn Fn(JsString, ExtensionCommandContext) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>;

pub type ArgumentCompletions = Arc<dyn Fn(&JsString) -> Result<Option<Vec<JsValue>>, JsString> + Send + Sync>;

#[derive(Clone)]
pub struct RegisteredCommand {
    pub name: JsString,
    pub source_info: SourceInfo,
    pub description: Option<JsString>,
    pub get_argument_completions: Option<ArgumentCompletions>,
    pub handler: CommandHandler,
}

#[derive(Clone)]
pub struct ResolvedCommand {
    pub name: JsString,
    pub source_info: SourceInfo,
    pub description: Option<JsString>,
    pub get_argument_completions: Option<ArgumentCompletions>,
    pub handler: CommandHandler,
    pub invocation_name: JsString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: JsString,
    pub parameters: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_guidelines: Option<Vec<JsString>>,
    pub source_info: SourceInfo,
}

// ============================================================================
// Extension API
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FlagValue {
    Boolean(bool),
    String(String),
}

impl FlagValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            Self::String(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Boolean(_) => None,
            Self::String(value) => Some(value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionFlag {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub flag_type: FlagType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<FlagValue>,
    pub extension_path: JsString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FlagType {
    Boolean,
    String,
}

#[derive(Clone)]
pub struct ExtensionShortcut {
    pub shortcut: KeyId,
    pub description: Option<JsString>,
    pub handler: ContextShortcutHandler,
    pub extension_path: JsString,
}

#[derive(Clone)]
pub struct RegisteredTool {
    pub definition: ToolDefinition,
    pub source_info: SourceInfo,
}

#[derive(Clone, Default)]
pub struct ExtensionHandlers {
    pub project_trust: Vec<ProjectTrustHandlerHandle>,
    pub resources_discover: Vec<HandlerHandle<ResourcesDiscoverEvent, ResourcesDiscoverResult>>,
    pub session_start: Vec<HandlerHandle<SessionStartEvent, ()>>,
    pub session_info_changed: Vec<HandlerHandle<SessionInfoChangedEvent, ()>>,
    pub session_before_switch: Vec<HandlerHandle<SessionBeforeSwitchEvent, SessionBeforeSwitchResult>>,
    pub session_before_fork: Vec<HandlerHandle<SessionBeforeForkEvent, SessionBeforeForkResult>>,
    pub session_before_compact: Vec<HandlerHandle<SessionBeforeCompactEvent, SessionBeforeCompactResult>>,
    pub compaction: Vec<HandlerHandle<CompactionEvent, CompactionEventResult>>,
    pub session_compaction_source: Vec<HandlerHandle<SessionCompactionSourceEvent, SessionCompactionSourceResult>>,
    pub session_compact: Vec<HandlerHandle<SessionCompactEvent, ()>>,
    pub session_shutdown: Vec<HandlerHandle<SessionShutdownEvent, ()>>,
    pub runtime_mailbox: Vec<HandlerHandle<RuntimeMailboxEvent, RuntimeMailboxEventResult>>,
    pub session_before_tree: Vec<HandlerHandle<SessionBeforeTreeEvent, SessionBeforeTreeResult>>,
    pub session_tree: Vec<HandlerHandle<SessionTreeEvent, ()>>,
    pub context: Vec<HandlerHandle<ContextEvent, ContextEventResult>>,
    pub before_provider_request: Vec<HandlerHandle<BeforeProviderRequestEvent, JsValue>>,
    pub after_provider_response: Vec<HandlerHandle<AfterProviderResponseEvent, ()>>,
    pub before_agent_start: Vec<HandlerHandle<BeforeAgentStartEvent, BeforeAgentStartEventResult>>,
    pub agent_start: Vec<HandlerHandle<AgentStartEvent, ()>>,
    pub model_request_start: Vec<HandlerHandle<ModelRequestStartEvent, ()>>,
    pub model_request_end: Vec<HandlerHandle<ModelRequestEndEvent, ()>>,
    pub agent_end: Vec<HandlerHandle<AgentEndEvent, ()>>,
    pub turn_start: Vec<HandlerHandle<TurnStartEvent, ()>>,
    pub turn_end: Vec<HandlerHandle<TurnEndEvent, ()>>,
    pub message_start: Vec<HandlerHandle<MessageStartEvent, ()>>,
    pub message_update: Vec<HandlerHandle<MessageUpdateEvent, ()>>,
    pub message_end: Vec<HandlerHandle<MessageEndEvent, MessageEndEventResult>>,
    pub tool_execution_start: Vec<HandlerHandle<ToolExecutionStartEvent, ()>>,
    pub tool_execution_update: Vec<HandlerHandle<ToolExecutionUpdateEvent, ()>>,
    pub tool_execution_end: Vec<HandlerHandle<ToolExecutionEndEvent, ()>>,
    pub model_select: Vec<HandlerHandle<ModelSelectEvent, ()>>,
    pub thinking_level_select: Vec<HandlerHandle<ThinkingLevelSelectEvent, ()>>,
    pub tool_call: Vec<HandlerHandle<ToolCallEvent, ToolCallEventResult>>,
    pub tool_result: Vec<HandlerHandle<ToolResultEvent, ToolResultEventResult>>,
    pub user_bash: Vec<HandlerHandle<UserBashEvent, UserBashEventResult>>,
    pub input: Vec<HandlerHandle<InputEvent, InputEventResult>>,
}

impl ExtensionHandlers {
    /// Number of events with at least one handler: TS `handlers.size`, since
    /// `on()` creates a map entry only when it pushes a handler.
    pub fn len(&self) -> usize {
        self.event_lists_empty().iter().filter(|empty| !**empty).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn event_lists_empty(&self) -> [bool; 34] {
        [
            self.project_trust.is_empty(),
            self.resources_discover.is_empty(),
            self.session_start.is_empty(),
            self.session_info_changed.is_empty(),
            self.session_before_switch.is_empty(),
            self.session_before_fork.is_empty(),
            self.session_before_compact.is_empty(),
            self.compaction.is_empty(),
            self.session_compaction_source.is_empty(),
            self.session_compact.is_empty(),
            self.session_shutdown.is_empty(),
            self.runtime_mailbox.is_empty(),
            self.session_before_tree.is_empty(),
            self.session_tree.is_empty(),
            self.context.is_empty(),
            self.before_provider_request.is_empty(),
            self.after_provider_response.is_empty(),
            self.before_agent_start.is_empty(),
            self.agent_start.is_empty(),
            self.agent_end.is_empty(),
            self.turn_start.is_empty(),
            self.turn_end.is_empty(),
            self.message_start.is_empty(),
            self.message_update.is_empty(),
            self.message_end.is_empty(),
            self.tool_execution_start.is_empty(),
            self.tool_execution_update.is_empty(),
            self.tool_execution_end.is_empty(),
            self.model_select.is_empty(),
            self.thinking_level_select.is_empty(),
            self.tool_call.is_empty(),
            self.tool_result.is_empty(),
            self.user_bash.is_empty(),
            self.input.is_empty(),
        ]
    }
}

/// Loaded extension identity shared by the API, resource loader, and runner.
#[derive(Clone)]
pub struct Extension(Arc<Mutex<ExtensionState>>);

impl Extension {
    pub fn new(state: ExtensionState) -> Self {
        Self(Arc::new(Mutex::new(state)))
    }

    pub fn read(&self) -> std::sync::MutexGuard<'_, ExtensionState> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Registration state. Never retained as a separate load-time copy.
pub struct ExtensionState {
    pub path: JsString,
    pub resolved_path: JsString,
    pub hidden: Option<bool>,
    pub source_info: SourceInfo,
    pub handlers: ExtensionHandlers,
    pub approval_reviewers: Vec<ApprovalReviewerHandle>,
    pub tool_gates: Vec<ToolGateHandle>,
    pub tools: IndexMap<String, RegisteredTool>,
    pub message_renderers: IndexMap<JsString, MessageRenderer>,
    pub markdown_transformer: Option<MarkdownTransformer>,
    pub entry_renderers: Option<IndexMap<JsString, EntryRenderer>>,
    pub commands: IndexMap<JsString, RegisteredCommand>,
    pub flags: IndexMap<String, ExtensionFlag>,
    pub shortcuts: IndexMap<KeyId, ExtensionShortcut>,
}

/// Result of loading extensions.
#[derive(Clone)]
pub struct LoadExtensionsResult {
    pub extensions: Vec<Extension>,
    pub errors: Vec<ExtensionLoadError>,
    pub runtime: ExtensionRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionLoadError {
    pub path: JsString,
    pub error: JsString,
}

// ============================================================================
// Extension Error
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionError {
    pub extension_path: JsString,
    pub event: String,
    pub error: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

// ============================================================================
// Provider Registration Types
// ============================================================================

/// Extension provider configuration input, shared with the model runtime.
pub type ProviderConfig = ProviderConfigInput;

/// Extension OAuth registration config, shared with the model runtime.
pub type ProviderOAuthConfig = crate::core::model_registry::ExtensionOAuthConfig;

/// Provider model entry; Rust models carry their full shape.
pub type ProviderModelConfig = Model;

// ============================================================================
// Runtime Actions
// ============================================================================

pub type SendMessageHandler =
    Arc<dyn Fn(CustomMessagePayload, Option<SendMessageOptions>) -> Result<(), HandlerError> + Send + Sync>;
pub type SendUserMessageHandler =
    Arc<dyn Fn(UserMessageContent, Option<SendUserMessageOptions>) -> Result<(), HandlerError> + Send + Sync>;
pub type AppendEntryHandler = Arc<dyn Fn(&JsString, Option<JsValue>) -> Result<(), HandlerError> + Send + Sync>;
pub type SetSessionNameHandler = Arc<dyn Fn(&JsString) -> Result<(), HandlerError> + Send + Sync>;
pub type GetSessionNameHandler = Arc<dyn Fn() -> Result<Option<JsString>, HandlerError> + Send + Sync>;
pub type SetLabelHandler = Arc<dyn Fn(&JsString, Option<&JsString>) -> Result<(), HandlerError> + Send + Sync>;
pub type GetActiveToolsHandler = Arc<dyn Fn() -> Result<Vec<String>, HandlerError> + Send + Sync>;
pub type GetAllToolsHandler = Arc<dyn Fn() -> Result<Vec<ToolInfo>, HandlerError> + Send + Sync>;
pub type SetActiveToolsHandler = Arc<dyn Fn(&[String]) -> Result<(), HandlerError> + Send + Sync>;
pub type CallToolHandler = Arc<
    dyn Fn(
            String,
            JsValue,
            Option<AbortSignal>,
            Option<String>,
        ) -> BoxFuture<'static, Result<AgentToolResult, HandlerError>>
        + Send
        + Sync,
>;
pub type CallCommandHandler =
    Arc<dyn Fn(JsString, Option<JsString>) -> BoxFuture<'static, Result<JsValue, HandlerError>> + Send + Sync>;
pub type GetCommandsHandler = Arc<dyn Fn() -> Result<Vec<SlashCommandInfo>, HandlerError> + Send + Sync>;
pub type SetModelHandler = Arc<dyn Fn(Model) -> BoxFuture<'static, Result<bool, HandlerError>> + Send + Sync>;
pub type GetThinkingLevelHandler = Arc<dyn Fn() -> Result<ThinkingLevel, HandlerError> + Send + Sync>;
pub type SetThinkingLevelHandler = Arc<dyn Fn(ThinkingLevel) -> Result<(), HandlerError> + Send + Sync>;

#[derive(Clone)]
pub struct ExtensionActions {
    pub send_message: SendMessageHandler,
    pub send_user_message: SendUserMessageHandler,
    pub append_entry: AppendEntryHandler,
    pub request_resume_continuation: Arc<dyn Fn() + Send + Sync>,
    pub set_session_name: SetSessionNameHandler,
    pub get_session_name: GetSessionNameHandler,
    pub set_label: SetLabelHandler,
    pub get_active_tools: GetActiveToolsHandler,
    pub get_all_tools: GetAllToolsHandler,
    pub set_active_tools: SetActiveToolsHandler,
    pub call_tool: CallToolHandler,
    pub call_command: CallCommandHandler,
    pub refresh_tools: Arc<dyn Fn() -> Result<(), HandlerError> + Send + Sync>,
    pub get_commands: GetCommandsHandler,
    pub set_model: SetModelHandler,
    pub get_thinking_level: GetThinkingLevelHandler,
    pub set_thinking_level: SetThinkingLevelHandler,
}

#[derive(Clone)]
pub struct ExtensionContextActions {
    pub get_model: Arc<dyn Fn() -> Option<Model> + Send + Sync>,
    pub get_thinking_level: Option<Arc<dyn Fn() -> ThinkingLevel + Send + Sync>>,
    pub set_thinking_level: Option<SetThinkingLevelHandler>,
    pub get_scoped_models: Option<Arc<dyn Fn() -> Vec<ScopedModel> + Send + Sync>>,
    pub get_footer_data: Option<Arc<dyn Fn() -> Option<Arc<FooterDataProvider>> + Send + Sync>>,
    pub is_idle: Arc<dyn Fn() -> bool + Send + Sync>,
    pub has_active_retry: Arc<dyn Fn() -> bool + Send + Sync>,
    pub is_project_trusted: Arc<dyn Fn() -> bool + Send + Sync>,
    pub get_signal: Arc<dyn Fn() -> Option<AbortSignal> + Send + Sync>,
    pub abort: Arc<dyn Fn() + Send + Sync>,
    pub has_pending_messages: Arc<dyn Fn() -> bool + Send + Sync>,
    pub shutdown: Arc<dyn Fn() + Send + Sync>,
    pub restart: RestartHandler,
    pub get_control_db_path: Option<Arc<dyn Fn() -> Option<JsString> + Send + Sync>>,
    pub get_context_usage: Arc<dyn Fn() -> Option<ContextUsage> + Send + Sync>,
    pub get_multi_agent_agent_id: Option<Arc<dyn Fn() -> Option<JsString> + Send + Sync>>,
    pub get_multi_agent_parent_session_id: Option<Arc<dyn Fn() -> Option<JsString> + Send + Sync>>,
    pub get_no_supervisor: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    pub get_multi_agent_requires_agent_id: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    pub get_multi_agent_store: Option<Arc<dyn Fn() -> Option<Arc<MultiAgentStore>> + Send + Sync>>,
    pub get_tool_detach_registry: Option<Arc<dyn Fn() -> Option<Arc<ToolDetachRegistry>> + Send + Sync>>,
    pub compact: Arc<dyn Fn(Option<CompactOptions>) + Send + Sync>,
    pub get_system_prompt: Arc<dyn Fn() -> JsString + Send + Sync>,
    pub get_system_prompt_options: Option<Arc<dyn Fn() -> BuildSystemPromptOptions + Send + Sync>>,
}

#[derive(Clone)]
pub struct ExtensionCommandContextActions {
    pub show_approval_selector: Arc<dyn Fn() -> Result<(), HandlerError> + Send + Sync>,
    pub show_sandbox_selector: Arc<dyn Fn() -> Result<(), HandlerError> + Send + Sync>,
    pub set_sandbox_profile: SetSandboxProfileCallback,
    pub wait_for_idle: Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
    pub new_session: NewSessionCallback,
    pub fork: ForkCallback,
    pub navigate_tree: NavigateTreeCallback,
    pub switch_session: SwitchSessionCallback,
    pub reload: Arc<dyn Fn() -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>,
    pub restart: RestartHandler,
}

/// Shared provider registration hooks.
#[derive(Clone, Default)]
pub struct ProviderActions {
    pub register_provider: Option<RegisterProviderCallback>,
    pub unregister_provider: Option<UnregisterProviderCallback>,
}

/// Commands owned by the runtime, addressable like extension prompt commands.
pub type InternalCommandHandler =
    Arc<dyn Fn(JsString, ExtensionCommandContext) -> BoxFuture<'static, Result<(), HandlerError>> + Send + Sync>;

/// Custom provider stream handler registered by `registerProvider`.
pub type ProviderStreamSimple = crate::core::model_registry::StreamSimpleFn;

/// Event bus wrapper enforcing the stale-runtime check.
#[derive(Clone)]
pub struct ExtensionEventBus {
    bus: Arc<dyn EventBus>,
    runtime: ExtensionRuntime,
}

impl ExtensionEventBus {
    pub fn new(bus: Arc<dyn EventBus>, runtime: ExtensionRuntime) -> Self {
        Self { bus, runtime }
    }

    pub fn emit(
        &self,
        channel: &JsString,
        data: JsValue,
    ) -> Result<Vec<crate::core::event_bus::HandlerError>, JsString> {
        self.runtime.assert_active().map_err(JsString::from)?;
        Ok(self.bus.emit(channel, data))
    }

    pub fn emit_request(
        &self,
        channel: &JsString,
        request: &mut crate::core::event_bus::EventRequest,
    ) -> Result<Vec<crate::core::event_bus::HandlerError>, JsString> {
        self.runtime.assert_active().map_err(JsString::from)?;
        Ok(self.bus.emit_request(channel, request))
    }

    pub fn on_request(
        &self,
        channel: &JsString,
        handler: Arc<crate::core::event_bus::RequestHandler>,
    ) -> Result<Arc<dyn Fn() + Send + Sync>, JsString> {
        self.runtime.assert_active().map_err(JsString::from)?;
        let subscription = self.bus.on_request(channel, handler);
        Ok(self.runtime.track_event_bus_subscription(subscription))
    }

    pub fn on(
        &self,
        channel: &JsString,
        handler: Arc<crate::core::event_bus::Handler>,
    ) -> Result<Arc<dyn Fn() + Send + Sync>, JsString> {
        self.runtime.assert_active().map_err(JsString::from)?;
        let subscription = self.bus.on(channel, handler);
        Ok(self.runtime.track_event_bus_subscription(subscription))
    }
}

/// `ExtensionAPI` passed to extension factories.
#[derive(Clone)]
pub struct ExtensionApi {
    pub(crate) extension: Extension,
    pub runtime: ExtensionRuntime,
    pub(crate) cwd: JsString,
    pub events: ExtensionEventBus,
}

impl ExtensionApi {
    pub(crate) fn new(
        extension: Extension,
        runtime: ExtensionRuntime,
        cwd: JsString,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            extension,
            runtime: runtime.clone(),
            cwd,
            events: ExtensionEventBus::new(event_bus, runtime),
        }
    }

    fn extension_lock(&self) -> std::sync::MutexGuard<'_, ExtensionState> {
        self.extension.read()
    }

    fn push_handler<E, R>(
        &self,
        select: impl Fn(&mut ExtensionHandlers) -> &mut Vec<HandlerHandle<E, R>>,
        handler: impl Handler<E, R> + 'static,
    ) -> Result<(), String> {
        self.runtime.assert_active()?;
        let mut extension = self.extension_lock();
        select(&mut extension.handlers).push(Arc::new(handler));
        Ok(())
    }

    pub fn on_project_trust(&self, handler: impl ProjectTrustHandler + 'static) -> Result<(), String> {
        self.runtime.assert_active()?;
        self.extension_lock().handlers.project_trust.push(Arc::new(handler));
        Ok(())
    }

    pub fn on_resources_discover(
        &self,
        handler: impl Handler<ResourcesDiscoverEvent, ResourcesDiscoverResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.resources_discover, handler)
    }

    pub fn on_session_start(&self, handler: impl Handler<SessionStartEvent, ()> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.session_start, handler)
    }

    pub fn on_session_info_changed(
        &self,
        handler: impl Handler<SessionInfoChangedEvent, ()> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.session_info_changed, handler)
    }

    pub fn on_session_before_switch(
        &self,
        handler: impl Handler<SessionBeforeSwitchEvent, SessionBeforeSwitchResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.session_before_switch, handler)
    }

    pub fn on_session_before_fork(
        &self,
        handler: impl Handler<SessionBeforeForkEvent, SessionBeforeForkResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.session_before_fork, handler)
    }

    pub fn on_session_before_compact(
        &self,
        handler: impl Handler<SessionBeforeCompactEvent, SessionBeforeCompactResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.session_before_compact, handler)
    }

    pub fn on_compaction(
        &self,
        handler: impl Handler<CompactionEvent, CompactionEventResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.compaction, handler)
    }

    pub fn on_session_compaction_source(
        &self,
        handler: impl Handler<SessionCompactionSourceEvent, SessionCompactionSourceResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.session_compaction_source, handler)
    }

    pub fn on_session_compact(&self, handler: impl Handler<SessionCompactEvent, ()> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.session_compact, handler)
    }

    pub fn on_session_shutdown(&self, handler: impl Handler<SessionShutdownEvent, ()> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.session_shutdown, handler)
    }

    pub fn on_runtime_mailbox(
        &self,
        handler: impl Handler<RuntimeMailboxEvent, RuntimeMailboxEventResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.runtime_mailbox, handler)
    }

    pub fn on_session_before_tree(
        &self,
        handler: impl Handler<SessionBeforeTreeEvent, SessionBeforeTreeResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.session_before_tree, handler)
    }

    pub fn on_session_tree(&self, handler: impl Handler<SessionTreeEvent, ()> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.session_tree, handler)
    }

    pub fn on_context(&self, handler: impl Handler<ContextEvent, ContextEventResult> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.context, handler)
    }

    pub fn on_before_provider_request(
        &self,
        handler: impl Handler<BeforeProviderRequestEvent, JsValue> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.before_provider_request, handler)
    }

    pub fn on_after_provider_response(
        &self,
        handler: impl Handler<AfterProviderResponseEvent, ()> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.after_provider_response, handler)
    }

    pub fn on_before_agent_start(
        &self,
        handler: impl Handler<BeforeAgentStartEvent, BeforeAgentStartEventResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.before_agent_start, handler)
    }

    pub fn on_agent_start(&self, handler: impl Handler<AgentStartEvent, ()> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.agent_start, handler)
    }

    pub fn on_model_request_start(
        &self,
        handler: impl Handler<ModelRequestStartEvent, ()> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.model_request_start, handler)
    }
    pub fn on_model_request_end(
        &self,
        handler: impl Handler<ModelRequestEndEvent, ()> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.model_request_end, handler)
    }

    pub fn on_agent_end(&self, handler: impl Handler<AgentEndEvent, ()> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.agent_end, handler)
    }

    pub fn on_turn_start(&self, handler: impl Handler<TurnStartEvent, ()> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.turn_start, handler)
    }

    pub fn on_turn_end(&self, handler: impl Handler<TurnEndEvent, ()> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.turn_end, handler)
    }

    pub fn on_message_start(&self, handler: impl Handler<MessageStartEvent, ()> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.message_start, handler)
    }

    pub fn on_message_update(&self, handler: impl Handler<MessageUpdateEvent, ()> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.message_update, handler)
    }

    pub fn on_message_end(
        &self,
        handler: impl Handler<MessageEndEvent, MessageEndEventResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.message_end, handler)
    }

    pub fn on_tool_execution_start(
        &self,
        handler: impl Handler<ToolExecutionStartEvent, ()> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.tool_execution_start, handler)
    }

    pub fn on_tool_execution_update(
        &self,
        handler: impl Handler<ToolExecutionUpdateEvent, ()> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.tool_execution_update, handler)
    }

    pub fn on_tool_execution_end(
        &self,
        handler: impl Handler<ToolExecutionEndEvent, ()> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.tool_execution_end, handler)
    }

    pub fn on_model_select(&self, handler: impl Handler<ModelSelectEvent, ()> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.model_select, handler)
    }

    pub fn on_thinking_level_select(
        &self,
        handler: impl Handler<ThinkingLevelSelectEvent, ()> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.thinking_level_select, handler)
    }

    pub fn on_tool_call(
        &self,
        handler: impl Handler<ToolCallEvent, ToolCallEventResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.tool_call, handler)
    }

    pub fn on_tool_result(
        &self,
        handler: impl Handler<ToolResultEvent, ToolResultEventResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.tool_result, handler)
    }

    pub fn on_user_bash(
        &self,
        handler: impl Handler<UserBashEvent, UserBashEventResult> + 'static,
    ) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.user_bash, handler)
    }

    pub fn on_input(&self, handler: impl Handler<InputEvent, InputEventResult> + 'static) -> Result<(), String> {
        self.push_handler(|handlers| &mut handlers.input, handler)
    }

    pub fn register_tool(&self, tool: ToolDefinition) -> Result<(), HandlerError> {
        self.runtime.assert_active()?;
        let mut extension = self.extension_lock();
        let source_info = extension.source_info.clone();
        extension.tools.insert(
            tool.name.clone(),
            RegisteredTool {
                definition: tool,
                source_info,
            },
        );
        drop(extension);
        (self.runtime.actions().refresh_tools)()?;
        Ok(())
    }

    pub fn register_approval_reviewer(&self, reviewer: ApprovalReviewerHandle) -> Result<(), String> {
        self.runtime.assert_active()?;
        self.extension_lock().approval_reviewers.push(reviewer);
        Ok(())
    }

    pub fn register_tool_gate(&self, gate: ToolGateHandle) -> Result<(), String> {
        self.runtime.assert_active()?;
        self.extension_lock().tool_gates.push(gate);
        Ok(())
    }

    pub fn register_command(
        &self,
        name: impl Into<JsString>,
        description: Option<JsString>,
        get_argument_completions: Option<ArgumentCompletions>,
        handler: CommandHandler,
    ) -> Result<(), JsString> {
        self.runtime.assert_active()?;
        let name = name.into();
        let mut extension = self.extension_lock();
        let source_info = extension.source_info.clone();
        extension.commands.insert(
            name.to_owned(),
            RegisteredCommand {
                name: name.to_owned(),
                source_info,
                description,
                get_argument_completions,
                handler,
            },
        );
        Ok(())
    }

    pub fn register_shortcut(
        &self,
        shortcut: KeyId,
        description: Option<JsString>,
        handler: ContextShortcutHandler,
    ) -> Result<(), String> {
        self.runtime.assert_active()?;
        let mut extension = self.extension_lock();
        let extension_path = extension.path.clone();
        extension.shortcuts.insert(
            shortcut.clone(),
            ExtensionShortcut {
                shortcut,
                description,
                handler,
                extension_path,
            },
        );
        Ok(())
    }

    pub fn register_flag(
        &self,
        name: &str,
        description: Option<String>,
        flag_type: FlagType,
        default: Option<FlagValue>,
    ) -> Result<(), String> {
        self.runtime.assert_active()?;
        let mut extension = self.extension_lock();
        let extension_path = extension.path.clone();
        extension.flags.insert(
            name.to_owned(),
            ExtensionFlag {
                name: name.to_owned(),
                description,
                flag_type,
                default: default.clone(),
                extension_path,
            },
        );
        drop(extension);
        if let Some(default) = default {
            self.runtime.set_default_flag_value(name, default);
        }
        Ok(())
    }

    pub fn register_message_renderer(
        &self,
        custom_type: impl Into<JsString>,
        renderer: MessageRenderer,
    ) -> Result<(), String> {
        self.runtime.assert_active()?;
        self.extension_lock()
            .message_renderers
            .insert(custom_type.into(), renderer);
        Ok(())
    }

    pub fn register_markdown_transformer(&self, transformer: MarkdownTransformer) -> Result<(), String> {
        self.runtime.assert_active()?;
        self.extension_lock().markdown_transformer = Some(transformer);
        Ok(())
    }

    pub fn register_entry_renderer(
        &self,
        custom_type: impl Into<JsString>,
        renderer: EntryRenderer,
    ) -> Result<(), String> {
        self.runtime.assert_active()?;
        let mut extension = self.extension_lock();
        extension
            .entry_renderers
            .get_or_insert_with(IndexMap::new)
            .insert(custom_type.into(), renderer);
        Ok(())
    }

    pub fn get_flag(&self, name: &str) -> Result<Option<FlagValue>, String> {
        self.runtime.assert_active()?;
        let registered = self.extension_lock().flags.contains_key(name);
        if !registered {
            return Ok(None);
        }
        Ok(self.runtime.flag_value(name))
    }

    pub fn send_message(
        &self,
        message: CustomMessagePayload,
        options: Option<SendMessageOptions>,
    ) -> Result<(), HandlerError> {
        self.runtime.assert_active()?;
        (self.runtime.actions().send_message)(message, options)
    }

    pub fn send_user_message(
        &self,
        content: UserMessageContent,
        options: Option<SendUserMessageOptions>,
    ) -> Result<(), HandlerError> {
        self.runtime.assert_active()?;
        (self.runtime.actions().send_user_message)(content, options)
    }

    pub fn append_entry(&self, custom_type: &JsString, data: Option<JsValue>) -> Result<(), HandlerError> {
        self.runtime.assert_active()?;
        (self.runtime.actions().append_entry)(custom_type, data)
    }

    pub fn request_resume_continuation(&self) -> Result<(), String> {
        self.runtime.assert_active()?;
        (self.runtime.actions().request_resume_continuation)();
        Ok(())
    }

    pub fn set_session_name(&self, name: &JsString) -> Result<(), HandlerError> {
        self.runtime.assert_active()?;
        (self.runtime.actions().set_session_name)(name)
    }

    pub fn get_session_name(&self) -> Result<Option<JsString>, HandlerError> {
        self.runtime.assert_active()?;
        (self.runtime.actions().get_session_name)()
    }

    pub fn set_label(&self, entry_id: &JsString, label: Option<&JsString>) -> Result<(), HandlerError> {
        self.runtime.assert_active()?;
        (self.runtime.actions().set_label)(entry_id, label)
    }

    pub fn exec<'a>(
        &'a self,
        command: String,
        args: Vec<String>,
        options: Option<ExtensionExecOptions>,
    ) -> BoxFuture<'a, Result<ExecResult, String>> {
        if let Err(error) = self.runtime.assert_active() {
            return Box::pin(async move { Err(error) });
        }
        let cwd = options
            .as_ref()
            .and_then(|options| options.cwd.clone())
            .unwrap_or_else(|| self.cwd.clone());
        let exec_options = crate::core::exec::ExecOptions {
            signal: options.as_ref().and_then(|options| options.signal.clone()),
            timeout: options.as_ref().and_then(|options| options.timeout),
        };
        Box::pin(async move {
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            Ok(crate::core::exec::exec_command(&command, &arg_refs, &cwd, &exec_options).await)
        })
    }

    pub fn get_active_tools(&self) -> Result<Vec<String>, HandlerError> {
        self.runtime.assert_active()?;
        (self.runtime.actions().get_active_tools)()
    }

    pub fn get_all_tools(&self) -> Result<Vec<ToolInfo>, HandlerError> {
        self.runtime.assert_active()?;
        (self.runtime.actions().get_all_tools)()
    }

    pub fn set_active_tools(&self, tool_names: Vec<String>) -> Result<(), HandlerError> {
        self.runtime.assert_active()?;
        (self.runtime.actions().set_active_tools)(&tool_names)
    }

    pub fn call_tool(
        &self,
        name: String,
        params: JsValue,
        signal: Option<AbortSignal>,
        tool_call_id: Option<String>,
    ) -> BoxFuture<'static, Result<AgentToolResult, HandlerError>> {
        if let Err(error) = self.runtime.assert_active() {
            return Box::pin(async move { Err(error.into()) });
        }
        (self.runtime.actions().call_tool)(name, params, signal, tool_call_id)
    }

    pub fn call_command(
        &self,
        name: JsString,
        args: Option<JsString>,
    ) -> BoxFuture<'static, Result<JsValue, HandlerError>> {
        if let Err(error) = self.runtime.assert_active() {
            return Box::pin(async move { Err(error.into()) });
        }
        (self.runtime.actions().call_command)(name, args)
    }

    pub fn get_commands(&self) -> Result<Vec<SlashCommandInfo>, HandlerError> {
        self.runtime.assert_active()?;
        (self.runtime.actions().get_commands)()
    }

    pub fn set_model(&self, model: Model) -> BoxFuture<'static, Result<bool, HandlerError>> {
        if let Err(error) = self.runtime.assert_active() {
            return Box::pin(async move { Err(error.into()) });
        }
        (self.runtime.actions().set_model)(model)
    }

    pub fn get_thinking_level(&self) -> Result<ThinkingLevel, HandlerError> {
        self.runtime.assert_active()?;
        (self.runtime.actions().get_thinking_level)()
    }

    pub fn set_thinking_level(&self, level: ThinkingLevel) -> Result<(), HandlerError> {
        self.runtime.assert_active()?;
        (self.runtime.actions().set_thinking_level)(level)
    }

    pub fn register_provider(&self, name: &str, config: ProviderConfig) -> Result<(), CallbackError> {
        self.runtime.assert_active()?;
        let extension_path = self.extension_lock().path.clone();
        self.runtime.register_provider(name, config, Some(&extension_path))
    }

    pub fn unregister_provider(&self, name: &str) -> Result<(), CallbackError> {
        self.runtime.assert_active()?;
        let extension_path = self.extension_lock().path.clone();
        self.runtime.unregister_provider(name, Some(&extension_path))
    }
}

/// Exec options accepted by `pi.exec`.
///
/// The ported `ExecOptions` has no `cwd`; the extension API resolves the
/// working directory before calling it.
#[derive(Debug, Clone, Default)]
pub struct ExtensionExecOptions {
    pub signal: Option<AbortSignal>,
    pub timeout: Option<u64>,
    pub cwd: Option<JsString>,
}

/// Re-exported sandbox scope name for command contexts.
pub type ExtensionSandboxProfileScope = SandboxProfileScope;

/// Re-exported session entry union for event payloads.
pub type ExtensionSessionEntry = SessionEntry;

/// Re-exported custom entry for renderers.
pub type ExtensionCustomEntry = CustomEntry;

/// Preserve standalone tool-definition inference (`defineTool`).
pub fn define_tool(tool: ToolDefinition) -> ToolDefinition {
    tool
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_error_round_trip_preserves_raw_callback_rejections() {
        use pi_js_compat::abort::AbortReason;
        use pi_js_compat::js_json::JsObject;
        use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
        let raw = JsString::from_utf16(vec![114, 97, 119, 32, 0xd800]);
        for rejection in [
            AbortReason::Value(JsValue::String(raw.clone())),
            AbortReason::Value(JsValue::Object(JsObject::from([(
                raw.clone(),
                JsValue::Array(vec![JsValue::Null, JsValue::Bool(false)]),
            )]))),
            AbortReason::Error(
                NativeNodeError::from_constructor(NativeErrorClass::TypeError, raw.clone())
                    .with_property("code", JsValue::String(raw.clone())),
            ),
        ] {
            let callback = pi_ai::request::CallbackError::from_rejection(rejection.clone());
            let handler = HandlerError::from(callback.clone());
            let restored = pi_ai::request::CallbackError::from(handler.clone());
            assert_eq!(restored.thrown_value(), rejection);
            assert_eq!(restored.message, callback.message);
            assert_eq!(restored.name, callback.name);
        }
    }

    #[test]
    fn extension_ui_done_clones_share_local_callback_state() {
        let values = Rc::new(RefCell::new(Vec::new()));
        let captured = Rc::clone(&values);
        let done = ExtensionUiDone::new(Rc::new(move |value| captured.borrow_mut().push(value)));
        let cloned = done.clone();
        drop(done);
        cloned.call(Some(pi_js_compat::js_json!("closed")));
        cloned.call(None);
        assert_eq!(*values.borrow(), vec![Some(pi_js_compat::js_json!("closed")), None]);
    }
}
