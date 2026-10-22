//! Port of `packages/coding-agent/src/extensions/llama/index.ts` and
//! `packages/coding-agent/src/extensions/index.ts`.

use std::sync::Arc;

use pi_ai::models::ModelsRefreshOptions;
use pi_ai::request::Fetch;

use crate::bundled_extensions::llama::client::{
    LlamaClient, LlamaListOptions, LlamaModelInfo, format_bytes, normalize_llama_server_url,
};
use crate::bundled_extensions::llama::huggingface::{
    HuggingFaceClient, HuggingFaceGated, find_hugging_face_token_from_process,
};
use crate::bundled_extensions::llama::provider::{
    LLAMA_PROVIDER_ID, LlamaProviderController, create_llama_provider_with_fetch,
};
use crate::bundled_extensions::llama::ui::{
    ConnectionErrorChoice, LlamaManagerAction, LlamaUi, RunWithProgressOptions, RunWithProgressOutcome,
    run_with_progress, show_llama_ui,
};
use crate::core::extensions::types::{ExtensionApi, ExtensionCommandContext, ExtensionMode, HandlerError, NotifyType};
use crate::utils::management_http::RequestDeadline;

/// `modelIsLoaded`.
pub fn model_is_loaded(model: &LlamaModelInfo) -> bool {
    model.status.value == "loaded" || model.status.value == "sleeping"
}

/// `isConnectionError`.
///
/// `CallbackError` carries no `Error.name`; the message is checked alone.
pub fn is_connection_error(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("fetch failed") || message.contains("timeout") || message.contains("network")
}

/// `connectionErrorMessage`.
pub fn connection_error_message(message: &str) -> String {
    if is_connection_error(message) {
        "Could not connect to the server.".to_owned()
    } else {
        message.to_owned()
    }
}

/// `parseHuggingFaceModel`.
pub fn parse_hugging_face_model(value: &str) -> (String, Option<String>) {
    let search_from = value.find('/').map_or(0, |index| index + 1);
    match value[search_from..].find(':').map(|index| search_from + index) {
        None => (value.to_owned(), None),
        Some(colon) => (value[..colon].to_owned(), Some(value[colon + 1..].to_owned())),
    }
}

fn notify(ctx: &ExtensionCommandContext, message: &str, kind: NotifyType) -> Result<(), String> {
    ctx.ui().map_err(|error| error.to_string())?.notify(message, Some(kind));
    Ok(())
}

async fn configured_client(
    ctx: &ExtensionCommandContext,
    fetch: Arc<dyn Fetch>,
) -> Result<Option<LlamaClient>, String> {
    let registry = ctx.model_registry().map_err(|error| error.to_string())?;
    let result = registry
        .resolve_provider_auth(LLAMA_PROVIDER_ID)
        .await
        .map_err(|error| error.to_string())?;
    let Some(result) = result else {
        notify(
            ctx,
            &format!("Configure llama.cpp with /login {LLAMA_PROVIDER_ID}"),
            NotifyType::Warning,
        )?;
        return Ok(None);
    };
    let configured_url = result
        .env
        .as_ref()
        .and_then(|env| env.get("LLAMA_BASE_URL"))
        .filter(|value| !value.is_empty());
    let server_url = normalize_llama_server_url(
        configured_url.map_or_else(|| result.auth.base_url.as_deref().unwrap_or(""), String::as_str),
    )
    .map_err(|error| error.to_string())?;
    LlamaClient::with_fetch(&server_url, result.auth.api_key.as_deref(), fetch)
        .map(Some)
        .map_err(|error| error.message)
}

async fn sync_catalog(
    ctx: &ExtensionCommandContext,
    client: &LlamaClient,
    controller: &LlamaProviderController,
    catalog: Option<Vec<LlamaModelInfo>>,
) -> Result<Vec<LlamaModelInfo>, String> {
    let deadline = RequestDeadline::start(None, Some(15_000));
    let signal = deadline.as_ref().map(|deadline| deadline.signal().clone());
    let current = match catalog {
        Some(catalog) => catalog,
        None => client
            .list(LlamaListOptions {
                reload: false,
                signal: signal.clone(),
            })
            .await
            .map_err(|error| error.to_string())?,
    };
    controller
        .set_catalog(&current, client.server_url())
        .map_err(|error| error.to_string())?;
    let registry = ctx.model_registry().map_err(|error| error.to_string())?;
    let result = registry
        .refresh(ModelsRefreshOptions {
            allow_network: None,
            providers: Some(vec![LLAMA_PROVIDER_ID.to_owned()]),
            force: None,
            signal,
        })
        .await
        .map_err(|error| error.to_string())?;
    if result.aborted {
        return Err("Model catalog refresh timed out.".to_owned());
    }
    if let Some(error) = result.errors.get(LLAMA_PROVIDER_ID) {
        return Err(error.message.clone());
    }
    Ok(current)
}

