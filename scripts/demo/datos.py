#!/usr/bin/env python3
"""Invented data for the titan-agent demo kit (scripts/demo/demo.sh).

Usage: python3 -I datos.py <dir>

Writes, under <dir>, everything the private daemon reads, with timestamps
relative to now so the "working" states look live:

- home/: a fake HOME with the Claude Code transcripts that give each
  session its model, and the demo projects under home/dev/.
- agents.json: what the fake `claude agents --json` prints.
- codex.json: threads, items and quota served by the fake app-server.
- home/dev/titan-ssh/.agents/: a team.toml and the events of two team runs
  (one in progress, one waiting on the user).

Every name, path, message and number here is made up: never put real
sessions, users, hosts or paths in it.
"""

import json
import os
import sys
import time

if len(sys.argv) != 2:
    sys.exit("usage: datos.py <dir>")
DIR = os.path.abspath(sys.argv[1])
HOME = os.path.join(DIR, "home")
DEV = os.path.join(HOME, "dev")
SSH = os.path.join(DEV, "titan-ssh")
WEB = os.path.join(DEV, "web-notas")
NOW_MS = int(time.time() * 1000)
NOW_S = NOW_MS // 1000


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


def escaped(cwd):
    return "".join("-" if c in "/.\\:" else c for c in cwd)


# --- Claude Code sessions (fake `claude agents --json`) --------------------

CLAUDE = [
    # id, sessionId, cwd, kind, status, state, name, minutes ago, model
    ("d1r3c70r", "00000000-0000-4000-8000-0000000000d1", SSH, "background", "busy", None,
     "titan-ssh: sftp renombrar", 25, "claude-opus-5-5"),
    ("r3v1s4r0", "00000000-0000-4000-8000-0000000000d2", SSH, "interactive", "idle", None,
     "repasar el changelog", 80, "claude-sonnet-5-5"),
    ("w3bn0t45", "00000000-0000-4000-8000-0000000000d3", WEB, "background", "busy", "blocked",
     "web-notas: maqueta del editor", 12, "claude-opus-5-5"),
]
agents = []
for short, sid, cwd, kind, status, state, name, ago, model in CLAUDE:
    entry = {"id": short, "pid": 1, "cwd": cwd, "kind": kind, "status": status,
             "startedAt": NOW_MS - ago * 60_000, "sessionId": sid, "name": name}
    if state:
        entry["state"] = state
    agents.append(entry)
    transcript = os.path.join(HOME, ".claude", "projects", escaped(cwd), sid + ".jsonl")
    # Compact JSON: the daemon looks for `"model":"claude-…"` in the tail.
    line = json.dumps({"type": "assistant", "message": {"model": model}}, separators=(",", ":"))
    write(transcript, line + "\n")
write(os.path.join(DIR, "agents.json"), json.dumps(agents))

# --- Team of titan-ssh: team.toml and events --------------------------------

write(os.path.join(SSH, ".agents", "team.toml"),
      'version = 1\nname = "titan-ssh"\nstate_dir = ".agents/state"\n')
ROLES = [
    {"name": "director", "cli": "claude", "model": "opus", "effort": None, "sandbox": None},
    {"name": "disenador_tests", "cli": "codex", "model": "gpt-6.1-sol", "effort": "medium",
     "sandbox": "workspace-write"},
    {"name": "mecanico", "cli": "codex", "model": "gpt-6-luna", "effort": "medium",
     "sandbox": "workspace-write"},
    {"name": "revisor", "cli": "codex", "model": "gpt-6.1-sol", "effort": "high",
     "sandbox": "read-only"},
]
DIRECTOR = "00000000-0000-4000-8000-0000000000d1"


def run(task, steps):
    """Events of one task; each step is (seconds ago, type, fields)."""
    team = {"name": "titan-ssh", "director": "director", "roles": ROLES,
            "steps": ["disenador_tests", "revisor"], "review_role": "revisor",
            "max_review_rounds": 1}
    events = [{"v": 1, "ts_ms": NOW_MS - steps[0][0] * 1000 - 5000, "task": task,
               "type": "task_created", "director_session": DIRECTOR, "base": "main",
               "base_commit": "a1b2c3d", "branch": "encargo/" + task,
               "worktree": ".agents/worktrees/" + task, "team": team}]
    for ago, kind, fields in steps:
        events.append({"v": 1, "ts_ms": NOW_MS - ago * 1000, "task": task, "type": kind,
                       "director_session": DIRECTOR, **fields})
    path = os.path.join(SSH, ".agents", "state", task, "events.jsonl")
    write(path, "".join(json.dumps(e, ensure_ascii=False) + "\n" for e in events))


