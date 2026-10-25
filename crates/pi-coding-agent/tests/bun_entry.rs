//! `src/bun/` entry shims: Bedrock registration and the standalone-binary entry.

use std::sync::Mutex;

use pi_ai::request::BoxFuture;
use pi_coding_agent::bun::cli::run_bun_cli_entry;
use pi_coding_agent::bun::register_bedrock::register_bedrock;
use pi_coding_agent::cli_entry::CliMain;
use pi_coding_agent::internal_runner::{DETACHED_BASH_RUNNER_MODE, DetachedRunnerHost};
use pi_js_compat::js_json::JsString;

#[path = "../../pi-ai/tests/bedrock_support/mod.rs"]
mod bedrock_support;

/// Protect this process-wide module override from other entrypoint cases.
static BEDROCK_MODULE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn entrypoint_restores_the_real_bedrock_module_before_main() {
    use pi_ai::bedrock_provider::{bedrock_converse_stream_api, set_bedrock_provider_module};
    use pi_ai::request::{ProviderRequestOptions, ProviderStreams, SimpleStreamOptions, StreamOptions, error_stream};
    use pi_ai::types::{Context, Model};
    use pi_ai::utils::event_stream::AssistantMessageEventStream;
    use std::sync::Arc;
    struct Marker;
    impl ProviderStreams for Marker {
        fn stream(
            &self,
            model: Model,
            _: Context,
            _: StreamOptions,
        ) -> Result<AssistantMessageEventStream, pi_ai::request::CallbackError> {
            Ok(error_stream(&model, "old module"))
        }
        fn stream_simple(
            &self,
            model: Model,
            _: Context,
            _: SimpleStreamOptions,
        ) -> Result<AssistantMessageEventStream, pi_ai::request::CallbackError> {
            Ok(error_stream(&model, "old module"))
        }
    }
    let _module = BEDROCK_MODULE.lock().await;
    set_bedrock_provider_module(Arc::new(Marker));
    let runner = RecordingRunner {
        calls: Mutex::new(Vec::new()),
    };
    let main: CliMain<'_> = Box::new(|_| {
        Box::pin(async {
            use bedrock_support::{ScriptedFetch, ScriptedResponse, message_start, message_stop, text_delta};
            let body = [
                message_start(),
                text_delta(0, "entry registration works"),
                message_stop("end_turn"),
            ]
            .concat();
            let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
            let options = StreamOptions {
                request: ProviderRequestOptions {
                    fetch: Some(fetch.clone()),
                    env: Some(bedrock_support::env()),
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut stream = bedrock_converse_stream_api()
                .stream(
                    bedrock_support::model(),
                    Context {
                        system_prompt: None,
                        messages: Vec::new(),
                        tools: None,
                    },
                    options,
                )
                .unwrap();
            while let Some(event) = stream.next().await {
                event.unwrap();
            }
            let message = stream.result().await.unwrap();
            assert_eq!(
                serde_json::to_value(&message.content).unwrap()[0]["text"],
                "entry registration works"
            );
            assert_eq!(fetch.request_count(), 1);
            Ok(0)
        })
    });
    assert_eq!(run_bun_cli_entry(&[], &runner, main).await.unwrap(), 0);
}

/// `registerBedrock` installs the real module; the override slot itself is
/// covered by `pi-ai`'s `bedrock_provider` tests. This asserts the shim is
/// callable and idempotent (the binary runs it once per process).
#[tokio::test]
async fn register_bedrock_is_idempotent() {
    let _module = BEDROCK_MODULE.lock().await;
    register_bedrock();
    register_bedrock();
}

struct RecordingRunner {
    calls: Mutex<Vec<JsString>>,
}

impl DetachedRunnerHost for RecordingRunner {
    fn run_bash<'a>(&'a self, manifest_path: &'a JsString) -> BoxFuture<'a, Result<(), JsString>> {
        self.calls.lock().unwrap().push(JsString::from("bash:") + manifest_path);
        Box::pin(async { Ok(()) })
    }

    fn run_pyrun<'a>(&'a self, manifest_path: &'a JsString) -> BoxFuture<'a, Result<(), JsString>> {
        self.calls
            .lock()
            .unwrap()
            .push(JsString::from("pyrun:") + manifest_path);
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn bun_cli_entry_registers_and_hands_off_to_the_cli() {
    let _module = BEDROCK_MODULE.lock().await;
    let runner = RecordingRunner {
        calls: Mutex::new(Vec::new()),
    };
    let args = vec![DETACHED_BASH_RUNNER_MODE.to_owned(), "/tmp/manifest.json".to_owned()];
    let main_called = Mutex::new(false);
    let main: CliMain<'_> = Box::new(|_args: &[String]| {
        *main_called.lock().unwrap() = true;
        Box::pin(async { Ok(0) })
    });

    let result = run_bun_cli_entry(&args, &runner, main).await.unwrap();
    assert_eq!(result, 0);
    assert!(!*main_called.lock().unwrap());
    assert_eq!(
        runner.calls.lock().unwrap().clone(),
        vec!["bash:/tmp/manifest.json".to_owned()]
    );
}

#[tokio::test]
async fn bun_cli_entry_runs_main_when_no_internal_runner_matches() {
    let _module = BEDROCK_MODULE.lock().await;
    let runner = RecordingRunner {
        calls: Mutex::new(Vec::new()),
    };
    let args = vec!["--version".to_owned()];
    let seen = Mutex::new(Vec::new());
    let main: CliMain<'_> = Box::new(|args: &[String]| {
        seen.lock().unwrap().extend(args.iter().cloned());
        Box::pin(async { Ok(0) })
    });

    let result = run_bun_cli_entry(&args, &runner, main).await.unwrap();
    assert_eq!(result, 0, "main path returns its exit code");
    assert_eq!(*seen.lock().unwrap(), vec!["--version".to_owned()]);
    assert!(runner.calls.lock().unwrap().is_empty());
}
