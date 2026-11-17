#!/bin/sh
# Demo kit of titan-agent: the real window with invented data, for
# screenshots, the visual evaluator and the user's hand tests. Linux with
# KDE Plasma on Wayland (KWin, spectacle, qdbus6, journalctl).
#
# Usage: scripts/demo/demo.sh <command> [args]
#   up                  invented data, fake Codex app-server and a private daemon
#   escenas             list the scenes
#   ui <escena>         open the window in that scene (closes the previous one)
#   shot <escena> [png] capture the scene; only if KWin confirms our window
#   shots [escena...]   one capture per scene (all of them by default)
#   probar [escena]     open the window for the user to click by hand and wait
#                       until it is closed
#   down                close the window, the daemon and the app-server
#
# shot, shots and probar start the demo themselves when it is not running
# and then stop it on the way out (also on Ctrl+C): nothing stays alive.
# Only an explicit `up` leaves it running, until `down`.
#
# Environment: TITAN_DEMO_DIR (invented data, default under
# $XDG_RUNTIME_DIR, one per checkout), TITAN_DEMO_SHOTS
# (captures, default <repo>/.previa/demo), TITAN_DEMO_BIN (binaries, default
# <repo>/target/debug), TITAN_DEMO_WAIT (seconds before a capture, default 6)
# and TITAN_DEMO_SIZE (default 1440x900).
#
# The daemon listens on its own socket and the window never connects to the
# user's daemon: no real session is ever shown. Build first with
# `cargo build -p titan-agent -p titan-agentd`.
set -eu

KIT=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH='' cd -- "$KIT/../.." && pwd)
RUNTIME=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
# The data lives outside the repo and outside HOME: inside a worktree
# (.claude/worktrees/, .agents/worktrees/) every demo project would group
# under this repo, and a path under HOME would show the user's name in the
# captures (the "se ejecutará" line prints the absolute folder). It is
# rewritten on every `up`.
DIR=${TITAN_DEMO_DIR:-$RUNTIME/titan-agent-demo-$(printf %s "$ROOT" | cksum | cut -d' ' -f1)}
SHOTS=${TITAN_DEMO_SHOTS:-$ROOT/.previa/demo}
BIN=${TITAN_DEMO_BIN:-$ROOT/target/debug}
WAIT=${TITAN_DEMO_WAIT:-6}
SIZE=${TITAN_DEMO_SIZE:-1440x900}
# Pids and sockets (a Unix socket path has at most 108 bytes: keep it short).
RUN="$DIR/run"
export TITAN_DEMO_DIR="$DIR" TITAN_DEMO_RUN="$RUN"

die() { echo "demo: $*" >&2; exit 1; }

alive() { [ -f "$RUN/$1.pid" ] && kill -0 "$(cat "$RUN/$1.pid")" 2>/dev/null; }

stop() {
  if alive "$1"; then kill "$(cat "$RUN/$1.pid")" 2>/dev/null || true; fi
  rm -f "$RUN/$1.pid"
}

# Scenes: name, description and the variables the window reads. Each scene
# sets its own variables here, so a capture never depends on the caller's
# shell splitting words.
SCENES="inicio terminal filtro nueva nueva-menu nueva-error ajustes grafo carriles mesa hilo hilo-largo hilo-confirmar"

describe() {
  case "$1" in
    inicio) echo "árbol de sesiones, nada elegido" ;;
    terminal) echo "terminal de la sesión del director (claude attach falso)" ;;
    filtro) echo "árbol filtrado por «sftp»" ;;
    nueva) echo "nueva sesión con un encargo escrito" ;;
    nueva-menu) echo "nueva sesión con el menú de modelo abierto" ;;
    nueva-error) echo "nueva sesión con un lanzamiento rechazado" ;;
    ajustes) echo "ajustes por defecto de nueva sesión" ;;
    grafo) echo "monitor de titan-ssh, vista grafo (revisor trabajando, otro encargo te necesita)" ;;
    carriles) echo "monitor de titan-ssh, vista carriles" ;;
    mesa) echo "monitor de titan-ssh, vista mesa" ;;
    hilo) echo "hilo de Sol (tests) en solo lectura (desde la 0.5.0)" ;;
    hilo-largo) echo "hilo largo del revisor: debe abrir abajo (desde la 0.5.0)" ;;
    hilo-confirmar) echo "hilo con la confirmación de tomar el control (desde la 0.5.0)" ;;
    *) return 1 ;;
  esac
}

