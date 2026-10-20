#[path = "../examples/extensions/todo.rs"]
mod example;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::{Value, json};
use std::sync::Arc;
#[tokio::test]
async fn todo_state_errors_branch_restore_and_all_renderers_match() {
    let mut cases = vec![
        json!({"event":"session_start"}),
        json!({"event":"tool_metadata","toolName":"todo"}),
        json!({"event":"command","name":"todos","keys":["\u{1b}"]}),
        json!({"event":"command","name":"todos","mode":"rpc"}),
    ];
    for params in [
        json!({"action":"list"}),
        json!({"action":"add"}),
        json!({"action":"add","text":""}),
        json!({"action":"toggle"}),
        json!({"action":"toggle","id":88}),
        json!({"action":"other"}),
        json!({"action":"add","text":"first λ🚀"}),
        json!({"action":"toggle","id":1}),
        json!({"action":"toggle","id":1}),
        json!({"action":"list"}),
        json!({"action":"clear"}),
    ] {
        cases.push(json!({"event":"execute","toolName":"todo","params":params}));
        cases.push(json!({"event":"render_call","toolName":"todo","params":params}));
    }
    let saved = json!({"action":"list","todos":[{"id":9,"text":"restored","done":true}],"nextId":10});
    cases.push(json!({"event":"append","message":{"role":"toolResult","toolCallId":"a","toolName":"todo","content":[],"details":saved,"isError":false,"timestamp":0}}));
    cases.push(json!({"event":"event","value":{"type":"session_tree","newLeafId":null,"oldLeafId":null}}));
    cases.push(json!({"event":"execute","toolName":"todo","params":{"action":"add","text":"after restore"}}));
    cases.push(json!({"event":"command","name":"todos","keys":["x","\u{3}"],"width":24}));
    let mut todos = vec![];
    for id in 1..=7 {
        todos.push(json!({"id":id,"text":format!("item {id} with longer text"),"done":id%2==0}));
    }
    let mut details = vec![
        Value::Null,
        json!({"action":"list","todos":[],"nextId":1}),
        json!({"action":"list","todos":todos,"nextId":8}),
        json!({"action":"add","todos":todos,"nextId":8}),
        json!({"action":"toggle","todos":todos,"nextId":8}),
        json!({"action":"clear","todos":[],"nextId":1}),
        json!({"action":"add","todos":[],"nextId":1,"error":"text required"}),
    ];
    for details in details.drain(..) {
        for expanded in [false, true] {
            for width in [16, 40] {
                cases.push(json!({"event":"render_result","toolName":"todo","width":width,"expanded":expanded,"result":{"content":[{"type":"text","text":"fixture fallback"}],"details":details}}));
            }
        }
    }
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::todo_extension(api);
        Box::pin(async move { result })
    });
    let actual = support::compare("todo", factory, json!({"cases":cases})).await;
    assert_eq!(
        actual["cases"][4]["result"]["value"],
        json!({"content":[{"type":"text","text":"No todos"}],"details":{"action":"list","todos":[],"nextId":1}})
    );
}
