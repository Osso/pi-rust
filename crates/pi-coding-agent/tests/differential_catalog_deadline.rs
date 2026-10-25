use crate::shared_catalog_fetch as catalog_fetch;
use crate::shared_oracle as oracle;

use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse};
use pi_coding_agent::core::model_catalog_cache::{
    EnsureModelCatalogOptions, ModelCatalogCache, fetch_openrouter_catalog,
};
use pi_js_compat::{
    abort::{AbortController, AbortReason, AbortSignal},
    js_json::JsValue,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct DeadlineFetch {
    response: catalog_fetch::CatalogFetch,
    delay_ms: Option<u64>,
    expected_signal: Option<AbortSignal>,
    observed_signal: Mutex<Option<AbortSignal>>,
}
impl DeadlineFetch {
    fn new(delay_ms: Option<u64>, expected_signal: Option<AbortSignal>) -> Self {
        Self {
            response: catalog_fetch::CatalogFetch(
                pi_js_compat::js_json!({"data":[{"id":"q15-deadline","supported_parameters":["tools"]}]}),
            ),
            delay_ms,
            expected_signal,
            observed_signal: Mutex::new(None),
        }
    }
    fn signal_snapshot(&self) -> JsValue {
        let signal = self.observed_signal.lock().unwrap().clone().unwrap();
        let reason = signal.reason().unwrap();
        pi_js_compat::js_json!({"aborted":signal.aborted(), "reason":reason.value(), "dom":matches!(reason, AbortReason::DomException(_))})
    }
}
impl Fetch for DeadlineFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            let signal = signal.unwrap();
            if let Some(expected) = &self.expected_signal {
                assert_eq!(&signal, expected);
            }
            *self.observed_signal.lock().unwrap() = Some(signal.clone());
            match self.delay_ms {
                Some(milliseconds) => tokio::time::sleep(Duration::from_millis(milliseconds)).await,
                None => return Err(CallbackError::from_rejection(signal.wait().await)),
            }
            self.response.fetch(request, Some(signal)).await
        })
    }
}

#[tokio::test]
async fn refresh_deadline_aborts_but_does_not_race_an_ignoring_fetch() {
    for delay_ms in [Some(5100_u64), None] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pi/models/openrouter.json");
        let mut source = oracle::TsOracle::spawn_with_environment(
            "model-catalog-cache-oracle.ts",
            "packages/coding-agent/src/core/model-catalog-cache.ts",
            &[("XDG_CACHE_HOME", directory.path().as_os_str().to_owned())],
        );
        let fetch = Arc::new(DeadlineFetch::new(delay_ms, None));
        let expected = source.request(&pi_js_compat::js_json!({
            "operation":"ensure", "force":true, "now":1766000000000_f64, "inspectError":true,
            "inspectSignal":true, "delayMs":delay_ms, "awaitAbort":delay_ms.is_none(), "payload":fetch.response.0,
        }));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }
        let result = ModelCatalogCache::default()
            .ensure_fresh(
                &path,
                EnsureModelCatalogOptions {
                    force: true,
                    fetch: fetch.clone(),
                    now: Arc::new(|| 1766000000000_f64),
                },
            )
            .await;
        assert_eq!(
            result.is_ok(),
            expected["output"].get("result").is_some(),
            "delay={delay_ms:?}"
        );
        match result {
            Ok(result) => assert_eq!(
                pi_js_compat::js_json::to_value(&result).unwrap(),
                expected["output"]["result"]
            ),
            Err(error) => assert_eq!(error.value(), expected["output"]["error"]),
        }
        assert_eq!(fetch.signal_snapshot(), expected["output"]["signalFinal"]);
    }
}

#[tokio::test]
async fn public_fetch_keeps_supplied_signal_identity_and_falsey_abort_reasons() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-cache-oracle.ts",
        "packages/coding-agent/src/core/model-catalog-cache.ts",
    );
    for reason in [
        None,
        Some(AbortReason::Value(JsValue::Bool(false))),
        Some(AbortReason::Value(pi_js_compat::js_json!({"message":"raw reason"}))),
    ] {
        let controller = AbortController::new();
        controller.abort(reason.clone());
        let signal = controller.signal();
        let fetch = DeadlineFetch::new(None, Some(signal.clone()));
        let mut request = pi_js_compat::js_json!({"operation":"fetch", "inspectError":true, "inspectSignal":true, "rejectSignalReason":true, "abortDefault":reason.is_none()});
        if let Some(reason) = reason {
            request.as_object_mut().unwrap().insert(
                "abortSpec".into(),
                pi_js_compat::js_json!({"kind":"value","value":reason.value()}),
            );
        }
        let expected = source.request(&request);
        assert_eq!(expected["ok"], JsValue::Bool(true));
        let actual = fetch_openrouter_catalog(&fetch, Some(signal)).await.unwrap_err();
        assert_eq!(actual.value(), expected["output"]["error"]);
        assert_eq!(fetch.signal_snapshot(), expected["output"]["signalFinal"]);
    }
}
