#!/usr/bin/python3
import json
import os
from pathlib import Path
import sys

program = Path(sys.argv[0]).name
args = sys.argv[1:]
operation = os.environ["PI_TMPDIR_PROBE_OPERATION"]
if program == "wl-paste" and operation == "image":
    sys.stdout.write("image/png\n" if args[0] == "--list-types" else "clipboard-image-bytes")
    sys.exit(0)
if program == "wl-paste" and operation == "image-text":
    sys.stdout.write("text/plain\n" if args[0] == "--list-types" else "old-only clipboard text")
    sys.exit(0)
if program == "wslpath":
    Path(os.environ["PI_TMPDIR_PROBE_REPORT"]).write_text(json.dumps({"path": args[1]}))
    sys.exit(0)
if program == "gh":
    if args[0] == "auth":
        sys.exit(0)
    Path(os.environ["PI_TMPDIR_PROBE_REPORT"]).write_text(json.dumps({"path": args[-1], "exists": Path(args[-1]).is_file()}))
sys.exit(1)
