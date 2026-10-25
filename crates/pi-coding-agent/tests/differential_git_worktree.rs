use crate::shared_oracle as oracle;

use std::collections::VecDeque;
use std::sync::Mutex;

use pi_coding_agent::utils::git_worktree::{GitExec, GitExecError, GitOutput, resolve_worktree_with_exec};
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

const REQUEST_ID: u64 = 26_011;

#[derive(Clone)]
enum Step {
    Output {
        stdout: &'static str,
    },
    Reject {
        message: &'static str,
        stderr: Option<&'static str>,
        stdout: Option<&'static str>,
    },
}

struct Case {
    name: &'static str,
    cwd: &'static str,
    worktree_name: &'static str,
    steps: Vec<Step>,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "reuse existing",
            cwd: "/repo/project",
            worktree_name: "feature",
            steps: vec![
                Step::Output {
                    stdout: "/repo/project\n",
                },
                Step::Output {
                    stdout: "worktree /repo/project\nHEAD abc\n\nworktree /repo/project-feature\nHEAD def\n",
                },
            ],
        },
        Case {
            name: "filesystem root repository concatenates paths",
            cwd: "/",
            worktree_name: "feature",
            steps: vec![
                Step::Output { stdout: "/\n" },
                Step::Output {
                    stdout: "worktree /\nHEAD abc\n\nworktree //-feature\nHEAD def\n",
                },
            ],
        },
        Case {
            name: "create from main",
            cwd: "/repo/project/subdir",
            worktree_name: "feature",
            steps: vec![
                Step::Output {
                    stdout: "/repo/project\n",
                },
                Step::Output {
                    stdout: "worktree /repo/project\nHEAD abc\n",
                },
                Step::Output {
                    stdout: "origin/main\n",
                },
                Step::Output { stdout: "" },
            ],
        },
        Case {
            name: "fallback master",
            cwd: "/repo/project",
            worktree_name: "feature",
            steps: vec![
                Step::Output {
                    stdout: "/repo/project\n",
                },
                Step::Output {
                    stdout: "worktree /repo/project\nHEAD abc\n",
                },
                Step::Reject {
                    message: "missing main",
                    stderr: None,
                    stdout: None,
                },
                Step::Output {
                    stdout: "origin/master\n",
                },
                Step::Output { stdout: "" },
            ],
        },
        Case {
            name: "no base",
            cwd: "/repo/project",
            worktree_name: "feature",
            steps: vec![
                Step::Output {
                    stdout: "/repo/project\n",
                },
                Step::Output {
                    stdout: "worktree /repo/project\nHEAD abc\n",
                },
                Step::Reject {
                    message: "missing main",
                    stderr: None,
                    stdout: None,
                },
                Step::Reject {
                    message: "missing master",
                    stderr: None,
                    stdout: None,
                },
            ],
        },
        Case {
            name: "not a repository",
            cwd: "/repo/project",
            worktree_name: "feature",
            steps: vec![Step::Reject {
                message: "Command failed: git rev-parse --show-toplevel",
                stderr: Some("fatal: not a git repository"),
                stdout: None,
            }],
        },
        Case {
            name: "creation failure with stderr",
            cwd: "/repo/project",
            worktree_name: "feature",
            steps: vec![
                Step::Output {
                    stdout: "/repo/project\n",
                },
                Step::Output {
                    stdout: "worktree /repo/project\nHEAD abc\n",
                },
                Step::Output {
                    stdout: "origin/main\n",
                },
                Step::Reject {
                    message: "Command failed: git worktree add",
                    stderr: Some("fatal: 'feature' is already checked out"),
                    stdout: None,
                },
            ],
        },
        Case {
            name: "creation failure with stdout only",
            cwd: "/repo/project",
            worktree_name: "feature",
            steps: vec![
                Step::Output {
                    stdout: "/repo/project\n",
                },
                Step::Output {
                    stdout: "worktree /repo/project\nHEAD abc\n",
                },
                Step::Output {
                    stdout: "origin/main\n",
                },
                Step::Reject {
                    message: "plain",
                    stderr: None,
                    stdout: Some("stdout detail"),
                },
            ],
        },
        Case {
            name: "creation failure with message only",
            cwd: "/repo/project",
            worktree_name: "feature",
            steps: vec![
                Step::Output {
                    stdout: "/repo/project\n",
                },
                Step::Output {
                    stdout: "worktree /repo/project\nHEAD abc\n",
                },
                Step::Output {
                    stdout: "origin/main\n",
                },
                Step::Reject {
                    message: "plain message",
                    stderr: None,
                    stdout: None,
                },
            ],
        },
        Case {
            name: "crlf worktree list",
            cwd: "/repo/project",
            worktree_name: "feature",
            steps: vec![
                Step::Output {
                    stdout: "/repo/project\n",
                },
                Step::Output {
                    stdout: "worktree /repo/project\r\nHEAD abc\r\n\r\nworktree /repo/project-feature\r\n",
                },
            ],
        },
        Case {
            name: "worktree name with spaces",
            cwd: "/repo/project",
            worktree_name: "feature x",
            steps: vec![
                Step::Output {
                    stdout: "/repo/project\n",
                },
                Step::Output {
                    stdout: "worktree /repo/project-feature x\nHEAD def\n",
                },
            ],
        },
    ]
}

