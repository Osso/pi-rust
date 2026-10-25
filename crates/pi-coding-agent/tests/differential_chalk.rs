#[path = "support/oracle.rs"]
mod oracle;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use pi_coding_agent::utils::chalk::{Chalk, Level, Style, detect_level_for};
use serde_json::{Value, json};

const REQUEST_ID: u64 = 26_100;

struct StyleCase {
    level: Level,
    styles: Vec<Style>,
    text: &'static str,
}

#[test]
fn chalk_styling_matches_the_pinned_chalk_package() {
    let cases = style_cases();
    let mut oracle = oracle::TsOracle::spawn("chalk-oracle.ts", "node_modules/chalk/source/index.js");
    let expected = oracle.request(&request(&cases));
    let results = expected["output"]["results"]
        .as_array()
        .expect("the chalk oracle must return results");
    assert_eq!(results.len(), cases.len(), "one oracle result per case");

    for (case, result) in cases.iter().zip(results) {
        let actual = Chalk::new(case.level).paint(&case.styles, case.text);
        assert_eq!(
            actual,
            result["value"].as_str().expect("oracle value must be a string"),
            "chalk mismatch for level {:?}, styles {:?}, text {:?}",
            case.level,
            case.styles,
            case.text
        );
    }
}

#[test]
fn native_chalk_styles_preserve_unmatched_units_and_reopen_lines() {
    use pi_js_compat::js_json::{JsString, JsValue};
    let mut oracle = oracle::TsOracle::spawn("chalk-oracle.ts", "node_modules/chalk/source/index.js");
    for level in [Level::Plain, Level::Basic, Level::Ansi256, Level::Ansi16m] {
        for wire in [
            r#""\ud800""#,
            r#""\udfff""#,
            r#""\ud800\r\n\udfff\u001b[22m\ud800""#,
            r#""😀\ud800\n\udfff""#,
        ] {
            let text = pi_js_compat::js_json::parse_lossless(wire)
                .unwrap()
                .as_string()
                .unwrap()
                .clone();
            let expected = oracle.request(&pi_js_compat::js_json!({
                "id":REQUEST_ID,"operation":"style","cases":[{"name":"raw","level":level.number(),"styles":["bold","red"],"text":text}]
            }));
            let expected = &expected["output"]["results"].as_array().unwrap()[0]["value"];
            let actual = Chalk::new(level).paint(&[Style::Bold, Style::Red], &text);
            assert_eq!(JsValue::String(actual), *expected);
        }
    }
}

#[test]
fn supports_color_detection_matches_the_pinned_supports_color_package() {
    let module = source_worktree().join("node_modules/chalk/source/vendor/supports-color/index.js");
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/chalk-detect-oracle.mjs");
    let node = node_binary();

    for scenario in detection_scenarios() {
        let output = Command::new(&node)
            .arg(&script)
            .arg(&module)
            .args(scenario.flags)
            .env_clear()
            .envs(scenario.env.iter().copied())
            .output()
            .expect("the node detection oracle must run");
        assert!(
            output.status.success(),
            "{} failed: {}",
            scenario.name,
            String::from_utf8_lossy(&output.stderr)
        );
        let detected: Value = serde_json::from_slice(&output.stdout).expect("the detection oracle must print JSON");
        let env = scenario
            .env
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>();
        let argv = detection_argv(&script, &module, scenario.flags);

        assert_eq!(
            detect_level_for(&env, &argv, true),
            level_from_oracle(&detected["tty"]),
            "{} with a TTY",
            scenario.name
        );
        assert_eq!(
            detect_level_for(&env, &argv, false),
            level_from_oracle(&detected["noTty"]),
            "{} without a TTY",
            scenario.name
        );
    }
}

fn detection_argv(script: &Path, module: &Path, flags: &[&str]) -> Vec<String> {
    let mut argv = vec![
        "node".to_owned(),
        script.display().to_string(),
        module.display().to_string(),
    ];
    argv.extend(flags.iter().map(|flag| (*flag).to_owned()));
    argv
}

