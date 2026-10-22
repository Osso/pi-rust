use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use pi_coding_agent::core::tools::file_mutation_queue::with_file_mutation_queue;
use pi_js_compat::js_json::JsString;
use tokio::sync::Barrier;

fn path(name: &str) -> JsString {
    std::env::temp_dir()
        .join(name)
        .to_str()
        .expect("utf-8 test path")
        .into()
}

/// Records start/end order and tracks the peak number of concurrent operations.
#[derive(Clone, Default)]
struct Recorder {
    order: Arc<std::sync::Mutex<Vec<String>>>,
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
}

impl Recorder {
    fn push(&self, entry: impl Into<String>) {
        self.order.lock().unwrap().push(entry.into());
    }

    fn enter(&self) -> usize {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(active, Ordering::SeqCst);
        active
    }

    fn leave(&self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }

    fn order(&self) -> Vec<String> {
        self.order.lock().unwrap().clone()
    }

    fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }
}

async fn operation(recorder: &Recorder, name: &str, delay: Duration) {
    recorder.enter();
    recorder.push(format!("{name}:start"));
    tokio::time::sleep(delay).await;
    recorder.push(format!("{name}:end"));
    recorder.leave();
}

#[tokio::test]
async fn serializes_operations_for_the_same_file() {
    let recorder = Recorder::default();
    let file = path("pi-file-mutation-queue-same");

    let first = {
        let recorder = recorder.clone();
        let file = file.clone();
        async move {
            let _ = with_file_mutation_queue(&file, async || {
                operation(&recorder, "first", Duration::from_millis(30)).await;
            })
            .await;
        }
    };
    let second = {
        let recorder = recorder.clone();
        let file = file.clone();
        async move {
            let _ = with_file_mutation_queue(&file, async || {
                operation(&recorder, "second", Duration::ZERO).await;
            })
            .await;
        }
    };
    tokio::join!(first, second);

    assert_eq!(
        recorder.order(),
        ["first:start", "first:end", "second:start", "second:end"]
    );
    assert_eq!(recorder.peak(), 1);
}

#[tokio::test]
async fn allows_different_files_to_proceed_in_parallel() {
    let recorder = Recorder::default();
    let barrier = Arc::new(Barrier::new(2));

    let run = |name: &'static str, file: &'static str| {
        let recorder = recorder.clone();
        let barrier = Arc::clone(&barrier);
        async move {
            let _ = with_file_mutation_queue(&path(file), async || {
                recorder.enter();
                recorder.push(format!("{name}:start"));
                // Both operations only finish if they run concurrently.
                barrier.wait().await;
                recorder.push(format!("{name}:end"));
                recorder.leave();
            })
            .await;
        }
    };
    tokio::join!(
        run("a", "pi-file-mutation-queue-parallel-a"),
        run("b", "pi-file-mutation-queue-parallel-b")
    );

    assert_eq!(recorder.peak(), 2);
    let order = recorder.order();
    assert!(order.iter().position(|entry| entry == "a:start") < order.iter().position(|entry| entry == "a:end"));
    assert!(order.iter().position(|entry| entry == "b:start") < order.iter().position(|entry| entry == "b:end"));
}

#[cfg(unix)]
#[tokio::test]
async fn uses_the_same_queue_for_symlink_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.txt");
    let alias = dir.path().join("alias.txt");
    std::fs::write(&target, "hello\n").unwrap();
    std::os::unix::fs::symlink(&target, &alias).unwrap();

    let recorder = Recorder::default();
    let target_path = JsString::from(target.to_str().expect("utf-8 test path"));
    let alias_path = JsString::from(alias.to_str().expect("utf-8 test path"));
    let first = {
        let recorder = recorder.clone();
        let target_path = target_path.clone();
        async move {
            let _ = with_file_mutation_queue(&target_path, async || {
                operation(&recorder, "target", Duration::from_millis(30)).await;
            })
            .await;
        }
    };
    let second = {
        let recorder = recorder.clone();
        async move {
            let _ = with_file_mutation_queue(&alias_path, async || {
                operation(&recorder, "alias", Duration::ZERO).await;
            })
            .await;
        }
    };
    tokio::join!(first, second);

    assert_eq!(
        recorder.order(),
        ["target:start", "target:end", "alias:start", "alias:end"]
    );
}

#[tokio::test]
async fn releases_the_queue_after_a_failed_operation() {
    let recorder = Recorder::default();
    let file = path("pi-file-mutation-queue-failure");

    let failure = with_file_mutation_queue(&file, async || Err::<(), _>("boom")).await;
    let after = with_file_mutation_queue(&file, async || {
        operation(&recorder, "after", Duration::ZERO).await;
    })
    .await;

    assert_eq!(failure.unwrap().unwrap_err(), "boom");
    after.unwrap();
    assert_eq!(recorder.order(), ["after:start", "after:end"]);
}

#[tokio::test]
async fn serializes_many_waiters_in_registration_order() {
    let recorder = Recorder::default();
    let file = path("pi-file-mutation-queue-many");

    let mut tasks = Vec::new();
    for index in 0..8 {
        let recorder = recorder.clone();
        let file = file.clone();
        tasks.push(tokio::spawn(async move {
            let _ = with_file_mutation_queue(&file, async || {
                operation(&recorder, &index.to_string(), Duration::from_millis(5)).await;
            })
            .await;
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }

    assert_eq!(recorder.peak(), 1);
    let order = recorder.order();
    assert_eq!(order.len(), 16);
    for index in 0..8 {
        let name = index.to_string();
        let start = order
            .iter()
            .position(|entry| *entry == format!("{name}:start"))
            .unwrap();
        let end = order.iter().position(|entry| *entry == format!("{name}:end")).unwrap();
        assert!(start < end);
    }
}
