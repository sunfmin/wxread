# The Frida driver `wxread keys` runs to capture WeChat's database keys
# (docs/adr/0005-keys-from-the-crypto-call.md). wxread runs it with uv, which
# supplies a pinned frida<17, as:
#
#     uv run --with 'frida<17' python driver.py <hook.js> <pid> <seconds>
#
# It attaches to the running (re-signed) WeChat, loads the hook, and prints
# one JSON line per distinct key the hook sees, each {"wxkey": "<64 hex>"}, on
# stdout; a line wxread cannot read goes to stderr. It changes nothing and
# keeps nothing. Keys appear only while WeChat decrypts pages, so the person
# browses chats, search and files during the window; what is not touched is
# not captured, and `wxread doctor` then names the databases still unkeyed.
import json
import sys
import threading
import time

import frida

hook = open(sys.argv[1], "r", encoding="utf-8").read()
pid = int(sys.argv[2])
seconds = float(sys.argv[3])

seen = set()


def on_message(message, data):
    if message.get("type") == "send":
        payload = message.get("payload") or {}
        key = payload.get("wxkey")
        if key and key not in seen:
            seen.add(key)
            sys.stdout.write(json.dumps({"wxkey": key}) + "\n")
            sys.stdout.flush()
    elif message.get("type") == "error":
        sys.stderr.write(json.dumps({"frida_error": message.get("description", "")}) + "\n")
        sys.stderr.flush()


try:
    session = frida.attach(pid)
except Exception as exc:  # frida could not attach: usually no get-task-allow
    sys.stderr.write(json.dumps({"attach_error": str(exc)}) + "\n")
    sys.exit(3)

try:
    script = session.create_script(hook)
    script.on("message", on_message)
    script.load()
except Exception as exc:
    sys.stderr.write(json.dumps({"script_error": str(exc)}) + "\n")
    sys.exit(4)

# Stop when wxread closes our stdin (it does so once the keys stop coming, to
# end a capture early instead of running the whole window), as well as at the
# time limit. Watching stdin on a thread keeps the detach below clean — no
# signal, so frida is never left attached.
stop = threading.Event()


def watch_stdin():
    try:
        for _ in sys.stdin:
            pass
    except Exception:
        pass
    stop.set()


threading.Thread(target=watch_stdin, daemon=True).start()

deadline = time.time() + seconds
while time.time() < deadline and not stop.is_set():
    time.sleep(0.3)

try:
    session.detach()
except Exception:
    pass

sys.stdout.write(json.dumps({"done": True, "captured": len(seen)}) + "\n")
sys.stdout.flush()
