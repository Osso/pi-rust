import json
import os
from pathlib import Path
import signal
import sys
import time

trace = Path(sys.argv[1])
heartbeat = Path(sys.argv[2])
mode = sys.argv[3]
path = Path(sys.argv[-1])
deadline = time.monotonic() + 1.5
while not heartbeat.exists() and time.monotonic() < deadline:
    time.sleep(0.005)
with trace.open("a") as output:
    output.write(json.dumps({"event": "spawn", "path": str(path), "args": sys.argv[4:-1], "bytes": list(path.read_bytes()), "heartbeat": heartbeat.exists()}) + "\n")
if mode == "fail":
    sys.exit(7)
if mode == "signal":
    os.kill(os.getpid(), signal.SIGTERM)
if mode == "remove":
    path.unlink()
elif mode == "directory":
    path.unlink()
    path.mkdir()
elif mode == "empty":
    path.write_bytes(b"")
else:
    path.write_bytes(b"edited\n\n")