def started(role, turn, model, thread=None, round_=None):
    return {"role": role, "turn": turn, "cli": "codex", "model": model,
            "effort": "high" if role == "revisor" else "medium",
            "sandbox": "read-only" if role == "revisor" else "workspace-write",
            "thread_id": thread, "review_round": round_}


def finished(role, turn, thread, text, inp, out, secs, files=(), warnings=(), status="ok"):
    return {"role": role, "turn": turn, "status": status, "duration_ms": secs * 1000,
            "thread_id": thread, "commit": "c0ffee1" if files else None,
            "usage": {"input_tokens": inp, "cached_input_tokens": inp * 9 // 10,
                      "output_tokens": out},
            "changed_files": list(files), "warnings": list(warnings), "text": text}


def brief(to, turn, text, resumed=False):
    return {"from": "director", "to": to, "turn": turn, "resumed": resumed, "text": text}


def act(role, turn, text):
    return {"role": role, "turn": turn, "text": text}


# In progress: tests and Luna done (with their checks), the reviewer working.
run("sftp-renombrar", [
    (1150, "brief_sent", brief("disenador_tests", 1, "encargo inicial")),
    (1149, "turn_started", started("disenador_tests", 1, "gpt-6.1-sol")),
    (1100, "activity", act("disenador_tests", 1, "edita crates/shared/tests/sftp_rename_acceptance.rs")),
    (1040, "activity", act("disenador_tests", 1, "$ cargo fmt --all")),
    (1000, "activity", act("disenador_tests", 1, "$ cargo test --workspace")),
    (990, "turn_finished", finished("disenador_tests", 1, "th-tests", "6 tests de aceptación (C1–C4)",
                                     98_000, 7_400, 209, ["crates/shared/tests/sftp_rename_acceptance.rs"],
                                     ["comprobación test falló (exit 101)"])),
    (950, "decision", {"by": "director", "text": "Acepto los 6 tests de Sol; fallan por falta de implementación."}),
    (900, "brief_sent", brief("mecanico", 1, "renombra los helpers de sftp.rs según la lista")),
    (899, "turn_started", started("mecanico", 1, "gpt-6-luna")),
    (860, "activity", act("mecanico", 1, "edita crates/shared/src/sftp.rs")),
    (800, "activity", act("mecanico", 1, "$ cargo fmt --all")),
    (760, "activity", act("mecanico", 1, "$ cargo fmt --all --check && cargo clippy … && cargo test …")),
    (700, "turn_finished", finished("mecanico", 1, "th-mec", "4 helpers renombrados; compuerta en verde",
                                    150_000, 12_000, 199, ["crates/shared/src/sftp.rs"])),
    (240, "brief_sent", brief("revisor", 1, "El diff a revisar es git diff a1b2c3d..HEAD")),
    (239, "turn_started", started("revisor", 1, "gpt-6.1-sol", None, 1)),
    (200, "activity", act("revisor", 1, "$ git diff a1b2c3d..HEAD --stat")),
    (60, "activity", act("revisor", 1, "$ sed -n 1,200p crates/shared/src/sftp.rs")),
])

# Waiting on the user: the reviewer's thread is open in titan-agent (busy).
run("sftp-enlaces", [
    (5400, "brief_sent", brief("disenador_tests", 1, "encargo inicial")),
    (5399, "turn_started", started("disenador_tests", 1, "gpt-6.1-sol")),
    (5250, "turn_finished", finished("disenador_tests", 1, "th-links-tests", "3 tests de aceptación",
                                     70_000, 5_100, 149, ["crates/shared/tests/symlink_acceptance.rs"])),
    (3000, "brief_sent", brief("revisor", 1, "El diff a revisar es git diff 9f8e7d6..HEAD")),
    (2999, "turn_started", started("revisor", 1, "gpt-6.1-sol", None, 1)),
    (2998, "turn_finished", finished("revisor", 1, None, "hilo de revisor abierto en titan-agent",
                                     0, 0, 1, status="busy")),
])

# --- Codex threads (fake app-server) ----------------------------------------

WT = os.path.join(SSH, ".agents", "worktrees")
TESTS_PROMPT = ("Eres el DISEÑADOR DE TESTS DE ACEPTACIÓN de un equipo. El director (otro agente) "
                "implementará después la especificación de abajo.\n\n# Encargo\n\n"
                "Renombrar un fichero remoto por SFTP desde el explorador: orden `rename` del "
                "agente, error claro si el destino existe.")


