#!/usr/bin/python3
"""Bounded real-CLI regression. Always mask the host helper before launching Pi.

Usage: python3 scripts/test-isolation-test.py /absolute/path/to/built/pi
The outer namespace is mandatory even when testing the unprotected old runner.
"""

import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import sqlite3
import struct
import subprocess
import sys
import tempfile
import termios
import time

ROOT = Path(__file__).resolve().parent.parent
HELPER = "/home/osso/.cargo/bin/claude-memory"


def interactive(command, environment, work):
    pid, master = pty.fork()
    if pid == 0:
        os.chdir(work)
        os.execve(command[0], command, environment)
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
    output = bytearray()
    deadline = time.monotonic() + 30
    first_signal = None
    second_signal = False
    status = None
    try:
        while time.monotonic() < deadline:
            if select.select([master], [], [], 0.05)[0]:
                try:
                    output.extend(os.read(master, 65536))
                except OSError as error:
                    if error.errno != errno.EIO:
                        raise
            if first_signal is None and b"Ctrl+C twice to exit" in output:
                time.sleep(0.2)
                os.write(master, b"\x03")
                first_signal = time.monotonic()
            elif (
                first_signal
                and not second_signal
                and time.monotonic() - first_signal > 0.2
            ):
                os.write(master, b"\x03")
                second_signal = True
            finished, child_status = os.waitpid(pid, os.WNOHANG)
            if finished:
                status = child_status
                break
        if status is None:
            raise AssertionError("default CLI failed to exit within 30 seconds")
    finally:
        if status is None:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        os.close(master)
        (work.parent / "interactive.raw").write_bytes(output)
    assert os.waitstatus_to_exitcode(status) == 0, output.decode(errors="replace")
    assert second_signal and b"Pi can explain its own features" in output
    return pid


def assert_gone(pid):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        stat = Path(f"/proc/{pid}/stat")
        if not stat.exists() or stat.read_text().split(") ", 1)[1].startswith("Z"):
            return
        time.sleep(0.02)
    raise AssertionError(f"child {pid} remains running")


def inside(proof, binary):
    environment = {
        "PATH": "/usr/bin:/bin",
        "HOME": str(proof / "home"),
        "XDG_CONFIG_HOME": str(proof / "config"),
        "XDG_STATE_HOME": str(proof / "state"),
        "XDG_CACHE_HOME": str(proof / "cache"),
        "PI_CODING_AGENT_DIR": str(proof / "agent"),
        "PI_CODING_AGENT_STATE_DIR": str(proof / "state" / "pi"),
        "PI_TEST_SANDBOX_DIR": str(proof),
        "TMPDIR": str(proof / "tmp"),
        "PI_OFFLINE": "1",
        "TERM": "xterm-256color",
    }
    for key in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "PI_CODING_AGENT_DIR",
        "PI_CODING_AGENT_STATE_DIR",
        "TMPDIR",
    ]:
        Path(environment[key]).mkdir(parents=True, exist_ok=True)
    (proof / "agent/auth.json").write_text("{}")
    work = proof / "work"
    work.mkdir()
    (proof / "deps").mkdir()
    test_binary = proof / "deps/pi"
    test_binary.symlink_to(binary)
    cli_pid = interactive(
        [str(ROOT / "scripts/cargo-runner.sh"), str(test_binary)], environment, work
    )
    forbidden = proof / "forbidden.jsonl"
    controlled = proof / "helper.jsonl"
    deadline = time.monotonic() + 5
    while (
        not forbidden.exists()
        and not controlled.exists()
        and time.monotonic() < deadline
    ):
        time.sleep(0.02)
    records = []
    for log in [forbidden, controlled]:
        if log.exists():
            records.extend(json.loads(line) for line in log.read_text().splitlines())
    for record in records:
        assert_gone(record["pid"])
    assert_gone(cli_pid)
    assert not forbidden.exists(), (
        "old boundary executed forbidden absolute helper on default shutdown"
    )
    assert len(records) == 1, records
    assert records[0]["argv"][0] == "index-file", records
    assert len(records[0]["argv"]) == 2 and records[0]["argv"][1].endswith(".jsonl"), (
        records
    )
    hook_log = proof / "cache/claude-memory/pi-index.log"
    assert (
        "starting index-file for session " + records[0]["argv"][1]
        in hook_log.read_text()
    )
    db = sqlite3.connect(f"file:{proof}/state/pi/control.sqlite?mode=ro", uri=True)
    assert db.execute("PRAGMA user_version").fetchone() == (15,)
    assert db.execute("PRAGMA quick_check").fetchone() == ("ok",)
    db.close()
    print(
        "PASS default startup, PTY, shutdown hook argv, private DB, no forbidden helper, no running children",
        flush=True,
    )


