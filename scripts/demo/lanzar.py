#!/usr/bin/env python3
"""Fake `claude --bg` of the demo kit: adds an invented background session to
agents.json, with a transcript that gives it the chosen model, and prints the
line the real CLI prints, so a launch from the draft becomes a session of the
tree.

Usage: python3 -I lanzar.py <agents.json> <cwd> <claude args...>

It only writes inside the demo folder ($TITAN_DEMO_DIR, which must also hold
the fake HOME): anything else is refused before writing.
"""

import json
import os
import sys
import tempfile
import time
import uuid


def inside(path, root):
    path, root = os.path.realpath(path), os.path.realpath(root)
    return os.path.commonpath([path, root]) == root


def write_atomic(path, text, root):
    """Writes through a new exclusive temporary file (never a link that is
    already there) and renames it over `path`, inside `root` only."""
    folder = os.path.dirname(path)
    os.makedirs(folder, exist_ok=True)
    if not inside(folder, root) or os.path.islink(path):
        sys.exit(f"claude (demo): no escribo fuera de la demo: {path}")
    fd, tmp = tempfile.mkstemp(dir=folder, prefix=".lanzar-")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            f.write(text)
        os.replace(tmp, path)
    except BaseException:
        os.unlink(tmp)
        raise


def escaped(cwd):
    return "".join("-" if c in "/.\\:" else c for c in cwd)


def option(args, name, default):
    if name in args and args.index(name) + 1 < len(args):
        return args[args.index(name) + 1]
    return default


root = os.environ.get("TITAN_DEMO_DIR", "")
home = os.environ.get("HOME", "")
path, cwd, args = sys.argv[1], sys.argv[2], sys.argv[3:]
if not root or not inside(path, root) or not inside(home, root):
    sys.exit("claude (demo): fuera de la carpeta de la demo")
name = option(args, "-n", "demo")
model = option(args, "--model", "opus")
sid = str(uuid.uuid4())
short = sid[:8]
with open(path, encoding="utf-8") as f:
    agents = json.load(f)
agents.append({"id": short, "pid": 1, "cwd": cwd, "kind": "background", "status": "busy",
               "startedAt": int(time.time() * 1000), "sessionId": sid, "name": name})
transcript = os.path.join(home, ".claude", "projects", escaped(cwd), sid + ".jsonl")
line = json.dumps({"type": "assistant", "message": {"model": f"claude-{model}-5-5"}},
                  separators=(",", ":"))
write_atomic(transcript, line + "\n", root)
write_atomic(path, json.dumps(agents), root)
print(f"backgrounded · {short} · {name}")
