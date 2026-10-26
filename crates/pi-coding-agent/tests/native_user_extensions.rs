use crate::shared_extensions as support;
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::{js_json, js_json::JsValue};
use serde_json::json;
use std::sync::{Arc, Mutex};

struct RegexFixture {
    temp: tempfile::TempDir,
    tool: ToolDefinition,
    context: ExtensionContext,
}

impl RegexFixture {
    async fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("extensions")).unwrap();
        std::fs::create_dir_all(temp.path().join("target/release")).unwrap();
        std::fs::write(temp.path().join("package.json"), r#"{"name":"regex-replace-pi-tool"}"#).unwrap();
        std::fs::write(temp.path().join("extensions/regex-replace.ts"), "").unwrap();
        std::fs::write(temp.path().join("scenario.json"), "{}").unwrap();
        let binary = temp.path().join("target/release/regex-replace-json");
        std::fs::write(&binary, r#"#!/usr/bin/python3
import json, os, pathlib, sys, time
root = pathlib.Path(__file__).resolve().parents[2]
request = json.loads(pathlib.Path(sys.argv[1]).read_text())
with (root / 'requests.jsonl').open('a') as log:
    log.write(json.dumps({'request': request, 'cwd': os.getcwd(), 'requestPath': sys.argv[1]}) + '\n')
scenario = json.loads((root / 'scenario.json').read_text())
if scenario.get('sleepPlan') and request['action'] == 'plan':
    (root / 'planning').write_text('started')
    time.sleep(scenario['sleepPlan'])
if 'errorCode' in scenario:
    sys.stderr.write(scenario.get('stderr', ''))
    sys.exit(scenario['errorCode'])
if scenario.get('malformed'):
    print('{')
    sys.exit(0)
if scenario.get('invalid'):
    print('{}')
    sys.exit(0)
if request['action'] == 'apply':
    (root / 'applying').write_text('started')
    time.sleep(scenario.get('sleepApply', 0))
    (root / 'applied').write_text('done')
paths = [str(root / name) for name in ['z.txt', 'a.txt', 'z.txt']]
changes = [{'absolutePath': path, 'path': pathlib.Path(path).name, 'originalHash': 'original', 'replacements': 1, 'diff': scenario.get('diff', '-old\n+new'), 'lineChanges': []} for path in paths]
print(json.dumps({'dryRun': request['action'] == 'plan', 'planHash': 'plan-123', 'matchedFiles': 2, 'totalReplacements': 3, 'filesModified': 2, 'changes': changes}))
"#).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cwd = temp.path().to_str().unwrap();
        let mut registry = ExtensionRegistry::new();
        pi_coding_agent_extensions::native_user_extensions::register(&mut registry);
        let runtime = support::runtime();
        let result = registry
            .load_extensions(
                &[JsString::from("extensions/regex-replace.ts")],
                &JsString::from(cwd),
                None,
                Some(runtime.clone()),
            )
            .await
            .unwrap();
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        assert_eq!(result.extensions.len(), 1);
        assert_eq!(result.extensions[0].read().path, "extensions/regex-replace.ts");
        let tool = result.extensions[0].read().tools["regex_replace"].definition.clone();
        let runner = support::runner_for(result.extensions, cwd, runtime, support::model_registry().await);
        Self {
            temp,
            tool,
            context: runner.create_context(),
        }
    }

    fn params(&self, dry_run: bool) -> JsValue {
        js_json!({"files":"@*.txt", "pattern":"old", "replacement":"new", "expectedMatches":3, "dryRun":dry_run})
    }

    fn requests(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.temp.path().join("requests.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn scenario(&self, value: serde_json::Value) {
        std::fs::write(
            self.temp.path().join("scenario.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }

    async fn execute(
        &self,
        params: JsValue,
        signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> Result<pi_agent_core::AgentToolResult, HandlerError> {
        (self.tool.execute)("test-call".to_owned(), params, signal, None, Some(self.context.clone())).await
    }
}

#[tokio::test]
async fn native_regex_replace_plans_applies_and_cleans_request_files() {
    let fixture = RegexFixture::new().await;
    let result = fixture.execute(fixture.params(false), None).await.unwrap();
    let requests = fixture.requests();
    assert_eq!(requests.len(), 2);
    let root = fixture.temp.path().to_str().unwrap();
    assert_eq!(
        requests[0]["request"],
        json!({"action":"plan","cwd":root,"files":"*.txt","pattern":"old","replacement":"new","expectedMatches":3,"maxFiles":100,"maxTotalBytes":10485760,"maxMatches":10000})
    );
    let mut apply = requests[0]["request"].clone();
    apply["action"] = json!("apply");
    apply["planHash"] = json!("plan-123");
    apply["targets"] = json!([fixture.temp.path().join("a.txt"), fixture.temp.path().join("z.txt")]);
    assert_eq!(requests[1]["request"], apply);
    for request in requests {
        assert_eq!(request["cwd"], root);
        assert!(!std::path::Path::new(request["requestPath"].as_str().unwrap()).exists());
    }
    assert!(fixture.temp.path().join("applied").exists());
    let pi_ai::types::TextOrImageContent::Text(text) = &result.content[0] else {
        panic!("text result")
    };
    assert_eq!(
        text.text,
        "Applied 3 replacements in 2 files. Plan: plan-123\n\n-old\n+new\n-old\n+new\n-old\n+new"
    );
    assert_eq!(
        result.details,
        Some(
            js_json!({"dryRun":false,"totalReplacements":3,"filesModified":2,"diff":"-old\n+new\n-old\n+new\n-old\n+new"})
        )
    );
}

#[tokio::test]
async fn native_regex_replace_dry_run_and_explicit_limits() {
    let fixture = RegexFixture::new().await;
    fixture.execute(js_json!({"files":"@@*.txt","pattern":"old","replacement":"$1","expectedMatches":3,"dryRun":true,"maxFiles":4,"maxTotalBytes":99,"maxMatches":7}), None).await.unwrap();
    let requests = fixture.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]["request"],
        json!({"action":"plan","cwd":fixture.temp.path(),"files":"@*.txt","pattern":"old","replacement":"$1","expectedMatches":3,"maxFiles":4,"maxTotalBytes":99,"maxMatches":7})
    );
    assert!(!fixture.temp.path().join("applied").exists());
}

#[tokio::test]
async fn native_regex_replace_errors_and_precancelled_signal() {
    let fixture = RegexFixture::new().await;
    let signal = pi_js_compat::abort::AbortSignal::abort(None);
    let error = fixture.execute(fixture.params(false), Some(signal)).await.unwrap_err();
    assert_eq!(error.caught_message(), "Regex replacement cancelled before planning");
    assert!(!fixture.temp.path().join("requests.jsonl").exists());
    for (scenario, expected) in [
        (json!({"errorCode":9,"stderr":"  rejected\n"}), "rejected"),
        (json!({"errorCode":7}), "regex-replace-json exited with code 7"),
        (json!({"invalid":true}), "regex-replace-json returned an invalid result"),
    ] {
        fixture.scenario(scenario);
        let error = fixture.execute(fixture.params(true), None).await.unwrap_err();
        assert_eq!(error.caught_message(), expected);
    }
    for request in fixture.requests() {
        assert!(!std::path::Path::new(request["requestPath"].as_str().unwrap()).exists());
    }
}

#[tokio::test]
async fn native_regex_replace_planning_abort_and_malformed_json_fail_without_apply() {
    use pi_js_compat::abort::AbortController;
    use std::time::Duration;
    let fixture = RegexFixture::new().await;
    fixture.scenario(json!({"sleepPlan":5}));
    let controller = AbortController::new();
    let operation = fixture.execute(fixture.params(false), Some(controller.signal()));
    let abort = async {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !fixture.temp.path().join("planning").exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        controller.abort(None);
    };
    let (result, ()) = tokio::join!(operation, abort);
    assert!(result.is_err());
    assert!(!fixture.temp.path().join("applying").exists());
    fixture.scenario(json!({"malformed":true}));
    let error = fixture.execute(fixture.params(true), None).await.unwrap_err();
    assert!(pi_js_compat::js_json::encode_node_utf8(&error.caught_message()).contains("JSON"));
    for request in fixture.requests() {
        assert!(!std::path::Path::new(request["requestPath"].as_str().unwrap()).exists());
    }
}

#[tokio::test]
async fn native_regex_replace_renderers_keep_call_summary_diff_and_error_text() {
    use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, ThemeColor};
    use pi_coding_agent::utils::ansi::strip_ansi;
    let fixture = RegexFixture::new().await;
    let colors = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                JsString::from(*key),
                ColorValue::Text(format!("#{:06x}", index * 100).into()),
            )
        })
        .collect::<Vec<_>>();
    let theme = Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).unwrap();
    let context = ToolRenderContext {
        args: fixture.params(true),
        tool_call_id: "render".to_owned(),
        invalidate: Arc::new(|| {}),
        last_component: None,
        call_component: None,
        state: ToolRenderState::new(),
        cwd: JsString::from(fixture.temp.path().to_str().unwrap()),
        execution_started: false,
        args_complete: true,
        is_partial: false,
        expanded: true,
        show_images: false,
        is_error: false,
    };
    let call = fixture.tool.render_call.as_ref().unwrap();
    for (dry_run, mode) in [(true, "preview"), (false, "apply")] {
        let component = call(&fixture.params(dry_run), &theme, &context).unwrap();
        let lines = component.borrow_mut().render(200);
        assert_eq!(
            strip_ansi(&lines[0]).trim_end(),
            format!("regex_replace @*.txt /old/ expected=3 {mode}")
        );
    }
    let render = fixture.tool.render_result.as_ref().unwrap();
    let mut result = pi_agent_core::AgentToolResult {
        content: Vec::new(),
        details: Some(
            js_json!({"dryRun":false,"totalReplacements":1,"filesModified":1,"diff":"--- old\n+++ new\n-old\n+new\n context"}),
        ),
        is_error: None,
        terminate: None,
    };
    let options = ToolRenderResultOptions {
        expanded: true,
        is_partial: false,
    };
    let component = render(&result, &options, &theme, &context).unwrap();
    let lines = component.borrow_mut().render(200);
    let plain = lines
        .iter()
        .map(|line| strip_ansi(line).trim_end().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        plain,
        [
            "Applied 1 replacement in 1 file",
            "--- old",
            "+++ new",
            "-old",
            "+new",
            " context"
        ]
    );
    assert!(lines[1].contains(theme.fg(ThemeColor::Dim, "--- old")));
    assert!(lines[3].contains(theme.fg(ThemeColor::Error, "-old")));
    assert!(lines[4].contains(theme.fg(ThemeColor::Success, "+new")));
    let component = render(
        &result,
        &ToolRenderResultOptions {
            expanded: false,
            is_partial: false,
        },
        &theme,
        &context,
    )
    .unwrap();
    assert_eq!(component.borrow_mut().render(200).len(), 1);
    result.details = None;
    for (partial, expected) in [
        (false, "Invalid replacement details"),
        (true, "Planning regex replacement..."),
    ] {
        let component = render(
            &result,
            &ToolRenderResultOptions {
                expanded: true,
                is_partial: partial,
            },
            &theme,
            &context,
        )
        .unwrap();
        assert_eq!(strip_ansi(&component.borrow_mut().render(200)[0]).trim_end(), expected);
    }
}

