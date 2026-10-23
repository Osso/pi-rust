import json
import os
import signal
import socket
import sys
import threading
import time

path = os.environ["TEST_CONTROL_DB"]
version = os.environ["TEST_VERSION"]
identity = {
    "version": version,
    "pid": os.getpid(),
    "executable": sys.executable,
    "entrypoint": sys.argv[0],
    "instanceId": str(os.getpid()),
    "managedBy": os.environ.get("TEST_MANAGED_BY", "pi"),
    "ready": True,
}
with open(path + ".launches", "a") as output:
    output.write(json.dumps({"pid": os.getpid(), "sid": os.getsid(0), "cwd": os.getcwd(), "autostart": os.environ.get("PI_SUPERVISOR_AUTOSTARTED")}) + "\n")
server = socket.socket(socket.AF_UNIX)
socket_path = path + ".supervisor-console.sock"
try:
    os.unlink(socket_path)
except FileNotFoundError:
    pass
server.bind(socket_path)
server.listen()

def handle(connection):
    with connection:
        reader = connection.makefile("rb")
        for line in reader:
            request = json.loads(line)
            if request["type"] in ("probe", "attach"):
                response = {"type": "probed" if request["type"] == "probe" else "attached", "version": 1, "service": "supervisor", "sessionId": "supervisor", "cwd": os.getcwd(), "generation": 1, "identity": identity, "branch": []}
                connection.sendall((json.dumps(response) + "\n").encode())

stop_at = None

def stop(signum, frame):
    global stop_at
    os.unlink(socket_path)
    stop_at = time.monotonic() + 0.05

signal.signal(signal.SIGTERM, stop)
server.settimeout(0.01)
while stop_at is None or time.monotonic() < stop_at:
    try:
        connection, _ = server.accept()
    except TimeoutError:
        continue
    threading.Thread(target=handle, args=(connection,), daemon=True).start()
server.close()
