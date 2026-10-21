#[path = "support/oracle.rs"]
mod oracle;
use serde_json::{Value, json};
fn setup(root: &std::path::Path, role: &str) {
    let directory = root.join(format!("{role}-sessions"));
    std::fs::create_dir_all(&directory).unwrap();
    for name in [
        format!("{role}.jsonl"),
        format!("2026-01-01_{role}.jsonl"),
        format!("2026-02-01_{role}.jsonl"),
    ] {
        std::fs::write(
            directory.join(name),
            format!(
                "{}\n",
                json!({"type":"session","version":3,"id":role,"timestamp":"2026-01-01T00:00:00.000Z","cwd":"/fixture"})
            ),
        )
        .unwrap();
    }
}
fn output(root: &std::path::Path, role: &str, file: &str) -> Value {
    let mut files: Vec<String> = std::fs::read_dir(root.join(format!("{role}-sessions")))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    json!({"file":std::path::Path::new(file).file_name().unwrap().to_string_lossy(),"files":files})
}
#[test]
fn architect_open_prepares_and_prunes_resident_files_like_frozen() {
    let rust = tempfile::tempdir().unwrap();
    let tsdir = tempfile::tempdir().unwrap();
    setup(rust.path(), "architect");
    setup(tsdir.path(), "architect");
    let mut ts = oracle::TsOracle::spawn("q16-runtime-oracle.ts", "packages/coding-agent/src/architect/main.ts");
    let expected=ts.request(&json!({"id":2,"operation":"openResident","cases":[{"agentDir":tsdir.path(),"cwd":"/fixture","db":tsdir.path().join("control.sqlite"),"role":"architect"}]}));
    assert_eq!(expected["ok"], true, "{expected}");
    let manager = pi_coding_agent::architect::main::open_architect_session(
        &rust.path().display().to_string(),
        "/fixture",
        &rust.path().join("control.sqlite").display().to_string(),
    )
    .unwrap();
    assert_eq!(
        output(rust.path(), "architect", manager.get_session_file().unwrap()),
        expected["output"][0]
    );
}

#[test]
fn supervisor_open_prepares_and_prunes_resident_files_like_frozen() {
    let rust = tempfile::tempdir().unwrap();
    let tsdir = tempfile::tempdir().unwrap();
    setup(rust.path(), "supervisor");
    setup(tsdir.path(), "supervisor");
    let mut ts = oracle::TsOracle::spawn("q16-runtime-oracle.ts", "packages/coding-agent/src/supervisor/main.ts");
    let expected=ts.request(&json!({"id":1,"operation":"openResident","cases":[{"agentDir":tsdir.path(),"cwd":"/fixture","db":tsdir.path().join("control.sqlite"),"role":"supervisor"}]}));
    assert_eq!(expected["ok"], true, "{expected}");
    let manager = pi_coding_agent::supervisor::main::open_supervisor_session(
        &rust.path().display().to_string(),
        "/fixture",
        &rust.path().join("control.sqlite").display().to_string(),
    )
    .unwrap();
    assert_eq!(
        output(rust.path(), "supervisor", manager.get_session_file().unwrap()),
        expected["output"][0]
    );
}