fn level_from_oracle(value: &Value) -> Level {
    let number =
        u8::try_from(value.as_u64().expect("oracle level must be a number")).expect("oracle level must fit in u8");
    Level::from_number(number).expect("oracle level must be 0..=3")
}

fn request(cases: &[StyleCase]) -> Value {
    json!({
        "id": REQUEST_ID,
        "operation": "style",
        "cases": cases
            .iter()
            .enumerate()
            .map(|(index, case)| json!({
                "name": format!("case {index}"),
                "level": case.level.number(),
                "styles": case.styles.iter().map(|style| chalk_name(*style)).collect::<Vec<_>>(),
                "text": case.text,
            }))
            .collect::<Vec<_>>(),
    })
}

fn chalk_name(style: Style) -> &'static str {
    match style {
        Style::Reset => "reset",
        Style::Bold => "bold",
        Style::Dim => "dim",
        Style::Italic => "italic",
        Style::Underline => "underline",
        Style::Overline => "overline",
        Style::Inverse => "inverse",
        Style::Hidden => "hidden",
        Style::Strikethrough => "strikethrough",
        Style::Black => "black",
        Style::Red => "red",
        Style::Green => "green",
        Style::Yellow => "yellow",
        Style::Blue => "blue",
        Style::Magenta => "magenta",
        Style::Cyan => "cyan",
        Style::White => "white",
        Style::BlackBright => "blackBright",
        Style::Gray => "gray",
        Style::RedBright => "redBright",
        Style::GreenBright => "greenBright",
        Style::YellowBright => "yellowBright",
        Style::BlueBright => "blueBright",
        Style::MagentaBright => "magentaBright",
        Style::CyanBright => "cyanBright",
    }
}

fn style_cases() -> Vec<StyleCase> {
    let chains: [&[Style]; 18] = [
        &[],
        &[Style::Red],
        &[Style::Bold],
        &[Style::Dim],
        &[Style::Yellow],
        &[Style::Green],
        &[Style::Cyan],
        &[Style::Italic],
        &[Style::Strikethrough],
        &[Style::Blue],
        &[Style::Underline],
        &[Style::Gray],
        &[Style::Magenta],
        &[Style::Inverse],
        &[Style::Red, Style::Bold],
        &[Style::Bold, Style::Cyan],
        &[Style::Bold, Style::Cyan, Style::Underline],
        &[Style::Red, Style::Underline, Style::Italic],
    ];
    let texts = [
        "",
        "x",
        "a b",
        "multi word text",
        "\n",
        "a\nb",
        "a\r\nb",
        "a\nb\nc",
        "a\n",
        "\na",
        "\n\n",
        "a\r\nb\r\nc",
        "a\rb",
        "a\tb",
        "héllo ✓",
        "\x1b",
        "a\x1b[1mb",
        "a\x1b[39mb",
        "a\x1b[22mb",
        "a\x1b[39m\x1b[22mb",
        "\x1b[39m",
        "\x1b[22m",
        "\x1b[0m",
        "a\x1b[33m\nb\x1b[39m",
        "line1\nline2\n",
        "a\r\nb\nc",
    ];
    let levels = [Level::Plain, Level::Basic, Level::Ansi256, Level::Ansi16m];

    let mut cases = Vec::with_capacity(levels.len() * chains.len() * texts.len());
    for level in levels {
        for chain in chains {
            for text in texts {
                cases.push(StyleCase {
                    level,
                    styles: chain.to_vec(),
                    text,
                });
            }
        }
    }
    cases
}

struct Scenario {
    name: &'static str,
    env: &'static [(&'static str, &'static str)],
    flags: &'static [&'static str],
}

