use super::*;
use pi_ai::request::{BoxFuture, HttpBody, HttpResponse};
use std::sync::atomic::{AtomicUsize, Ordering};

mod oracle {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
}

struct EmptyBody;
impl HttpBody for EmptyBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async { None })
    }
}
struct RetryFailureFetch {
    headers: Vec<(String, String)>,
    calls: AtomicUsize,
    signal: Mutex<Option<AbortSignal>>,
}
impl Fetch for RetryFailureFetch {
    fn fetch(&self, _: HttpRequest, signal: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let signal = signal.unwrap();
        let mut observed = self.signal.lock().unwrap();
        if let Some(previous) = observed.as_ref() {
            assert_eq!(previous, &signal);
        }
        *observed = Some(signal);
        self.calls.fetch_add(1, Ordering::SeqCst);
        let headers = self.headers.clone();
        Box::pin(async move {
            Ok(HttpResponse {
                status: 503,
                status_text: String::new(),
                headers,
                body: Box::new(EmptyBody),
            })
        })
    }
}

#[tokio::test(start_paused = true)]
async fn catalog_retry_runtime_matches_frozen_timer_boundary() {
    const NOW: f64 = 1_766_000_000_000.0;
    let future_date = crate::utils::iso_time::format_iso_millis((NOW + 2000.0) as i64);
    let maximum_jitter = f64::from_bits(0x3fefffffffffffff);
    let cases = [
        ("jitter bounds", vec![], 0.0, vec![0.0, maximum_jitter], false),
        (
            "fractional waits",
            vec![],
            0.0,
            vec![0.123456789, 0.3333333333333],
            false,
        ),
        (
            "trimmed seconds",
            vec![("Retry-After", " 1 ".to_owned())],
            1000.0,
            vec![0.5],
            false,
        ),
        (
            "future date",
            vec![("retry-after", future_date)],
            2000.0,
            vec![0.5],
            false,
        ),
        (
            "past date",
            vec![("retry-after", "2020-01-01".to_owned())],
            0.0,
            vec![0.5],
            false,
        ),
        (
            "invalid date",
            vec![("retry-after", "nonsense".to_owned())],
            0.0,
            vec![0.5],
            false,
        ),
        (
            "duplicates",
            vec![("Retry-After", "1".to_owned()), ("retry-after", "2".to_owned())],
            0.0,
            vec![0.5],
            false,
        ),
        (
            "deadline equality",
            vec![("retry-after", "5".to_owned())],
            5000.0,
            vec![0.5],
            false,
        ),
        (
            "huge seconds",
            vec![("retry-after", "99999999999".to_owned())],
            99_999_999_999_000.0,
            vec![0.5],
            false,
        ),
        (
            "abort during delay",
            vec![("retry-after", "4".to_owned())],
            4000.0,
            vec![0.5],
            true,
        ),
    ];
    for (label, headers, retry_after, random_values, real_delay) in cases {
        let directory = tempfile::tempdir().unwrap();
        let mut source = oracle::TsOracle::spawn_with_environment(
            "model-catalog-cache-oracle.ts",
            "packages/coding-agent/src/core/model-catalog-cache.ts",
            &[("XDG_CACHE_HOME", directory.path().as_os_str().to_owned())],
        );
        let expected = source.request(&serde_json::json!({
            "operation":"ensure", "force":true, "now":NOW, "inspectError":true,
            "inspectSignal":true, "status":503, "headers":headers, "inspectRetry":true,
            "clockValues":[NOW], "randomValues":random_values, "realRetryDelay":real_delay,
        }));
        assert_eq!(expected["ok"], true, "{label}: {expected}");
        let output = &expected["output"];
        let clock_calls = AtomicUsize::new(0);
        let random_calls = AtomicUsize::new(0);
        let now = || {
            clock_calls.fetch_add(1, Ordering::SeqCst);
            NOW
        };
        let random = || {
            let index = random_calls.fetch_add(1, Ordering::SeqCst);
            random_values[index.min(random_values.len() - 1)]
        };
        let fetch = RetryFailureFetch {
            headers: headers
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
            calls: AtomicUsize::new(0),
            signal: Mutex::new(None),
        };
        let start = tokio::time::Instant::now();
        let actual = fetch_catalog_with_runtime(
            &fetch,
            CatalogRetryRuntime {
                now: &now,
                random: &random,
            },
        )
        .await
        .unwrap_err();
        let elapsed = tokio::time::Instant::now().duration_since(start).as_millis();
        assert_eq!(actual.value().to_serde().unwrap(), output["error"], "{label}");
        assert_eq!(
            fetch.calls.load(Ordering::SeqCst),
            output["calls"].as_array().unwrap().len(),
            "{label}"
        );
        assert_eq!(
            clock_calls.load(Ordering::SeqCst),
            output["clockCalls"].as_u64().unwrap() as usize,
            "{label}"
        );
        assert_eq!(
            random_calls.load(Ordering::SeqCst),
            output["randomCalls"].as_u64().unwrap() as usize,
            "{label}"
        );
        let waits = output["waits"].as_array().unwrap();
        for (attempt, expected_wait) in waits.iter().enumerate() {
            let jitter = random_values[attempt.min(random_values.len() - 1)];
            assert_eq!(
                retry_wait_ms(retry_after, attempt as u32, jitter),
                expected_wait.as_f64().unwrap(),
                "{label}"
            );
        }
        let expected_elapsed = if real_delay {
            5000
        } else {
            waits.iter().map(|value| value.as_f64().unwrap().floor() as u128).sum()
        };
        assert_eq!(elapsed, expected_elapsed, "{label}");
        let signal = fetch.signal.lock().unwrap().clone().unwrap();
        assert_eq!(
            signal.aborted(),
            output["signalFinal"]["aborted"].as_bool().unwrap(),
            "{label}"
        );
        let aborted = signal.aborted();
        tokio::time::advance(Duration::from_secs(6)).await;
        assert_eq!(signal.aborted(), aborted, "timer was not cleared: {label}");
    }
}
