use crate::shared_oracle as oracle;

use pi_coding_agent::utils::pi_user_agent::{format_pi_user_agent, node_arch_name, node_platform_name};
use serde_json::{Value, json};

const REQUEST_ID: u64 = 26_005;

const VERSIONS: [&str; 5] = ["0.0.0", "1.2.3", "0.84.1", "10.20.30-beta.1", "1.0.0+build.5"];

#[test]
fn coding_agent_pi_user_agent_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-pi-user-agent-oracle.ts",
        "packages/coding-agent/src/utils/pi-user-agent.ts",
    );
    let expected = oracle.request(&request());
    let actual = response(&expected);

    assert_eq!(actual, expected, "coding-agent pi-user-agent mismatch");
}

fn request() -> Value {
    let cases = VERSIONS
        .iter()
        .enumerate()
        .map(|(index, version)| json!({"name": format!("case {index}"), "version": version}))
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "operation": "coding_agent_pi_user_agent", "cases": cases})
}

fn response(expected: &Value) -> Value {
    let platform = expected["output"]["platform"].as_str().expect("platform");
    let arch = expected["output"]["arch"].as_str().expect("arch");
    let runtime = expected["output"]["runtime"].as_str().expect("runtime");

    assert_eq!(node_platform_name(std::env::consts::OS), platform);
    assert_eq!(node_arch_name(std::env::consts::ARCH), arch);

    let results = VERSIONS
        .iter()
        .enumerate()
        .map(|(index, version)| {
            json!({
                "name": format!("case {index}"),
                "value": format_pi_user_agent(&(*version).into(), platform, runtime, arch),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "id": REQUEST_ID,
        "ok": true,
        "output": {
            "platform": platform,
            "arch": arch,
            "runtime": runtime,
            "results": results,
        },
    })
}
