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
#   flujos              list the end-to-end kits (real flows, not fixed states)
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
# and TITAN_DEMO_SIZE (window size of the scenes that do not set their own,
# default 1440x900).
#
# A scene may set, besides the window's own variables, TITAN_AGENT_DEMO_SIZE
# (its window size, WxH) and TITAN_AGENT_DEMO_KEYS (keys the window applies
# 1.5 s after opening: text, {enter}, {shift-enter}, {ctrl-n}, {pause 500};
# syntax in crates/titan-agent/src/demo.rs). The invented config folder has
# a projects.json (ADR-0014) with an imported project without sessions, one
# whose folder is gone and web-notas left out (its sessions go to "otras
# carpetas").
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
SCENES="inicio terminal filtro nueva nueva-teclas nueva-estrecho nueva-menu nueva-error lanzar directora directora-monitor directora-encargos dos-equipos resumen resumen-sin-directora monitor-sin-directora borrador-proyecto proyectos-estrecho ajustes grafo carriles mesa hilo hilo-largo hilo-confirmar act-lista act-estrecho act-descarga act-confirmar act-aplicando act-fallo act-atras act-daemon act-ajustes"

describe() {
  case "$1" in
    inicio) echo "árbol de proyectos importados (uno sin sesiones, uno borrado) y otras carpetas" ;;
    terminal) echo "terminal de la sesión del director (claude attach falso)" ;;
    filtro) echo "árbol filtrado por «sftp»" ;;
    nueva) echo "nueva sesión con un encargo escrito" ;;
    nueva-teclas) echo "borrador con un encargo, una lista y un elemento sangrado escritos por el guion de teclas" ;;
    nueva-estrecho) echo "borrador con el encargo escrito en una ventana estrecha (1000x800)" ;;
    lanzar) echo "borrador: el guion escribe un encargo y pulsa Enter; la vista pasa a la sesión lanzada" ;;
    directora) echo "sesión directora (sftp renombrar): barra con [m] monitor, [*] en el árbol" ;;
    directora-monitor) echo "[m] de la directora: su monitor en lugar del terminal, con la miga y el selector" ;;
    directora-encargos) echo "monitor de la directora con el selector de sus dos encargos abierto" ;;
    dos-equipos) echo "segunda directora del mismo proyecto (sincronizar hosts) con su propio monitor" ;;
    resumen) echo "resumen de titan-ssh: acciones, sus tres equipos (dos directoras) y sus sesiones" ;;
    resumen-sin-directora) echo "resumen de recetas (importado, sin sesiones): un equipo sin sesión directora" ;;
    monitor-sin-directora) echo "[m] del equipo sin directora: el monitor en el resumen, con [<] resumen" ;;
    borrador-proyecto) echo "el + de recetas: borrador con ese proyecto puesto" ;;
    proyectos-estrecho) echo "árbol con importados, sin sesiones, carpeta borrada y otras carpetas a 1000x800" ;;
    nueva-menu) echo "nueva sesión con el menú de modelo abierto" ;;
    nueva-error) echo "nueva sesión con un lanzamiento rechazado" ;;
    ajustes) echo "ajustes por defecto de nueva sesión" ;;
    grafo) echo "monitor de la directora sftp renombrar, vista grafo (revisor trabajando)" ;;
    carriles) echo "monitor de la directora sftp renombrar, vista carriles" ;;
    mesa) echo "monitor de la directora sftp renombrar, vista mesa" ;;
    hilo) echo "hilo de Sol (tests) en solo lectura" ;;
    hilo-largo) echo "hilo largo de Luna (terminado): debe abrir abajo" ;;
    hilo-confirmar) echo "hilo con la confirmación de tomar el control" ;;
    act-lista) echo "aviso [^] con una versión nueva lista (actualizar)" ;;
    act-estrecho) echo "aviso [^] con una versión lista en una ventana estrecha (1000x800)" ;;
    act-descarga) echo "aviso [^] descargando la versión nueva" ;;
    act-confirmar) echo "aviso [^] con la confirmación en línea (2 sesiones trabajando)" ;;
    act-aplicando) echo "aviso [^] actualizando" ;;
    act-fallo) echo "aviso [^] de una versión que no se pudo verificar" ;;
    act-atras) echo "aviso [^] de vuelta atrás reciente, con [x]" ;;
    act-daemon) echo "aviso [^] de daemon de otra versión (reiniciar)" ;;
    act-ajustes) echo "Configuración con el bloque ACTUALIZACIONES y una versión lista" ;;
    *) return 1 ;;
  esac
}