scene_env() {
  case "$1" in
    inicio) ;;
    terminal) export TITAN_AGENT_SELECT="sftp renombrar" ;;
    filtro) export TITAN_AGENT_FILTER="sftp" ;;
    nueva) export TITAN_AGENT_NEW=1 TITAN_AGENT_DEMO_BRIEF="Añade la orden rename al explorador SFTP.\nSi el destino existe, error claro sin tocar nada." ;;
    nueva-menu) scene_env nueva; export TITAN_AGENT_DEMO_MENU=model ;;
    nueva-error) scene_env nueva; export TITAN_AGENT_DEMO_ERROR="La carpeta no existe: ~/dev/titan-sh" ;;
    ajustes) export TITAN_AGENT_SETTINGS=1 ;;
    grafo | carriles | mesa) export TITAN_AGENT_PROJECT=titan-ssh TITAN_AGENT_MONITOR="$1" ;;
    hilo) scene_env grafo; export TITAN_AGENT_THREAD=disenador_tests ;;
    hilo-largo) scene_env grafo; export TITAN_AGENT_THREAD=revisor ;;
    hilo-confirmar) scene_env hilo; export TITAN_AGENT_DEMO_CONFIRM=1 ;;
    *) die "escena desconocida: $1 (demo.sh escenas)" ;;
  esac
}

display_env() {
  export WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-1}" XDG_RUNTIME_DIR="$RUNTIME"
  export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=$RUNTIME/bus}"
}

up() {
  [ -x "$BIN/titan-agentd" ] || die "falta $BIN/titan-agentd (cargo build -p titan-agent -p titan-agentd)"
  down >/dev/null
  mkdir -p "$RUN" "$DIR"
  python3 -I "$KIT/datos.py" "$DIR" >/dev/null
  nohup python3 -I "$KIT/appserver.py" "$RUN/codex.sock" "$DIR/codex.json" >"$DIR/appserver.log" 2>&1 &
  echo $! >"$RUN/appserver.pid"
  HOME="$DIR/home" TITAN_AGENTD_SOCKET="$RUN/agentd.sock" TITAN_AGENTD_CLAUDE="$KIT/bin/claude" \
    TITAN_AGENTD_CODEX="$KIT/bin/codex" TITAN_AGENTD_POLL_MS=1000 \
    nohup "$BIN/titan-agentd" >"$DIR/daemon.log" 2>&1 &
  echo $! >"$RUN/daemon.pid"
  sleep 1
  alive daemon || die "el daemon no arrancó: $DIR/daemon.log"
  echo "demo en marcha: datos en $DIR, daemon privado en $RUN/agentd.sock"
}

down() {
  stop ui
  stop daemon
  stop appserver
  echo "demo parada"
}

