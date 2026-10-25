#[path = "../examples/extensions/space_invaders.rs"]
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
        let result = example::space_invaders_extension_with_random(
            api,
            Arc::new(move || values.lock().unwrap().next().expect("random fixture exhausted")),
        );
        Box::pin(async move { result })
    })
}
fn command(steps: Value) -> Value {
    json!({"widths":[24,64,90,90],"steps":steps})
}
#[tokio::test(start_paused = true)]
async fn dispose_running_game_stops_ticks_on_its_own_runtime_without_quit() {
    support::compare(
        "space-invaders",
        factory(&json!([0.5])),
        json!({"random":[0.5],"commands":[{"widths":[90],"disposeRunning":true,"steps":[{"op":"key","key":"a"},{"op":"ticks","count":2,"ms":50}]}]}),
    ).await;
}

fn state() -> Value {
    json!({"player":{"x":30,"lives":3},"aliens":[{"x":4,"y":2,"type":2,"alive":true}],"alienDirection":1,"alienMoveCounter":0,"alienMoveDelay":18,"alienDropping":false,"bullets":[],"shields":[],"score":0,"highScore":40,"level":1,"gameOver":false,"victory":false,"alienShootCounter":0})
}
async fn saved(saved: Value, steps: Value) -> Value {
    let random = json!([0.0, 0.99, 0.5, 0.2, 0.7]);
    support::compare("space-invaders",factory(&random),json!({"fixed":true,"random":random,"entries":[{"customType":"space-invaders-save","data":saved}],"commands":[command(steps)]})).await
}
#[tokio::test(start_paused = true)]
async fn release_stops_held_movement_and_fire_and_default_registration_matches() {
    let default: ExtensionFactory = Arc::new(|api| {
        let result = example::space_invaders_extension(api);
        Box::pin(async move { result })
    });
    support::compare(
        "space-invaders",
        default,
        json!({"random":[],"commands":[{"mode":"rpc"}]}),
    )
    .await;
    for release in [true, false] {
        let mut steps = vec![json!({"op":"key","key":"a"}), json!({"op":"ticks","count":2,"ms":50})];
        if release {
            steps.push(json!({"op":"key","key":"\x1b[97;1:3u"}));
        }
        steps.extend([
            json!({"op":"ticks","count":2,"ms":50}),
            json!({"op":"key","key":"\x1b"}),
        ]);
        let result = support::compare(
            "space-invaders",
            factory(&json!([])),
            json!({"random":[],"commands":[command(json!(steps))]}),
        )
        .await;
        assert_eq!(
            result["entries"][0]["data"]["player"]["x"],
            if release { 29 } else { 28 }
        );
    }
    saved(
        state(),
        json!([
            {"op":"key","key":"x"},{"op":"key","key":"\x1b[D"},{"op":"key","key":"d"},
            {"op":"key","key":" "},{"op":"ticks","count":10,"ms":50},
            {"op":"key","key":"\x1b[32;1:3u"},{"op":"key","key":"\x1b[57350;1:3u"},
            {"op":"key","key":"\x1b[100;1:3u"},{"op":"ticks","count":25,"ms":50},
            {"op":"invalidate"},{"op":"key","key":"\x1b"}
        ]),
    )
    .await;
}
#[tokio::test(start_paused = true)]
async fn collisions_scoring_shields_victory_restart_and_owned_saves_match() {
    let mut s = state();
    s["bullets"] = json!([{"x":4,"y":3,"direction":-1}]);
    let result = saved(
        s.clone(),
        json!([
            {"op":"key","key":"x"},{"op":"ticks","count":1,"ms":50},
            {"op":"key","key":"R"},{"op":"ticks","count":1,"ms":50},{"op":"key","key":"\x1b"}
        ]),
    )
    .await;
    assert_eq!(result["entries"][0]["data"], s);
    assert_eq!(result["entries"][2]["data"]["level"], 2);
    let mut s = state();
    s["player"]["lives"] = json!(1);
    s["bullets"] = json!([{"x":30,"y":21,"direction":1},{"x":8,"y":18,"direction":-1},{"x":59,"y":23,"direction":1}]);
    s["shields"] = json!([{"x":8,"segments":[[true,true,true,true],[true,true,true,true],[true,false,false,true]]}]);
    saved(
        s,
        json!([{"op":"key","key":"x"},{"op":"ticks","count":1,"ms":50},{"op":"key","key":"r"},{"op":"key","key":"Q"}]),
    )
    .await;
}
#[tokio::test(start_paused = true)]
async fn alien_edges_drop_invasion_shooting_speed_and_pause_quit_match() {
    for (x, y, dropping, count) in [(57, 2, false, 5), (2, 20, true, 1)] {
        let mut s = state();
        s["aliens"][0]["x"] = json!(x);
        s["aliens"][0]["y"] = json!(y);
        s["alienMoveDelay"] = json!(1);
        s["alienDropping"] = json!(dropping);
        s["alienShootCounter"] = json!(29);
        saved(
            s,
            json!([{"op":"key","key":"x"},{"op":"ticks","count":count,"ms":50},{"op":"key","key":"\x1b"}]),
        )
        .await;
    }
    saved(
        state(),
        json!([{"op":"key","key":"\x1b[97;1:3u"},{"op":"key","key":"Q"}]),
    )
    .await;
    let mut s = state();
    s["gameOver"] = json!(true);
    saved(s, json!([{"op":"ticks","count":2,"ms":50},{"op":"key","key":"q"}])).await;
}

#[tokio::test(start_paused = true)]
async fn speed_bands_column_shooters_caps_and_screen_edges_match() {
    for count in [0, 5, 10, 20, 21] {
        let mut s = state();
        s["aliens"] = json!(
            (0..count)
                .map(|i| json!({"x":4+(i%5)*5,"y":2+(i/5)*2,"type":i%3,"alive":true}))
                .collect::<Vec<_>>()
        );
        s["alienMoveCounter"] = json!(17);
        s["alienShootCounter"] = json!(29);
        s["player"]["x"] = json!(2);
        saved(s,json!([{"op":"key","key":"x"},{"op":"key","key":"A"},{"op":"key","key":"f"},{"op":"ticks","count":32,"ms":50},{"op":"key","key":"\x1b[102;1:3u"},{"op":"key","key":"\x1b"}])).await;
    }
    let mut s = state();
    s["player"]["x"] = json!(57);
    saved(s,json!([{"op":"key","key":"x"},{"op":"key","key":"D"},{"op":"ticks","count":4,"ms":50},{"op":"key","key":"\x1b"}])).await;
}
#[tokio::test(start_paused = true)]
async fn multiple_hits_reproduce_negative_lives_source_error() {
    let mut s = state();
    s["player"]["lives"] = json!(1);
    s["bullets"] = json!([{"x":29,"y":21,"direction":1},{"x":30,"y":21,"direction":1},{"x":31,"y":21,"direction":1}]);
    saved(
        s,
        json!([{"op":"key","key":"x"},{"op":"ticks","count":1,"ms":50},{"op":"key","key":"q"}]),
    )
    .await;
}
