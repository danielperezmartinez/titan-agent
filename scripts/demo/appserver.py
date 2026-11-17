#!/usr/bin/env python3
"""Fake `codex app-server` for the demo kit: JSON-RPC over WebSocket on a
Unix socket, serving the invented threads of codex.json (datos.py).

Usage: python3 -I appserver.py <socket> <codex.json>

It answers only what titan-agentd asks (initialize, thread/loaded/list,
thread/list, thread/read, thread/items/list, thread/turns/list,
account/rateLimits/read) and never runs anything. Requests are logged to
calls.log next to codex.json.
"""

import base64
import hashlib
import json
import os
import socket
import struct
import sys
import threading

SOCK, DATA = sys.argv[1], sys.argv[2]
LOG = os.path.join(os.path.dirname(os.path.abspath(DATA)), "calls.log")
GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


def data():
    with open(DATA, encoding="utf-8") as f:
        return json.load(f)


def meta(tid, t):
    status = {"type": "active", "activeFlags": []} if t.get("active") else {"type": "notLoaded"}
    out = {"id": tid, "cwd": t["cwd"], "source": t.get("source", "cli"), "status": status,
           "preview": t.get("preview", ""), "createdAt": t["updated"] - 600,
           "updatedAt": t["updated"], "model": t.get("model")}
    if t.get("name"):
        out["name"] = t["name"]
    return out


def answer(method, p):
    d = data()
    threads = d["threads"]
    if method == "initialize":
        return {"userAgent": "demo"}
    if method == "account/rateLimits/read":
        return d.get("quota", {"rateLimits": None})
    if method == "thread/loaded/list":
        return {"data": [tid for tid, t in threads.items() if t.get("active")], "nextCursor": None}
    if method == "thread/list":
        rows = sorted(threads.items(), key=lambda kv: -kv[1]["updated"])
        return {"data": [meta(tid, t) for tid, t in rows], "nextCursor": None}
    tid = p.get("threadId", "")
    if tid not in threads:
        raise KeyError(f"thread not found: {tid}")
    t = threads[tid]
    if method == "thread/read":
        return {"thread": meta(tid, t)}
    if method == "thread/items/list":
        rows = [{"turnId": turn, "item": item} for turn, item in t["items"]]
        if p.get("sortDirection", "desc") == "desc":
            rows.reverse()
        limit = int(p.get("limit") or len(rows))
        return {"data": rows[:limit], "nextCursor": "older" if len(rows) > limit else None}
    if method == "thread/turns/list":
        status = "inProgress" if t.get("active") else "completed"
        return {"data": [{"id": t["items"][-1][0], "status": status}], "nextCursor": None}
    raise KeyError(f"method not served by the demo: {method}")


def recv_exact(conn, n, buf):
    while len(buf[0]) < n:
        chunk = conn.recv(65536)
        if not chunk:
            raise EOFError
        buf[0] += chunk
    out, buf[0] = buf[0][:n], buf[0][n:]
    return out


def read_frame(conn, buf):
    b0, b1 = recv_exact(conn, 2, buf)
    n = b1 & 0x7F
    if n == 126:
        n = struct.unpack(">H", recv_exact(conn, 2, buf))[0]
    elif n == 127:
        n = struct.unpack(">Q", recv_exact(conn, 8, buf))[0]
    mask = recv_exact(conn, 4, buf) if b1 & 0x80 else b"\0\0\0\0"
    payload = recv_exact(conn, n, buf)
    return b0 & 0x0F, bytes(x ^ mask[i % 4] for i, x in enumerate(payload))


def send(conn, obj):
    body = json.dumps(obj).encode()
    n = len(body)
    if n < 126:
        head = bytes([0x81, n])
    elif n < 65536:
        head = bytes([0x81, 126]) + struct.pack(">H", n)
    else:
        head = bytes([0x81, 127]) + struct.pack(">Q", n)
    conn.sendall(head + body)


def serve(conn):
    request = b""
    while b"\r\n\r\n" not in request:
        chunk = conn.recv(4096)
        if not chunk:
            return
        request += chunk
    head, rest = request.split(b"\r\n\r\n", 1)
    key = next(line.split(":", 1)[1].strip() for line in head.decode().split("\r\n")
               if line.lower().startswith("sec-websocket-key"))
    accept = base64.b64encode(hashlib.sha1((key + GUID).encode()).digest()).decode()
    conn.sendall(("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n"
                  f"Connection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n").encode())
    buf = [rest]
    try:
        while True:
            op, payload = read_frame(conn, buf)
            if op == 8:
                return
            if op != 1:
                continue
            msg = json.loads(payload)
            if "id" not in msg:
                continue
            with open(LOG, "a", encoding="utf-8") as f:
                f.write(msg["method"] + "\n")
            try:
                send(conn, {"id": msg["id"], "result": answer(msg["method"], msg.get("params") or {})})
            except KeyError as error:
                send(conn, {"id": msg["id"], "error": {"code": -32600, "message": error.args[0]}})
    except (EOFError, OSError):
        return


if os.path.exists(SOCK):
    os.unlink(SOCK)
server = socket.socket(socket.AF_UNIX)
server.bind(SOCK)
server.listen(16)
while True:
    client, _ = server.accept()
    threading.Thread(target=serve, args=(client,), daemon=True).start()
