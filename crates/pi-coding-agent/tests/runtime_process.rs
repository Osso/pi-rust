use std::process::{Command, Stdio};

use pi_coding_agent::core::runtime_process::{
    ProcessIdentity, command_line_is_pi_runtime, is_pi_runtime_process_alive, is_process_identity_alive,
    is_verified_pi_runtime_process, read_process_identity,
};

#[test]
fn matches_a_live_process_only_at_its_exact_start_time() {
    let identity = read_process_identity(std::process::id()).expect("own identity must be readable");
    assert_eq!(identity.pid, std::process::id());
    assert!(identity.start_time_ticks > 0);
    assert!(is_process_identity_alive(&identity));
    assert!(!is_process_identity_alive(&ProcessIdentity {
        start_time_ticks: identity.start_time_ticks + 1,
        ..identity
    }));
}

#[test]
fn reports_missing_processes_as_dead() {
    let identity = read_process_identity(std::process::id()).unwrap();
    let missing = ProcessIdentity {
        pid: 4_194_303,
        ..identity
    };
    assert!(!is_process_identity_alive(&missing));
    assert!(read_process_identity(4_194_303).is_err());
}

#[test]
fn treats_an_exited_zombie_as_dead_before_its_parent_reaps_it() {
    let mut helper = Command::new("python3")
        .arg("-c")
        .arg(
            [
                "import os, sys",
                "pid = os.fork()",
                "if pid == 0:",
                "    os._exit(0)",
                "print(f'READY {pid}', flush=True)",
                "os.waitid(os.P_PID, pid, os.WEXITED | os.WNOWAIT)",
                "print('ZOMBIE', flush=True)",
                "if sys.stdin.readline().strip() == 'REAP':",
                "    os.waitpid(pid, 0)",
            ]
            .join("\n"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("python3 helper must start");
    let mut stdout = std::io::BufReader::new(helper.stdout.take().unwrap());
    let child_pid = read_ready_pid(&mut stdout);
    let identity = read_process_identity(child_pid).expect("child identity must be readable");
    let mut zombie_line = String::new();
    std::io::BufRead::read_line(&mut stdout, &mut zombie_line).unwrap();
    assert_eq!(zombie_line.trim(), "ZOMBIE");
    assert!(
        !is_process_identity_alive(&identity),
        "a zombie must not count as alive"
    );

    use std::io::Write;
    helper.stdin.take().unwrap().write_all(b"REAP\n").unwrap();
    helper.wait().unwrap();
}

fn read_ready_pid(stdout: &mut impl std::io::BufRead) -> u32 {
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    line.trim()
        .strip_prefix("READY ")
        .expect("helper must announce READY")
        .parse()
        .expect("READY line must carry a pid")
}

#[test]
fn recognizes_pi_runtime_command_lines() {
    let accepted: [&[&str]; 7] = [
        &["pi"],
        &["/usr/local/bin/pi"],
        &["/opt/Pi.EXE"],
        &["node", "/home/osso/Projects/pi/packages/coding-agent/src/cli.ts"],
        &["bun", "packages/coding-agent/src/bun/cli.ts"],
        &["node", "packages/coding-agent/dist/cli.js"],
        &["node", "C:\\work\\packages\\coding-agent\\dist\\cli.js"],
    ];
    for command_line in accepted {
        assert!(
            command_line_is_pi_runtime(&command_line.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>()),
            "{command_line:?}"
        );
    }
    let rejected: [&[&str]; 6] = [
        &["bash"],
        &["node", "other.js"],
        &["node", "packages/coding-agent/src/cli.tsx"],
        &["node", "dist/cli.js"],
        &["pi-extra"],
        &[],
    ];
    for command_line in rejected {
        assert!(
            !command_line_is_pi_runtime(&command_line.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>()),
            "{command_line:?}"
        );
    }
}

#[test]
fn identifies_pi_runtime_processes_by_command_line() {
    assert!(is_pi_runtime_process_alive(std::process::id()));
    assert!(is_verified_pi_runtime_process(std::process::id()));

    let mut pi_named = Command::new("bash")
        .args(["-c", "exec -a pi sleep 30"])
        .spawn()
        .expect("bash must start");
    let mut foreign = Command::new("sleep").arg("30").spawn().expect("sleep must start");
    std::thread::sleep(std::time::Duration::from_millis(100));

    assert!(is_pi_runtime_process_alive(pi_named.id()));
    assert!(is_verified_pi_runtime_process(pi_named.id()));
    assert!(!is_pi_runtime_process_alive(foreign.id()));
    assert!(!is_verified_pi_runtime_process(foreign.id()));

    pi_named.kill().unwrap();
    pi_named.wait().unwrap();
    foreign.kill().unwrap();
    foreign.wait().unwrap();
    assert!(!is_pi_runtime_process_alive(foreign.id()));
}
