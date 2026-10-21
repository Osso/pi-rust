#!/usr/bin/python3
"""Opt-in resident differential runner; the mandatory sandbox remains in force.

Cargo builds outside this wrapper. Configure the target runner as
`python3 scripts/resident-delta-runner.py`, then select `delta_ -- --ignored`.
No global runner or frozen source selection changes.
"""

import json
import os
from pathlib import Path
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
PIN = ROOT / "docs/porting/deltas/resident-session-private-regex.json"


def main():
    if len(sys.argv) < 2:
        raise RuntimeError("resident delta runner requires a test executable")
    pin = json.loads(PIN.read_text())
    checkout = Path(pin["checkout"])
    if not checkout.is_dir():
        raise RuntimeError(f"approved resident delta checkout missing: {checkout}")
    state = Path(tempfile.mkdtemp(prefix="pi-resident-delta-"))
    os.environ["PI_TEST_SANDBOX_DIR"] = str(state)
    # The inner mandatory guard hides /home. Expose exactly the approved
    # checkout read-only under /opt; Rust verifies HEAD and module bytes before
    # starting its oracle. The sole writable outer mount is private proof state.
    argv = [
        "/usr/bin/bwrap", "--die-with-parent", "--ro-bind", "/", "/",
        "--proc", "/proc", "--bind", str(state), str(state),
        "--tmpfs", "/opt", "--ro-bind", str(checkout), pin["sandboxAlias"],
        "--", "/usr/bin/python3", str(ROOT / "scripts/test-sandbox.py"),
        "--", *sys.argv[1:],
    ]
    os.execvpe(argv[0], argv, os.environ)


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError) as error:
        print(f"resident delta isolation failed (no fallback): {error}", file=sys.stderr)
        raise SystemExit(1)
