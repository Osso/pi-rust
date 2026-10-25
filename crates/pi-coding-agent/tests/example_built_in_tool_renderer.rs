#[path = "../examples/extensions/built_in_tool_renderer.rs"]
mod example;
#[path = "support/extension_examples_h.rs"]
mod support;

use pi_coding_agent::core::extensions::*;
use serde_json::{Value, json};
use std::sync::Arc;

fn fixtures() -> Vec<Value> {
    let mut cases = vec![
        json!({"tool":"read","args":{"path":"src/中文 λ.rs"}}),
        json!({"tool":"read","args":{"path":"x","offset":2,"limit":10}}),
        json!({"tool":"read","args":{"path":"x","offset":0,"limit":3}}),
        json!({"tool":"read","args":{"path":"x","offset":7,"limit":0}}),
        json!({"tool":"read","args":{"path":"x","offset":2.0,"limit":1e21}}),
        json!({"tool":"bash","args":{"command":"echo numeric","timeout":2.0}}),
        json!({"tool":"bash","args":{"command":"printf 'hello'"}}),
        json!({"tool":"bash","args":{"command":"x".repeat(80),"timeout":0}}),
        json!({"tool":"bash","args":{"command":format!("{} λ🚀", "x".repeat(100)),"timeout":2.5}}),
        json!({"tool":"bash","args":{"command":format!("{}🚀{}", "x".repeat(75),"z".repeat(10))}}),
        json!({"tool":"bash","args":{"command":format!("{}🚀{}", "x".repeat(76),"z".repeat(10))}}),
        json!({"tool":"edit","args":{"path":"src/λ.rs"}}),
        json!({"tool":"write","args":{"path":"x","content":"one\ntwo\n"}}),
        json!({"tool":"write","args":{"path":"x","content":""}}),
    ];
    let read_results = vec![
        json!({"content":[]}),
        json!({"content":[{"type":"image","data":"aGVsbG8=","mimeType":"image/png"}]}),
        json!({"content":[{"type":"text","text":""}]}),
        json!({"content":[{"type":"text","text":"first\n中文\n\u{1b}[31mred\u{1b}[0m\n"}]}),
        json!({"content":[{"type":"text","text":(0..18).map(|i| format!("row {i}")).collect::<Vec<_>>().join("\n")}],"details":{"truncation":{"truncated":true,"totalLines":321.0}}}),
        json!({"content":[{"type":"text","text":"Error: read denied\ncontext"}],"isError":true}),
    ];
    let bash_results = vec![
        json!({"content":[]}),
        json!({"content":[{"type":"text","text":""}]}),
        json!({"content":[{"type":"text","text":"  \n\t\n\u{feff}\nline\nexit code: 0"}]}),
        json!({"content":[{"type":"text","text":"exit code: 7\nfailed\n"}],"isError":true}),
        json!({"content":[{"type":"text","text":"exit code: -1\nexit code: 02\nexit code: 0"}],"details":{"truncation":{"truncated":false}}}),
        json!({"content":[{"type":"text","text":(0..23).map(|i| format!("out {i}")).collect::<Vec<_>>().join("\n")}],"details":{"truncation":{"truncated":true}}}),
    ];
    let edit_results = vec![
        json!({"content":[]}),
        json!({"content":[{"type":"text","text":"Error missing\nsecond"}],"details":{"diff":"+ignored"},"isError":true}),
        json!({"content":[{"type":"text","text":"ok"}],"details":{"diff":""}}),
        json!({"content":[{"type":"text","text":"ok"}],"details":{"diff":"--- old\n+++ new\n-a\n+b\n+c\n unchanged\n@@ header\n"}}),
        json!({"content":[],"details":{"diff":(0..34).map(|i| format!("{}{i}",if i%2==0 {"+"} else {"-"})).collect::<Vec<_>>().join("\n")}}),
    ];
    let write_results = vec![
        json!({"content":[]}),
        json!({"content":[{"type":"text","text":"Error failed\nsecond"}],"isError":true}),
        json!({"content":[{"type":"text","text":"ok"}]}),
    ];
    for (tool, results) in [
        ("read", read_results),
        ("bash", bash_results),
        ("edit", edit_results),
        ("write", write_results),
    ] {
        for result in results {
            for expanded in [false, true] {
                for is_partial in [false, true] {
                    cases.push(json!({"tool":tool,"args":{},"result":result,"expanded":expanded,"isPartial":is_partial,"isError":result["isError"] == true}));
                }
            }
        }
    }
    cases
}

#[tokio::test]
async fn complete_renderers_and_delegated_execution_match_pinned_extension() {
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::current_dir().unwrap();
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
        &(temp.path()),
    )))
    .unwrap();
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::built_in_tool_renderer_extension(api);
        Box::pin(async move { result })
    });
    let actual = support::compare(factory,json!({
        "cwd":temp.path(),
        "colors":{"toolTitle":"#abcdef","accent":"#50b4e6","dim":"#5a5b5c","muted":"#969798","success":"#22cc66","error":"#ee4455","warning":"#ffbb33"},
        "widths":[1,7,16,40,80,120],"renders":fixtures(),
        "initial":{"read.txt":"first\nsecond\nthird\n","edit.txt":"before\n"},
        "executions":[
            {"tool":"read","params":{"path":"read.txt","offset":2,"limit":1}},
            {"tool":"bash","params":{"command":"printf 'private-renderer-output'"}},
            {"tool":"edit","params":{"path":"edit.txt","edits":[{"oldText":"before","newText":"after"}]}},
            {"tool":"write","params":{"path":"write.txt","content":"written\nλ"}},
            {"tool":"read","params":{"path":"missing.txt"}},
            {"tool":"edit","params":{"path":"edit.txt","edits":[{"oldText":"absent","newText":"unreachable"}]}},
            {"tool":"bash","params":{"command":"printf 'failed'; exit 7"}}
        ],"files":["read.txt","edit.txt","write.txt"]
    })).await;
    pi_js_compat::process_cwd::chdir(&pi_js_compat::node_fs::decode_node_path(std::ffi::OsStr::new(
        &(previous),
    )))
    .unwrap();
    assert_eq!(actual["renders"].as_array().unwrap().len(), 94);
    assert!(
        actual["renders"][10][5][0]
            .as_string()
            .unwrap()
            .units()
            .windows(4)
            .any(|units| units == [0xd83d, 46, 46, 46])
    );
    assert_eq!(actual["files"]["edit.txt"].as_string().unwrap(), "after\n");
    assert_eq!(actual["files"]["write.txt"].as_string().unwrap(), "written\nλ");
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
