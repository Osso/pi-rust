#[path = "support/oracle.rs"]
mod oracle;

use pi_agent_core::agent::{AgentTool, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::{
    bash::{BashExecOptions, BashExecResult, BashOperations, BashTool, BashToolOptions},
    edit::{EditOperations, EditTool, EditToolOptions},
    find::{FindOperations, FindTool, FindToolOptions},
    grep::{GrepOperations, GrepTool, GrepToolOptions},
    ls::{LsOperations, LsTool, LsToolOptions},
    read::{ReadOperations, ReadTool, ReadToolOptions},
    write::{WriteOperations, WriteTool, WriteToolOptions},
};
use serde_json::{Value, json};
use std::{io, sync::Arc};

struct Fault(&'static str);
impl GrepOperations for Fault {
    fn is_directory<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async { self.result("directory", true) })
    }
    fn read_file<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<String>> {
        Box::pin(async { self.result("readFile", String::new()) })
    }
}
impl FindOperations for Fault {
    fn supports_glob(&self) -> bool {
        true
    }
    fn exists<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async { Ok(false) })
    }
}
impl BashOperations for Fault {
    fn exec<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        Box::pin(async { Err(ToolError::new(self.0)) })
    }
}
impl Fault {
    fn result<T>(&self, stage: &str, value: T) -> io::Result<T> {
        if self.0 == stage {
            Err(io::Error::other("SSH failed (7): scripted failure\n"))
        } else {
            Ok(value)
        }
    }
}
impl ReadOperations for Fault {
    fn access<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async { self.result("access", ()) })
    }
    fn read_file<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async { self.result(if self.0 == "image" { "image" } else { "readFile" }, b"old".to_vec()) })
    }
    fn detect_image_mime_type<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<Option<String>>> {
        Box::pin(async {
            self.result(
                "mime",
                if self.0 == "image" {
                    Some("image/png".into())
                } else {
                    None
                },
            )
        })
    }
    fn supports_read_range(&self) -> bool {
        self.0 == "range"
    }
    fn stat<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<Option<u64>>> {
        Box::pin(async { self.result("stat", Some(100_000)) })
    }
    fn read_range<'a>(&'a self, _: &'a str, _: u64, _: u64) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        Box::pin(async { self.result("range", Vec::new()) })
    }
}
impl WriteOperations for Fault {
    fn mkdir<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async { self.result("mkdir", ()) })
    }
    fn write_file<'a>(&'a self, _: &'a str, _: &'a str) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(async { self.result("writeFile", ()) })
    }
}
impl EditOperations for Fault {
    fn access<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<()>> {
        ReadOperations::access(self, "")
    }
    fn read_file<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<Vec<u8>>> {
        ReadOperations::read_file(self, "")
    }
    fn write_file<'a>(&'a self, _: &'a str, _: &'a str) -> BoxFuture<'a, io::Result<()>> {
        WriteOperations::write_file(self, "", "")
    }
}
impl LsOperations for Fault {
    fn exists<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async { Ok(true) })
    }
    fn is_directory<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async { self.result("stat", true) })
    }
    fn readdir<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<Vec<String>>> {
        Box::pin(async { self.result("readdir", Vec::new()) })
    }
}
async fn error(tool: &dyn AgentTool, params: Value) -> String {
    tool.execute("fault".into(), params, None, Arc::new(|_| {}), None)
        .await
        .err()
        .map(|e: ToolError| e.message)
        .expect("must fail")
}
#[tokio::test]
async fn custom_errors_match_pinned_tool_catch_boundaries() {
    let mut oracle = oracle::TsOracle::spawn(
        "custom-operation-errors-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
    );
    let mut mismatches = Vec::new();
    for (name, stages) in [
        ("read", vec!["access", "mime", "stat", "readFile", "range", "image"]),
        ("write", vec!["mkdir", "writeFile"]),
        ("edit", vec!["access", "readFile", "writeFile"]),
        ("ls", vec!["stat", "readdir"]),
    ] {
        for stage in stages {
            let ops = Arc::new(Fault(stage));
            let tool: Box<dyn AgentTool> = match name {
                "read" => Box::new(ReadTool::with_options(
                    "/fixture",
                    ReadToolOptions {
                        operations: Some(ops),
                        ..Default::default()
                    },
                )),
                "write" => Box::new(WriteTool::with_options(
                    "/fixture",
                    WriteToolOptions { operations: Some(ops) },
                )),
                "edit" => Box::new(EditTool::with_options(
                    "/fixture",
                    EditToolOptions { operations: Some(ops) },
                )),
                _ => Box::new(LsTool::with_options(
                    "/fixture",
                    LsToolOptions { operations: Some(ops) },
                )),
            };
            let params = json!({"path":"file.txt", "content":"new", "edits":[{"oldText":"old","newText":"new"}]});
            let expected = oracle.request(&json!({"tool":name,"stage":stage,"params":params}));
            let actual = error(tool.as_ref(), params).await;
            if actual != expected["error"].as_str().unwrap() {
                mismatches.push(format!("{name}/{stage}: {actual:?} != {}", expected["error"]));
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[tokio::test]
async fn intentional_grep_find_and_bash_error_handling_matches_source() {
    let mut oracle = oracle::TsOracle::spawn(
        "custom-operation-errors-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
    );
    for (name, stage) in [
        ("grep", "directory"),
        ("find", "missing"),
        ("bash", "remote denied"),
        ("bash", "aborted"),
        ("bash", "timeout:2"),
    ] {
        let ops = Arc::new(Fault(stage));
        let tool: Box<dyn AgentTool> = match name {
            "grep" => Box::new(GrepTool::with_options(
                "/fixture",
                GrepToolOptions { operations: Some(ops) },
            )),
            "find" => Box::new(FindTool::with_options(
                "/fixture",
                FindToolOptions { operations: Some(ops) },
            )),
            _ => Box::new(BashTool::with_options(
                "/fixture",
                BashToolOptions {
                    operations: Some(ops),
                    ..Default::default()
                },
            )),
        };
        let params = json!({"path":"file.txt","pattern":"needle","command":"unused"});
        let expected = oracle.request(&json!({"tool":name,"stage":stage,"message":stage,"params":params}));
        assert_eq!(
            error(tool.as_ref(), params).await,
            expected["error"].as_str().unwrap(),
            "{name}/{stage}"
        );
    }
}

struct RejectingOperations {
    stage: &'static str,
    message: &'static str,
    abort: &'static str,
    signal: tokio_util::sync::CancellationToken,
}
impl RejectingOperations {
    async fn reject<T>(&self, stage: &str, value: T) -> io::Result<T> {
        if self.stage != stage {
            return Ok(value);
        }
        if self.abort == "pending" {
            self.signal.cancel();
            return std::future::pending().await;
        }
        if self.abort == "reject" {
            self.signal.cancel();
        }
        Err(io::Error::other(self.message))
    }
}
impl LsOperations for RejectingOperations {
    fn exists<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async { self.reject("exists", true).await })
    }
    fn is_directory<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async { Err(io::Error::other("later stat failure")) })
    }
    fn readdir<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<Vec<String>>> {
        Box::pin(async { Err(io::Error::other("later readdir failure")) })
    }
}
impl FindOperations for RejectingOperations {
    fn supports_glob(&self) -> bool {
        true
    }
    fn exists<'a>(&'a self, _: &'a str) -> BoxFuture<'a, io::Result<bool>> {
        Box::pin(async { self.reject("exists", self.stage != "missing").await })
    }
    fn glob<'a>(&'a self, _: &'a str, _: &'a str, _: &'a [String], _: usize) -> BoxFuture<'a, io::Result<Vec<String>>> {
        Box::pin(async { self.reject("glob", Vec::new()).await })
    }
}

