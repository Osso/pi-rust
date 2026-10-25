use crate::shared_doom_engine as engine;
use crate::shared_oracle as oracle;
use pi_js_compat::{js_format, js_json::JsString};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Fault {
    calls: usize,
    fail_at: Option<usize>,
}
struct Module {
    generation: usize,
    next: u32,
    trace: Arc<Mutex<Vec<Value>>>,
    fault: Arc<Mutex<Fault>>,
}
impl Module {
    fn record(&self, event: Value) -> Result<(), String> {
        let mut row = vec![json!(self.generation)];
        row.extend(event.as_array().unwrap().clone());
        self.trace.lock().unwrap().push(json!(row));
        let mut fault = self.fault.lock().unwrap();
        fault.calls += 1;
        if Some(fault.calls) == fault.fail_at {
            Err("fixture ABI failure".into())
        } else {
            Ok(())
        }
    }
    fn pointer(&self) -> u32 {
        1024 + self.generation as u32 * 32
    }
}
impl engine::DoomModule for Module {
    fn create_path(&mut self, parent: &str, path: &str) -> Result<(), String> {
        self.record(json!(["path", parent, path, true, true]))
    }
    fn create_data_file(&mut self, parent: &str, name: &str, data: &[u8]) -> Result<(), String> {
        self.record(json!(["file", parent, name, data, true, false]))
    }
    fn malloc(&mut self, size: u32) -> Result<u32, String> {
        let ptr = self.next;
        self.record(json!(["malloc", size, ptr]))?;
        self.next += size;
        Ok(ptr)
    }
    fn set_value(&mut self, ptr: u32, value: u32, kind: &str) -> Result<(), String> {
        self.record(json!(["set", ptr, value, kind]))
    }
    fn create(&mut self, argc: u32, argv: u32) -> Result<(), String> {
        self.record(json!(["create", argc, argv]))
    }
    fn free(&mut self, ptr: u32) -> Result<(), String> {
        self.record(json!(["free", ptr]))
    }
    fn frame_buffer(&self) -> Result<u32, String> {
        self.record(json!(["frameBuffer"]))?;
        Ok(self.pointer())
    }
    fn width(&self) -> Result<usize, String> {
        self.record(json!(["width"]))?;
        Ok(if self.generation % 2 == 1 { 2 } else { 1 })
    }
    fn height(&self) -> Result<usize, String> {
        self.record(json!(["height"]))?;
        Ok(if self.generation % 2 == 1 { 1 } else { 2 })
    }
    fn get_i32(&self, ptr: u32) -> Result<i32, String> {
        self.record(json!(["get", ptr, "i32"]))?;
        Ok(if ptr == self.pointer() {
            0x12345678
        } else {
            0xffabcdefu32 as i32
        })
    }
    fn tick(&mut self) -> Result<(), String> {
        self.record(json!(["tick"]))
    }
    fn push_key(&mut self, pressed: bool, key: u32) -> Result<(), String> {
        self.record(json!(["key", u32::from(pressed), key]))
    }
}
fn compare(ts: &mut oracle::TsOracle, operations: Value) -> Value {
    let temp = tempfile::tempdir().unwrap();
    let wad = temp.path().join("fixture.wad");
    let bytes = b"IWAD-ABI-fixture";
    std::fs::write(&wad, bytes).unwrap();
    let expected = ts.request(&json!({"kind":"engine-script","wad":wad,"operations":operations}));
    let trace = Arc::new(Mutex::new(Vec::new()));
    let mut faults: Vec<Arc<Mutex<Fault>>> = Vec::new();
    let mut engine = engine::DoomEngine::new(wad.clone());
    let mut results = Vec::new();
    for op in operations.as_array().unwrap() {
        let result: Result<Value, JsString> = match op["op"].as_str().unwrap() {
            "init" => {
                let fault = Arc::new(Mutex::new(Fault {
                    calls: 0,
                    fail_at: op["failAt"].as_u64().map(|v| v as usize),
                }));
                faults.push(fault.clone());
                engine
                    .init_with_module(Box::new(Module {
                        generation: faults.len(),
                        next: 64,
                        trace: trace.clone(),
                        fault,
                    }))
                    .map(|_| Value::Null)
                    .map_err(|e| e.message())
            }
            "remove" => {
                std::fs::remove_file(&wad).unwrap();
                Ok(Value::Null)
            }
            "restore" => {
                std::fs::write(&wad, bytes).unwrap();
                Ok(Value::Null)
            }
            "fault" => {
                let mut fault = faults[op["generation"].as_u64().unwrap() as usize - 1].lock().unwrap();
                fault.fail_at = Some(fault.calls + op["offset"].as_u64().unwrap_or(1) as usize);
                Ok(Value::Null)
            }
            "state" => Ok(json!([engine.is_initialized(), engine.width(), engine.height()])),
            "tick" => engine.tick().map(|_| Value::Null).map_err(JsString::from),
            "key" => engine
                .push_key(op["pressed"].as_bool().unwrap(), op["key"].as_u64().unwrap() as u32)
                .map(|_| Value::Null)
                .map_err(JsString::from),
            "frame" => engine
                .get_frame_rgba()
                .map(|bytes| json!({"length":bytes.len(),"head":bytes.iter().take(12).collect::<Vec<_>>()}))
                .map_err(JsString::from),
            name => panic!("unknown operation {name}"),
        };
        results.push(json!([
            op["op"],
            result.unwrap_or_else(|error| json!({"error":js_format!("Error: {}", error)}))
        ]));
    }
    let actual = json!({"trace":trace.lock().unwrap().clone(),"results":results});
    assert_eq!(actual, expected, "operations {operations}");
    actual
}
fn ts() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "example-doom-helpers-oracle.ts",
        "packages/coding-agent/examples/extensions/doom-overlay/doom-engine.ts",
    )
}
#[test]
fn every_init_abi_failure_preserves_exact_partial_state_and_reinitialization() {
    let mut ts = ts();
    let baseline = compare(&mut ts, json!([{"op":"init"}]));
    let calls = baseline["trace"].as_array().unwrap().len();
    assert!(calls > 40);
    for failure in 1..=calls {
        let operations = json!([{"op":"init","failAt":failure},{"op":"state"},{"op":"tick"},{"op":"key","pressed":true,"key":173},{"op":"frame"}]);
        compare(&mut ts, operations.clone());
        let mut retry = vec![json!({"op":"init"})];
        retry.extend(operations.as_array().unwrap().clone());
        retry.extend([json!({"op":"init"}), json!({"op":"state"}), json!({"op":"frame"})]);
        compare(&mut ts, json!(retry));
    }
}
#[test]
fn runtime_abi_failures_and_missing_wad_reinit_match_pinned() {
    let mut ts = ts();
    for op in [
        json!({"op":"tick"}),
        json!({"op":"key","pressed":true,"key":173}),
        json!({"op":"key","pressed":false,"key":173}),
        json!({"op":"frame"}),
    ] {
        compare(
            &mut ts,
            json!([{"op":"init"},{"op":"fault","generation":1},op,{"op":"state"},{"op":"frame"}]),
        );
    }
    compare(
        &mut ts,
        json!([{"op":"init"},{"op":"fault","generation":1,"offset":2},{"op":"frame"},{"op":"frame"}]),
    );
    compare(
        &mut ts,
        json!([{"op":"init"},{"op":"remove"},{"op":"init"},{"op":"state"},{"op":"tick"},{"op":"key","pressed":true,"key":255},{"op":"frame"},{"op":"restore"},{"op":"init"},{"op":"state"},{"op":"frame"}]),
    );
}