def probe_network_and_nested_inputs(proof):
    # A literal filename (including spaces) created before either guard must
    # survive both. HTTP traffic is real TCP entirely within the new namespace.
    literal = proof / "tmp/parent fixture.txt"
    literal.write_text("parent fixture")
    probe = proof / "network-probe.py"
    probe.write_text("""import errno, http.server, os, pathlib, pty, socket, subprocess, threading, urllib.request
master, slave = pty.openpty()
os.close(master)
os.close(slave)
subprocess.run(['/usr/bin/python3', '-c', 'import tempfile; f = tempfile.TemporaryFile(); f.write(b"private tmp")'], env={}, check=True)
for line in pathlib.Path('/proc/self/status').read_text().splitlines():
    if line.startswith(('CapInh:', 'CapAmb:', 'CapEff:')):
        assert int(line.split()[1], 16) == 0, line
helper = subprocess.run(['/home/osso/.cargo/bin/claude-memory', 'runner-probe'], capture_output=True, text=True, check=True)
assert helper.stdout.strip() == 'test helper: invocation recorded', helper
assert pathlib.Path(os.environ["LITERAL_FIXTURE"]).read_text() == "parent fixture"
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"fixture response")
    def log_message(self, *args):
        pass
server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
thread = threading.Thread(target=server.handle_request)
thread.start()
with urllib.request.urlopen(f"http://127.0.0.1:{server.server_port}", timeout=2) as response:
    assert response.read() == b"fixture response"
thread.join(timeout=2)
assert not thread.is_alive()
server.server_close()
with socket.socket() as client:
    client.settimeout(1)
    try:
        client.connect(("192.0.2.1", 9))
    except OSError as error:
        assert error.errno == errno.ENETUNREACH, error
    else:
        raise AssertionError("external network route exists")
assert "PI_OFFLINE" not in os.environ
assert "ANTHROPIC_API_KEY" not in os.environ
assert not pathlib.Path("/home/osso/.ssh").exists()
print("PASS private PTY, dropped capabilities, masked helper, actual loopback HTTP, denied external route, nested literal fixture, private environment")
""")
    # Put the literal path in argv, not an inherited arbitrary environment entry.
    payload = proof / "network-entry.py"
    payload.write_text(
        "import os, runpy, sys\n"
        "os.environ['LITERAL_FIXTURE'] = sys.argv[1]\n"
        f"runpy.run_path({str(probe)!r})\n"
    )
    launcher = str(ROOT / "scripts/test-sandbox.py")
    result = subprocess.run(
        [
            "/usr/bin/python3",
            launcher,
            "--",
            "/usr/bin/python3",
            launcher,
            "--",
            "/usr/bin/python3",
            str(payload),
            str(literal),
        ],
        env={
            "PATH": "/usr/bin:/bin",
            "TMPDIR": str(proof / "tmp"),
            "PI_TEST_SANDBOX_DIR": str(proof),
            "ANTHROPIC_API_KEY": "forbidden-sentinel",
        },
        cwd=ROOT,
        capture_output=True,
        text=True,
        timeout=15,
    )
    (proof / "network.stdout").write_text(result.stdout)
    (proof / "network.stderr").write_text(result.stderr)
    assert result.returncode == 0, result.stdout + result.stderr
    print(result.stdout, end="")


def outer(binary):
    with tempfile.TemporaryDirectory(prefix="pi-cli-isolation-") as temporary:
        proof = Path(temporary)
        (proof / "tmp").mkdir()
        probe_network_and_nested_inputs(proof)
        # The startup assertion below expects only the shutdown hook invocation.
        (proof / "helper.jsonl").unlink()
        fake = proof / "forbidden-helper"
        fake.write_text(
            "#!/usr/bin/python3\nimport json, os, sys\n"
            f"with open({str(proof / 'forbidden.jsonl')!r}, 'a') as log:\n"
            "    log.write(json.dumps({'argv': sys.argv[1:], 'pid': os.getpid()}) + '\\n')\n"
        )
        fake.chmod(0o755)
        command = [
            "/usr/bin/bwrap",
            "--die-with-parent",
            "--ro-bind",
            "/",
            "/",
            "--bind",
            str(proof),
            str(proof),
            "--unshare-net",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--ro-bind",
            str(fake),
            HELPER,
            "--",
            "/usr/bin/python3",
            str(Path(__file__).resolve()),
            "--inside",
            str(proof),
            binary,
        ]
        result = subprocess.run(
            command,
            env={"PATH": "/usr/bin:/bin"},
            capture_output=True,
            text=True,
            timeout=45,
        )
        print(result.stdout, end="")
        print(result.stderr, end="", file=sys.stderr)
        artifact = os.environ.get("PI_ISOLATION_PROOF_DIR")
        if artifact:
            import shutil

            shutil.copytree(proof, Path(artifact) / proof.name, symlinks=True)
        return result.returncode


if __name__ == "__main__":
    if sys.argv[1] == "--inside":
        inside(Path(sys.argv[2]), sys.argv[3])
    else:
        raise SystemExit(outer(str(Path(sys.argv[1]).resolve())))