async fn compare_rejection(tool_name: &str, stage: &'static str, abort: &'static str, message: &'static str) {
    let mut oracle = oracle::TsOracle::spawn(
        "custom-operation-errors-oracle.ts",
        "packages/coding-agent/src/core/tools/read.ts",
    );
    let signal = tokio_util::sync::CancellationToken::new();
    if abort == "before" {
        signal.cancel();
    }
    let ops = Arc::new(RejectingOperations {
        stage,
        message,
        abort,
        signal: signal.clone(),
    });
    let tool: Box<dyn AgentTool> = if tool_name == "ls" {
        Box::new(LsTool::with_options(
            "/fixture",
            LsToolOptions { operations: Some(ops) },
        ))
    } else {
        Box::new(FindTool::with_options(
            "/fixture",
            FindToolOptions { operations: Some(ops) },
        ))
    };
    let params = json!({"path":"file.txt","pattern":"needle"});
    let expected =
        oracle.request(&json!({"tool":tool_name,"stage":stage,"message":message,"abort":abort,"params":params}));
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        tool.execute("fault".into(), params, Some(signal), Arc::new(|_| {}), None),
    )
    .await;
    let actual = outcome
        .expect("abort must settle without waiting for pending operation")
        .err()
        .map(|error| error.message);
    assert_eq!(json!({"error":actual}), expected, "{tool_name}/{stage}/{abort}");
}

#[tokio::test]
async fn ls_exists_rejection_precedes_stat_and_readdir() {
    for message in ["SSH failed (7): scripted failure\n", "", "拒否: é\nsecond line"] {
        compare_rejection("ls", "exists", "none", message).await;
    }
}
#[tokio::test]
async fn find_exists_rejection_precedes_glob() {
    for message in ["SSH failed (7): scripted failure\n", "", "拒否: é\nsecond line"] {
        compare_rejection("find", "exists", "none", message).await;
    }
}
#[tokio::test]
async fn find_glob_rejection_propagates_without_wrapping() {
    for message in ["SSH failed (7): scripted failure\n", "", "拒否: é\nsecond line"] {
        compare_rejection("find", "glob", "none", message).await;
    }
}
#[tokio::test]
async fn operation_abort_wins_before_rejection_or_while_pending() {
    for tool in ["ls", "find"] {
        for stage in if tool == "ls" {
            vec!["exists"]
        } else {
            vec!["exists", "glob"]
        } {
            for abort in ["before", "reject", "pending"] {
                compare_rejection(tool, stage, abort, "remote denied").await;
            }
        }
    }
}
