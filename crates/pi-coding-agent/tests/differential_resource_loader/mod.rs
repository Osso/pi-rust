use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use std::fs;
use std::path::Path;

use pi_coding_agent::core::resource_loader::{load_rules_files_from_dir, load_rules_from_dir};
use serde_json::{Value, json};
use tempfile::TempDir;

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-resource-loader-oracle.ts",
        "packages/coding-agent/src/core/resource-loader.ts",
    );

    let temp = TempDir::new().expect("temp dir");
    write_rules_fixture(temp.path());
    let missing = temp.path().join("missing").to_string_lossy().into_owned();

    for dir in [temp.path().to_string_lossy().into_owned(), missing] {
        let response = oracle.request(&json!({"id": 1, "operation": "rules", "dir": &dir}));
        assert_eq!(response["ok"], true, "oracle failed: {response}");

        let files = load_rules_files_from_dir(&JsString::from(&dir)).expect("rules files load");
        let actual_files: Vec<Value> = files
            .iter()
            .map(|file| json!({"path": file.path, "content": file.content}))
            .collect();
        assert_eq!(Value::Array(actual_files), response["output"]["files"]);

        let content = load_rules_from_dir(&JsString::from(&dir)).expect("rules content load");
        assert_eq!(json!(content), response["output"]["content"]);
    }
}

fn write_rules_fixture(root: &Path) {
    fs::write(root.join("b-rule.md"), "Second rule.\n\n").unwrap();
    fs::write(root.join("a-rule.md"), "  First rule.  \n").unwrap();
    fs::write(root.join("empty.md"), "   \n\n").unwrap();
    fs::write(root.join("notes.txt"), "ignored").unwrap();
    fs::write(root.join("éclair.md"), "Accented rule.").unwrap();
    fs::write(root.join("Z-rule.md"), "Upper rule.").unwrap();
}

use pi_coding_agent::core::resource_loader::load_project_context_files;
use std::os::unix::fs::symlink;

pub(crate) fn run_context_files() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-resource-loader-oracle.ts",
        "packages/coding-agent/src/core/resource-loader.ts",
    );
    for (id, fixture) in (1..).zip(context_fixtures()) {
        let response = oracle.request(&json!({
            "id": id,
            "operation": "context_files",
            "cwd": fixture.cwd,
            "agentDir": fixture.agent_dir,
        }));
        assert_eq!(response["ok"], true, "oracle failed: {response}");
        let actual: Vec<Value> =
            load_project_context_files(&JsString::from(&fixture.cwd), &JsString::from(&fixture.agent_dir))
                .unwrap()
                .into_iter()
                .map(|file| json!({"path": file.path, "content": file.content}))
                .collect();
        assert_eq!(
            Value::Array(actual),
            response["output"]["files"],
            "context files mismatch for {}",
            fixture.name
        );
    }
}

struct ContextFixture {
    name: &'static str,
    cwd: String,
    agent_dir: String,
}

