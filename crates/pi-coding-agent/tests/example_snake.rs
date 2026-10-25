#[path = "../examples/extensions/snake.rs"]
mod example;
use crate::shared_extension_examples_l_games as support;
use pi_coding_agent::core::extensions::ExtensionFactory;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
fn factory(random: &Value) -> ExtensionFactory {
    let values: Vec<f64> = serde_json::from_value(random.clone()).unwrap();
    let values = Arc::new(Mutex::new(values.into_iter()));
    Arc::new(move |api| {
        let values = values.clone();
        let result = example::snake_extension_with_random(
            api,
            Arc::new(move || values.lock().unwrap().next().expect("random fixture exhausted")),
        );
        Box::pin(async move { result })
    })
}
#[tokio::test(start_paused = true)]
async fn dispose_running_game_stops_ticks_on_its_own_runtime_without_quit() {
    let random = json!([0.1, 0.1]);
    support::compare(
        "snake",
        factory(&random),
        json!({"random":random,"commands":[{"widths":[44],"disposeRunning":true,"steps":[{"op":"ticks","count":1,"ms":100}]}]}),
    ).await;
}

fn command(steps: Value) -> Value {
    json!({"widths":[12,44,100],"steps":steps})
}
#[tokio::test(start_paused = true)]
async fn virtual_clock_food_collisions_restart_save_resume_and_frames_match() {
    let default_factory: ExtensionFactory = Arc::new(|api| {
        let result = example::snake_extension(api);
        Box::pin(async move { result })
    });
    support::compare(
        "snake",
        default_factory,
        json!({"random":[],"commands":[{"mode":"rpc"}]}),
    )
    .await;
    let random = json!([0.5, 0.5, 0.525, 0.49, 0.1, 0.1, 0.2, 0.2, 0.3, 0.3, 0.4, 0.4, 0.7, 0.7]);
    let first = command(json!([
     {"op":"ticks","count":1},{"op":"key","key":"a"},{"op":"ticks","count":1},
     {"op":"key","key":"w"},{"op":"ticks","count":1},{"op":"key","key":"s"},
     {"op":"ticks","count":8},{"op":"key","key":"R"},{"op":"invalidate"},
     {"op":"ticks","count":1},{"op":"key","key":"\u{1b}"}
    ]));
    support::compare(
        "snake",
        factory(&random),
        json!({"random":random,"commands":[first.clone()]}),
    )
    .await;
    let request = json!({"fixed":true,"random":random,"commands":[{"mode":"rpc"},first,command(json!([{"op":"key","key":"Q"}])),command(json!([{"op":"key","key":"x"},{"op":"ticks","count":2},{"op":"key","key":"q"}]))]});
    support::compare("snake", factory(&random), request).await;
}
#[tokio::test(start_paused = true)]
async fn restored_state_is_an_owned_copy_and_self_collision_uses_pre_tail_body() {
    let random = json!([0.1, 0.2, 0.3, 0.4]);
    let saved = json!({"snake":[{"x":2,"y":2},{"x":2,"y":3},{"x":1,"y":3},{"x":1,"y":2}],"food":{"x":10,"y":10},"direction":"left","nextDirection":"left","score":20,"gameOver":false,"highScore":30});
    let request = json!({"fixed":true,"random":random,"entries":[{"customType":"unrelated","data":{}},{"customType":"snake-save","data":saved}],"commands":[command(json!([{"op":"key","key":"\u{1b}"}])),command(json!([{"op":"key","key":"x"},{"op":"ticks","count":1},{"op":"key","key":" "},{"op":"ticks","count":1},{"op":"key","key":"\u{1b}"}]))]});
    let actual = support::compare("snake", factory(&random), request).await;
    assert_eq!(
        actual["entries"][1]["data"], saved,
        "later mutation must not alter stored entry"
    );
    assert_eq!(actual["entries"][3]["data"]["highScore"], 30);
}

#[tokio::test(start_paused = true)]
async fn ended_save_keeps_high_score_and_narrow_width_errors_match() {
    let random = json!([0.1, 0.1]);
    let saved = json!({"snake":[{"x":39,"y":0}],"food":{"x":1,"y":1},"direction":"right","nextDirection":"right","score":20,"gameOver":true,"highScore":60});
    let actual=support::compare("snake",factory(&random),json!({"random":random,"entries":[{"customType":"snake-save","data":saved}],"commands":[{"widths":[0,1,2,3,4,5,84,84],"steps":[{"op":"key","key":"D"},{"op":"key","key":"\u{1b}[B"},{"op":"ticks","count":1},{"op":"key","key":"A"},{"op":"ticks","count":1},{"op":"key","key":"W"},{"op":"ticks","count":1},{"op":"key","key":"\u{1b}[C"},{"op":"ticks","count":1},{"op":"key","key":"\u{1b}"}]}]})).await;
    assert_eq!(actual["entries"][1]["data"]["highScore"], 60);
    assert_eq!(actual["entries"][1]["data"]["score"], 0);
}
