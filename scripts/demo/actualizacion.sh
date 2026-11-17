#!/bin/sh
# Prueba de la actualización de punta a punta con la ventana real
# (docs/Decisiones/ADR-0011, §6–§7), sin el repo real ni la instalación del
# usuario: todo vive en una carpeta temporal, con su propio socket, datos
# inventados (los del kit de demo), CLI falsos, un servidor HTTP local y una
# clave de prueba generada aquí. Builds de debug, que son las únicas que
# aceptan otra URL y otra clave.
#
# Uso: scripts/demo/actualizacion.sh <orden>
#   preparar   compila la versión actual y la siguiente (parche + 1, en una
#              copia de las fuentes), las empaqueta, firma el manifiesto de la
#              siguiente e instala la actual (tarda: compila dos veces)
#   abrir      arranca el servidor y abre titan-agent instalado (el menú no se
#              toca: se abre con su lanzador); espera a que se cierre la ventana
#   romper     publica en su lugar una versión siguiente que no arranca (para
#              probar la vuelta atrás); `preparar` la deja bien otra vez
#   estado     muestra current, pending.json, bad.json y rolled-back.json
#   down       para el servidor, el daemon y la ventana
#
# Variables: TITAN_ACT_DIR (carpeta, por defecto en /tmp, una por checkout),
# TITAN_ACT_PORT (puerto del servidor, 8765).
set -eu

KIT=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH='' cd -- "$KIT/../.." && pwd)
DIR=${TITAN_ACT_DIR:-${TMPDIR:-/tmp}/titan-agent-act-$(printf %s "$ROOT" | cksum | cut -d' ' -f1)}
PORT=${TITAN_ACT_PORT:-8765}
DATA="$DIR/data"
INSTALL="$DATA/titan-agent"
RUN="$DIR/run"
export TITAN_DEMO_DIR="$DIR/demo" TITAN_DEMO_RUN="$RUN"

die() { echo "actualizacion: $*" >&2; exit 1; }

