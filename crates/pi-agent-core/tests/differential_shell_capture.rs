use crate::shared_oracle as oracle;

use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::types::*;
use pi_agent_core::harness::utils::{ShellCaptureOptions, execute_shell_with_capture};
use pi_ai::request::BoxFuture;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

struct CaptureEnv {
    inner: NodeExecutionEnv,
    request: Value,
    file: Arc<Mutex<JsString>>,
}

impl FileSystem for CaptureEnv {
    fn cwd(&self) -> &JsString {
        FileSystem::cwd(&self.inner)
    }
    fn absolute_path<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<JsString, OperationFailure<FileError>>> {
        FileSystem::absolute_path(&self.inner, path, abort)
    }
    fn join_path<'a>(
        &'a self,
        parts: &'a [JsString],
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<JsString, FileError>> {
        FileSystem::join_path(&self.inner, parts, abort)
    }
    fn read_text_file<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<String, OperationFailure<FileError>>> {
        FileSystem::read_text_file(&self.inner, path, abort)
    }
    fn read_text_lines<'a>(
        &'a self,
        path: &'a JsString,
        options: ReadTextLinesOptions,
    ) -> BoxFuture<'a, Result<Vec<String>, OperationFailure<FileError>>> {
        FileSystem::read_text_lines(&self.inner, path, options)
    }
    fn read_binary_file<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<Vec<u8>, OperationFailure<FileError>>> {
        FileSystem::read_binary_file(&self.inner, path, abort)
    }
    fn write_file<'a>(
        &'a self,
        path: &'a JsString,
        content: FileContent<'a>,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<(), OperationFailure<FileError>>> {
        FileSystem::write_file(&self.inner, path, content, abort)
    }
    fn file_info<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<FileInfo, OperationFailure<FileError>>> {
        FileSystem::file_info(&self.inner, path)
    }
    fn list_dir<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<Vec<FileInfo>, OperationFailure<FileError>>> {
        FileSystem::list_dir(&self.inner, path, abort)
    }
    fn canonical_path<'a>(
        &'a self,
        path: &'a JsString,
    ) -> BoxFuture<'a, Result<JsString, OperationFailure<FileError>>> {
        FileSystem::canonical_path(&self.inner, path)
    }
    fn exists<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<bool, OperationFailure<FileError>>> {
        FileSystem::exists(&self.inner, path, abort)
    }
    fn create_dir<'a>(
        &'a self,
        path: &'a JsString,
        options: CreateDirOptions,
    ) -> BoxFuture<'a, Result<(), OperationFailure<FileError>>> {
        FileSystem::create_dir(&self.inner, path, options)
    }
    fn remove<'a>(
        &'a self,
        path: &'a JsString,
        options: RemoveOptions,
    ) -> BoxFuture<'a, Result<(), OperationFailure<FileError>>> {
        FileSystem::remove(&self.inner, path, options)
    }
    fn create_temp_dir<'a>(&'a self, prefix: &'a JsString) -> BoxFuture<'a, Result<JsString, FileError>> {
        FileSystem::create_temp_dir(&self.inner, prefix)
    }
    fn cleanup(&self) -> BoxFuture<'_, ()> {
        FileSystem::cleanup(&self.inner)
    }
    fn create_temp_file<'a>(&'a self, _options: CreateTempFileOptions) -> BoxFuture<'a, Result<JsString, FileError>> {
        Box::pin(async move {
            if self.request["failCreate"] == true {
                Err(FileError::new(
                    FileErrorCode::PermissionDenied,
                    "create denied",
                    None,
                    None,
                ))
            } else {
                Ok("/capture.log".into())
            }
        })
    }
    fn append_file<'a>(
        &'a self,
        _path: &'a JsString,
        content: FileContent<'a>,
        _abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<(), OperationFailure<FileError>>> {
        Box::pin(async move {
            if self.request["failAppend"] == true {
                return Err(FileError::new(FileErrorCode::PermissionDenied, "append denied", None, None).into());
            }
            let FileContent::Text(text) = content else {
                panic!("capture writes text");
            };
            self.file.lock().unwrap().push_str(text);
            Ok(())
        })
    }
}

