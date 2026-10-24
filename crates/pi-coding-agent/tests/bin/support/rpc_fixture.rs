use pi_ai::request::{BoxFuture, CallbackError};
use pi_coding_agent::core::extensions::types::{ExtensionContext, ExtensionMode, HandlerError};
use pi_coding_agent::core::extensions::{
    ExtensionFactory, LoadExtensionsResult, create_extension_runtime, load_extension_from_factory,
};
use pi_coding_agent::core::footer_data_provider::ExtensionStatus;
use pi_coding_agent::core::resource_loader::{ResourceLoader, StaticResourceLoader};
use pi_js_compat::abort::AbortReason;
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless, stringify_lossless};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

pub async fn resource_loader(cwd: &JsString) -> Arc<dyn ResourceLoader> {
    let encoded = std::env::var("PI_HARNESS_RPC_FIXTURE").unwrap();
    let config = parse_lossless(encoded).unwrap();
    let mode = config["status"].as_string().unwrap().clone();
    let root = PathBuf::from(std::env::var_os("PI_HARNESS_RPC_ROOT").unwrap());
    let factory: ExtensionFactory = Arc::new(move |api| {
        let root = root.clone();
        let mode = mode.clone();
        api.on_session_start(
            move |_, context: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
                let root = root.clone();
                let mode = mode.clone();
                Box::pin(async move {
                    if context.mode()? != ExtensionMode::Rpc {
                        return Ok(None);
                    }
                    let calls_root = root.clone();
                    let result = context.ui()?.set_status(
                        &"callable-status".into(),
                        Some(ExtensionStatus::Callback(Arc::new(move |view| {
                            let mut calls = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(calls_root.join("status-calls.jsonl"))
                                .unwrap();
                            writeln!(
                                calls,
                                "{}",
                                stringify_lossless(
                                    &view.map_or(JsValue::Null, |view| pi_js_compat::js_json!({"cwd":view.cwd}))
                                )
                            )
                            .unwrap();
                            if mode == "throw" {
                                return Err(NativeNodeError::from_constructor(
                                    NativeErrorClass::TypeError,
                                    JsString::from("rpc-status-") + JsString::from_utf16(vec![0xd800]),
                                )
                                .with_property("code", JsValue::String("ESTATUS".into()))
                                .into());
                            }
                            Ok((mode != "omit")
                                .then(|| JsString::from("rpc-value-") + JsString::from_utf16(vec![0xdc00])))
                        }))),
                    );
                    let report = match &result {
                        Ok(()) => pi_js_compat::js_json!({"returned":true}),
                        Err(error) => pi_js_compat::js_json!({"rejected":error_value(error)}),
                    };
                    std::fs::write(root.join("status-report.json"), stringify_lossless(&report)).unwrap();
                    result?;
                    Ok(None)
                })
            },
        )
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let runtime = create_extension_runtime();
    let extension = load_extension_from_factory(
        factory,
        cwd,
        Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
        runtime.clone(),
        &"<rpc-fixture>".into(),
    )
    .await
    .unwrap();
    Arc::new(StaticResourceLoader {
        extensions: Some(LoadExtensionsResult {
            extensions: vec![extension],
            errors: vec![],
            runtime,
        }),
        ..Default::default()
    })
}

fn error_value(error: &CallbackError) -> JsValue {
    match error.thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => error.value(),
        AbortReason::Value(value) => value,
    }
}
