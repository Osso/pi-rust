#[path = "../examples/extensions/hello.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn greetings_and_schema_match_pinned_runtime() {
    let actual = harness::compare(
        "hello",
        harness::factory(example::hello_extension),
        json!({"steps":[
            {"op":"tool","name":"hello","params":{"name":"Alessio"}},
            {"op":"tool","name":"hello","params":{"name":""}},
            {"op":"tool","name":"hello","params":{"name":"🚀𐀀\nλ"}}
        ]}),
    )
    .await;
    assert_eq!(
        actual["trace"][0],
        json!(["tool",{"content":[{"type":"text","text":"Hello, Alessio!"}],"details":{"greeted":"Alessio"}}])
    );
    assert_eq!(
        actual["tools"][0][3],
        json!({"type":"object","properties":{"name":{"type":"string","description":"Name to greet"}},"required":["name"]})
    );
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
