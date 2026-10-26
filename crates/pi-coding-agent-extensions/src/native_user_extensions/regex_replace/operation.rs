//! Serialized planning and file-queue-protected apply.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use pi_js_compat::abort::AbortSignal;
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};

use super::cli_runner::JsonCliRunner;
use crate::core::extensions::{BoxFuture, HandlerError};
use crate::core::tools::file_mutation_queue::with_file_mutation_queue;

static OPERATION_MUTEX: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

pub(super) async fn run_regex_replace(
    params: &JsValue,
    cwd: JsString,
    runner: &JsonCliRunner,
    signal: Option<AbortSignal>,
) -> Result<JsValue, HandlerError> {
    let _operation = OPERATION_MUTEX.lock().await;
    let mut request = build_plan_request(params, cwd)?;
    let plan = runner.run(&request, signal).await?;
    if params.get("dryRun").and_then(JsValue::as_bool).unwrap_or(false) {
        return Ok(plan);
    }
    let targets = sorted_targets(&plan)?;
    request["action"] = js_json!("apply");
    request["planHash"] = plan["planHash"].clone();
    request["targets"] = JsValue::Array(targets.iter().cloned().map(JsValue::String).collect());
    apply_with_file_queues(runner, &request, &targets).await
}

fn build_plan_request(params: &JsValue, cwd: JsString) -> Result<JsValue, HandlerError> {
    let files = params
        .get("files")
        .and_then(JsValue::as_string)
        .ok_or_else(|| HandlerError::new("files must be a string"))?;
    let files = files.strip_prefix("@").unwrap_or_else(|| files.clone());
    Ok(js_json!({
        "action": "plan", "cwd": cwd, "files": files,
        "pattern": params["pattern"].clone(), "replacement": params["replacement"].clone(),
        "expectedMatches": params["expectedMatches"].clone(),
        "maxFiles": non_null_or(params, "maxFiles", 100),
        "maxTotalBytes": non_null_or(params, "maxTotalBytes", 10 * 1024 * 1024),
        "maxMatches": non_null_or(params, "maxMatches", 10_000),
    }))
}

fn non_null_or(params: &JsValue, key: &str, default: u64) -> JsValue {
    params
        .get(key)
        .filter(|value| !value.is_null())
        .cloned()
        .unwrap_or_else(|| js_json!(default))
}

fn sorted_targets(plan: &JsValue) -> Result<Vec<JsString>, HandlerError> {
    let changes = plan["changes"]
        .as_array()
        .ok_or_else(|| HandlerError::new("Invalid replacement changes"))?;
    let targets = changes
        .iter()
        .map(|change| {
            change
                .get("absolutePath")
                .and_then(JsValue::as_string)
                .cloned()
                .ok_or_else(|| HandlerError::new("Invalid replacement target"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    Ok(targets.into_iter().collect())
}

fn apply_with_file_queues<'a>(
    runner: &'a JsonCliRunner,
    request: &'a JsValue,
    targets: &'a [JsString],
) -> BoxFuture<'a, Result<JsValue, HandlerError>> {
    Box::pin(async move {
        let Some((first, rest)) = targets.split_first() else {
            return runner.run(request, None).await;
        };
        with_file_mutation_queue(first, || apply_with_file_queues(runner, request, rest))
            .await
            .map_err(|error| HandlerError::new(error.to_string()))?
    })
}
