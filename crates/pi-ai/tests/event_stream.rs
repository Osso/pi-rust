use std::time::Duration;

use pi_ai::utils::event_stream::{EventStream, StreamFailure, event_stream};

fn number_stream() -> (
    pi_ai::utils::event_stream::EventSender<i32, String>,
    EventStream<i32, String>,
) {
    event_stream(|event: &i32| *event < 0, |event: &i32| format!("final:{event}"))
}

#[tokio::test]
async fn queued_events_are_yielded_in_order_and_completion_event_is_delivered() {
    let (sender, mut stream) = number_stream();
    sender.push(1);
    sender.push(2);
    sender.push(-3);
    sender.push(4); // ignored after completion

    let mut seen = Vec::new();
    while let Some(event) = stream.next().await {
        seen.push(event.expect("no failure"));
    }
    assert_eq!(seen, vec![1, 2, -3]);
    assert_eq!(stream.result().await.expect("result"), "final:-3");
}

#[tokio::test]
async fn waiting_consumer_receives_events_pushed_later() {
    let (sender, mut stream) = number_stream();
    let producer = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        sender.push(7);
        tokio::time::sleep(Duration::from_millis(10)).await;
        sender.end(Some("explicit".to_owned()));
    });
    assert_eq!(stream.next().await.map(Result::unwrap), Some(7));
    assert!(stream.next().await.is_none());
    assert_eq!(stream.result().await.expect("result"), "explicit");
    producer.await.expect("producer");
}

#[tokio::test]
async fn end_keeps_queued_events_and_ignores_later_pushes() {
    let (sender, mut stream) = number_stream();
    sender.push(1);
    sender.end(Some("done".to_owned()));
    sender.push(2);
    assert_eq!(stream.next().await.map(Result::unwrap), Some(1));
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn end_without_result_leaves_result_pending() {
    let (sender, stream) = number_stream();
    sender.end(None);
    let pending = tokio::time::timeout(Duration::from_millis(20), stream.result()).await;
    assert!(pending.is_err(), "TS leaves result() unresolved after end(undefined)");
}

#[tokio::test]
async fn fail_rejects_iteration_after_queue_drains_and_rejects_result() {
    let (sender, mut stream) = number_stream();
    sender.push(1);
    sender.fail(StreamFailure::new("boom"));
    sender.push(2);
    assert_eq!(stream.next().await.map(Result::unwrap), Some(1));
    let failure = stream.next().await.expect("failure item").expect_err("must fail");
    assert_eq!(failure.message(), "boom");
    assert_eq!(stream.result().await.expect_err("rejected").message(), "boom");
}

#[tokio::test]
async fn fail_wakes_waiting_consumer() {
    let (sender, mut stream) = number_stream();
    let producer = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        sender.fail(StreamFailure::new("late"));
    });
    let failure = stream.next().await.expect("item").expect_err("fails");
    assert_eq!(failure.message(), "late");
    producer.await.expect("producer");
}

#[tokio::test]
async fn completion_after_fail_is_ignored() {
    let (sender, stream) = number_stream();
    sender.fail(StreamFailure::new("first"));
    sender.end(Some("late".to_owned()));
    sender.push(-1);
    assert_eq!(stream.result().await.expect_err("still failed").message(), "first");
}