#[tokio::test]
async fn native_regex_replace_schema_and_prompt_contract() {
    let fixture = RegexFixture::new().await;
    assert_eq!(fixture.tool.name, "regex_replace");
    assert_eq!(fixture.tool.label, "Regex Replace");
    assert_eq!(
        fixture.tool.description,
        "Safely replace a Rust regex across gitignore-aware files with an exact expected match count, dry-run diffs, stale-plan protection, and bounded scope."
    );
    assert_eq!(fixture.tool.approval_required, Some(true));
    assert_eq!(
        fixture.tool.prompt_snippet,
        Some(JsString::from(
            "Regex replace across files with exact expected match counts and unified diffs"
        ))
    );
    assert_eq!(fixture.tool.prompt_guidelines, Some(vec![
        "Use regex_replace for multi-file regex changes; use edit for exact single-file text replacements.".into(),
        "Call regex_replace with dryRun=true first when the expected match count is uncertain.".into(),
        "Do not pass maxFiles, maxTotalBytes, or maxMatches unless the user explicitly requests a non-default limit.".into(),
    ]));
    assert_eq!(
        fixture.tool.parameters,
        json!({
            "type":"object", "additionalProperties":false, "required":["files","pattern","replacement","expectedMatches"],
            "properties": {
                "files":{"type":"string","description":"Gitignore-aware glob relative to the current working directory"},
                "pattern":{"type":"string","description":"Pattern using Rust regex syntax"},
                "replacement":{"type":"string","description":"Replacement using $1, $2, and $0 capture references"},
                "expectedMatches":{"type":"integer","minimum":1,"description":"Exact number of matches required"},
                "dryRun":{"type":"boolean","description":"Preview without writing (default: false)"},
                "maxFiles":{"type":"integer","minimum":1,"description":"Maximum matched files (default: 100)"},
                "maxTotalBytes":{"type":"integer","minimum":1,"description":"Maximum total input bytes (default: 10 MiB)"},
                "maxMatches":{"type":"integer","minimum":1,"description":"Maximum replacements (default: 10000)"}
            }
        })
    );
}

