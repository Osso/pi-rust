//! Opt-in native port of `coding-agent/examples/extensions/provider-payload.ts`.
//! Embed and register explicitly. Logs may contain secrets; nothing loads by default.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use pi_coding_agent::config::config_dir_name;
use pi_coding_agent::core::extensions::{
    AfterProviderResponseEvent, BeforeProviderRequestEvent, BoxFuture, ExtensionApi, ExtensionContext, HandlerError,
};
use pi_js_compat::js_json::{stringify, stringify_pretty};
use serde_json::Value;

pub fn provider_payload_extension(api: &ExtensionApi) -> Result<(), HandlerError> {
    api.on_before_provider_request(
        |event: BeforeProviderRequestEvent,
         ctx: ExtensionContext|
         -> BoxFuture<'static, Result<Option<Value>, HandlerError>> {
            Box::pin(async move {
                append_provider_log(&ctx.cwd()?, &stringify_pretty(&event.payload))?;
                Ok(None)
            })
        },
    )?;
    api.on_after_provider_response(
        |event: AfterProviderResponseEvent,
         ctx: ExtensionContext|
         -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            Box::pin(async move {
                let headers = serde_json::to_value(event.headers).map_err(|error| error.to_string())?;
                let line = format!("[{}] {}", event.status, stringify(&headers));
                append_provider_log(&ctx.cwd()?, &line)?;
                Ok(None)
            })
        },
    )?;
    Ok(())
}

fn append_provider_log(cwd: &str, text: &str) -> Result<(), HandlerError> {
    let path = Path::new(cwd).join(config_dir_name()).join("provider-payload.log");
    let append = || -> std::io::Result<()> {
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        file.write_all(format!("{text}\n\n").as_bytes())
    };
    append().map_err(|error| HandlerError::from(format!("append {}: {error}", path.display())))
}