impl Shell for CaptureEnv {
    fn exec<'a>(
        &'a self,
        _command: &'a JsString,
        options: ShellExecOptions,
    ) -> BoxFuture<'a, Result<ShellExecResult, OperationFailure<ExecutionError>>> {
        Box::pin(async move {
            for chunk in self.request["chunks"].as_array().unwrap() {
                options.on_stdout.as_ref().unwrap()(chunk.as_str().unwrap()).expect("capture catches callback errors");
            }
            if let Some(code) = self.request["execError"].as_str() {
                let code = match code {
                    "aborted" => ExecutionErrorCode::Aborted,
                    "timeout" => ExecutionErrorCode::Timeout,
                    _ => panic!("unknown fixture code"),
                };
                return Err(ExecutionError::new(code, "execution failed", None).into());
            }
            Ok(ShellExecResult {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: 7,
            })
        })
    }
    fn cleanup(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}

fn capture_json(result: Result<pi_agent_core::harness::utils::ShellCaptureResult, ExecutionError>) -> JsValue {
    match result {
        Ok(result) => {
            let mut value = pi_js_compat::js_json!({"output":result.output,"cancelled":result.cancelled,"truncated":result.truncated});
            if let Some(code) = result.exit_code {
                value["exitCode"] = JsValue::Number(f64::from(code));
            }
            if let Some(path) = result.full_output_path {
                value["fullOutputPath"] = JsValue::String(path);
            }
            pi_js_compat::js_json!({"ok":true,"value":value})
        }
        Err(error) => {
            pi_js_compat::js_json!({"ok":false,"error":{"name":error.name,"code":error.code.as_str(),"message":error.message}})
        }
    }
}

#[tokio::test]
async fn chunk_retention_spill_errors_callbacks_and_cancellation_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-core-shell-capture-oracle.ts",
        "packages/agent-core/src/harness/utils/shell-output.ts",
    );
    let spill = "x".repeat(11000);
    let cases = [
        json!({"chunks":["a\rb\u{0001}c"]}),
        json!({"chunks":["\u{0001}".repeat(11000)]}),
        json!({"chunks":["x".repeat(15000),"y".repeat(10000),"z"]}),
        json!({"chunks":["x\n".repeat(2100)]}),
        json!({"chunks":[spill.clone(),"later"],"failCreate":true}),
        json!({"chunks":[spill.clone(),"later"],"failAppend":true}),
        json!({"chunks":[spill.clone(),"later"],"failCallback":true}),
        json!({"chunks":[spill.clone(),"later"],"failCreate":true,"failCallback":true}),
        json!({"chunks":[spill],"failAppend":true,"failCallback":true}),
        json!({"chunks":["output"],"execError":"timeout"}),
        json!({"chunks":["output"],"execError":"aborted"}),
        json!({"chunks":["output"],"abort":true}),
        json!({"chunks":["output"],"execError":"aborted","failCallback":true}),
    ];
    for request in cases {
        let file = Arc::new(Mutex::new(JsString::default()));
        let observed = Arc::new(Mutex::new(Vec::new()));
        let root = tempfile::tempdir().unwrap();
        let env: Arc<dyn ExecutionEnv> = Arc::new(CaptureEnv {
            inner: NodeExecutionEnv::new(NodeExecutionEnvOptions {
                cwd: root.path().to_str().unwrap().into(),
                shell_path: None,
                shell_env: Default::default(),
            }),
            request: request.clone(),
            file: Arc::clone(&file),
        });
        let fail_callback = request["failCallback"] == true;
        let callback_observed = Arc::clone(&observed);
        let controller_abort = pi_js_compat::abort::AbortController::new();
        let abort = controller_abort.signal();
        if request["abort"] == true {
            controller_abort.abort(None);
        }
        let result = execute_shell_with_capture(
            &env,
            &"fixture".into(),
            ShellCaptureOptions {
                abort: Some(abort),
                on_chunk: Some(Arc::new(move |text| {
                    callback_observed.lock().unwrap().push(text.clone());
                    if fail_callback {
                        Err(ExecutionError::new(
                            ExecutionErrorCode::Unknown,
                            "callback failed",
                            None,
                        ))
                    } else {
                        Ok(())
                    }
                })),
                ..Default::default()
            },
        )
        .await;
        let actual = pi_js_compat::js_json!({"result":capture_json(result),"file":file.lock().unwrap().clone(),"observed":observed.lock().unwrap().clone()});
        let expected = oracle.request(&request);
        assert_eq!(
            actual,
            parse_lossless(expected["wire"].as_str().unwrap()).unwrap(),
            "fixture {:?}",
            request.as_object().unwrap().keys().collect::<Vec<_>>()
        );
    }
}