#[tokio::test]
async fn native_regex_replace_bounds_output_and_render_details() {
    let fixture = RegexFixture::new().await;
    let diff = "line \"quote\" and unicode é\n".repeat(1500);
    fixture.scenario(json!({"diff":diff}));
    let result = fixture.execute(fixture.params(true), None).await.unwrap();
    let pi_ai::types::TextOrImageContent::Text(text) = &result.content[0] else {
        panic!("text result")
    };
    let text = pi_js_compat::js_json::encode_node_utf8(&text.text);
    assert!(text.starts_with("Previewed 3 replacements in 2 files. Plan: plan-123\n\nline \"quote\" and unicode é\n"));
    assert!(text.ends_with("]"));
    assert!(text.contains("[Diff truncated: "));
    assert!(text.len() < 8500);
    let details = result.details.unwrap();
    assert!(pi_js_compat::js_json::stringify_lossless(&details).len() <= 8192);
    assert!(
        details["diff"]
            .as_string()
            .unwrap()
            .ends_with("\n[Diff truncated in tool details.]")
    );
}

#[tokio::test]
async fn native_regex_replace_file_queue_and_operation_mutex_preserve_order() {
    use pi_coding_agent::core::tools::file_mutation_queue::with_file_mutation_queue;
    use std::time::Duration;
    use tokio::sync::oneshot;
    let fixture = RegexFixture::new().await;
    let target = JsString::from(fixture.temp.path().join("a.txt").to_str().unwrap());
    let (held_tx, held_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let holder = tokio::spawn(async move {
        with_file_mutation_queue(&target, || async move {
            held_tx.send(()).unwrap();
            release_rx.await.unwrap();
        })
        .await
        .unwrap();
    });
    held_rx.await.unwrap();
    let tool = fixture.tool.clone();
    let context = fixture.context.clone();
    let params = fixture.params(false);
    let first =
        tokio::spawn(async move { (tool.execute)("first".to_owned(), params, None, None, Some(context)).await });
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if fixture.temp.path().join("requests.jsonl").exists() && fixture.requests().len() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let second = fixture.execute(fixture.params(true), None);
    tokio::pin!(second);
    tokio::select! {
        result = &mut second => panic!("second operation ran while first waits on file queue: {result:?}"),
        _ = tokio::time::sleep(Duration::from_millis(50)) => {}
    }
    assert!(
        !fixture.temp.path().join("applying").exists(),
        "apply must wait for existing file mutations"
    );
    release_tx.send(()).unwrap();
    holder.await.unwrap();
    first.await.unwrap().unwrap();
    second.await.unwrap();
    let actions: Vec<_> = fixture
        .requests()
        .iter()
        .map(|request| request["request"]["action"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actions, ["plan", "apply", "plan"]);
}

#[tokio::test]
async fn native_regex_replace_apply_ignores_late_abort() {
    use pi_js_compat::abort::AbortController;
    use std::time::Duration;
    let fixture = RegexFixture::new().await;
    fixture.scenario(json!({"sleepApply":0.1}));
    let controller = AbortController::new();
    let operation = fixture.execute(fixture.params(false), Some(controller.signal()));
    tokio::pin!(operation);
    let abort = async {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !fixture.temp.path().join("applying").exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        controller.abort(None);
    };
    let (result, ()) = tokio::join!(&mut operation, abort);
    result.unwrap();
    assert!(fixture.temp.path().join("applied").exists());
}

#[tokio::test]
async fn native_disable_broadcast_filters_only_broadcast_on_session_start() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let path = JsString::from(temp.path().join("disable-broadcast.ts").to_str().unwrap());
    std::fs::write(temp.path().join("disable-broadcast.ts"), "").unwrap();
    let mut registry = ExtensionRegistry::new();
    pi_coding_agent_extensions::native_user_extensions::register(&mut registry);
    let runtime = support::runtime();
    let active = Arc::new(Mutex::new(vec![
        "read".to_owned(),
        "broadcast".to_owned(),
        "broadcasting".to_owned(),
        "broadcast".to_owned(),
        "edit".to_owned(),
    ]));
    let mut actions = support::actions();
    let getter = Arc::clone(&active);
    actions.get_active_tools = Arc::new(move || Ok(getter.lock().unwrap().clone()));
    let setter = Arc::clone(&active);
    actions.set_active_tools = Arc::new(move |tools| {
        *setter.lock().unwrap() = tools.to_vec();
        Ok(())
    });
    runtime.set_actions(actions);
    let result = registry
        .load_extensions(
            std::slice::from_ref(&path),
            &JsString::from(cwd),
            None,
            Some(runtime.clone()),
        )
        .await
        .unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.extensions.len(), 1);
    assert_eq!(active.lock().unwrap().len(), 5, "loading must not change active tools");
    let runner = support::runner_for(result.extensions, cwd, runtime, support::model_registry().await);
    let mut event = ExtensionEvent::SessionStart(SessionStartEvent {
        reason: SessionStartReason::Startup,
        previous_session_file: None,
    });
    runner.emit(&mut event).await;
    assert_eq!(*active.lock().unwrap(), ["read", "broadcasting", "edit"]);
    *active.lock().unwrap() = vec!["broadcast".to_owned(), "write".to_owned()];
    runner.emit(&mut event).await;
    assert_eq!(*active.lock().unwrap(), ["write"]);
}