fn context_fixtures() -> Vec<ContextFixture> {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path();
    let agent_dir = root.join("agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let mut fixtures = Vec::new();

    // Layered instructions with memory and override files.
    let layered = root.join("layered");
    let layered_nested = layered.join("service");
    fs::create_dir_all(&layered_nested).unwrap();
    fs::create_dir_all(layered.join("docs/local")).unwrap();
    fs::create_dir_all(agent_dir.join("docs/local")).unwrap();
    fs::write(agent_dir.join("AGENTS.md"), "global instructions").unwrap();
    fs::write(agent_dir.join("AGENTS.override.md"), "global override").unwrap();
    fs::write(agent_dir.join("docs/local/memory.md"), "global memory").unwrap();
    fs::write(layered.join("AGENTS.md"), "project instructions").unwrap();
    fs::write(layered.join("docs/local/memory.md"), "project memory").unwrap();
    fs::write(layered_nested.join("AGENTS.md"), "service instructions").unwrap();
    fs::write(layered_nested.join("AGENTS.override.md"), "service override").unwrap();
    fixtures.push(ContextFixture {
        name: "layered",
        cwd: layered_nested.to_string_lossy().into_owned(),
        agent_dir: agent_dir.to_string_lossy().into_owned(),
    });

    // CLAUDE fallback with directory candidates.
    let claude = root.join("claude");
    fs::create_dir_all(claude.join("AGENTS.md")).unwrap();
    fs::create_dir_all(claude.join("AGENTS.override.md")).unwrap();
    fs::write(claude.join("CLAUDE.md"), "fallback instructions").unwrap();
    fs::write(claude.join("CLAUDE.local.md"), "fallback local").unwrap();
    fixtures.push(ContextFixture {
        name: "claude fallback",
        cwd: claude.to_string_lossy().into_owned(),
        agent_dir: agent_dir.to_string_lossy().into_owned(),
    });

    // Symlinked global memory and duplicated filename variants.
    let symlinked = root.join("symlinked");
    fs::create_dir_all(symlinked.join("docs/local")).unwrap();
    fs::write(agent_dir.join("docs/local/memory.md"), "instruction content").unwrap();
    symlink(
        agent_dir.join("docs/local/memory.md"),
        symlinked.join("docs/local/memory.md"),
    )
    .unwrap();
    fs::write(symlinked.join("AGENTS.md"), "shared context").unwrap();
    symlink(symlinked.join("AGENTS.md"), symlinked.join("AGENTS.MD")).unwrap();
    fixtures.push(ContextFixture {
        name: "symlinked memory",
        cwd: symlinked.to_string_lossy().into_owned(),
        agent_dir: agent_dir.to_string_lossy().into_owned(),
    });

    // Nested linked worktree.
    let outer = root.join("outer");
    let main = outer.join("main");
    let worktree = main.join("worktrees/feat");
    let worktree_src = worktree.join("src");
    fs::create_dir_all(&worktree_src).unwrap();
    let git_dir = main.join(".git/worktrees/feat");
    fs::create_dir_all(&git_dir).unwrap();
    fs::write(main.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/feat\n").unwrap();
    fs::write(git_dir.join("commondir"), "../..").unwrap();
    fs::write(worktree.join(".git"), format!("gitdir: {}\n", git_dir.display())).unwrap();
    fs::write(outer.join("AGENTS.md"), "outer instructions").unwrap();
    fs::write(main.join("AGENTS.md"), "main repo instructions").unwrap();
    fs::write(worktree.join("AGENTS.md"), "worktree instructions").unwrap();
    fixtures.push(ContextFixture {
        name: "nested worktree",
        cwd: worktree_src.to_string_lossy().into_owned(),
        agent_dir: agent_dir.to_string_lossy().into_owned(),
    });

    // Bare layout container.
    let proj = root.join("proj");
    let bare = proj.join(".bare");
    let bare_worktree = proj.join("main");
    let bare_git_dir = bare.join("worktrees/main");
    fs::create_dir_all(&bare_git_dir).unwrap();
    fs::create_dir_all(&bare_worktree).unwrap();
    fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(bare_git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(bare_git_dir.join("commondir"), "../..").unwrap();
    fs::write(
        bare_worktree.join(".git"),
        format!("gitdir: {}\n", bare_git_dir.display()),
    )
    .unwrap();
    fs::write(proj.join("AGENTS.md"), "container instructions").unwrap();
    fs::write(bare_worktree.join("AGENTS.md"), "worktree instructions").unwrap();
    fixtures.push(ContextFixture {
        name: "bare layout",
        cwd: bare_worktree.to_string_lossy().into_owned(),
        agent_dir: agent_dir.to_string_lossy().into_owned(),
    });

    // Sibling worktree.
    let sibling_outer = root.join("sibling-outer");
    let sibling_main = sibling_outer.join("main");
    let sibling = sibling_outer.join("sib-feat");
    let sibling_src = sibling.join("src");
    fs::create_dir_all(&sibling_src).unwrap();
    fs::create_dir_all(&sibling_main).unwrap();
    fs::write(sibling_outer.join("AGENTS.md"), "outer instructions").unwrap();
    fs::write(sibling.join("AGENTS.md"), "sibling worktree instructions").unwrap();
    let sibling_git_dir = sibling_main.join(".git/worktrees/sib");
    fs::create_dir_all(&sibling_git_dir).unwrap();
    fs::write(sibling_main.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(sibling_git_dir.join("HEAD"), "ref: refs/heads/feat\n").unwrap();
    fs::write(sibling_git_dir.join("commondir"), "../..").unwrap();
    fs::write(sibling.join(".git"), format!("gitdir: {}\n", sibling_git_dir.display())).unwrap();
    fixtures.push(ContextFixture {
        name: "sibling worktree",
        cwd: sibling_src.to_string_lossy().into_owned(),
        agent_dir: agent_dir.to_string_lossy().into_owned(),
    });

    // Submodule.
    let sup = root.join("super");
    let sub = sup.join("vendor/lib");
    let sub_src = sub.join("src");
    fs::create_dir_all(&sub_src).unwrap();
    fs::write(sup.join("AGENTS.md"), "superproject instructions").unwrap();
    fs::write(sub.join("AGENTS.md"), "submodule instructions").unwrap();
    let sub_git_dir = sup.join(".git/modules/vendor/lib");
    fs::create_dir_all(&sub_git_dir).unwrap();
    fs::write(sub_git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(sub.join(".git"), format!("gitdir: {}\n", sub_git_dir.display())).unwrap();
    fixtures.push(ContextFixture {
        name: "submodule",
        cwd: sub_src.to_string_lossy().into_owned(),
        agent_dir: agent_dir.to_string_lossy().into_owned(),
    });

    // Keep the temp dir alive for the oracle by leaking it for the test process.
    std::mem::forget(temp);
    fixtures
}
