#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::image_catalog_generator::{
    generate_image_models_file, parse_openrouter_image_models, read_strict_option, run,
};
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_js_compat::js_json::{parse_lossless, stringify_lossless};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{cell::RefCell, io::Write, rc::Rc};

const SOURCE: &str = "packages/ai/scripts/generate-image-models.ts";

#[test]
fn parser_matches_actual_pinned_export_for_raw_json() {
    let mut ts = oracle::TsOracle::spawn("image-catalog-generator-oracle.ts", SOURCE);
    let fixtures = [
        r#"{"data":[{"id":"overflow","architecture":{"output_modalities":["image"]},"pricing":{"prompt":1e400}}]}"#,
        "null",
        "[]",
        "{}",
        r#"{"data":{}}"#,
        r#"{"data":[]}"#,
        r#"{"data":[{"id":"skip","architecture":{"output_modalities":["text"]}}]}"#,
        r#"{"data":[{"id":"a","name":"A","architecture":{"input_modalities":["audio","image","text","image"],"output_modalities":["video","image","text","image"]},"pricing":{"prompt":"  0.0000002tail","completion":"bad","input_cache_read":"Infinity","input_cache_write":"-0"}},{"id":"a","name":"duplicate","architecture":{"output_modalities":["image"]}}]}"#,
        r#"{"data":[{"id":"x"},{"id":"x","name":"usable","architecture":{"output_modalities":["image"]}}]}"#,
        r#"{"data":[{"id":"\ud800","name":"\udc00","architecture":{"output_modalities":["image"]}},{"id":"�","name":"replacement","architecture":{"output_modalities":["image"]}}]}"#,
        r#"{"data":[null]}"#,
        r#"{"data":[false,1,"x"]}"#,
        r#"{"data":[{"architecture":{"input_modalities":false}}]}"#,
        r#"{"data":[{"architecture":{"output_modalities":"image"}}]}"#,
        r#"{"data":[{"architecture":{"output_modalities":["image"]}}]}"#,
        r#"{"data":[{"id":null,"name":false,"architecture":{"output_modalities":["image"]},"pricing":{"prompt":[".2tail"],"completion":true,"input_cache_read":{},"input_cache_write":0}}]}"#,
        r#"{"data":[{"id":{},"architecture":{"output_modalities":["image"]},"pricing":{"prompt":{"toString":null}}}]}"#,
        r#"{"data":[{"id":"a","architecture":{"output_modalities":["image"]},"pricing":{"prompt":1,"completion":[null,"2"],"input_cache_read":false,"input_cache_write":null}}]}"#,
    ];
    for payload in fixtures {
        for strict in [false, true] {
            let expected = ts.request(&json!({"operation":"parse", "payload":payload, "strict":strict}));
            let actual = match parse_openrouter_image_models(&parse_lossless(payload).unwrap(), strict) {
                Ok(models) => {
                    let cost_numbers: Vec<_> = models
                        .iter()
                        .map(|model| {
                            let pi_js_compat::js_json::JsValue::Object(model) = model else {
                                panic!("model")
                            };
                            let pi_js_compat::js_json::JsValue::Object(cost) =
                                &model[&pi_js_compat::js_json::JsString::from("cost")]
                            else {
                                panic!("cost")
                            };
                            let entries: serde_json::Map<String, serde_json::Value> = cost
                                .iter()
                                .map(|(key, value)| {
                                    let pi_js_compat::js_json::JsValue::Number(number) = value else {
                                        panic!("cost remains numeric")
                                    };
                                    let text = if *number == 0.0 && number.is_sign_negative() {
                                        "-0".into()
                                    } else {
                                        pi_js_compat::number::js_number_to_string(*number)
                                    };
                                    (key.to_utf8().unwrap(), json!(text))
                                })
                                .collect();
                            serde_json::Value::Object(entries)
                        })
                        .collect();
                    json!({"ok":true,"models":stringify_lossless(&pi_js_compat::js_json::JsValue::Array(models)),"costNumbers":cost_numbers})
                }
                Err(error) => json!({"ok":false,"name":error.name,"message":error.message}),
            };
            assert_eq!(actual, expected, "payload={payload}, strict={strict}");
        }
    }
}

