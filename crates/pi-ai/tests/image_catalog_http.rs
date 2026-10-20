//! Controlled loopback HTTP proof: use the real runtime client without contacting OpenRouter.
#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

struct LoopbackFetch {
    endpoint: String,
    fetch: std::sync::Arc<dyn Fetch>,
}
impl Fetch for LoopbackFetch {
    fn fetch(
        &self,
        mut request: HttpRequest,
        signal: Option<tokio_util::sync::CancellationToken>,
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
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let endpoint = format!("http://{address}/models?output_modalities=image");
    let mut source = oracle::TsOracle::spawn(
        "image-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-image-models.ts",
    );
    let expected = source.request(
        &serde_json::json!({"operation":"workflow","payload":"{}","args":["--strict"],"closedLoopback":endpoint}),
    );
    assert_eq!(expected["status"], 1, "{expected}");
    let fetch = LoopbackFetch {
        endpoint,
        fetch: pi_ai::http::fetch_with_options(pi_ai::http::FetchClientOptions {
            http1_only: true,
            idle_timeout: Some(Duration::from_secs(3)),
        })
        .unwrap(),
    };
    let root = std::env::temp_dir().join(format!("pi-image-network-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let result = pi_ai::image_catalog_generator::run(
        &["--strict".into()],
        &fetch,
        &root.join("image-models.json"),
        &mut stdout,
        &mut stderr,
    )
    .await;
    let error = result.unwrap_err();
    writeln!(stderr, "{error}").unwrap();
    let native_stderr = String::from_utf8(stderr).unwrap();
    let proof = std::path::Path::new("/tmp/pi-rust-image-generator-proof");
    std::fs::create_dir_all(proof).unwrap();
    std::fs::write(
        proof.join("closed-loopback-source.json"),
        serde_json::to_string_pretty(&expected).unwrap(),
    )
    .unwrap();
    std::fs::write(proof.join("closed-loopback-native-stderr.txt"), &native_stderr).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        native_stderr.lines().next(),
        expected["stderr"].as_str().unwrap().lines().next()
    );
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
