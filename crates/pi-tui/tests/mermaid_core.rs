#[path = "../src/mermaid.rs"]
mod mermaid;

use serde_json::Value;
use std::process::Command;

#[test]
fn normal_diagrams_match_locked_js() {
    let diagrams = [
        "graph TD\n A --> B",
        "flowchart LR\n A[Start] --> B[Done]",
        "sequenceDiagram\n Alice->>Bob: Hello\n Bob-->>Alice: Hi",
        "stateDiagram-v2\n [*] --> Idle\n Idle --> Running",
        "classDiagram\n Animal <|-- Dog",
        "erDiagram\n CUSTOMER ||--o{ ORDER : places",
        "flowchart LR\n A[你好] --> B[café]",
        "flowchart LR\n A[🙂] --> B[Done]",
        "flowchart LR\n A[é] --> B[e\u{301}]",
        "flowchart LR\n A[👩‍💻] --> B[👨‍👩‍👧‍👦]",
        "flowchart RL\n A[👩‍💻] --> B[e\u{301}]",
        "sequenceDiagram\n Alice->>Bob: 👩‍💻 e\u{301}",
        "flowchart LR\n A[👩‍💻👩‍💻👩‍💻👩‍💻👩‍💻👩‍💻👩‍💻👩‍💻👩‍💻👩‍💻👩‍💻👩‍💻👩‍💻] --> B[Done]",
    ];
    let oracle = "/home/osso/.worktrees/pi-rust-source-532f7d12/node_modules/grok-mermaid/dist/index.js";
    for diagram in diagrams {
        let output = Command::new("node")
            .args([
                "--input-type=module",
                "-e",
                &format!("import {{render}} from {oracle:?}; console.log(JSON.stringify(render(process.argv[1])))"),
                diagram,
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
        let actual = mermaid::render(diagram).unwrap();
        assert_eq!(serde_json::json!(actual.plain_lines), expected["plain"], "{diagram}");
        assert_eq!(actual.width, expected["width"], "{diagram}");
        assert_eq!(actual.warnings, None);
        let styled: Vec<Vec<Value>> = actual
            .styled_lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| {
                        let cls = match span.style {
                            mermaid::Style::Empty => "none",
                            mermaid::Style::Border => "border",
                            mermaid::Style::Text => "text",
                            mermaid::Style::Edge => "edge",
                            mermaid::Style::EdgeLabel => "edgeLabel",
                        };
                        serde_json::json!({"text": span.text, "cls": cls})
                    })
                    .collect()
            })
            .collect();
        assert_eq!(serde_json::json!(styled), expected["styled"], "{diagram}");
    }
}

#[test]
fn unsupported_and_empty_input_return_none() {
    for source in ["", "pie\n title Pets\n \"Dogs\" : 4"] {
        assert!(mermaid::render(source).is_none());
    }
}