scene_env() {
  case "$1" in
    inicio) ;;
    terminal) export TITAN_AGENT_SELECT="sftp renombrar" ;;
    filtro) export TITAN_AGENT_FILTER="sftp" ;;
    nueva) export TITAN_AGENT_NEW=1 TITAN_AGENT_DEMO_BRIEF="Añade la orden rename al explorador SFTP.\nSi el destino existe, error claro sin tocar nada." ;;
    nueva-teclas)
      export TITAN_AGENT_NEW=1
      export TITAN_AGENT_DEMO_KEYS="Añade la orden rename al explorador SFTP.{shift-enter}{shift-enter}- Si el destino existe, error claro.{shift-enter}{tab}con el nombre del fichero{shift-enter}{shift-tab}Sin tocar nada en el servidor."
      ;;
    nueva-estrecho) scene_env nueva-teclas; export TITAN_AGENT_DEMO_SIZE=1000x800 ;;
    lanzar) export TITAN_AGENT_NEW=1 TITAN_AGENT_DEMO_KEYS="Revisa el changelog de la 0.7.0{enter}" ;;
    directora) export TITAN_AGENT_SELECT="sftp renombrar" ;;
    directora-monitor) scene_env directora; export TITAN_AGENT_BODY=monitor ;;
    directora-encargos) scene_env directora-monitor; export TITAN_AGENT_DEMO_MENU=task ;;
    dos-equipos) export TITAN_AGENT_SELECT="sincronizar hosts" TITAN_AGENT_BODY=monitor ;;
    nueva-menu) scene_env nueva; export TITAN_AGENT_DEMO_MENU=model ;;
    nueva-error) scene_env nueva; export TITAN_AGENT_DEMO_ERROR="La carpeta no existe: ~/dev/titan-sh" ;;
    ajustes) export TITAN_AGENT_SETTINGS=1 ;;
    grafo | carriles | mesa) export TITAN_AGENT_SELECT="sftp renombrar" TITAN_AGENT_BODY=monitor TITAN_AGENT_MONITOR="$1" ;;
    resumen) export TITAN_AGENT_PROJECT=titan-ssh ;;
    resumen-sin-directora) export TITAN_AGENT_PROJECT=recetas ;;
    monitor-sin-directora) export TITAN_AGENT_PROJECT=recetas TITAN_AGENT_BODY=monitor ;;
    # The invented HOME of the demo, not a personal folder.
    borrador-proyecto) export TITAN_AGENT_NEW=1 TITAN_AGENT_FOLDER="$DIR/home/dev/recetas" ;; # gitleaks:allow
    proyectos-estrecho) export TITAN_AGENT_DEMO_SIZE=1000x800 ;;
    hilo) scene_env grafo; export TITAN_AGENT_THREAD=disenador_tests ;;
    hilo-largo) scene_env grafo; export TITAN_AGENT_THREAD=mecanico ;;
    hilo-confirmar) scene_env hilo; export TITAN_AGENT_DEMO_CONFIRM=1 ;;
    # The updater of the demo daemon is off (a build tree): the window shows
    # a fixed state of the notice instead.
    act-lista) export TITAN_AGENT_DEMO_UPDATE=ready ;;
    act-estrecho) export TITAN_AGENT_DEMO_UPDATE=ready TITAN_AGENT_DEMO_SIZE=1000x800 ;;
    act-descarga) export TITAN_AGENT_DEMO_UPDATE=downloading ;;
    act-confirmar) export TITAN_AGENT_DEMO_UPDATE=confirm ;;
    act-aplicando) export TITAN_AGENT_DEMO_UPDATE=applying ;;
    act-fallo) export TITAN_AGENT_DEMO_UPDATE=failed ;;
    act-atras) export TITAN_AGENT_DEMO_UPDATE=rollback ;;
    act-daemon) export TITAN_AGENT_DEMO_UPDATE=daemon ;;
    act-ajustes) export TITAN_AGENT_SETTINGS=1 TITAN_AGENT_DEMO_UPDATE=ready ;;
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
    TITAN_AGENTD_CODEX="$KIT/bin/codex" TITAN_AGENTD_POLL_MS=1000 TITAN_AGENT_CONFIG_DIR="$DIR/config" \
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

