#!/usr/bin/env python3
"""Fake `claude --bg` of the demo kit: adds an invented background session to
agents.json and prints the line the real CLI prints, so a launch from the
draft becomes a session of the tree.

Usage: python3 -I lanzar.py <agents.json> <cwd> <claude args...>
"""

import json
import os
import sys
import time
import uuid

path, cwd, args = sys.argv[1], sys.argv[2], sys.argv[3:]
name = args[args.index("-n") + 1] if "-n" in args and args.index("-n") + 1 < len(args) else "demo"
sid = str(uuid.uuid4())
short = sid[:8]
with open(path, encoding="utf-8") as f:
    agents = json.load(f)
agents.append({"id": short, "pid": 1, "cwd": cwd, "kind": "background", "status": "busy",
               "startedAt": int(time.time() * 1000), "sessionId": sid, "name": name})
with open(path + ".new", "w", encoding="utf-8") as f:
    json.dump(agents, f)
os.replace(path + ".new", path)
print(f"backgrounded · {short} · {name}")
