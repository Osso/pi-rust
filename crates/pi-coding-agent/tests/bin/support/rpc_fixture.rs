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
    let mode = config.get("status").and_then(JsValue::as_string).cloned();
    let bash_hook = config.get("bashHook").and_then(JsValue::as_bool) == Some(true);
    let images_probe = config.get("imagesProbe").and_then(JsValue::as_bool) == Some(true);
    let stdout_probe = config.get("stdoutProbe").and_then(JsValue::as_bool) == Some(true);
    let pending_shutdown = config.get("pendingShutdown").and_then(JsValue::as_bool) == Some(true);
    let shutdown_flood = config.get("shutdownFlood").and_then(JsValue::as_bool) == Some(true);
    let ui_probe = config.get("uiProbe").and_then(JsValue::as_bool) == Some(true);
    let fire_probe = config.get("fireProbe").and_then(JsValue::as_bool) == Some(true);
    let shutdown_command = config.get("shutdownCommand").and_then(JsValue::as_bool) == Some(true);
    let ui_timeout = config.get("uiTimeout").and_then(JsValue::as_f64);
    let early_signal = config
        .get("earlySignal")
        .and_then(JsValue::as_string)
        .map(|signal| match signal {
            signal if signal == "SIGTERM" => rustix::process::Signal::TERM,
            signal if signal == "SIGHUP" => rustix::process::Signal::HUP,
            _ => panic!("unsupported fixture signal"),
        });
    let root = PathBuf::from(std::env::var_os("PI_HARNESS_RPC_ROOT").unwrap());
    let factory: ExtensionFactory = Arc::new(move |api| {
        if images_probe {
            use pi_coding_agent::core::extensions::types::{InputEvent, InputEventResult};
            let root = root.clone();
            api.on_input(move |event: InputEvent, _context| {
                let root = root.clone();
                Box::pin(async move {
                    let images = event.images.map_or(serde_json::json!("absent"), |images| {
                        serde_json::to_value(images).unwrap()
                    });
                    let mut file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(root.join("images-report.jsonl"))
                        .unwrap();
                    writeln!(file, "{images}").unwrap();
                    Ok(Some(InputEventResult::Handled))
                }) as BoxFuture<'static, Result<Option<InputEventResult>, HandlerError>>
            })
            .unwrap();
        }
        if shutdown_flood {
            let signal = early_signal.expect("flood fixture requires signal");
            api.register_command(
                "rpc-flood-signal",
                None,
                None,
                Arc::new(move |_, context| {
                    Box::pin(async move {
                        context.ui()?.notify(&"x".repeat(512 * 1024).into(), None)?;
                        tokio::spawn(async move {
                            rustix::process::kill_process(
                                rustix::process::Pid::from_raw(std::process::id() as i32).unwrap(),
                                signal,
                            )
                            .unwrap();
                        });
                        Ok(())
                    })
                }),
            )
            .unwrap();
        }
        if shutdown_command {
            api.register_command(
                "rpc-shutdown-fixture",
                None,
                None,
                Arc::new(|_, context| {
                    Box::pin(async move {
                        context.shutdown()?;
                        Ok(())
                    })
                }),
            )
            .unwrap();
        }
        if fire_probe {
            let root = root.clone();
            api.register_command(
                "rpc-fire-fixture",
                None,
                None,
                Arc::new(move |_, context| {
                    let root = root.clone();
                    Box::pin(async move {
                        let ui = context.ui()?;
                        let high = JsString::from_utf16(vec![0xd800]);
                        let low = JsString::from_utf16(vec![0xdc00]);
                        ui.notify(&(JsString::from("notice-") + &high), None)?;
                        ui.notify(
                            &"warning".into(),
                            Some(pi_coding_agent::core::extensions::types::NotifyType::Warning),
                        )?;
                        ui.set_status(&"clear".into(), None)?;
                        ui.set_status(
                            &"text".into(),
                            Some(ExtensionStatus::Text(JsString::from("status-") + &low)),
                        )?;
                        ui.set_widget(&"clear".into(), None, None)?;
                        let options = pi_coding_agent::core::extensions::types::ExtensionWidgetOptions {
                            placement: Some(pi_coding_agent::core::extensions::types::WidgetPlacement::BelowEditor),
                        };
                        ui.set_widget(
                            &"lines".into(),
                            Some(&["one".into(), JsString::from("two-") + &high]),
                            Some(&options),
                        )?;
                        ui.set_title(&(JsString::from("title-") + &low))?;
                        ui.set_editor_text(&(JsString::from("editor-") + &high))?;
                        ui.paste_to_editor(&(JsString::from("paste-") + &low))?;
                        std::fs::write(
                            root.join("fire-report.json"),
                            stringify_lossless(&pi_js_compat::js_json!({"editor":ui.get_editor_text()?})),
                        )
                        .unwrap();
                        Ok(())
                    })
                }),
            )
            .unwrap();
        }
        if ui_probe {
            let root = root.clone();
            api.register_command(
                "rpc-ui-fixture",
                None,
                None,
                Arc::new(move |_, context| {
                    let root = root.clone();
                    Box::pin(async move {
                        let ui = context.ui()?;
                        let options = pi_coding_agent::core::extensions::types::ExtensionUiDialogOptions {
                            timeout: ui_timeout,
                            ..Default::default()
                        };
                        let selected = ui
                            .select(&"select".into(), &["one".into(), "two".into()], Some(&options))
                            .await
                            .map_err(HandlerError::from)?;
                        std::fs::write(
                            root.join("ui-selected.json"),
                            stringify_lossless(&pi_js_compat::js_json!({"value":selected})),
                        )
                        .unwrap();
                        let confirmed = ui
                            .confirm(&"confirm".into(), &"message".into(), None)
                            .await
                            .map_err(HandlerError::from)?;
                        let input = ui
                            .input(&"input".into(), None, None)
                            .await
                            .map_err(HandlerError::from)?;
                        let editor = ui.editor(&"editor".into(), None).await.map_err(HandlerError::from)?;
                        let report = pi_js_compat::js_json!({"values":[selected,confirmed,input,editor]});
                        std::fs::write(root.join("ui-report.json"), stringify_lossless(&report)).unwrap();
                        Ok(())
                    })
                }),
            )
            .unwrap();
        }
        if early_signal.is_some() {
            let root = root.clone();
            api.on_session_shutdown(move |_, _| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
                let root = root.clone();
                Box::pin(async move {
                    std::fs::write(root.join("rpc-shutdown.json"), "{\"disposed\":true}").unwrap();
                    if pending_shutdown {
                        let ready = pi_js_compat::js_json!({"pid":std::process::id()});
                        std::fs::write(root.join("rpc-shutdown-ready.json"), stringify_lossless(&ready)).unwrap();
                        std::future::pending::<()>().await;
                    }
                    Ok(None)
                })
            })
            .unwrap();
        }
        if bash_hook {
            let root = root.clone();
            api.on_user_bash(
                move |_: pi_coding_agent::core::extensions::types::UserBashEvent,
                      _: ExtensionContext|
                      -> BoxFuture<
                    'static,
                    Result<Option<pi_coding_agent::core::extensions::types::UserBashEventResult>, HandlerError>,
                > {
                    let root = root.clone();
                    Box::pin(async move {
                        let mut log = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(root.join("bash-hook.jsonl"))
                            .unwrap();
                        writeln!(log, "called").unwrap();
                        Ok(None)
                    })
                },
            )
            .unwrap();
        }
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
                    if let Some(signal) = early_signal.filter(|_|!shutdown_flood) {
                        tokio::spawn(async move {
                            rustix::process::kill_process(rustix::process::Pid::from_raw(std::process::id() as i32).unwrap(), signal).unwrap();
                        });
                    }
                    if stdout_probe {
                        let probe = pi_js_compat::js_json!({"takenOver":pi_coding_agent::core::output_guard::is_stdout_taken_over()});
                        std::fs::write(root.join("stdout-probe.json"), stringify_lossless(&probe)).unwrap();
                    }
                    let Some(mode) = mode else { return Ok(None); };
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
