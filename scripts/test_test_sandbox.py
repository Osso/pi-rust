"""Behavioral regressions for the mandatory test-process boundary."""

import json
from pathlib import Path
import shutil
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parent.parent
GUARD = ROOT / "scripts/test-sandbox.py"


class SandboxTests(unittest.TestCase):
    def run_guard(self, command):
        result = subprocess.run(
            [sys.executable, str(GUARD), "--", *command],
            cwd=ROOT, capture_output=True, text=True, timeout=60,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

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
        python = Path(executable).resolve().parent / "python"
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
