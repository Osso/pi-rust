use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_server::PiServerOptions;
use pi_server::connection::ByteConnectionAcceptor;
use pi_server::listener::{ListenerResult, PiServerListener};
use pi_server::testing::create_test_server;
use pi_server::types::BoxFuture;

struct TestListener {
    address: Mutex<Option<String>>,
    accept: Mutex<Option<ByteConnectionAcceptor>>,
    start_count: AtomicUsize,
    close_count: AtomicUsize,
    start_error: Option<&'static str>,
}

impl TestListener {
    fn new(address: &str) -> Self {
        Self {
            address: Mutex::new(Some(address.to_string())),
            accept: Mutex::new(None),
            start_count: AtomicUsize::new(0),
            close_count: AtomicUsize::new(0),
            start_error: None,
        }
    }

    fn failing(address: &str, message: &'static str) -> Self {
        Self {
            start_error: Some(message),
            ..Self::new(address)
        }
    }
}

impl PiServerListener for TestListener {
    fn address(&self) -> Option<String> {
        self.address.lock().expect("address mutex poisoned").clone()
    }

    fn start(&self, accept: ByteConnectionAcceptor) -> BoxFuture<'_, ListenerResult> {
        Box::pin(async move {
            self.start_count.fetch_add(1, Ordering::SeqCst);
            *self.accept.lock().expect("accept mutex poisoned") = Some(accept);
            match self.start_error {
                Some(message) => {
                    Err(Box::new(std::io::Error::other(message)) as Box<dyn std::error::Error + Send + Sync>)
                }
                None => Ok(()),
            }
        })
    }

    fn close(&self) -> BoxFuture<'_, ListenerResult> {
        Box::pin(async move {
            self.close_count.fetch_add(1, Ordering::SeqCst);
            *self.address.lock().expect("address mutex poisoned") = None;
            Ok(())
        })
    }
}

#[tokio::test]
async fn starts_and_closes_every_configured_listener() {
    let first = Arc::new(TestListener::new("first"));
    let second = Arc::new(TestListener::new("second"));
    let test_server = create_test_server(PiServerOptions {
        listeners: vec![first.clone(), second.clone()],
        ..PiServerOptions::default()
    })
    .expect("server options must be valid");
    let server = &test_server.server;

    server.start().await.expect("server must start");
    assert_eq!(server.addresses(), vec!["first".to_string(), "second".to_string()]);
    assert!(first.accept.lock().expect("accept mutex poisoned").is_some());
    assert!(second.accept.lock().expect("accept mutex poisoned").is_some());

    server.close().await.expect("server must close");
    assert_eq!(first.close_count.load(Ordering::SeqCst), 1);
    assert_eq!(second.close_count.load(Ordering::SeqCst), 1);
    assert!(server.addresses().is_empty());
}

#[tokio::test]
async fn closes_previously_started_listeners_when_startup_fails() {
    let first = Arc::new(TestListener::new("first"));
    let second = Arc::new(TestListener::failing("second", "listener failed"));
    let test_server = create_test_server(PiServerOptions {
        listeners: vec![first.clone(), second.clone()],
        ..PiServerOptions::default()
    })
    .expect("server options must be valid");
    let server = &test_server.server;

    let error = server.start().await.expect_err("startup must fail");
    assert_eq!(error.to_string(), "listener failed");
    assert_eq!(first.close_count.load(Ordering::SeqCst), 1);
    assert_eq!(second.close_count.load(Ordering::SeqCst), 0);
}
