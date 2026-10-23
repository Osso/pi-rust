use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::types::*;
use pi_ai::request::BoxFuture;
use pi_js_compat::{abort::AbortSignal, js_json::JsString, node_error::NativeNodeError};
use std::sync::Mutex;

#[derive(Clone, Copy)]
pub struct FaultRule {
    pub method: &'static str,
    pub nth: usize,
    pub after: bool,
    pub rejected: bool,
    pub code: FileErrorCode,
}
pub fn fault_message() -> JsString {
    JsString::from("fault") + JsString::from_utf16(vec![0xd800])
}
pub struct FaultFs {
    inner: NodeExecutionEnv,
    rule: Mutex<Option<FaultRule>>,
}
impl FaultFs {
    pub fn new(root: JsString) -> Self {
        Self {
            inner: NodeExecutionEnv::new(NodeExecutionEnvOptions {
                cwd: root,
                ..Default::default()
            }),
            rule: Mutex::new(None),
        }
    }
    pub fn arm(&self, rule: Option<FaultRule>) {
        *self.rule.lock().unwrap() = rule;
    }
    fn take(&self, method: &str) -> Option<(bool, OperationFailure<FileError>)> {
        let mut rule = self.rule.lock().unwrap();
        let current = rule.as_mut()?;
        if current.method != method {
            return None;
        }
        current.nth -= 1;
        if current.nth != 0 {
            return None;
        }
        let selected = rule.take().unwrap();
        let error = if selected.rejected {
            OperationFailure::Rejected(NodeRejection {
                error: NativeNodeError::new("TypeError", fault_message()),
            })
        } else {
            FileError::new(selected.code, fault_message(), None, None).into()
        };
        Some((selected.after, error))
    }
    fn guard<'a, T: Send + 'a>(
        &'a self,
        method: &str,
        future: BoxFuture<'a, Result<T, OperationFailure<FileError>>>,
    ) -> BoxFuture<'a, Result<T, OperationFailure<FileError>>> {
        let fault = self.take(method);
        Box::pin(async move {
            if let Some((false, error)) = fault {
                return Err(error.into());
            }
            let result = future.await;
            if let Some((true, error)) = fault {
                return Err(error.into());
            }
            result
        })
    }
}
impl FileSystem for FaultFs {
    fn cwd(&self) -> &JsString {
        self.inner.cwd()
    }
    fn absolute_path<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<JsString, OperationFailure<FileError>>> {
        self.guard("absolutePath", self.inner.absolute_path(path, abort))
    }
    fn join_path<'a>(
        &'a self,
        parts: &'a [JsString],
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<JsString, FileError>> {
        if let Some((_, error)) = self.take("joinPath") {
            let OperationFailure::Returned(error) = error else {
                panic!("joinPath has no promise-rejection carrier");
            };
            return Box::pin(async move { Err(error) });
        }
        self.inner.join_path(parts, abort)
    }
    fn read_text_file<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<String, OperationFailure<FileError>>> {
        self.guard("readTextFile", self.inner.read_text_file(path, abort))
    }
    fn read_text_lines<'a>(
        &'a self,
        path: &'a JsString,
        options: ReadTextLinesOptions,
    ) -> BoxFuture<'a, Result<Vec<String>, OperationFailure<FileError>>> {
        self.guard("readTextLines", self.inner.read_text_lines(path, options))
    }
    fn read_binary_file<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<Vec<u8>, OperationFailure<FileError>>> {
        self.inner.read_binary_file(path, abort)
    }
    fn write_file<'a>(
        &'a self,
        path: &'a JsString,
        content: FileContent<'a>,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<(), OperationFailure<FileError>>> {
        self.guard("writeFile", self.inner.write_file(path, content, abort))
    }
    fn append_file<'a>(
        &'a self,
        path: &'a JsString,
        content: FileContent<'a>,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<(), OperationFailure<FileError>>> {
        self.guard("appendFile", self.inner.append_file(path, content, abort))
    }
    fn file_info<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<FileInfo, OperationFailure<FileError>>> {
        self.inner.file_info(path)
    }
    fn list_dir<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<Vec<FileInfo>, OperationFailure<FileError>>> {
        self.guard("listDir", self.inner.list_dir(path, abort))
    }
    fn canonical_path<'a>(
        &'a self,
        path: &'a JsString,
    ) -> BoxFuture<'a, Result<JsString, OperationFailure<FileError>>> {
        self.inner.canonical_path(path)
    }
    fn exists<'a>(
        &'a self,
        path: &'a JsString,
        abort: Option<&'a AbortSignal>,
    ) -> BoxFuture<'a, Result<bool, OperationFailure<FileError>>> {
        self.guard("exists", self.inner.exists(path, abort))
    }
    fn create_dir<'a>(
        &'a self,
        path: &'a JsString,
        options: CreateDirOptions,
    ) -> BoxFuture<'a, Result<(), OperationFailure<FileError>>> {
        self.guard("createDir", self.inner.create_dir(path, options))
    }
    fn remove<'a>(
        &'a self,
        path: &'a JsString,
        options: RemoveOptions,
    ) -> BoxFuture<'a, Result<(), OperationFailure<FileError>>> {
        self.guard("remove", self.inner.remove(path, options))
    }
    fn create_temp_dir<'a>(&'a self, prefix: &'a JsString) -> BoxFuture<'a, Result<JsString, FileError>> {
        self.inner.create_temp_dir(prefix)
    }
    fn create_temp_file<'a>(&'a self, options: CreateTempFileOptions) -> BoxFuture<'a, Result<JsString, FileError>> {
        self.inner.create_temp_file(options)
    }
    fn cleanup(&self) -> BoxFuture<'_, ()> {
        FileSystem::cleanup(&self.inner)
    }
}
