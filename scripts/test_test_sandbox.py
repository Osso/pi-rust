"""Behavioral regressions for the mandatory test-process boundary."""

import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
GUARD = ROOT / "scripts/test-sandbox.py"
SPEC = importlib.util.spec_from_file_location("test_sandbox", GUARD)
SANDBOX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SANDBOX)


class SandboxTests(unittest.TestCase):
    def run_guard(self, command):
        result = subprocess.run(
            [sys.executable, str(GUARD), "--", *command],
            cwd=ROOT, capture_output=True, text=True, timeout=60,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def delta_fixture(self, directory):
        checkout = Path(directory) / "delta"
        checkout.mkdir()
        module = checkout / "resident.ts"
        module.write_text("export const separator = 'backslash';\n")
        subprocess.run(["git", "init", "-q", str(checkout)], check=True)
        subprocess.run(["git", "-C", str(checkout), "add", "resident.ts"], check=True)
        subprocess.run([
            "git", "-C", str(checkout), "-c", "user.name=Fixture",
            "-c", "user.email=fixture@example.invalid", "commit", "-qm", "fixture",
        ], check=True)
        commit = subprocess.check_output(["git", "-C", str(checkout), "rev-parse", "HEAD"], text=True).strip()
        return {"deltas": [{"checkout": str(checkout), "commit": commit,
                            "modules": {"resident.ts": hashlib.sha256(module.read_bytes()).hexdigest()}}]}

    def test_locked_delta_exposes_verified_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            lock = self.delta_fixture(directory)
            self.assertEqual(list(SANDBOX.locked_delta_paths(lock)), [Path(lock["deltas"][0]["checkout"])])

    def test_locked_delta_rejects_missing_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            lock = self.delta_fixture(directory)
            lock["deltas"][0]["checkout"] += "/absent"
            with self.assertRaisesRegex(RuntimeError, "HEAD"):
                list(SANDBOX.locked_delta_paths(lock))

    def test_locked_delta_rejects_wrong_head(self):
        with tempfile.TemporaryDirectory() as directory:
            lock = self.delta_fixture(directory)
            lock["deltas"][0]["commit"] = "0" * 40
            with self.assertRaisesRegex(RuntimeError, "HEAD mismatch"):
                list(SANDBOX.locked_delta_paths(lock))

    def test_locked_delta_rejects_changed_module(self):
        with tempfile.TemporaryDirectory() as directory:
            lock = self.delta_fixture(directory)
            (Path(lock["deltas"][0]["checkout"]) / "resident.ts").write_text("changed\n")
            with self.assertRaisesRegex(RuntimeError, "SHA256 mismatch"):
                list(SANDBOX.locked_delta_paths(lock))

    def test_locked_delta_rejects_missing_module(self):
        with tempfile.TemporaryDirectory() as directory:
            lock = self.delta_fixture(directory)
            (Path(lock["deltas"][0]["checkout"]) / "resident.ts").unlink()
            with self.assertRaisesRegex(RuntimeError, "cannot read locked delta module"):
                list(SANDBOX.locked_delta_paths(lock))

    def test_locked_delta_is_read_only_inside_guard(self):
        probe = (
            "import errno,json,pathlib; "
            f"lock=json.loads(pathlib.Path({str(ROOT / 'docs/porting/delta-lock.json')!r}).read_text()); "
            "module=pathlib.Path(lock['deltas'][0]['checkout']) / next(iter(lock['deltas'][0]['modules'])); "
            "assert module.is_file(); "
            "exec(\"try:\\n module.open('r+b')\\nexcept OSError as error:\\n assert error.errno == errno.EROFS\\nelse:\\n raise AssertionError('delta checkout is writable')\")"
        )
        self.run_guard(["/usr/bin/python3", "-c", probe])

    def test_device_entropy_and_getrandom_survive_nested_guards(self):
        probe = (
            "import os; "
            "assert len(os.getrandom(32)) == 32; "
            "assert len(open('/dev/urandom', 'rb').read(32)) == 32; "
            "assert len(open('/dev/random', 'rb').read(32)) == 32; "
            "assert open('/dev/zero', 'rb').read(8) == bytes(8)"
        )
        command = ["/usr/bin/python3", "-c", probe]
        self.run_guard(command)
        self.run_guard(["/usr/bin/python3", str(GUARD), "--", *command])

    def test_installed_standalone_python_starts_with_hash_randomization(self):
        executable = shutil.which("pyrun-jsonl")
        self.assertIsNotNone(executable, "pyrun-jsonl is required for real-runner proof")
        # Binding the installed launcher into a parent guard dereferences its
        # symlink. Its shebang still names the actual standalone interpreter.
        shebang = Path(executable).read_text().splitlines()[0]
        self.assertTrue(shebang.startswith("#!/"), "pyrun-jsonl must name its interpreter")
        python = Path(shebang.removeprefix("#!"))
        self.run_guard([str(python), "-c", "import os; assert len(os.urandom(32)) == 32; print(hash('entropy'))"])

    def test_ripgrep_is_available_without_exposing_other_host_tools(self):
        probe = (
            "import pathlib,subprocess,tempfile; "
            "directory=pathlib.Path(tempfile.mkdtemp()); "
            "fixture=directory/'search.txt'; fixture.write_text('needle\\n'); "
            "result=subprocess.run(['rg','needle',str(fixture)],capture_output=True,text=True,check=True); "
            "assert result.stdout == 'needle\\n'; "
            "assert not pathlib.Path('/home/osso/.cargo/bin/cargo').exists(); "
            "assert not pathlib.Path('/home/osso/.config/pi').exists()"
        )
        self.run_guard(["/usr/bin/python3", "-c", probe])

    def test_host_state_hidden_and_network_is_loopback_only(self):
        probe = (
            "import json,os,pathlib,subprocess; "
            "assert not pathlib.Path('/home/osso/.config/pi').exists(); "
            "assert not pathlib.Path('/root/.config/pi').exists(); "
            "assert os.environ['HOME'] != '/home/osso'; "
            "links=json.loads(subprocess.check_output(['/usr/bin/ip','-j','link'])); "
            "assert [link['ifname'] for link in links] == ['lo']; "
            "assert 'UP' in links[0]['flags']; "
            "assert json.loads(subprocess.check_output(['/usr/bin/ip','-j','route'])) == []"
        )
        self.run_guard(["/usr/bin/python3", "-c", probe])


if __name__ == "__main__":
    unittest.main()
