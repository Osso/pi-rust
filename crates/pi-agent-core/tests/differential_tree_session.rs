#[path = "support/operation.rs"]
mod operation;
use crate::shared_oracle as oracle;

use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::session::*;
use pi_agent_core::harness::types::{FileError, FileSystem};
use pi_js_compat::js_json::{self, JsString, JsValue, from_value, parse_lossless, to_value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const SOURCE: &str = "packages/agent-core/src/harness/session/session.ts";
const ISO: &str = "2023-11-14T22:13:20.000Z";

fn parse(text: &str) -> JsValue {
    parse_lossless(text).unwrap()
}
fn ts_oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn("tree-session-oracle.ts", SOURCE)
}
fn request(oracle: &mut oracle::TsOracle, value: &JsValue) -> JsValue {
    let result = oracle.request(&serde_json::json!({"wire":js_json::stringify_lossless(value)}));
    parse_lossless(result["wire"].as_str().expect("outer JSON wire")).unwrap()
}
fn ids() -> Vec<JsString> {
    let mut oracle = ts_oracle();
    let result = request(
        &mut oracle,
        &pi_js_compat::js_json!({"kind":"uuidSamples","count":2048}),
    );
    result["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_string().unwrap().clone())
        .collect()
}
fn id_generator(values: &[JsString]) -> IdGenerator {
    let values = Arc::new(Mutex::new(VecDeque::from(values.to_vec())));
    Arc::new(move || {
        values
            .lock()
            .unwrap()
            .pop_front()
            .expect("fixture has enough frozen UUID values")
    })
}
fn clock() -> Clock {
    Arc::new(|| ISO.into())
}
fn string(value: &JsValue) -> JsString {
    value.as_string().unwrap().clone()
}
fn optional_string(value: Option<&JsValue>) -> Option<JsString> {
    value.and_then(JsValue::as_string).cloned()
}
fn field<'a>(value: &'a JsValue, key: &str) -> Option<&'a JsValue> {
    value.as_object().and_then(|value| value.get(key))
}
fn entries(value: &JsValue) -> Vec<SessionTreeEntry> {
    field(value, "entries")
        .and_then(JsValue::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .cloned()
        .map(|value| SessionTreeEntry::from_value(value).unwrap())
        .collect()
}
fn error_json(error: impl Into<SessionFailure>) -> JsValue {
    let error = operation::returned_error(error.into());
    let mut value = pi_js_compat::js_json!({"name":"SessionError","code":error.code.as_str(),"message":error.message});
    if let Some(cause) = error.cause {
        let cause = if let Some(error) = cause.downcast_ref::<FileError>() {
            pi_js_compat::js_json!({"name":error.name,"code":error.code.as_str(),"message":error.message})
        } else if let Some(error) = cause.downcast_ref::<js_json::JsonParseError>() {
            pi_js_compat::js_json!({"name":"SyntaxError","message":error.message()})
        } else {
            panic!("unexpected fixture cause: {cause}");
        };
        value["cause"] = cause;
    }
    value
}

async fn storage_operation<M: SessionMetadataValue + serde::Serialize>(
    storage: &dyn SessionStorage<M>,
    operation: &JsValue,
) -> Result<Option<JsValue>, SessionFailure> {
    match operation["op"].as_string().unwrap().to_utf8().unwrap().as_str() {
        "append" => {
            storage
                .append_entry(SessionTreeEntry::from_value(operation["entry"].clone())?)
                .await?;
            Ok(None)
        }
        "setLeaf" => {
            storage
                .set_leaf_id(optional_string(field(operation, "id")).as_ref())
                .await?;
            Ok(None)
        }
        "leaf" => Ok(Some(to_value(&storage.get_leaf_id().await?).unwrap())),
        "createId" => Ok(Some(JsValue::String(storage.create_entry_id().await?))),
        "entry" => Ok(storage
            .get_entry(&string(&operation["id"]))
            .await?
            .map(|entry| entry.value())),
        "entries" => Ok(Some(to_value(&storage.get_entries().await?).unwrap())),
        "find" => Ok(Some(
            to_value(&storage.find_entries(&string(&operation["type"])).await?).unwrap(),
        )),
        "label" => Ok(storage.get_label(&string(&operation["id"])).await?.map(JsValue::String)),
        "path" => Ok(Some(
            to_value(
                &storage
                    .get_path_to_root(optional_string(field(operation, "id")).as_ref())
                    .await?,
            )
            .unwrap(),
        )),
        "metadata" => Ok(Some(to_value(&storage.get_metadata().await?).unwrap())),
        other => panic!("unknown storage operation {other}"),
    }
}

async fn facade_operation<M: SessionMetadataValue + serde::Serialize>(
    session: &Session<M>,
    operation: &JsValue,
) -> Result<Option<JsValue>, SessionFailure> {
    let args = field(operation, "args")
        .and_then(JsValue::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let op = operation["op"].as_string().unwrap().to_utf8().unwrap();
    let id = match op.as_str() {
        "appendMessage" => session.append_message(from_value(&args[0]).unwrap()).await?,
        "thinking" => session.append_thinking_level_change(string(&args[0])).await?,
        "model" => session.append_model_change(string(&args[0]), string(&args[1])).await?,
        "tools" => {
            session
                .append_active_tools_change(from_value(&args[0]).unwrap())
                .await?
        }
        "compaction" => {
            session
                .append_compaction(
                    string(&args[0]),
                    string(&args[1]),
                    args[2].as_f64().unwrap(),
                    CompactionAppendOptions {
                        details: args.get(3).cloned(),
                        from_hook: args.get(4).and_then(JsValue::as_bool),
                        duration_ms: args.get(5).and_then(JsValue::as_f64),
                    },
                )
                .await?
        }
        "custom" => {
            session
                .append_custom_entry(string(&args[0]), args.get(1).cloned())
                .await?
        }
        "customMessage" => {
            session
                .append_custom_message_entry(
                    string(&args[0]),
                    from_value(&args[1]).unwrap(),
                    args[2].as_bool().unwrap(),
                    args.get(3).cloned(),
                )
                .await?
        }
        "appendLabel" => {
            session
                .append_label(string(&args[0]), optional_string(args.get(1)))
                .await?
        }
        "name" => session.append_session_name(&string(&args[0])).await?,
        "getName" => return Ok(session.get_session_name().await?.map(JsValue::String)),
        "move" => {
            let summary = args
                .get(1)
                .filter(|value| !matches!(value, JsValue::Null))
                .map(|value| MoveSummary {
                    summary: string(&value["summary"]),
                    details: field(value, "details").cloned(),
                    from_hook: field(value, "fromHook").and_then(JsValue::as_bool),
                });
            return Ok(session
                .move_to(optional_string(args.first()).as_ref(), summary)
                .await?
                .map(JsValue::String));
        }
        "branch" => {
            return Ok(Some(
                to_value(&session.get_branch(optional_string(args.first()).as_ref()).await?).unwrap(),
            ));
        }
        "context" => {
            let context = session.build_context().await?;
            return Ok(Some(context_json(context)));
        }
        _ => return storage_operation(session.get_storage(), operation).await,
    };
    Ok(Some(JsValue::String(id)))
}
fn context_json(context: SessionContext) -> JsValue {
    pi_js_compat::js_json!({"messages":context.messages,"thinkingLevel":context.thinking_level,"model":context.model.map(|model|pi_js_compat::js_json!({"provider":model.provider,"modelId":model.model_id})),"activeToolNames":context.active_tool_names})
}
async fn script<M: SessionMetadataValue + serde::Serialize>(
    storage: Arc<dyn SessionStorage<M>>,
    request: &JsValue,
) -> Result<JsValue, SessionFailure> {
    if request["kind"].as_string().is_some_and(|kind| kind == "fork") {
        let options = &request["options"];
        let options = SessionForkOptions {
            entry_id: optional_string(field(options, "entryId")),
            position: if field(options, "position")
                .and_then(JsValue::as_string)
                .is_some_and(|position| position == "at")
            {
                ForkPosition::At
            } else {
                ForkPosition::Before
            },
            id: None,
        };
        return Ok(to_value(&get_entries_to_fork(storage.as_ref(), &options).await?).unwrap());
    }
    let session = Session::with_clock(storage, clock());
    let mut results = Vec::new();
    for operation in request["operations"].as_array().unwrap() {
        let result = if request["kind"].as_string().is_some_and(|kind| kind == "facade") {
            facade_operation(&session, operation).await
        } else {
            storage_operation(session.get_storage(), operation).await
        };
        let mut value = pi_js_compat::js_json!({"op":operation["op"]});
        match result {
            Ok(Some(result)) => value["value"] = result,
            Ok(None) => {}
            Err(error) => value["error"] = error_json(error),
        }
        results.push(value);
    }
    Ok(JsValue::Array(results))
}
async fn rust(request: &JsValue, ids: &[JsString]) -> JsValue {
    let result = if request["kind"].as_string().is_some_and(|kind| kind == "context") {
        build_session_context(&entries(request))
            .map(context_json)
            .map_err(Into::into)
    } else if field(request, "mode")
        .and_then(JsValue::as_string)
        .is_some_and(|mode| mode == "jsonl")
    {
        let root = string(&request["root"]);
        let fs: Arc<dyn FileSystem> = Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
            cwd: root.clone(),
            shell_path: None,
            shell_env: Default::default(),
        }));
        let path = root.clone() + "/session.jsonl";
        let storage = if let Some(content) = field(request, "fileContent") {
            std::fs::write(
                js_json::encode_node_utf8(&path),
                js_json::encode_node_utf8(&string(content)),
            )
            .unwrap();
            JsonlSessionStorage::open_with_runtime(fs, path, clock(), id_generator(ids)).await
        } else {
            JsonlSessionStorage::create_with_runtime(
                fs,
                path,
                JsonlSessionStorageCreateOptions {
                    cwd: root,
                    session_id: "fixture".into(),
                    parent_session_path: optional_string(field(request, "parentSessionPath")),
                },
                clock(),
                id_generator(ids),
            )
            .await
        };
        match storage {
            Ok(storage) => script(Arc::new(storage), request).await,
            Err(error) => Err(error),
        }
    } else {
        let metadata: SessionMetadata = from_value(&request["metadata"]).unwrap();
        match InMemorySessionStorage::with_runtime(metadata, entries(request), clock(), id_generator(ids)) {
            Ok(storage) => script(Arc::new(storage), request).await,
            Err(error) => Err(error.into()),
        }
    };
    match result {
        Ok(value) => pi_js_compat::js_json!({"value":value}),
        Err(error) => pi_js_compat::js_json!({"error":error_json(error)}),
    }
}
fn memory_request(kind: &str, entries: JsValue, operations: JsValue) -> JsValue {
    pi_js_compat::js_json!({"kind":kind,"metadata":{"id":"fixture","createdAt":ISO},"entries":entries,"operations":operations})
}