fn detection_scenarios() -> Vec<Scenario> {
    vec![
        scenario("bare", &[], &[]),
        scenario("term dumb", &[("TERM", "dumb")], &[]),
        scenario("term dumb forced", &[("TERM", "dumb"), ("FORCE_COLOR", "1")], &[]),
        scenario("term 256", &[("TERM", "xterm-256color")], &[]),
        scenario("term xterm", &[("TERM", "xterm")], &[]),
        scenario("term screen 256", &[("TERM", "screen-256color")], &[]),
        scenario("term screen", &[("TERM", "screen")], &[]),
        scenario("term vt100", &[("TERM", "vt100")], &[]),
        scenario("term linux", &[("TERM", "linux")], &[]),
        scenario("term unknown", &[("TERM", "foo")], &[]),
        scenario("colorterm basic", &[("TERM", "foo"), ("COLORTERM", "1")], &[]),
        scenario(
            "colorterm truecolor",
            &[("TERM", "foo"), ("COLORTERM", "truecolor")],
            &[],
        ),
        scenario("term kitty", &[("TERM", "xterm-kitty")], &[]),
        scenario("term ghostty", &[("TERM", "xterm-ghostty")], &[]),
        scenario("term wezterm", &[("TERM", "wezterm")], &[]),
        scenario(
            "iterm 3",
            &[
                ("TERM", "xterm"),
                ("TERM_PROGRAM", "iTerm.app"),
                ("TERM_PROGRAM_VERSION", "3.4.1"),
            ],
            &[],
        ),
        scenario(
            "iterm 2",
            &[
                ("TERM", "xterm"),
                ("TERM_PROGRAM", "iTerm.app"),
                ("TERM_PROGRAM_VERSION", "2.9"),
            ],
            &[],
        ),
        scenario(
            "iterm unversioned",
            &[("TERM", "xterm"), ("TERM_PROGRAM", "iTerm.app")],
            &[],
        ),
        scenario(
            "apple terminal",
            &[("TERM", "xterm"), ("TERM_PROGRAM", "Apple_Terminal")],
            &[],
        ),
        scenario(
            "other terminal program",
            &[
                ("TERM", "foo"),
                ("TERM_PROGRAM", "WezTerm"),
                ("TERM_PROGRAM_VERSION", "1.0"),
            ],
            &[],
        ),
        scenario("force 1", &[("FORCE_COLOR", "1")], &[]),
        scenario("force 0", &[("FORCE_COLOR", "0")], &[]),
        scenario("force 2", &[("FORCE_COLOR", "2")], &[]),
        scenario("force 3", &[("FORCE_COLOR", "3")], &[]),
        scenario("force 7", &[("FORCE_COLOR", "7")], &[]),
        scenario("force true", &[("FORCE_COLOR", "true")], &[]),
        scenario("force false", &[("FORCE_COLOR", "false")], &[]),
        scenario("force empty", &[("FORCE_COLOR", "")], &[]),
        scenario("force garbage", &[("FORCE_COLOR", "abc")], &[]),
        scenario(
            "force negative",
            &[("FORCE_COLOR", "-1"), ("TERM", "xterm-256color")],
            &[],
        ),
        scenario("no color env", &[("NO_COLOR", "1"), ("TERM", "xterm")], &[]),
        scenario(
            "empty no color env",
            &[("NO_COLOR", ""), ("TERM", "xterm-256color")],
            &[],
        ),
        scenario("flag color", &[], &["--color"]),
        scenario("flag color false", &[], &["--color=false"]),
        scenario("flag color never", &[], &["--color=never"]),
        scenario("flag no color", &[], &["--no-color"]),
        scenario("flag no colors", &[], &["--no-colors"]),
        scenario("flag colors", &[], &["--colors"]),
        scenario("flag color true", &[], &["--color=true"]),
        scenario("flag color always", &[], &["--color=always"]),
        scenario("flag color 256", &[], &["--color=256"]),
        scenario("flag color 16m", &[], &["--color=16m"]),
        scenario("flag color full", &[], &["--color=full"]),
        scenario("flag color truecolor", &[], &["--color=truecolor"]),
        scenario("flag 16m then no color", &[], &["--color=16m", "--no-color"]),
        scenario("flag after terminator", &[], &["--", "--color"]),
        scenario("flag before terminator", &[], &["--color", "--"]),
        scenario("flag no color then 256", &[], &["--no-color", "--color=256"]),
        scenario("force beats no color flag", &[("FORCE_COLOR", "1")], &["--no-color"]),
        scenario("force zero beats 16m flag", &[("FORCE_COLOR", "0")], &["--color=16m"]),
        scenario("ci", &[("CI", "1")], &[]),
        scenario("github actions", &[("CI", "1"), ("GITHUB_ACTIONS", "1")], &[]),
        scenario("gitea actions", &[("CI", "1"), ("GITEA_ACTIONS", "1")], &[]),
        scenario("circleci", &[("CI", "1"), ("CIRCLECI", "1")], &[]),
        scenario("travis", &[("CI", "1"), ("TRAVIS", "1")], &[]),
        scenario("appveyor", &[("CI", "1"), ("APPVEYOR", "1")], &[]),
        scenario("gitlab", &[("CI", "1"), ("GITLAB_CI", "1")], &[]),
        scenario("buildkite", &[("CI", "1"), ("BUILDKITE", "1")], &[]),
        scenario("drone", &[("CI", "1"), ("DRONE", "1")], &[]),
        scenario("codeship", &[("CI", "1"), ("CI_NAME", "codeship")], &[]),
        scenario("other ci name", &[("CI", "1"), ("CI_NAME", "other")], &[]),
        scenario("ci forced 2", &[("CI", "1"), ("FORCE_COLOR", "2")], &[]),
        scenario("ci forced 1", &[("CI", "1"), ("FORCE_COLOR", "1")], &[]),
        scenario("ci term 256", &[("CI", "1"), ("TERM", "xterm-256color")], &[]),
        scenario("teamcity 9.1", &[("TEAMCITY_VERSION", "9.1.0")], &[]),
        scenario("teamcity 9.10", &[("TEAMCITY_VERSION", "9.10.0")], &[]),
        scenario("teamcity 9.0", &[("TEAMCITY_VERSION", "9.0.5")], &[]),
        scenario("teamcity 10", &[("TEAMCITY_VERSION", "10.0.0")], &[]),
        scenario("teamcity 2020", &[("TEAMCITY_VERSION", "2020.1")], &[]),
        scenario("teamcity 8", &[("TEAMCITY_VERSION", "8.3.0")], &[]),
        scenario("azure pipeline", &[("TF_BUILD", "1"), ("AGENT_NAME", "agent")], &[]),
        scenario("azure without agent", &[("TF_BUILD", "1")], &[]),
        scenario(
            "azure with garbage force",
            &[("FORCE_COLOR", "garbage"), ("TF_BUILD", "1"), ("AGENT_NAME", "agent")],
            &[],
        ),
        scenario(
            "dumb with truecolor",
            &[("TERM", "dumb"), ("COLORTERM", "truecolor")],
            &[],
        ),
        scenario(
            "ci on xterm",
            &[("TERM", "xterm"), ("CI", "1"), ("GITHUB_ACTIONS", "1")],
            &[],
        ),
        scenario(
            "no color with forced zero",
            &[("TERM", "xterm-256color"), ("NO_COLOR", "1"), ("FORCE_COLOR", "0")],
            &[],
        ),
    ]
}

fn scenario(
    name: &'static str,
    env: &'static [(&'static str, &'static str)],
    flags: &'static [&'static str],
) -> Scenario {
    Scenario { name, env, flags }
}

fn node_binary() -> PathBuf {
    let path = std::env::var_os("PATH").expect("PATH must locate the node binary");
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join("node");
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("node must be installed to run the supports-color oracle");
}

fn source_worktree() -> PathBuf {
    let lock: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/porting/source-lock.json"
    )))
    .expect("source lock must be valid JSON");
    let worktree = lock["worktree"]
        .as_str()
        .map(PathBuf::from)
        .expect("source lock worktree must be a string");
    let commit = lock["commit"].as_str().expect("source lock commit must be a string");
    let output = Command::new("git")
        .arg("-C")
        .arg(&worktree)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git must verify the pinned TypeScript source HEAD");
    assert!(output.status.success(), "git rev-parse failed");
    let actual = String::from_utf8(output.stdout)
        .expect("git HEAD must be UTF-8")
        .trim()
        .to_owned();
    assert_eq!(actual, commit, "TypeScript source HEAD must equal the lock");
    worktree
}
