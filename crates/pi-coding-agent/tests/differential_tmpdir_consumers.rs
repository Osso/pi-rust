use crate::shared_oracle as oracle;

use pi_agent_core::agent::AgentToolResult;
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::types::{CreateTempFileOptions, FileSystem};
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::tools::output_accumulator::{OutputAccumulator, OutputAccumulatorOptions, SnapshotOptions};
use pi_coding_agent::core::tools::tool_definition_wrapper::{ToolOutputSpill, cap_tool_result_output};
use pi_coding_agent::utils::paths::node_dirname;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use pi_js_compat::{js_json as json, process_env};
use std::path::Path;
use std::sync::Mutex;

static ENVIRONMENT: Mutex<()> = Mutex::new(());

fn set_environment(env: &JsValue) {
    for key in ["TMPDIR", "TMP", "TEMP"] {
        process_env::remove(key);
    }
    for (key, value) in env.as_object().unwrap() {
        process_env::set(&encode_node_utf8(key), &encode_node_utf8(value.as_string().unwrap()));
    }
}
fn describe_file(path: &JsString, prefix: &str) -> JsValue {
    let name = Path::new(&encode_node_utf8(path))
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let suffix = name
        .strip_prefix(&format!("{prefix}-"))
        .unwrap()
        .strip_suffix(".log")
        .unwrap();
    json!({"root": node_dirname(path), "bytes": std::fs::read(encode_node_utf8(path)).unwrap().iter().map(|byte|format!("{byte:02x}")).collect::<String>(), "name":suffix.len()==16&&suffix.bytes().all(|byte|byte.is_ascii_hexdigit()&&!byte.is_ascii_uppercase())})
}
fn run_cases(operation: &str, module: &str, mut probe: impl FnMut(&JsValue) -> JsValue) {
    let _guard = ENVIRONMENT.lock().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let before = directory.path().join("before");
    let after = directory.path().join("after");
    std::fs::create_dir(&before).unwrap();
    std::fs::create_dir(&after).unwrap();
    let before = before.to_str().unwrap();
    let after = after.to_str().unwrap();
    let mut oracle = oracle::TsOracle::spawn("tmpdir-consumers-oracle.ts", module);
    for env in [
        json!({"TMPDIR":after}),
        json!({"TMPDIR":"","TMP":after}),
        json!({"TEMP":after}),
        json!({"TMPDIR":format!("{after}///")}),
    ] {
        let initial = json!({"TMPDIR":before});
        let expected = oracle.request(&json!({"id":1,"operation":operation,"before":initial,"env":env}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        set_environment(&initial);
        let actual = probe(&env);
        process_env::clear();
        assert_eq!(actual, expected["output"], "{operation}, env={env:?}");
    }
}
#[test]
fn accumulator_reads_tmpdir_at_first_spill_not_construction() {
    run_cases(
        "accumulator",
        "packages/coding-agent/src/core/tools/output-accumulator.ts",
        |env| {
            let mut output = OutputAccumulator::new(OutputAccumulatorOptions {
                max_bytes: 1.0,
                ..Default::default()
            });
            set_environment(env);
            output.append(b"payload").unwrap();
            output.finish().unwrap();
            let path = output
                .snapshot(SnapshotOptions::default())
                .unwrap()
                .full_output_path
                .unwrap();
            output.close_temp_file().unwrap();
            let same = output
                .snapshot(SnapshotOptions {
                    persist_if_truncated: true,
                })
                .unwrap()
                .full_output_path
                .as_ref()
                == Some(&path);
            let mut result = describe_file(&path, "pi-output");
            result
                .as_object_mut()
                .unwrap()
                .insert("afterCloseSamePath".into(), json!(same));
            output.close_temp_file().unwrap();
            std::fs::remove_file(encode_node_utf8(&path)).unwrap();
            result
        },
    );
}
#[test]
fn node_environment_uses_mutable_node_tmpdir_for_directories_and_files() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    run_cases("node", "packages/agent-core/src/harness/env/nodejs.ts", |env| {
        runtime.block_on(async {
        let filesystem = NodeExecutionEnv::new(NodeExecutionEnvOptions {cwd:"/cwd".into(),..Default::default()});
        set_environment(env);
        let directory = filesystem.create_temp_dir(&"probe-".into()).await.unwrap();
        let file = filesystem.create_temp_file(CreateTempFileOptions::default()).await.unwrap();
        let path = encode_node_utf8(&directory);
        let name = Path::new(&path).file_name().unwrap().to_str().unwrap();
        let suffix = name.strip_prefix("probe-").unwrap();
        let result = json!({"root":node_dirname(&directory),"directory":std::fs::metadata(&path).unwrap().is_dir(),"name":suffix.len()==6&&suffix.bytes().all(|byte|byte.is_ascii_alphanumeric()),"fileRoot":node_dirname(node_dirname(&file)),"fileBytes":std::fs::read(encode_node_utf8(&file)).unwrap().iter().map(|byte|format!("{byte:02x}")).collect::<String>()});
        std::fs::remove_dir(encode_node_utf8(&directory)).unwrap();
        std::fs::remove_dir_all(encode_node_utf8(&node_dirname(&file))).unwrap();
        result
    })
    });
}
#[test]
fn wrapper_spill_uses_mutable_node_tmpdir() {
    run_cases(
        "wrapper",
        "packages/coding-agent/src/core/tools/tool-definition-wrapper.ts",
        |env| {
            set_environment(env);
            let result = AgentToolResult {
                content: vec![TextOrImageContent::Text(TextContent::new("line\n".repeat(2500)))],
                details: None,
                is_error: None,
                terminate: None,
            };
            let mut spill = ToolOutputSpill::default();
            cap_tool_result_output(result, &mut spill).unwrap();
            let path = spill.full_output_path.unwrap();
            let result = describe_file(&path, "pi-tool");
            std::fs::remove_file(encode_node_utf8(&path)).unwrap();
            result
        },
    );
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