#[tokio::test]
async fn frozen_storage_facade_and_fork_contracts_match() {
    let samples = ids();
    let base = parse(
        r#"[{"type":"message","id":"root","parentId":null,"timestamp":"2023-11-14T22:13:20.000Z","message":{"role":"user","content":"hello","timestamp":1}},{"type":"custom","id":"a","parentId":"root","timestamp":"t","customType":"x","data":{"raw":"\ud800"}}]"#,
    );
    let operations = parse(
        r#"[{"op":"metadata"},{"op":"entries"},{"op":"leaf"},{"op":"entry","id":"missing"},{"op":"path","id":"a"},{"op":"path","id":"missing"},{"op":"find","type":"custom"},{"op":"label","id":"root"},{"op":"append","entry":{"type":"label","id":"label","parentId":"a","timestamp":"t","targetId":"root","label":" \ud800 \uFEFF"}},{"op":"label","id":"root"},{"op":"setLeaf","id":"root"},{"op":"leaf"},{"op":"entries"},{"op":"setLeaf","id":null},{"op":"leaf"},{"op":"entries"},{"op":"setLeaf","id":"missing"},{"op":"createId"}]"#,
    );
    let facade = parse(
        r#"[{"op":"getName"},{"op":"thinking","args":["high"]},{"op":"model","args":["provider","model"]},{"op":"tools","args":[["read","write"]]},{"op":"appendMessage","args":[{"role":"user","content":"\ud800","timestamp":1}]},{"op":"custom","args":["marker",{"x":"\udfff"}]},{"op":"customMessage","args":["note","\ud800",true,null]},{"op":"appendLabel","args":["root","  label  "]},{"op":"name","args":[" a\r\n\r\nb "]},{"op":"getName"},{"op":"compaction","args":["summary","root",123,null,false,1.5]},{"op":"context"},{"op":"move","args":["root",{"summary":"branch","details":null,"fromHook":true}]},{"op":"branch"},{"op":"context"},{"op":"entries"},{"op":"appendLabel","args":["missing","x"]}]"#,
    );
    let mut cases = vec![
        memory_request("storage", base.clone(), operations),
        memory_request("facade", base.clone(), facade),
    ];
    for options in [
        parse("{}"),
        parse(r#"{"entryId":"root"}"#),
        parse(r#"{"entryId":"a","position":"at"}"#),
        parse(r#"{"entryId":"a"}"#),
        parse(r#"{"entryId":"missing"}"#),
        parse(r#"{"entryId":""}"#),
    ] {
        let mut request = memory_request("fork", base.clone(), JsValue::Array(Vec::new()));
        request["options"] = options;
        cases.push(request);
    }
    cases.push(memory_request(
        "storage",
        parse(r#"[{"type":"leaf","id":"leaf","parentId":null,"timestamp":"t","targetId":"missing"}]"#),
        parse("[]"),
    ));
    cases.push(memory_request(
        "storage",
        base,
        parse(r#"[{"op":"append","entry":{"type":"label","id":"l1","parentId":"a","timestamp":"t","targetId":"root","label":" kept "}},{"op":"label","id":"root"},{"op":"append","entry":{"type":"leaf","id":"bad-leaf","parentId":"l1","timestamp":"t","targetId":"missing"}},{"op":"leaf"},{"op":"append","entry":{"type":"custom","id":"recovered","parentId":"a","timestamp":"t","customType":"marker"}},{"op":"leaf"},{"op":"path","id":"recovered"},{"op":"append","entry":{"type":"label","id":"l2","parentId":"recovered","timestamp":"t","targetId":"root","label":"\uFEFF \r\n"}},{"op":"label","id":"root"},{"op":"append","entry":{"type":"custom","id":"root","parentId":null,"timestamp":"t","customType":"replacement"}},{"op":"entries"},{"op":"path","id":"a"},{"op":"entry","id":"root"},{"op":"find","type":"message"}]"#),
    ));
    for case in cases {
        let actual = rust(&case, &samples).await;
        let expected = request(&mut ts_oracle(), &case);
        assert_eq!(actual, expected, "case {}", js_json::stringify_lossless(&case));
    }
}

#[tokio::test]
async fn multiple_compaction_contexts_and_duplicate_boundary_ids_match_frozen() {
    let base = parse(
        r#"[{"type":"message","id":"root","parentId":null,"timestamp":"2023-11-14T22:13:20.000Z","message":{"role":"user","content":"root","timestamp":1}},{"type":"thinking_level_change","id":"think","parentId":"root","timestamp":"t","thinkingLevel":"high"},{"type":"model_change","id":"model","parentId":"think","timestamp":"t","provider":"fixture","modelId":"model"},{"type":"active_tools_change","id":"tools","parentId":"model","timestamp":"t","activeToolNames":["read","write"]}]"#,
    );
    let first = pi_js_compat::js_json!({"type":"compaction","id":"first","parentId":"tools","timestamp":ISO,"summary":"old summary","firstKeptEntryId":"root","tokensBefore":10});
    let middle = pi_js_compat::js_json!({"type":"custom_message","id":"middle","parentId":"first","timestamp":ISO,"customType":"note","content":JsString::from_utf16(vec![0xd800]),"display":false,"details":null});
    let tail = pi_js_compat::js_json!({"type":"branch_summary","id":"tail","parentId":"second","timestamp":ISO,"fromId":"other","summary":"branch"});
    let mut cases = vec![base.as_array().unwrap().clone()];
    for (first_kept, duplicate_id) in [
        ("root", false),
        ("middle", false),
        ("missing", false),
        ("first", false),
        ("tail", false),
        ("root", true),
        ("middle", true),
        ("missing", true),
    ] {
        let mut entries = base.as_array().unwrap().clone();
        entries.extend([first.clone(),middle.clone(),pi_js_compat::js_json!({"type":"compaction","id":if duplicate_id {"first"} else {"second"},"parentId":"middle","timestamp":ISO,"summary":"latest summary","firstKeptEntryId":first_kept,"tokensBefore":20,"durationMs":1.5}),tail.clone()]);
        cases.push(entries);
    }
    let mut oracle = ts_oracle();
    for entries in cases {
        let input = pi_js_compat::js_json!({"kind":"context","entries":entries});
        assert_eq!(rust(&input, &[]).await, request(&mut oracle, &input));
    }
}

#[tokio::test]
async fn frozen_jsonl_validation_and_raw_round_trips_match() {
    let samples = ids();
    let header = r#"{"type":"session","version":3,"id":"fixture","timestamp":"bad date","cwd":"/fixture"}"#;
    let entry = r#"{"type":"future","id":"\ud800","parentId":null,"timestamp":"t","extra":{"\udfff":"\ud800"}}"#;
    let files = [
        String::new(),
        "\n \n".into(),
        "{}".into(),
        "[]".into(),
        "null".into(),
        "{bad".into(),
        r#"{"type":"session","version":4}"#.into(),
        r#"{"type":"session","version":3,"id":"","timestamp":"x","cwd":"/"}"#.into(),
        r#"{"type":"session","version":3,"id":"id","timestamp":"x","cwd":"/","parentSession":null}"#.into(),
        format!("\n{header}\n\n{entry}\n"),
        format!("{header}\n[]\n"),
        format!("{header}\n{{}}\n"),
        format!("{header}\n{{bad\n"),
        format!("{header}\n{{\"type\":\"message\",\"id\":\"x\",\"timestamp\":\"t\"}}\n"),
        format!(
            "{header}\n{{\"type\":\"leaf\",\"id\":\"leaf\",\"parentId\":null,\"timestamp\":\"t\",\"targetId\":\"missing\"}}\n"
        ),
    ];
    for file in files {
        let root = tempfile::tempdir().unwrap();
        let case = pi_js_compat::js_json!({"kind":"storage","mode":"jsonl","root":root.path().to_str().unwrap(),"fileContent":file,"operations":[{"op":"metadata"},{"op":"entries"},{"op":"leaf"},{"op":"path","id":null}]});
        let actual = rust(&case, &samples).await;
        let expected = request(&mut ts_oracle(), &case);
        assert_eq!(
            actual,
            expected,
            "file {}",
            js_json::stringify_lossless(&case["fileContent"])
        );
    }
}