version() {
  awk '/^\[workspace\.package\]/ { s = 1; next } /^\[/ { s = 0 }
    s && /^[[:space:]]*version[[:space:]]*=/ { gsub(/^[^"]*"|".*$/, ""); print; exit }' "$1"
}

current=$(version "$ROOT/Cargo.toml")
next=$(echo "$current" | awk -F. '{ printf "%d.%d.%d", $1, $2, $3 + 1 }')

alive() { [ -f "$RUN/$1.pid" ] && kill -0 "$(cat "$RUN/$1.pid")" 2>/dev/null; }
stop() { if alive "$1"; then kill "$(cat "$RUN/$1.pid")" 2>/dev/null || true; fi; rm -f "$RUN/$1.pid"; }

# Publishes the archive of `next` from $DIR/pkg-<next> with a signed manifest.
publish() {
  www="$DIR/www/releases"
  rm -rf "$DIR/www"
  mkdir -p "$www/latest/download" "$www/download/v$next"
  cp "$DIR/pkg-$1/titan-agent-$next-linux-x86_64.tar.gz" "$www/download/v$next/"
  sh "$ROOT/packaging/manifest.sh" "$next" "$www/download/v$next" >/dev/null
  mv "$www/download/v$next/titan-agent-update.json" "$www/latest/download/"
  minisign -S -s "$DIR/prueba.key" -m "$www/latest/download/titan-agent-update.json" \
    -t "titan-agent-update format=1 version=$next" >/dev/null
}

preparar() {
  command -v minisign >/dev/null || die "falta minisign"
  mkdir -p "$DIR" "$RUN"
  echo "1/4 compilando $current (este checkout)…"
  (cd "$ROOT" && cargo build -q -p titan-agent -p titan-agentd -p titan-team -p titan-agent-launcher)
  rm -rf "$DIR/pkg-actual" && sh "$ROOT/packaging/package.sh" "$current" linux-x86_64 "$ROOT/target/debug" "$DIR/pkg-actual" >/dev/null
  echo "2/4 compilando $next (copia de las fuentes con la versión subida)…"
  src="$DIR/src-$next"
  rm -rf "$src" && mkdir -p "$src"
  cp -r "$ROOT/Cargo.toml" "$ROOT/Cargo.lock" "$ROOT/rust-toolchain.toml" "$ROOT/crates" "$ROOT/packaging" "$src/"
  sed -i "s/^version = \"$current\"/version = \"$next\"/" "$src/Cargo.toml"
  # The team's shared build folder: only the workspace crates are rebuilt.
  (cd "$src" && CARGO_TARGET_DIR="$ROOT/.agents/target" cargo build -q -p titan-agent -p titan-agentd -p titan-team -p titan-agent-launcher)
  rm -rf "$DIR/pkg-bien" && sh "$ROOT/packaging/package.sh" "$next" linux-x86_64 "$ROOT/.agents/target/debug" "$DIR/pkg-bien" >/dev/null
  # The broken one: the same, with a window that exits at once.
  rm -rf "$DIR/pkg-rota" "$DIR/rota" && mkdir -p "$DIR/rota"
  cp "$ROOT/.agents/target/debug/titan-agentd" "$ROOT/.agents/target/debug/titan-team" "$ROOT/.agents/target/debug/titan-agent-launcher" "$DIR/rota/"
  printf '#!/bin/sh\necho "titan-agent %s: fallo de arranque simulado" >&2\nexit 3\n' "$next" > "$DIR/rota/titan-agent"
  chmod 0755 "$DIR/rota/titan-agent"
  sh "$ROOT/packaging/package.sh" "$next" linux-x86_64 "$DIR/rota" "$DIR/pkg-rota" >/dev/null
  echo "3/4 clave de prueba y manifiesto firmado de $next…"
  rm -f "$DIR/prueba.key" "$DIR/prueba.pub"
  minisign -G -W -p "$DIR/prueba.pub" -s "$DIR/prueba.key" >/dev/null
  publish bien
  echo "4/4 instalando $current en $DATA…"
  stop daemon
  rm -rf "$DATA" "$DIR/home"
  python3 -I "$KIT/datos.py" "$TITAN_DEMO_DIR" >/dev/null
  tmp=$(mktemp -d) && tar -xzf "$DIR/pkg-actual/titan-agent-$current-linux-x86_64.tar.gz" -C "$tmp"
  HOME="$TITAN_DEMO_DIR/home" XDG_DATA_HOME="$DATA" "$tmp/titan-agent-$current-linux-x86_64/install.sh" >/dev/null
  rm -rf "$tmp"
  echo "listo: $current instalado; el servidor ofrece $next (abre con: $0 abrir)"
}

abrir() {
  [ -x "$INSTALL/titan-agent-launcher" ] || die "no hay nada instalado ($0 preparar)"
  mkdir -p "$RUN"
  if ! alive server; then
    nohup python3 -I -m http.server "$PORT" --bind 127.0.0.1 --directory "$DIR/www" >"$DIR/server.log" 2>&1 &
    echo $! >"$RUN/server.pid"
  fi
  if ! alive appserver; then
    nohup python3 -I "$KIT/appserver.py" "$RUN/codex.sock" "$TITAN_DEMO_DIR/codex.json" >"$DIR/appserver.log" 2>&1 &
    echo $! >"$RUN/appserver.pid"
  fi
  trap 'stop server; stop appserver' EXIT
  trap 'exit 130' INT TERM HUP
  echo "abriendo titan-agent instalado ($(readlink "$INSTALL/current")); servidor en 127.0.0.1:$PORT"
  # The daemon is started by the window with this environment and stays
  # alive after it closes (that is when it applies); `down` stops it.
  WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-1}" HOME="$TITAN_DEMO_DIR/home" XDG_DATA_HOME="$DATA" \
    TITAN_AGENT_CONFIG_DIR="$DIR/config" TITAN_AGENTD_SOCKET="$RUN/agentd.sock" \
    TITAN_AGENTD_CLAUDE="$KIT/bin/claude" TITAN_AGENTD_CODEX="$KIT/bin/codex" TITAN_AGENTD_POLL_MS=1000 \
    TITAN_AGENT_UPDATE_URL="http://127.0.0.1:$PORT/releases" \
    TITAN_AGENT_UPDATE_KEYS="$(sed -n 2p "$DIR/prueba.pub")" TITAN_AGENT_UPDATE_FIRST_CHECK_S=3 \
    "$INSTALL/titan-agent-launcher" || true
  # The window may reopen through the launcher after «actualizar»: wait
  # until no window of this install is left.
  while pgrep -f "^$INSTALL/(current|versions/[^/]+)/bin/titan-agent( |$)" >/dev/null 2>&1; do sleep 1; done
  if [ -f "$RUN/agentd.pid" ]; then echo $(cat "$RUN/agentd.pid") >"$RUN/daemon.pid"; fi
  echo "ventana cerrada; el daemon sigue vivo para aplicar ($0 estado, $0 down)"
}

estado() {
  echo "current:        $(readlink "$INSTALL/current" 2>/dev/null || echo '-')"
  for f in pending bad rolled-back; do
    printf '%-15s %s\n' "$f.json:" "$(cat "$INSTALL/update/$f.json" 2>/dev/null || echo '-')"
  done
  echo "versiones:      $(ls "$INSTALL/versions" 2>/dev/null | tr '\n' ' ')"
  if [ -f "$RUN/agentd.pid" ] && kill -0 "$(cat "$RUN/agentd.pid")" 2>/dev/null; then
    echo "daemon:         pid $(cat "$RUN/agentd.pid"), $(readlink "/proc/$(cat "$RUN/agentd.pid")/exe")"
  else
    echo "daemon:         parado"
  fi
}

down() {
  if [ -f "$RUN/agentd.pid" ]; then kill "$(cat "$RUN/agentd.pid")" 2>/dev/null || true; fi
  pkill -f "^$INSTALL/(current|versions/[^/]+)/bin/titan-agent( |$)" 2>/dev/null || true
  stop server; stop appserver; stop daemon
  echo "prueba de actualización parada (los datos siguen en $DIR)"
}

case "${1:-}" in
  preparar) preparar ;;
  abrir) abrir ;;
  romper) publish rota; echo "el servidor ofrece ahora una $next que no arranca" ;;
  estado) estado ;;
  down) down ;;
  *) sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
