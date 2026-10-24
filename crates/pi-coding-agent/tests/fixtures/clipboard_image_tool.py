#!/usr/bin/python3
"""Private clipboard-tool fixture: no display or clipboard access."""
import json
import os
from pathlib import Path
import signal
import sys
import time

state = Path(os.environ["PI_CLIPBOARD_MATRIX_STATE"])
config = json.loads((state / "config.json").read_text())
program = Path(sys.argv[0]).name
args = sys.argv[1:]
with (state / "trace.jsonl").open("a") as trace:
    trace.write(json.dumps({"program": program, "args": args}) + "\n")

if program == "wl-paste":
    key = "wlTypes" if args == ["--list-types"] else "wlData"
elif program == "xclip":
    key = "xTypes" if args[3] == "TARGETS" else "xData:" + args[3]
elif program == "wslpath":
    key = "winPath"
    (state / "temporary-path").write_text(args[1])
elif program == "powershell.exe":
    key = "powershell"
    temporary = state / "temporary-path"
    if "fileHex" in config and temporary.exists():
        Path(temporary.read_text()).write_bytes(bytes.fromhex(config["fileHex"]))
else:
    raise RuntimeError(program)

step = config.get(key, {"status": 1})
def terminated(_signal, _frame):
    with (state / "signals").open("a") as signals:
        signals.write("SIGTERM\n")
    sys.exit(0)
signal.signal(signal.SIGTERM, terminated)
if step.get("sleepMs"):
    time.sleep(step["sleepMs"] / 1000)
if step.get("stderrCount"):
    sys.stderr.buffer.write(b"e" * step["stderrCount"])
    sys.stderr.buffer.flush()
if step.get("stdoutCount"):
    sys.stdout.buffer.write(b"o" * step["stdoutCount"])
if "hex" in step:
    sys.stdout.buffer.write(bytes.fromhex(step["hex"]))
else:
    sys.stdout.buffer.write(step.get("text", "").encode())
sys.stdout.buffer.flush()
sys.exit(step.get("status", 0))
