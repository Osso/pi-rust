use crate::shared_oracle as oracle;
use pi_coding_agent::cli::args::{UnknownFlagValue, parse_args};
use serde_json::json;
use std::process::{Command, Stdio};

#[test]
fn refresh_models_flag_is_recognized_instead_of_consumed_as_an_extension_flag() {
    let mut source = oracle::TsOracle::spawn("catalog-startup-oracle.mts", "packages/coding-agent/src");
    for (values, plain) in [
        (vec!["--refresh-models"], true),
        (vec!["--refresh-models", "hello"], true),
        (vec!["--refresh-models", "--refresh-models"], true),
        (vec!["--print", "--refresh-models"], false),
        (vec!["--mode", "rpc", "--refresh-models"], false),
        (vec!["--help", "--refresh-models"], true),
        (vec!["--offline", "--refresh-models"], true),
        (vec![], false),
    ] {
        let args: Vec<_> = values.iter().map(|value| (*value).to_owned()).collect();
        let expected = source.request(&json!({"id":1,"operation":"args","args":args}));
        assert_eq!(expected["ok"], true, "{expected}");
        let parsed = parse_args(&args);
        assert_eq!(
            json!(parsed.refresh_models),
            expected["output"]
                .get("refreshModels")
                .cloned()
                .unwrap_or(serde_json::Value::Null)
        );
        assert_eq!(
            pi_coding_agent::main_runner::is_plain_runtime_metadata_command(&parsed),
            plain
        );
        let unknown: Vec<_> = parsed
            .unknown_flags
            .iter()
            .map(|(key, value)| {
                json!([
                    key,
                    match value {
                        UnknownFlagValue::Boolean(value) => json!(value),
                        UnknownFlagValue::String(value) => json!(value),
                    }
                ])
            })
            .collect();
        assert_eq!(json!(unknown), expected["output"]["unknownFlags"], "args={values:?}");
        assert_eq!(
            json!(parsed.messages),
            expected["output"]["messages"],
            "args={values:?}"
        );
    }
}

#[test]
fn refresh_models_help_entry_matches_frozen_without_claiming_other_args_help_rows() {
    let mut source = oracle::TsOracle::spawn("catalog-startup-oracle.mts", "packages/coding-agent/src");
    let expected = source.request(&json!({"id":1,"operation":"help"}));
    assert_eq!(expected["ok"], true, "{expected}");
    let source_rows: Vec<_> = expected["output"]
        .as_str()
        .unwrap()
        .lines()
        .filter(|line| line.trim_start().starts_with("--refresh-models"))
        .collect();
    let help = pi_coding_agent::cli::args::print_help(&[]);
    let native_rows: Vec<_> = help
        .lines()
        .filter(|line| line.trim_start().starts_with("--refresh-models"))
        .collect();
    assert_eq!(native_rows, source_rows);
    assert_eq!(native_rows.len(), 1);
}

#[test]
fn cli_startup_loads_fresh_catalog_before_the_registry_and_list_models() {
    let temp = tempfile::tempdir().unwrap();
    let agent = temp.path().join("agent");
    let project = temp.path().join("project");
    let cache = temp.path().join("cache");
    let state = temp.path().join("state");
    for directory in [&agent, &project, &state] {
        std::fs::create_dir_all(directory).unwrap();
    }
    let auth = agent.join("auth.json");
    let models = agent.join("models.json");
    std::fs::write(
        &auth,
        serde_json::to_vec(&json!({"openrouter":{"type":"api_key","key":"fixture"}})).unwrap(),
    )
    .unwrap();
    let cache_path = cache.join("pi/models/openrouter.json");
    std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
    let id = "zzq15-startup-cache-only-zz";
    let model = json!({"id":id,"name":"Startup cache probe","provider":"openrouter","api":"openai-completions","baseUrl":"https://openrouter.ai/api/v1","reasoning":false,"input":["text"],"cost":{"input":1,"output":2,"cacheRead":3,"cacheWrite":4},"contextWindow":4096.75,"maxTokens":128.25,"cacheMarker":{"kept":true}});
    let bytes = serde_json::to_vec(&json!({"fetchedAt":"2099-01-01T00:00:00.000Z","models":[model]})).unwrap();
    std::fs::write(&cache_path, &bytes).unwrap();
    let environment = [
        ("PI_CODING_AGENT_DIR", agent.clone().into_os_string()),
        ("XDG_CACHE_HOME", cache.clone().into_os_string()),
        ("NO_COLOR", "1".into()),
        ("PI_CODING_AGENT_STATE_DIR", state.clone().into_os_string()),
        ("HOME", temp.path().join("home").into_os_string()),
        ("XDG_CONFIG_HOME", temp.path().join("config").into_os_string()),
    ];
    let mut source = oracle::TsOracle::spawn_with_environment(
        "catalog-startup-oracle.mts",
        "packages/coding-agent/src",
        &environment,
    );
    let expected =
        source.request(&json!({"id":1,"operation":"startup","authPath":auth,"modelsPath":models,"modelId":id}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(expected["output"]["source"], "cache");
    assert_eq!(expected["output"]["model"], model);
    // A fresh source process must exercise main's startup call, not inherit the manually loaded memo above.
    let mut source_cli = oracle::TsOracle::spawn_with_environment(
        "catalog-startup-oracle.mts",
        "packages/coding-agent/src",
        &environment,
    );
    let actual_cli = source_cli.request(&json!({"operation":"main_list_models","cwd":project,"args":["--offline","--no-supervisor","--no-session","-ne","--list-models",id]}));
    assert_eq!(actual_cli["ok"], true, "{actual_cli}");
    assert_eq!(actual_cli["output"]["code"], 0, "{actual_cli}");
    assert!(actual_cli["output"]["stdout"].as_str().unwrap().contains(id));
    assert_eq!(actual_cli["output"]["stdout"], expected["output"]["stdout"]);
    assert_eq!(actual_cli["output"]["stderr"], expected["output"]["stderr"]);
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_pi"))
            .args(args)
            .current_dir(&project)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", temp.path().join("home"))
            .env("XDG_CONFIG_HOME", temp.path().join("config"))
            .env("XDG_CACHE_HOME", &cache)
            .env("PI_CODING_AGENT_DIR", &agent)
            .env("PI_CODING_AGENT_STATE_DIR", &state)
            .env("NO_COLOR", "1")
            .env("PI_SKIP_VERSION_CHECK", "1")
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    let output = run(&[
        "--offline",
        "--no-supervisor",
        "--no-session",
        "-ne",
        "--list-models",
        id,
    ]);
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        expected["output"]["stderr"].as_str().unwrap()
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        expected["output"]["stdout"].as_str().unwrap()
    );
    assert_eq!(
        std::fs::read(&cache_path).unwrap(),
        bytes,
        "startup must not rewrite the cache"
    );
    let args = [
        "--offline",
        "--no-supervisor",
        "--no-session",
        "-ne",
        "--refresh-models",
    ];
    let expected = source.request(&json!({"id":2,"operation":"main_offline_refresh","args":args,"cwd":project}));
    assert_eq!(expected["ok"], true, "{expected}");
    let output = run(&args);
    assert_eq!(
        output.status.code(),
        Some(expected["output"]["code"].as_i64().unwrap() as i32)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        expected["output"]["stdout"].as_str().unwrap()
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        expected["output"]["stderr"].as_str().unwrap()
    );
    assert_eq!(
        std::fs::read(cache_path).unwrap(),
        bytes,
        "forced offline failure must not rewrite fresh cache"
    );
}