ui() {
  [ $# -eq 1 ] || die "uso: demo.sh ui <escena>"
  describe "$1" >/dev/null || die "escena desconocida: $1 (demo.sh escenas)"
  [ -x "$BIN/titan-agent" ] || die "falta $BIN/titan-agent (cargo build -p titan-agent)"
  alive daemon || die "la demo no está en marcha (demo.sh up)"
  stop ui
  (
    display_env
    scene_env "$1"
    export HOME="$DIR/home" TITAN_AGENTD_SOCKET="$RUN/agentd.sock" TITAN_AGENT_CONFIG_DIR="$DIR/config"
    # If the private daemon dies, the window starts another one with its
    # own environment: it must also get the fake CLIs, never the real ones.
    export TITAN_AGENTD_CLAUDE="$KIT/bin/claude" TITAN_AGENTD_CODEX="$KIT/bin/codex" TITAN_AGENTD_POLL_MS=1000
    nohup "$BIN/titan-agent" >"$DIR/ui.log" 2>&1 &
    echo $! >"$RUN/ui.pid"
  )
}

# Runs a KWin script and prints what it printed (journal lines with $2).
kwin() {
  js="$RUN/kwin-$2.js"
  printf '%s\n' "$1" >"$js"
  since=$(date '+%Y-%m-%d %H:%M:%S')
  qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript "$js" "$2" >/dev/null
  qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.start >/dev/null
  sleep 0.5
  qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.unloadScript "$2" >/dev/null 2>&1 || true
  journalctl --user --since "$since" 2>/dev/null | grep "$2 pid " | tail -1 | sed 's/.* pid //'
  rm -f "$js"
}

active_pid() {
  kwin "const a = workspace.activeWindow; print(\"$1 pid \" + (a ? a.pid : -1));" "$1"
}

focus() {
  kwin "const ws = workspace.windowList ? workspace.windowList() : workspace.clientList();
for (const w of ws) { if (w.pid === $1 && w.normalWindow) { workspace.activeWindow = w; } }
const a = workspace.activeWindow; print(\"$2 pid \" + (a ? a.pid : -1));" "$2"
}

# Starts the demo for this command if it is not running, and stops it when
# the command ends, however it ends.
session() {
  alive daemon && return 0
  trap 'down >/dev/null' EXIT
  trap 'exit 130' INT TERM HUP
  up
}

shot() {
  [ $# -ge 1 ] && [ $# -le 2 ] || die "uso: demo.sh shot <escena> [fichero.png]"
  # A failed capture returns 1 instead of exiting, so shots goes on.
  miss() { echo "demo: $*" >&2; return 1; }
  out=${2:-$SHOTS/$1.png}
  mkdir -p "$(dirname "$out")"
  # An old capture with this name must not survive a failed one.
  rm -f "$out"
  ui "$1"
  sleep "$WAIT"
  display_env
  pid=$(cat "$RUN/ui.pid")
  alive ui || { miss "SIN CAPTURA de $1: la ventana se cerró ($DIR/ui.log)"; return 1; }
  tag="titandemo$$"
  got=$(focus "$pid" "$tag")
  if [ "$got" != "$pid" ]; then
    stop ui
    miss "SIN CAPTURA de $1: la ventana activa es el PID '$got', no la nuestra ($pid)"
    return 1
  fi
  rm -f "$out"
  spectacle -b -n -a -e -o "$out" >/dev/null 2>&1 || true
  # Checked again after capturing: if another window took the focus in
  # between, the file may show it and is deleted.
  after=$(active_pid "${tag}b")
  stop ui
  if [ "$after" != "$pid" ]; then
    rm -f "$out"
    miss "SIN CAPTURA de $1: el foco pasó al PID '$after' durante la captura"
    return 1
  fi
  [ -f "$out" ] || { miss "SIN CAPTURA de $1: spectacle no escribió $out"; return 1; }
  size=$(magick identify -format '%wx%h' "$out" 2>/dev/null || echo "?")
  if [ "$size" != "$SIZE" ]; then
    rm -f "$out"
    miss "SIN CAPTURA de $1: mide $size y se esperaba $SIZE"
    return 1
  fi
  echo "$1 → $out"
}

case "${1:-}" in
  up) up ;;
  down) down ;;
  escenas) for s in $SCENES; do printf '%-15s %s\n' "$s" "$(describe "$s")"; done ;;
  ui) shift; ui "$@"; echo "ventana abierta (PID $(cat "$RUN/ui.pid"))" ;;
  shot) shift; session; shot "$@" ;;
  shots)
    shift
    session
    failed=0
    for s in ${*:-$SCENES}; do shot "$s" || failed=1; done
    exit "$failed"
    ;;
  probar)
    session
    ui "${2:-inicio}"
    echo "ventana abierta para probar a mano (PID $(cat "$RUN/ui.pid")); datos inventados."
    echo "Sigue los pasos de la nota de la tarea. Al cerrar la ventana se para todo."
    while alive ui; do sleep 1; done
    ;;
  *) sed -n '2,27p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