def user(text):
    return {"type": "userMessage", "content": [{"type": "text", "text": text}]}


def agent(text):
    return {"type": "agentMessage", "text": text}


def cmd(command, code=0):
    return {"type": "commandExecution", "command": "/usr/bin/bash -lc '" + command + "'",
            "exitCode": code}


def mechanic_items():
    """A long finished thread (Luna): the read-only view must open at its end."""
    items = [("m1", user("Eres el AYUDANTE MECÁNICO de un equipo.\n\n"
                         "Renombra los helpers de sftp.rs según la lista del encargo."))]
    helpers = ["open_dir", "stat_entry", "rename_entry", "remove_entry", "read_link"]
    for n in range(1, 31):
        name = helpers[n % len(helpers)]
        items.append(("m1", cmd(f"rg -n {name} crates/shared/src")))
        items.append(("m1", {"type": "reasoning", "summary": [
            f"Paso {n}: cambiar las llamadas a {name} en el sitio {n}."], "content": []}))
    items.append(("m1", {"type": "fileChange", "changes": [{"path": "crates/shared/src/sftp.rs"}]}))
    items.append(("m1", cmd("cargo test -p shared sftp")))
    items.append(("m1", agent("Renombrados 4 helpers en sftp.rs; 8/8 tests pasan.")))
    return items


threads = {
    "th-tests": {"cwd": os.path.join(WT, "sftp-renombrar"), "source": "exec",
                 "preview": TESTS_PROMPT, "updated": NOW_S - 990, "model": "gpt-6.1-sol",
                 "items": [
                     ("t1", user(TESTS_PROMPT)),
                     ("t1", agent("Escribo pocos tests de integración desde el contrato, sin leer "
                                  "código de producción.")),
                     ("t1", {"type": "fileChange", "changes": [
                         {"path": "crates/shared/tests/sftp_rename_acceptance.rs",
                          "kind": {"type": "add"}}]}),
                     ("t1", agent("Creado sftp_rename_acceptance.rs con 6 tests:\n\n| Apartado | Test |\n"
                                  "| --- | --- |\n| C1 | rename_in_same_folder |\n"
                                  "| C2 | existing_target_is_refused |\n| C3 | outside_root_is_refused |\n"
                                  "| C4 | progress_events_in_order |")),
                 ]},
    "th-mec": {"cwd": os.path.join(WT, "sftp-renombrar"), "source": "exec",
               "preview": "Eres el AYUDANTE MECÁNICO de un equipo.", "updated": NOW_S - 700,
               "model": "gpt-6-luna",
               "items": mechanic_items()},
    "th-rev": {"cwd": os.path.join(WT, "sftp-renombrar"), "source": "exec", "active": True,
               "preview": "Eres el REVISOR de un equipo.", "updated": NOW_S - 60,
               "model": "gpt-6.1-sol",
               "items": [("r1", user("Eres el REVISOR de un equipo.\n\n# Compuerta (exit 0)\n\n"
                                     "test result: ok. 214 passed")),
                         ("r1", cmd("git diff a1b2c3d..HEAD --stat")),
                         ("r1", agent("Leyendo los tests de aceptación: cubren C1–C4."))]},
    "th-links-tests": {"cwd": os.path.join(WT, "sftp-enlaces"), "source": "exec",
                       "preview": "Eres el DISEÑADOR DE TESTS DE ACEPTACIÓN de un equipo.",
                       "updated": NOW_S - 5250, "model": "gpt-6.1-sol",
                       "items": [("l1", agent("3 tests de aceptación para los enlaces simbólicos."))]},
    "th-web": {"cwd": WEB, "source": "cli", "active": True, "name": "estilos del editor",
               "preview": "ajusta los estilos del editor", "updated": NOW_S - 30, "model": "gpt-6.1-sol",
               "items": [("w1", user("ajusta los estilos del editor")),
                         ("w1", agent("Cambio el espaciado de la barra lateral."))]},
}
quota = {"rateLimits": {"limitId": "codex",
                        "primary": {"usedPercent": 22, "windowDurationMins": 300,
                                    "resetsAt": NOW_S + 3 * 3600},
                        "secondary": {"usedPercent": 41, "windowDurationMins": 10080,
                                      "resetsAt": NOW_S + 4 * 86400}}}
write(os.path.join(DIR, "codex.json"), json.dumps({"threads": threads, "quota": quota},
                                                  ensure_ascii=False))
print(DIR)
