#[path = "support/oracle.rs"]
mod oracle;

use pi_agent_core::agent::{AgentTool, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::tools::{
    edit::{EditOperations, EditTool, EditToolOptions},
    ls::{LsOperations, LsTool, LsToolOptions},
    read::{ReadOperations, ReadTool, ReadToolOptions},
    write::{WriteOperations, WriteTool, WriteToolOptions},
};
use serde_json::{Value, json};
use std::{io, sync::Arc};

struct Fault(&'static str);
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
    fn exists<'a>(&'a self, _: &'a str) -> BoxFuture<'a, bool> {
        Box::pin(async { true })
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
            assert_eq!(
                error(tool.as_ref(), params).await,
                expected["error"].as_str().unwrap(),
                "{name}/{stage}"
            );
        }
    }
}