struct RecordingExec {
    steps: Mutex<VecDeque<Step>>,
    calls: Mutex<Vec<(Vec<String>, String)>>,
}

impl RecordingExec {
    fn new(steps: &[Step]) -> Self {
        Self {
            steps: Mutex::new(steps.to_vec().into()),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls_json(&self) -> Value {
        json!(
            self.calls
                .lock()
                .expect("calls lock")
                .iter()
                .map(|(args, cwd)| json!({"args": args, "cwd": cwd}))
                .collect::<Vec<_>>()
        )
    }
}

impl GitExec for RecordingExec {
    fn exec(&self, args: &[JsString], cwd: &JsString) -> Result<GitOutput, GitExecError> {
        self.calls.lock().expect("calls lock").push((
            args.iter().map(|arg| arg.to_utf8().expect("scalar arg")).collect(),
            cwd.to_utf8().expect("scalar cwd"),
        ));
        let step = self
            .steps
            .lock()
            .expect("steps lock")
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected git call: {args:?}"));
        match step {
            Step::Output { stdout } => Ok(GitOutput {
                stdout: stdout.as_bytes().to_vec(),
                stderr: Vec::new(),
            }),
            Step::Reject {
                message,
                stderr,
                stdout,
            } => Err(GitExecError {
                message: message.into(),
                stderr: stderr.map(JsString::from),
                stdout: stdout.map(JsString::from),
            }),
        }
    }
}

#[test]
fn coding_agent_git_worktree_matches_the_pinned_typescript_source() {
    let cases = cases();
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-git-worktree-oracle.ts",
        "packages/coding-agent/src/utils/git-worktree.ts",
    );
    let expected = oracle.request(&request(&cases));
    let actual = response(&cases);

    let actual_results = actual["output"]["results"].as_array().expect("actual results");
    let expected_results = expected["output"]["results"].as_array().expect("expected results");
    assert_eq!(actual_results.len(), expected_results.len());
    for (actual_case, expected_case) in actual_results.iter().zip(expected_results) {
        assert_eq!(
            actual_case, expected_case,
            "git-worktree mismatch: {}",
            expected_case["name"]
        );
    }
}

fn request(cases: &[Case]) -> Value {
    let cases = cases
        .iter()
        .map(|case| {
            let results = case
                .steps
                .iter()
                .map(|step| match step {
                    Step::Output { stdout } => json!({"stdout": stdout}),
                    Step::Reject {
                        message,
                        stderr,
                        stdout,
                    } => {
                        let mut reject = serde_json::Map::new();
                        reject.insert("message".to_owned(), json!(message));
                        if let Some(stderr) = stderr {
                            reject.insert("stderr".to_owned(), json!(stderr));
                        }
                        if let Some(stdout) = stdout {
                            reject.insert("stdout".to_owned(), json!(stdout));
                        }
                        json!({"reject": Value::Object(reject)})
                    }
                })
                .collect::<Vec<_>>();
            json!({
                "name": case.name,
                "cwd": case.cwd,
                "worktreeName": case.worktree_name,
                "results": results,
            })
        })
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "operation": "coding_agent_git_worktree", "cases": cases})
}

fn response(cases: &[Case]) -> Value {
    let results = cases
        .iter()
        .map(|case| {
            let exec = RecordingExec::new(&case.steps);
            let name = case.name;
            match resolve_worktree_with_exec(&case.worktree_name.into(), &case.cwd.into(), &exec) {
                Ok(worktree) => json!({
                    "name": name,
                    "ok": true,
                    "value": worktree.to_utf8().expect("scalar worktree"),
                    "calls": exec.calls_json(),
                }),
                Err(error) => json!({
                    "name": name,
                    "ok": false,
                    "message": error.to_string(),
                    "calls": exec.calls_json(),
                }),
            }
        })
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "ok": true, "output": {"results": results}})
}
