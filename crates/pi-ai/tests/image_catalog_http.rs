//! Controlled loopback HTTP proof: use the real runtime client without contacting OpenRouter.
use crate::shared_oracle as oracle;
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

struct RecordedStream {
    stream: &'static str,
    bytes: Vec<u8>,
    pending: Vec<u8>,
    events: std::rc::Rc<std::cell::RefCell<Vec<(&'static str, String)>>>,
}
impl Write for RecordedStream {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        for byte in bytes {
            self.pending.push(*byte);
            if *byte == b'\n' {
                self.events.borrow_mut().push((
                    self.stream,
                    String::from_utf8(std::mem::take(&mut self.pending)).unwrap(),
                ));
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct LoopbackFetch {
    endpoint: String,
    fetch: std::sync::Arc<dyn Fetch>,
}
impl Fetch for LoopbackFetch {
    fn fetch(
        &self,
        mut request: HttpRequest,
        signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        assert_eq!(
            request.url,
            "https://openrouter.ai/api/v1/models?output_modalities=image"
        );
        request.url = self.endpoint.clone();
        self.fetch.fetch(request, signal)
    }
}

#[tokio::test]
async fn native_closed_loopback_failure_matches_pinned_generator_error_header() {
    assert_transport_failure(false).await;
}

#[tokio::test]
async fn native_truncated_body_matches_pinned_generator_error_header() {
    assert_transport_failure(true).await;
}

async fn assert_transport_failure(truncated: bool) {
    let transport = if truncated { "truncated-body" } else { "closed-loopback" };
    let message = if truncated { "terminated" } else { "fetch failed" };
    for strict in [false, true] {
        let args: Vec<String> = if strict { vec!["--strict".into()] } else { vec![] };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = if truncated {
            Some(serve_truncated_responses(listener))
        } else {
            drop(listener);
            None
        };
        let endpoint = format!("http://{address}/models?output_modalities=image");
        let mut source = oracle::TsOracle::spawn(
            "image-catalog-generator-oracle.ts",
            "packages/ai/scripts/generate-image-models.ts",
        );
        let expected = source
            .request(&serde_json::json!({"operation":"workflow","payload":"{}","args":args,"closedLoopback":endpoint}));
        assert_eq!(expected["status"], 0, "{expected}");
        let fetch = LoopbackFetch {
            endpoint,
            fetch: pi_ai::http::fetch_with_options(pi_ai::http::FetchClientOptions {
                http1_only: true,
                idle_timeout: Some(Duration::from_secs(3)),
            })
            .unwrap(),
        };
        let root = std::env::temp_dir().join(format!("pi-image-{transport}-{strict}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let mut stdout = RecordedStream {
            stream: "stdout",
            bytes: vec![],
            pending: vec![],
            events: std::rc::Rc::clone(&events),
        };
        let mut stderr = RecordedStream {
            stream: "stderr",
            bytes: vec![],
            pending: vec![],
            events: std::rc::Rc::clone(&events),
        };
        let output = root.join("image-models.json");
        let result = pi_ai::image_catalog_generator::run(&args, &fetch, &output, &mut stdout, &mut stderr).await;
        if let Some(server) = server {
            server.join().unwrap();
        }
        assert!(result.is_ok(), "{result:?}");
        let artifact = std::fs::read_to_string(&output).unwrap();
        assert_eq!(
            pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json::parse_lossless(&artifact).unwrap()),
            expected["catalog"].as_str().unwrap()
        );
        let native_stderr = String::from_utf8(stderr.bytes).unwrap();
        let proof = std::path::Path::new("/tmp/pi-rust-image-generator-proof");
        std::fs::create_dir_all(proof).unwrap();
        std::fs::write(
            proof.join(format!("{transport}-structured-{strict}-source.json")),
            serde_json::to_string_pretty(&expected).unwrap(),
        )
        .unwrap();
        std::fs::write(
            proof.join(format!("{transport}-structured-{strict}-stderr.txt")),
            &native_stderr,
        )
        .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        // Explicit native-cause approval: compare top-level headers exactly, not native cause text
        // against Node's ECONNREFUSED metadata. Both complete diagnostics remain in separate files.
        assert_eq!(
            String::from_utf8(stdout.bytes)
                .unwrap()
                .replace(output.to_str().unwrap(), "<artifact>"),
            expected["stdout"]
                .as_str()
                .unwrap()
                .replace(expected["path"].as_str().unwrap(), "<artifact>")
        );
        let native_headers: Vec<_> = native_stderr
            .lines()
            .filter(|line| !line.starts_with(' ') && *line != "}")
            .collect();
        let source_headers: Vec<_> = expected["stderr"]
            .as_str()
            .unwrap()
            .lines()
            .filter(|line| !line.starts_with(' ') && *line != "}")
            .map(normalize_node_header)
            .collect();
        assert_eq!(native_headers, source_headers);
        assert_eq!(native_stderr.matches("[cause]:").count(), 1);
        if !truncated {
            assert!(native_stderr.contains("Connection refused (os error 111)"));
        }
        assert!(native_stderr.contains(&format!("TypeError: {message}")));
        assert_eq!(expected["events"].as_array().unwrap().len(), 3);
        assert_eq!(expected["events"][0]["stream"], "stdout");
        assert_eq!(expected["events"][1]["stream"], "stderr");
        assert_eq!(expected["events"][2]["stream"], "stdout");
        let native_event_headers: Vec<_> = events
            .borrow()
            .iter()
            .filter(|(_, line)| !line.starts_with(' ') && line.trim() != "}")
            .map(|(stream, line)| (*stream, line.clone()))
            .collect();
        let source_event_headers: Vec<_> = expected["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| {
                (
                    event["stream"].as_str().unwrap(),
                    format!(
                        "{}\n",
                        normalize_node_header(event["text"].as_str().unwrap().lines().next().unwrap())
                    ),
                )
            })
            .collect();
        let native_event_headers: Vec<_> = native_event_headers
            .into_iter()
            .map(|(stream, line)| (stream, line.replace(output.to_str().unwrap(), "<artifact>")))
            .collect();
        let source_event_headers: Vec<_> = source_event_headers
            .into_iter()
            .map(|(stream, line)| (stream, line.replace(expected["path"].as_str().unwrap(), "<artifact>")))
            .collect();
        assert_eq!(native_event_headers, source_event_headers);
    }
}

// Node's omitted-stack inspection wraps a fetch error in brackets; this is not its name/message.
fn normalize_node_header(header: &str) -> String {
    header.replace("[TypeError: fetch failed] {", "TypeError: fetch failed")
}

fn serve_truncated_responses(listener: std::net::TcpListener) -> std::thread::JoinHandle<()> {
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        for _ in 0..2 {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("truncated HTTP accept: {error}"),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            assert!(request.starts_with(b"GET /models?output_modalities=image HTTP/1.1\r\n"));
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{\"data\":").unwrap();
            stream.flush().unwrap();
            // Deliver headers before premature EOF so fetch resolves and body consumption fails.
            std::thread::sleep(Duration::from_millis(100));
        }
    })
}

#[tokio::test]
async fn generator_reads_real_chunked_http_and_writes_lossless_native_file() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("mock HTTP accept: {error}"),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        assert!(request.starts_with(b"GET /models?output_modalities=image HTTP/1.1\r\n"));
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").unwrap();
        let payload = r#"{"data":[{"id":"\ud800","name":"😀\udc00","architecture":{"output_modalities":["image"]},"pricing":{"prompt":".0000002tail"}}]}"#;
        for chunk in payload.as_bytes().chunks(3) {
            write!(stream, "{:x}\r\n", chunk.len()).unwrap();
            stream.write_all(chunk).unwrap();
            stream.write_all(b"\r\n").unwrap();
        }
        stream.write_all(b"0\r\n\r\n").unwrap();
    });
    let fetch = LoopbackFetch {
        endpoint: format!("http://{address}/models?output_modalities=image"),
        fetch: pi_ai::http::fetch_with_options(pi_ai::http::FetchClientOptions {
            http1_only: true,
            idle_timeout: Some(Duration::from_secs(3)),
        })
        .unwrap(),
    };
    let root = std::env::temp_dir().join(format!("pi-image-http-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let output = root.join("image-models.json");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    pi_ai::image_catalog_generator::run(&["--strict".into()], &fetch, &output, &mut stdout, &mut stderr)
        .await
        .unwrap();
    server.join().unwrap();
    assert!(stderr.is_empty());
    let artifact = std::fs::read_to_string(&output).unwrap();
    let pi_js_compat::js_json::JsValue::Object(catalog) = pi_js_compat::js_json::parse_lossless(&artifact).unwrap()
    else {
        panic!("catalog")
    };
    assert_eq!(catalog.len(), 1);
    assert!(artifact.contains("\\ud800"));
    assert!(artifact.contains("😀\\udc00"));
    assert!(artifact.contains("0.19999999999999998"));
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        format!(
            "Fetching image models from OpenRouter API...\nFetched 1 image models from OpenRouter\nGenerated {}\n",
            output.display()
        )
    );
    std::fs::remove_dir_all(root).unwrap();
}