async fn read_catalog(
    ctx: &ExtensionCommandContext,
    ui: &dyn LlamaUi,
    client: &LlamaClient,
    controller: &LlamaProviderController,
) -> Result<Option<Vec<LlamaModelInfo>>, String> {
    loop {
        match sync_catalog(ctx, client, controller, None).await {
            Ok(catalog) => return Ok(Some(catalog)),
            Err(error) => {
                if ui
                    .connection_error(client.server_url().to_owned(), connection_error_message(&error))
                    .await
                    == ConnectionErrorChoice::Close
                {
                    return Ok(None);
                }
            }
        }
    }
}

async fn restore_loaded(
    ctx: &ExtensionCommandContext,
    client: &LlamaClient,
    controller: &LlamaProviderController,
    loaded: &[LlamaModelInfo],
) -> Result<(), String> {
    notify(ctx, "Restoring previously loaded models", NotifyType::Info)?;
    for model in loaded {
        client
            .load_and_wait(&model.id, Arc::new(|_| {}), None)
            .await
            .map_err(|error| error.to_string())?;
    }
    sync_catalog(ctx, client, controller, None).await?;
    Ok(())
}

async fn load_model(
    ctx: &ExtensionCommandContext,
    ui: &Arc<dyn LlamaUi>,
    client: &LlamaClient,
    controller: &LlamaProviderController,
    catalog: &[LlamaModelInfo],
    target: &LlamaModelInfo,
) -> Result<(), String> {
    let loaded = catalog
        .iter()
        .filter(|model| model.id != target.id && model_is_loaded(model))
        .cloned()
        .collect::<Vec<_>>();
    let mut replace = false;
    if !loaded.is_empty() {
        let choice = ui
            .select(
                format!(
                    "{} model{} loaded",
                    loaded.len(),
                    if loaded.len() == 1 { " is" } else { "s are" }
                ),
                vec![
                    "Unload all and load".to_owned(),
                    "Keep loaded and load".to_owned(),
                    "Cancel".to_owned(),
                ],
            )
            .await;
        match choice.as_deref() {
            None | Some("Cancel") => return Ok(()),
            Some(choice) => replace = choice == "Unload all and load",
        }
    }
    if replace {
        for model in &loaded {
            client
                .unload_and_wait(&model.id, None)
                .await
                .map_err(|error| error.to_string())?;
        }
    }

    let result: Result<(), String> = async {
        let run_client = client.clone();
        let target_id = target.id.clone();
        let cancel_client = client.clone();
        let cancel_id = target.id.clone();
        let outcome = run_with_progress(
            Arc::clone(ui),
            RunWithProgressOptions {
                title: "Loading model".to_owned(),
                model: target.id.clone(),
                initial_message: "Starting…".to_owned(),
                cancel_title: "Stop loading?".to_owned(),
                cancel_message: target.id.clone(),
                run: Box::new(move |signal, update| {
                    Box::pin(async move {
                        run_client
                            .load_and_wait(&target_id, update, Some(signal))
                            .await
                            .map_err(|error| error.message)
                    })
                }),
                cancel: Box::new(move || {
                    let client = cancel_client.clone();
                    let model = cancel_id.clone();
                    Box::pin(async move { client.unload(&model, None).await.map_err(|error| error.message) })
                }),
            },
        )
        .await?;
        if outcome == RunWithProgressOutcome::Cancelled {
            if replace {
                restore_loaded(ctx, client, controller, &loaded).await?;
            }
            return Ok(());
        }
        let refreshed = sync_catalog(ctx, client, controller, None).await?;
        let loaded_model = refreshed.iter().find(|model| model.id == target.id);
        let message = if loaded_model.is_some_and(|model| model.status.value == "loaded") {
            format!("Loaded {}", target.id)
        } else {
            format!("Load started for {}", target.id)
        };
        notify(ctx, &message, NotifyType::Info)
    }
    .await;

    if let Err(error) = result {
        if replace {
            // Preserve the original load error.
            let _ = restore_loaded(ctx, client, controller, &loaded).await;
        }
        return Err(error);
    }
    Ok(())
}