# Drops the TITAN_AGENT_* variables inherited from the caller's shell, so a
# variable left over from a hand test never changes a scene (the daemon's
# TITAN_AGENTD_* are set explicitly where they are used).
clean_env() {
  for var in $(env | sed -n 's/^\(TITAN_AGENT_[A-Za-z0-9_]*\)=.*/\1/p'); do
    unset "$var"
  done
}

ui() {
  [ $# -eq 1 ] || die "uso: demo.sh ui <escena>"
  describe "$1" >/dev/null || die "escena desconocida: $1 (demo.sh escenas)"
  [ -x "$BIN/titan-agent" ] || die "falta $BIN/titan-agent (cargo build -p titan-agent)"
  alive daemon || die "la demo no está en marcha (demo.sh up)"
  stop ui
  (
    display_env
    clean_env
    # shot turns the hover styles off (NOHOVER), probar keeps them.
    [ -z "${NOHOVER:-}" ] || export TITAN_AGENT_DEMO_NO_HOVER=1
    export TITAN_AGENT_DEMO_SIZE="$SIZE"
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
  # The size the scene asks for (scene_env runs in a subshell: no leak).
  want=$(clean_env; TITAN_AGENT_DEMO_SIZE="$SIZE"; scene_env "$1"; printf %s "$TITAN_AGENT_DEMO_SIZE")
  mkdir -p "$(dirname "$out")"
  # An old capture with this name must not survive a failed one.
  rm -f "$out"
  # The window opens under the pointer: without hover styles a capture does
  # not depend on where it was left (probar keeps them, for the user).
  NOHOVER=1
  ui "$1"
  # A key script types for a while after the window opens: give it time.
  keys=$(clean_env; scene_env "$1"; printf %s "${TITAN_AGENT_DEMO_KEYS:-}")
  sleep "$WAIT"
  if [ -n "$keys" ]; then
    # The window says when its key script is done (20 s at most).
    n=0
    while [ "$n" -lt 40 ] && ! grep -q "guion de teclas terminado" "$DIR/ui.log" 2>/dev/null; do
      sleep 0.5
      n=$((n + 1))
    done
    sleep 1
  fi
  display_env
  pid=$(cat "$RUN/ui.pid")
  alive ui || { miss "SIN CAPTURA de $1: la ventana se cerró: $(tail -n 2 "$DIR/ui.log")"; return 1; }
  tag="titandemo$$"
  got=$(focus "$pid" "$tag")
  if [ "$got" != "$pid" ]; then
    # A window that died (a panic) also loses the focus: say which.
    alive ui || { miss "SIN CAPTURA de $1: la ventana se cerró: $(tail -n 2 "$DIR/ui.log")"; return 1; }
    stop ui
    miss "SIN CAPTURA de $1: la ventana activa es el PID '$got', no la nuestra ($pid)"
    return 1
  fi
  rm -f "$out"
  spectacle -b -n -a -e -o "$out" >/dev/null 2>&1 || true
  # Checked again after capturing: if another window took the focus in
  # between, the file may show it and is deleted.
  after=$(active_pid "${tag}b")
  closed=0
  alive ui || closed=1
  stop ui
  if [ "$after" != "$pid" ]; then
    rm -f "$out"
    [ "$closed" = 0 ] || { miss "SIN CAPTURA de $1: la ventana se cerró: $(tail -n 2 "$DIR/ui.log")"; return 1; }
    miss "SIN CAPTURA de $1: el foco pasó al PID '$after' durante la captura"
    return 1
  fi
  [ -f "$out" ] || { miss "SIN CAPTURA de $1: spectacle no escribió $out"; return 1; }
  size=$(magick identify -format '%wx%h' "$out" 2>/dev/null || echo "?")
  if [ "$size" != "$want" ]; then
    rm -f "$out"
    miss "SIN CAPTURA de $1: mide $size y se esperaba $want"
    return 1
  fi
  echo "$1 → $out"
}

case "${1:-}" in
  up) up ;;
  down) down ;;
  escenas) for s in $SCENES; do printf '%-15s %s\n' "$s" "$(describe "$s")"; done ;;
  flujos)
    # Kits that drive a real flow end to end (a service, an update…),
    # next to this one: their second line says what they test.
    for kit in "$KIT"/*.sh; do
      [ "$kit" = "$KIT/demo.sh" ] && continue
      printf '%-18s %s\n' "$(basename "$kit")" "$(sed -n '2s/^# *//p' "$kit")"
    done
    ;;
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
  *) sed -n '2,38p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
