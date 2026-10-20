#!/usr/bin/python3
"""Linux test-process boundary, not a product runtime or hostile-code sandbox.

Usage: python3 scripts/test-sandbox.py -- COMMAND [ARG ...]
PI_TEST_SANDBOX_DIR selects an explicit private writable proof directory.
Otherwise logs/state are retained in a fresh pi-test-* directory under TMPDIR.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
HELPER = "/home/osso/.cargo/bin/claude-memory"


def bind(arguments, source, writable=False):
    source = Path(source).absolute()
    arguments.extend(["--bind" if writable else "--ro-bind", str(source), str(source)])


def reference_inputs():
    lock = json.loads((ROOT / "docs/porting/source-lock.json").read_text())
    references = [
        ROOT,
        Path(lock["worktree"]),
        Path(lock["typecheckReference"]["worktree"]),
    ]
    for reference in references:
        yield reference
        git_file = reference / ".git"
        if git_file.is_file():
            git_dir = (
                reference / git_file.read_text().strip().removeprefix("gitdir: ")
            ).resolve()
            # Linked worktree metadata points back to its repository's common Git directory.
            yield git_dir.parent.parent


def pyrun_inputs():
    # Existing differential tests invoke the installed JSONL runtime. Expose only
    # that executable, its venv and its Python runtime, never host tool/config dirs.
    executable = shutil.which("pyrun-jsonl")
    if not executable:
        return
    executable = Path(executable)
    yield executable
    resolved = executable.resolve()
    venv = resolved.parent.parent
    yield venv
    yield (venv / "bin/python").resolve().parent.parent


def private_environment(state):
    environment = {
        "PATH": "/usr/bin:/bin:/home/osso/.local/bin",
        "HOME": str(state / "home"),
        "XDG_CONFIG_HOME": str(state / "config"),
        "XDG_STATE_HOME": str(state / "state"),
        "XDG_CACHE_HOME": str(state / "cache"),
        "XDG_RUNTIME_DIR": str(state / "run"),
        "TMPDIR": str(state / "tmp"),
        "PI_CODING_AGENT_DIR": str(state / "agent"),
        "PI_CODING_AGENT_STATE_DIR": str(state / "state/pi"),
        "CARGO_NET_OFFLINE": "true",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": "/dev/null",
        "PYTHONDONTWRITEBYTECODE": "1",
    }
    for key in [
        "TERM",
        "COLORTERM",
        "LANG",
        "LC_ALL",
        "NO_COLOR",
        "RUST_BACKTRACE",
        "RUST_TEST_THREADS",
        "PI_OFFLINE",
        "CARGO_MANIFEST_DIR",
        "CARGO_TARGET_DIR",
        "LLVM_PROFILE_FILE",
    ]:
        if key in os.environ:
            environment[key] = os.environ[key]
    for key in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
        "TMPDIR",
        "PI_CODING_AGENT_DIR",
        "PI_CODING_AGENT_STATE_DIR",
    ]:
        Path(environment[key]).mkdir(parents=True, exist_ok=True)
    (state / "agent/auth.json").write_text("{}")
    return environment


def enter(command):
    # This runs only after bwrap created a fresh network namespace. No host
    # interface changes; no veth/default route. Local HTTP fixtures need lo up.
    subprocess.run(["/usr/bin/ip", "link", "set", "lo", "up"], check=True)
    # Create devpts only AFTER configuring the network namespace: --dev creates
    # another user namespace whose NET_ADMIN cannot configure the earlier netns.
    devices = [
        "/usr/bin/bwrap", "--die-with-parent", "--unshare-user",
        "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc",
    ]
    # /tmp is already the outer guard's private tmpfs. Keep it writable for
    # env-cleared probe children that use the platform temp directory directly.
    bind(devices, "/tmp", writable=True)
    state = Path(os.environ["PI_CODING_AGENT_DIR"]).parent
    for writable in dict.fromkeys([state, Path(os.environ["TMPDIR"])]):
        bind(devices, writable, writable=True)
    # Drop bootstrap capabilities before the device namespace and test process.
    os.execvpe(
        "/usr/bin/setpriv",
        [
            "setpriv",
            "--inh-caps=-net_admin",
            "--ambient-caps=-net_admin",
            "--",
            *devices,
            "--",
            *command,
        ],
        os.environ,
    )


def launch(command):
    if not shutil.which("bwrap"):
        raise RuntimeError(
            "test isolation requires bubblewrap; no unsandboxed fallback"
        )
    parent_tmp = Path(os.environ.get("TMPDIR", "/tmp")).absolute()
    selected = os.environ.get("PI_TEST_SANDBOX_DIR")
    state = (
        Path(selected).absolute()
        if selected
        else Path(tempfile.mkdtemp(prefix="pi-test-", dir=parent_tmp))
    )
    state.mkdir(parents=True, exist_ok=True)
    if state in [Path("/"), Path("/tmp"), Path.home()]:
        raise RuntimeError("PI_TEST_SANDBOX_DIR must be a private test directory")
    environment = private_environment(state)
    if selected:
        environment["PI_TEST_SANDBOX_DIR"] = str(state)
    if parent_tmp != Path("/tmp"):
        environment["TMPDIR"] = str(parent_tmp)
    helper = state / "claude-memory-stub"
    helper.write_text(
        "#!/usr/bin/python3\nimport json, os, sys\n"
        f"with open({str(state / 'helper.jsonl')!r}, 'a') as log:\n"
        "    log.write(json.dumps({'argv': sys.argv[1:], 'pid': os.getpid()}) + '\\n')\n"
        "print('test helper: invocation recorded')\n"
    )
    helper.chmod(0o755)
    arguments = [
        "/usr/bin/bwrap",
        "--die-with-parent",
        "--unshare-user",
        "--unshare-net",
        "--cap-add",
        "CAP_NET_ADMIN",
        "--ro-bind",
        "/",
        "/",
        "--tmpfs",
        "/home",
        "--tmpfs",
        "/root",
        "--tmpfs",
        "/run",
        "--tmpfs",
        "/tmp",
        # Bootstrap devices only; enter() replaces these with private devpts
        # after enabling loopback and dropping the bootstrap capabilities.
        "--dev-bind",
        "/dev/pts",
        "/dev/pts",
        "--dev-bind",
        "/dev/null",
        "/dev/null",
        "--dev-bind",
        "/dev/tty",
        "/dev/tty",
        "--proc",
        "/proc",
    ]
    for source in dict.fromkeys([*reference_inputs(), *pyrun_inputs()]):
        bind(arguments, source)
    # Parent-created literal fixture files and sockets must survive nested guards.
    # Never expose the host's entire /tmp; the initial guard supplies private TMPDIR.
    if parent_tmp != Path("/tmp"):
        bind(arguments, parent_tmp, writable=True)
    bind(arguments, state, writable=True)
    arguments.extend(
        ["--ro-bind", str(helper), HELPER, "--chdir", str(Path.cwd()), "--"]
    )
    arguments.extend(
        ["/usr/bin/python3", str(Path(__file__).resolve()), "--enter", *command]
    )
    print(f"test isolation: state/logs {state}", file=sys.stderr, flush=True)
    # Replacing the runner retains normal PTY descriptors and process groups;
    # no PID namespace or wrapper wait changes the oracle watchdog contract.
    os.execve(arguments[0], arguments, environment)


if __name__ == "__main__":
    try:
        if sys.argv[1:2] == ["--enter"]:
            enter(sys.argv[2:])
        elif sys.argv[1:2] == ["--"] and len(sys.argv) > 2:
            launch(sys.argv[2:])
        else:
            raise RuntimeError("usage: test-sandbox.py -- COMMAND [ARG ...]")
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(
            f"test isolation failed (no unsandboxed fallback): {error}", file=sys.stderr
        )
        raise SystemExit(1)