struct FixtureFetch {
    payload: String,
    status: u16,
    error: Option<String>,
    body_error: Option<String>,
}
struct Body {
    bytes: Option<Vec<u8>>,
    error: Option<String>,
}
impl HttpBody for Body {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async {
            if let Some(error) = self.error.take() {
                Some(Err(CallbackError::new(error)))
            } else {
                self.bytes.take().map(Ok)
            }
        })
    }
}
impl Fetch for FixtureFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _: Option<tokio_util::sync::CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.url,
            "https://openrouter.ai/api/v1/models?output_modalities=image"
        );
        assert!(request.body.is_none());
        Box::pin(async {
            if let Some(error) = &self.error {
                return Err(CallbackError::new(error));
            }
            Ok(HttpResponse {
                status: self.status,
                status_text: String::new(),
                headers: vec![],
                body: Box::new(Body {
                    bytes: Some(self.payload.as_bytes().to_vec()),
                    error: self.body_error.clone(),
                }),
            })
        })
    }
}

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "pi-image-native-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

struct ConsoleCapture {
    stream: &'static str,
    bytes: Vec<u8>,
    pending: Vec<u8>,
    events: Rc<RefCell<Vec<serde_json::Value>>>,
}
impl ConsoleCapture {
    fn new(stream: &'static str, events: Rc<RefCell<Vec<serde_json::Value>>>) -> Self {
        Self {
            stream,
            bytes: vec![],
            pending: vec![],
            events,
        }
    }
}
impl Write for ConsoleCapture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        for byte in bytes {
            self.pending.push(*byte);
            if *byte == b'\n' {
                self.events.borrow_mut().push(
                    json!({"stream":self.stream,"text":String::from_utf8(std::mem::take(&mut self.pending)).unwrap()}),
                );
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn native_workflow_matches_pinned_script_values_order_logs_and_failures() {
    let fixtures = [
        json!({"payload":"{}"}),
        json!({"payload":"{\"data\":[]}"}),
        json!({"payload":r#"{"data":[{"id":"no-image"}]}"#}),
        json!({"payload":r#"{"data":[{"id":"usable"},{"id":"usable","name":"Kept","architecture":{"input_modalities":["audio","image","image","text"],"output_modalities":["image","text","image"]},"pricing":{"prompt":" .0000002trailing","completion":"-0","input_cache_read":"NaN","input_cache_write":"Infinity"}},{"id":"usable","name":"Dropped","architecture":{"output_modalities":["image"]}}]}"#,"args":["--strict","--strict"]}),
        json!({"payload":"{\"data\":[]}","args":["--strict"]}),
        json!({"payload":"{\"data\":[]}","args":["--strict"],"preexisting":true}),
        json!({"payload":"{}","args":["--strict","--strict"]}),
        json!({"payload":r#"{"data":[{"id":"a"}]}"#,"args":["--strict"]}),
        json!({"payload":"not json"}),
        json!({"payload":"{}","bodyError":"body offline"}),
        json!({"payload":"{}","bodyError":"body offline","args":["--strict"]}),
        json!({"payload":"\u{feff}{\"data\":[]}"}),
        json!({"payload":"not json","args":["--strict"]}),
        json!({"payload":"{}","status":503}),
        json!({"payload":"{}","status":503,"args":["--strict"]}),
        json!({"payload":"{}","networkError":"offline"}),
        json!({"payload":"{}","networkError":"offline","args":["--strict"]}),
        json!({"payload":"{}","args":["--unknown"]}),
        json!({"payload":"{}","args":["--strict","--unknown","--other"]}),
        json!({"payload":"{}","writeFailure":true}),
        json!({"payload":"{}","directoryOutput":true}),
        json!({"payload":r#"{"data":[null]}"#}),
        json!({"payload":r#"{"data":[{"id":"z","name":"Z","architecture":{"output_modalities":["image"]}},{"id":"10","architecture":{"output_modalities":["image"]}},{"id":"2","architecture":{"output_modalities":["image"]}},{"id":"a","architecture":{"output_modalities":["image"]}},{"id":"A","architecture":{"output_modalities":["image"]}},{"id":"\ud800","name":"\udc00","architecture":{"output_modalities":["image"]}},{"id":"�","architecture":{"output_modalities":["image"]}}]}"#}),
    ];
    compare_workflows("workflow", &fixtures).await;
}

async fn compare_workflows(group: &str, fixtures: &[serde_json::Value]) {
    let mut ts = oracle::TsOracle::spawn("image-catalog-generator-oracle.ts", SOURCE);
    for (case, fixture) in fixtures.iter().enumerate() {
        let mut fixture = fixture.clone();
        fixture["operation"] = json!("workflow");
        let expected = ts.request(&fixture);
        assert_eq!(expected["ok"], true, "{expected}");
        let proof = PathBuf::from("/tmp/pi-rust-image-generator-proof");
        std::fs::create_dir_all(&proof).unwrap();
        std::fs::write(
            proof.join(format!("{group}-{case}-source.json")),
            serde_json::to_string_pretty(&expected).unwrap(),
        )
        .unwrap();
        let root = temp_dir();
        let output = root.join(if fixture["writeFailure"] == true {
            "missing/image-models.json"
        } else {
            "image-models.json"
        });
        if fixture["directoryOutput"] == true {
            std::fs::create_dir(&output).unwrap();
        }
        if fixture["preexisting"] == true {
            std::fs::write(&output, r#"{"prior":true}"#).unwrap();
        }
        let fetch = FixtureFetch {
            payload: fixture["payload"].as_str().unwrap().into(),
            status: fixture["status"].as_u64().unwrap_or(200) as u16,
            error: fixture["networkError"].as_str().map(str::to_owned),
            body_error: fixture["bodyError"].as_str().map(str::to_owned),
        };
        let args: Vec<String> = fixture["args"]
            .as_array()
            .map(|args| args.iter().map(|arg| arg.as_str().unwrap().to_owned()).collect())
            .unwrap_or_default();
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut stdout = ConsoleCapture::new("stdout", Rc::clone(&events));
        let mut stderr = ConsoleCapture::new("stderr", Rc::clone(&events));
        let result = run(&args, &fetch, &output, &mut stdout, &mut stderr).await;
        if let Err(error) = &result {
            use std::io::Write;
            writeln!(stderr, "{error}").unwrap();
        }
        assert_eq!(i32::from(result.is_err()), expected["status"], "{fixture}\n{expected}");
        let stdout = String::from_utf8(stdout.bytes).unwrap();
        assert_eq!(
            stdout.replace(output.to_str().unwrap(), "<artifact>"),
            expected["stdout"]
                .as_str()
                .unwrap()
                .replace(expected["path"].as_str().unwrap(), "<artifact>")
        );
        // Approved 2026-10-20: omit JS-specific frames only. Raw source streams stay in proof files.
        let stderr = String::from_utf8(stderr.bytes).unwrap();
        let source_stderr = expected["stderr"].as_str().unwrap();
        std::fs::write(proof.join(format!("{group}-{case}-native-stderr.txt")), &stderr).unwrap();
        assert_eq!(
            stderr.replace(output.to_str().unwrap(), "<artifact>"),
            without_js_frames(source_stderr).replace(expected["path"].as_str().unwrap(), "<artifact>"),
            "{fixture}"
        );
        let source_events: Vec<_> = expected["events"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|event| {
                let stream = event["stream"].as_str().unwrap();
                without_js_frames(event["text"].as_str().unwrap())
                    .replace(expected["path"].as_str().unwrap(), "<artifact>")
                    .split_inclusive('\n')
                    .map(|text| json!({"stream":stream,"text":text}))
                    .collect::<Vec<_>>()
            })
            .collect();
        let native_events: Vec<_> = events.borrow().iter().map(|event|json!({"stream":event["stream"],"text":event["text"].as_str().unwrap().replace(output.to_str().unwrap(),"<artifact>")})).collect();
        assert_eq!(native_events, source_events, "console event ordering: {fixture}");
        if result.is_ok() {
            let native = std::fs::read_to_string(&output).unwrap();
            assert_eq!(
                stringify_lossless(&parse_lossless(&native).unwrap()),
                expected["catalog"].as_str().unwrap()
            );
        } else if fixture["preexisting"] == true {
            assert_eq!(std::fs::read_to_string(&output).unwrap(), r#"{"prior":true}"#);
        } else if fixture["directoryOutput"] == true {
            assert!(output.is_dir());
        } else {
            assert!(!output.exists());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn raw_json_ids_and_names_keep_source_generation_errors_and_values() {
    let records = [
        r#"[{"architecture":{"output_modalities":["image"]}}]"#,
        r#"[{"id":null,"architecture":{"output_modalities":["image"]}}]"#,
        r#"[{"id":1,"name":{"a":true},"architecture":{"output_modalities":["image"]}}]"#,
        r#"[{"id":{},"architecture":{"output_modalities":["image"]}}]"#,
        r#"[{"id":{"toString":null},"architecture":{"output_modalities":["image"]}}]"#,
        r#"[{"id":"a","architecture":{"output_modalities":["image"]}},{"id":null,"architecture":{"output_modalities":["image"]}}]"#,
        r#"[{"id":null,"architecture":{"output_modalities":["image"]}},{"id":"a","architecture":{"output_modalities":["image"]}}]"#,
        r#"[{"id":"a","architecture":{"output_modalities":["image"]}},{"architecture":{"output_modalities":["image"]}}]"#,
        r#"[{"id":{},"architecture":{"output_modalities":["image"]}},{"id":"a","architecture":{"output_modalities":["image"]}}]"#,
        r#"[{"id":"a","architecture":{"output_modalities":["image"]}},{"id":{},"architecture":{"output_modalities":["image"]}}]"#,
        r#"[{"id":"p","name":"P","architecture":{"input_modalities":["image","image"],"output_modalities":["image"]},"pricing":{"prompt":"junk","completion":"Infinity","input_cache_read":"-0","input_cache_write":"1e400"}}]"#,
    ];
    let fixtures: Vec<_> = records
        .iter()
        .map(|data| json!({"payload":format!("{{\"data\":{data}}}")}))
        .collect();
    compare_workflows("raw", &fixtures).await;
}

#[tokio::test]
async fn malformed_first_id_keeps_source_sort_boundary_for_longer_inputs() {
    let fixtures: Vec<_> = [3, 20, 21, 32, 63, 64, 65, 128].into_iter().map(|length| {
        let mut data = vec![json!({"id":null,"architecture":{"output_modalities":["image"]}})];
        for index in 1..length {
            data.push(json!({"id":format!("m{:03}",(index * 17) % length),"architecture":{"output_modalities":["image"]}}));
        }
        json!({"payload":serde_json::to_string(&json!({"data":data})).unwrap()})
    }).collect();
    compare_workflows("sort-boundary", &fixtures).await;
}

fn without_js_frames(source: &str) -> String {
    let mut result = String::new();
    for line in source.lines() {
        if line.starts_with("    at ") {
            if line.ends_with(" {") {
                result.pop();
                result.push_str(" {\n");
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

#[test]
fn strict_options_and_native_cli_reject_unknown_arguments_before_fetch() {
    assert_eq!(read_strict_option(&[]).unwrap(), false);
    assert!(read_strict_option(&["--strict".into(), "--strict".into()]).unwrap());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_generate-image-models"))
        .arg("--bad")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "Error: Unknown argument: --bad\n"
    );
}

#[test]
fn artifact_serialization_is_lossless_and_uses_javascript_key_enumeration() {
    let payload = parse_lossless(r#"{"data":[{"id":"10","architecture":{"output_modalities":["image"]}},{"id":"2","architecture":{"output_modalities":["image"]}},{"id":"\ud800","name":"\udc00","architecture":{"output_modalities":["image"]}}]}"#).unwrap();
    let models = parse_openrouter_image_models(&payload, true).unwrap();
    let artifact = generate_image_models_file(&models).unwrap();
    assert!(artifact.contains("\\ud800"));
    assert!(artifact.contains("\\udc00"));
    assert!(artifact.find("\"2\":").unwrap() < artifact.find("\"10\":").unwrap());
}