async fn unload_model(
    ctx: &ExtensionCommandContext,
    ui: &Arc<dyn LlamaUi>,
    client: &LlamaClient,
    controller: &LlamaProviderController,
    model: &LlamaModelInfo,
) -> Result<(), String> {
    if !ui.confirm("Unload model?".to_owned(), model.id.clone()).await {
        return Ok(());
    }
    client
        .unload_and_wait(&model.id, None)
        .await
        .map_err(|error| error.to_string())?;
    sync_catalog(ctx, client, controller, None).await?;
    notify(ctx, &format!("Unloaded {}", model.id), NotifyType::Info)
}

async fn download_model(
    ctx: &ExtensionCommandContext,
    ui: &Arc<dyn LlamaUi>,
    client: &LlamaClient,
    controller: &LlamaProviderController,
    fetch: Arc<dyn Fetch>,
) -> Result<(), String> {
    let token = find_hugging_face_token_from_process().await;
    let hugging_face = HuggingFaceClient::with_fetch(token.as_deref(), None, Arc::clone(&fetch));
    let hugging_face = Arc::new(hugging_face);
    let search_client = Arc::clone(&hugging_face);
    let selected = ui
        .search_models(Arc::new(move |query, signal| {
            let client = Arc::clone(&search_client);
            Box::pin(async move { client.search(&query, Some(&signal)).await })
        }))
        .await;
    let Some(selected) = selected else {
        return Ok(());
    };
    let (repository, mut quantization) = parse_hugging_face_model(&selected);
    ui.show_status("Loading model details".to_owned(), repository.clone());
    let details = hugging_face
        .details(&repository, None)
        .await
        .map_err(|error| error.to_string())?;
    if details.gated != HuggingFaceGated::NotGated {
        let approval = if details.gated == HuggingFaceGated::Manual {
            "Manual approval is required"
        } else {
            "Accept the access terms"
        };
        let choice = ui
            .select(
                format!(
                    "Hugging Face access required\n{}\n\n{approval} at:\nhttps://huggingface.co/{}\n\nThe llama.cpp server needs HF_TOKEN with access.",
                    details.id, details.id
                ),
                vec!["Continue".to_owned(), "Back".to_owned()],
            )
            .await;
        if choice.as_deref() != Some("Continue") {
            return Ok(());
        }
    }
    if quantization.is_none() && !details.quantizations.is_empty() {
        let options = details
            .quantizations
            .iter()
            .map(|entry| {
                let detail = [
                    entry.size.map(format_bytes),
                    (entry.name == "Q4_K_M").then(|| "recommended".to_owned()),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                if detail.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{} · {detail}", entry.name)
                }
            })
            .collect::<Vec<_>>();
        let choice = ui
            .select(format!("Select quantization\n{}", details.id), options.clone())
            .await;
        let Some(choice) = choice else {
            return Ok(());
        };
        let Some(index) = options.iter().position(|option| option == &choice) else {
            return Ok(());
        };
        let Some(entry) = details.quantizations.get(index) else {
            return Ok(());
        };
        quantization = Some(entry.name.clone());
    }
    let model = match quantization {
        Some(quantization) => format!("{}:{quantization}", details.id),
        None => details.id.clone(),
    };

    let run_client = client.clone();
    let run_model = model.clone();
    let cancel_client = client.clone();
    let cancel_model = model.clone();
    let outcome = run_with_progress(
        Arc::clone(ui),
        RunWithProgressOptions {
            title: "Downloading model".to_owned(),
            model: model.clone(),
            initial_message: "Starting…".to_owned(),
            cancel_title: "Stop download?".to_owned(),
            cancel_message: model.clone(),
            run: Box::new(move |signal, update| {
                Box::pin(async move {
                    run_client
                        .download_and_wait(&run_model, update, Some(signal))
                        .await
                        .map_err(|error| error.message)
                })
            }),
            cancel: Box::new(move || {
                let client = cancel_client.clone();
                let model = cancel_model.clone();
                Box::pin(async move { client.unload(&model, None).await.map_err(|error| error.message) })
            }),
        },
    )
    .await?;
    let RunWithProgressOutcome::Completed(models) = outcome else {
        return Ok(());
    };
    sync_catalog(ctx, client, controller, Some(models)).await?;
    notify(ctx, &format!("Downloaded {model}"), NotifyType::Info)
}

