//! Whole frozen image-generator workflow: exact source bytes with local fetch fixtures.
use crate::shared_oracle as oracle;
use pi_ai::image_catalog_generator::run;
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_js_compat::js_json::{parse_lossless, stringify_lossless};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{cell::RefCell, io::Write, rc::Rc};
const SOURCE: &str = "packages/ai/scripts/generate-image-models.ts";
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
        _: Option<pi_js_compat::abort::AbortSignal>,
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
        json!({"payload":r#"{"data":[{"id":"usable"},{"id":"usable","name":"First","architecture":{"input_modalities":["audio","image","image","text"],"output_modalities":["image","text","image"]},"pricing":{"prompt":"0.0000002","completion":"0.000003","input_cache_read":"0.0000001","input_cache_write":"0.000002"}},{"id":"usable","name":"Last","architecture":{"output_modalities":["image"]}}]}"#}),
        json!({"payload":"{\"data\":[]}","args":["--strict"],"preexisting":true}),
        json!({"payload":"{}","args":["--unknown"]}),
        json!({"payload":"not json"}),
        json!({"payload":"{}","bodyError":"body offline"}),
        json!({"payload":"\u{feff}{\"data\":[]}"}),
        json!({"payload":"{}","status":503}),
        json!({"payload":"{}","networkError":"offline"}),
        json!({"payload":"{}","networkError":"offline","args":["--strict"]}),
        json!({"payload":"{}","writeFailure":true}),
        json!({"payload":"{}","directoryOutput":true}),
        json!({"payload":r#"{"data":[{"id":"z","name":"Z","architecture":{"output_modalities":["image"]}},{"id":"10","name":"Ten","architecture":{"output_modalities":["image"]}},{"id":"2","name":"Two","architecture":{"output_modalities":["image"]}},{"id":"a","name":"a","architecture":{"output_modalities":["image"]}},{"id":"A","name":"A","architecture":{"output_modalities":["image"]}},{"id":"é","name":"\ud800","architecture":{"output_modalities":["image"]}}]}"#}),
    ];
    compare_workflows("workflow", &fixtures).await;
}

#[tokio::test]
async fn generated_artifact_matches_source_prototype_named_model() {
    compare_workflows("prototype", &[
        json!({"payload":r#"{"data":[{"id":"__proto__","name":"prototype","architecture":{"output_modalities":["image"]}}]}"#}),
        json!({"payload":r#"{"data":[{"id":"nested","name":{"__proto__":{"lost":true},"keep":[{"__proto__":1,"visible":2}]},"architecture":{"output_modalities":["image"]}}]}"#}),
    ]).await;
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
