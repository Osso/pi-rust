#!/usr/bin/python3
"""Deterministic pipe/status/signal inputs for the shared synchronous authority."""
import os
from pathlib import Path
import signal
import sys
import time

case = sys.argv[1]
marker = Path(sys.argv[2])
def terminated(_signal, _frame):
    marker.write_text("SIGTERM")
    sys.exit(0)
signal.signal(signal.SIGTERM, terminated)
if case == "out":
    sys.stdout.buffer.write(b"out")
    sys.stderr.buffer.write(b"err")
    sys.exit(7)
if case == "signal":
    signal.signal(signal.SIGTERM, signal.SIG_DFL)
    os.kill(os.getpid(), signal.SIGTERM)
if case == "stdout":
    sys.stdout.buffer.write(b"o" * 32)
    sys.stdout.buffer.flush()
if case == "stderr":
    sys.stderr.buffer.write(b"e" * 32)
    sys.stderr.buffer.flush()
if case == "combined":
    sys.stderr.buffer.write(b"e" * 8)
    sys.stderr.buffer.flush()
    sys.stdout.buffer.write(b"o" * 8)
    sys.stdout.buffer.flush()
if case == "closed":
    os.close(1)
    os.close(2)
time.sleep(2)