async fn run_llama_command(
    ctx: ExtensionCommandContext,
    controller: Arc<LlamaProviderController>,
    fetch: Arc<dyn Fetch>,
) -> Result<(), HandlerError> {
    if ctx.mode().map_err(|error| HandlerError::new(error.message))? != ExtensionMode::Tui {
        notify(&ctx, "/llama is available in interactive mode", NotifyType::Warning).map_err(HandlerError::new)?;
        return Ok(());
    }
    let Some(client) = configured_client(&ctx, Arc::clone(&fetch))
        .await
        .map_err(HandlerError::new)?
    else {
        return Ok(());
    };
    let run_ctx = ctx.clone();
    let run_controller = Arc::clone(&controller);
    let run_fetch = Arc::clone(&fetch);
    show_llama_ui(
        &ctx,
        Box::new(move |ui| {
            Box::pin(async move {
                let Some(mut catalog) = read_catalog(&run_ctx, ui.as_ref(), &client, &run_controller).await? else {
                    return Ok(());
                };
                loop {
                    let action = ui.show_models(client.server_url().to_owned(), catalog.clone()).await;
                    let action_result = match action {
                        LlamaManagerAction::Close => return Ok(()),
                        LlamaManagerAction::Download => {
                            download_model(&run_ctx, &ui, &client, &run_controller, Arc::clone(&run_fetch)).await
                        }
                        LlamaManagerAction::Model(model) if model_is_loaded(&model) => {
                            unload_model(&run_ctx, &ui, &client, &run_controller, &model).await
                        }
                        LlamaManagerAction::Model(model) if model.status.value == "unloaded" => {
                            load_model(&run_ctx, &ui, &client, &run_controller, &catalog, &model).await
                        }
                        LlamaManagerAction::Model(model) => notify(
                            &run_ctx,
                            &format!("{} is {}", model.id, model.status.value),
                            NotifyType::Warning,
                        ),
                    };
                    let Some(refreshed) = read_catalog(&run_ctx, ui.as_ref(), &client, &run_controller).await? else {
                        return Ok(());
                    };
                    catalog = refreshed;
                    if let Err(error) = action_result
                        && !is_connection_error(&error)
                    {
                        notify(&run_ctx, &error, NotifyType::Error)?;
                    }
                }
            })
        }),
    )
    .await
    .map_err(HandlerError::new)
}

/// The llama.cpp inline extension (`llamaExtension`).
pub fn llama_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    llama_extension_with_fetch(pi, pi_ai::http::default_fetch())
}

/// [`llama_extension`] with an injected HTTP implementation.
pub fn llama_extension_with_fetch(pi: &ExtensionApi, fetch: Arc<dyn Fetch>) -> Result<(), HandlerError> {
    let controller = Arc::new(create_llama_provider_with_fetch(Arc::clone(&fetch)));
    pi.register_native_provider(controller.provider())
        .map_err(HandlerError::new)?;
    let handler_fetch = Arc::clone(&fetch);
    pi.register_command(
        "llama",
        Some("Manage llama.cpp router models".to_owned()),
        None,
        Arc::new(move |_args, ctx| {
            let fetch = Arc::clone(&handler_fetch);
            let controller = Arc::clone(&controller);
            Box::pin(async move { run_llama_command(ctx, controller, fetch).await })
        }),
    )
    .map_err(HandlerError::new)?;
    Ok(())
}

/// `builtInExtensions` registration for `extensions/index.ts`.
pub fn register_builtin_extensions(registry: &mut crate::core::extensions::loader::ExtensionRegistry) {
    registry.register_hidden(
        "<inline:llama.cpp>",
        Arc::new(|pi| {
            let result = llama_extension(pi);
            Box::pin(async move { result })
        }),
    );
}
