use crate::shared_editor_host as editor_host;
use crate::shared_oracle as oracle;

use pi_js_compat::{
    abort::{AbortReason, AbortSignal},
    js_json::{JsString, JsValue},
};
use pi_tui::autocomplete::{
    AutocompleteFuture, AutocompleteItem, AutocompleteOptions, AutocompleteProvider, CompletionResult,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
};

fn reason(value: Option<AbortReason>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let error = value.as_error().unwrap();
    json!({"name":error.name,"message":error.message,"code":error.value().get("code").and_then(JsValue::as_f64).unwrap() as i64,"dom":matches!(value,AbortReason::DomException(_))})
}

struct Provider {
    events: Arc<Mutex<Vec<Value>>>,
    signals: RefCell<Vec<AbortSignal>>,
    held: RefCell<Option<tokio::sync::oneshot::Receiver<()>>>,
}
impl AutocompleteProvider for Provider {
    fn get_suggestions(
        self: Rc<Self>,
        _lines: Vec<JsString>,
        _line: usize,
        _col: usize,
        options: AutocompleteOptions,
    ) -> AutocompleteFuture {
        let id = self.signals.borrow().len();
        self.signals.borrow_mut().push(options.signal.clone());
        self.events.lock().unwrap().push(json!([
            "enter",
            id,
            options.signal.aborted(),
            reason(options.signal.reason())
        ]));
        let events = self.events.clone();
        options.signal.add_listener(move |value| {
            events
                .lock()
                .unwrap()
                .push(json!(["abort", id, reason(Some(value.clone()))]))
        });
        let held = self.held.borrow_mut().take();
        Box::pin(async move {
            if let Some(held) = held {
                held.await.unwrap();
            }
            self.events.lock().unwrap().push(json!(["return", id]));
            None
        })
    }
    fn apply_completion(
        &self,
        _lines: &[JsString],
        _line: usize,
        _col: usize,
        _item: &AutocompleteItem,
        _prefix: &JsString,
    ) -> CompletionResult {
        panic!("unexpected completion")
    }
}
fn snapshot(provider: &Provider) -> Value {
    let signals: Vec<_> = provider
        .signals
        .borrow()
        .iter()
        .map(|signal| json!({"aborted":signal.aborted(),"reason":reason(signal.reason())}))
        .collect();
    json!({"events":provider.events.lock().unwrap().clone(),"signals":signals})
}

#[tokio::test(flavor = "current_thread")]
async fn editor_serializes_cancelled_provider_and_preserves_default_abort_reason() {
    let mut oracle = oracle::TsOracle::spawn("editor-abort-oracle.mts", "packages/tui/src");
    let expected = oracle.request(&json!({}));
    assert_eq!(expected["ok"], true, "{expected}");
    let (release, held) = tokio::sync::oneshot::channel();
    let provider = Rc::new(Provider {
        events: Arc::default(),
        signals: RefCell::default(),
        held: RefCell::new(Some(held)),
    });
    let (mut editor, _) = editor_host::editor();
    editor.set_autocomplete_provider(provider.clone());
    editor.set_text("first");
    editor_host::send(&mut editor, "\t");
    editor.begin_autocomplete_request();
    let mut actual = vec![snapshot(&provider)];
    editor.set_text("second");
    editor_host::send(&mut editor, "\t");
    editor.begin_autocomplete_request();
    actual.push(snapshot(&provider));
    release.send(()).unwrap();
    editor.poll_autocomplete().await;
    actual.push(snapshot(&provider));
    editor.set_text("done");
    actual.push(snapshot(&provider));
    assert_eq!(json!(actual), expected["output"]);
}
