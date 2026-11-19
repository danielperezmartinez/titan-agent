#!/bin/sh
# Prueba de punta a punta de los turnos de titan-team por el app-server de
# Codex (docs/Decisiones/ADR-0013), con el Codex real pero sin el repo real:
# un repo de prueba en una carpeta temporal, con un rol barato (gpt-6-luna,
# esfuerzo bajo) y un encargo inventado. Gasta unas pocas llamadas cortas de
# la cuota de Codex.
#
# Uso: scripts/demo/equipo-appserver.sh <orden>
#   up         compila titan-team, crea el repo y lanza el primer turno del rol
#   hilo       imprime la orden para hablar con el rol desde el terminal de
#              Codex (`codex resume <hilo>`), como haría el chat de titan-agent
#   siguiente  lanza otro turno del rol (`ask -m`): cuenta los turnos del
#              usuario desde el anterior, o sale con 4 si el hilo está ocupado
#   eventos    muestra el log de eventos del encargo
#   down       borra el hilo del rol en Codex (`codex delete`) y la carpeta
#
# Variable: TITAN_EQUIPO_DIR (carpeta, por defecto en /tmp, una por checkout).
set -eu

KIT=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(CDPATH='' cd -- "$KIT/../.." && pwd)
DIR=${TITAN_EQUIPO_DIR:-${TMPDIR:-/tmp}/titan-agent-equipo-$(printf %s "$ROOT" | cksum | cut -d' ' -f1)}
REPO="$DIR/repo"
TEAM="$ROOT/target/debug/titan-team"

team() {
    (cd "$REPO" && env -u CLAUDE_CODE_SESSION_ID "$TEAM" "$@")
}

thread() {
    python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["roles"]["escritor"]["thread_id"] or "")' \
        "$REPO/.agents/state/cometa.json" 2>/dev/null || true
}

case "${1:-}" in
up)
    (cd "$ROOT" && cargo build -q -p titan-team)
    rm -rf "$DIR"
    mkdir -p "$REPO/.agents"
    git -C "$REPO" init -q -b main
    git -C "$REPO" config user.name "Prueba"
    git -C "$REPO" config user.email "prueba@example.invalid"
    git -C "$REPO" commit -q --allow-empty -m inicio
    printf 'Eres el rol de prueba de un equipo. Haz solo lo que pida el encargo o el mensaje.\n' > "$REPO/prompt.md"
    printf 'Crea el fichero cometa.txt con el texto «cometa de papel» y responde solo: hecho\n' > "$REPO/encargo.md"
    cat > "$REPO/.agents/team.toml" <<'EOF'
version = 1
name = "prueba"
[project]
autocommit = true
[roles.director]
cli = "claude"
model = "opus"
launch = false
[roles.escritor]
cli = "codex"
model = "gpt-6-luna"
effort = "low"
sandbox = "workspace-write"
prompt_file = "prompt.md"
timeout_s = 300
EOF
    team new cometa --brief encargo.md
    team ask escritor cometa
    printf '\nHilo del rol: %s\n' "$(thread)"
    ;;
hilo)
    printf 'cd %s && codex resume %s\n' "$REPO/.agents/worktrees/cometa" "$(thread)"
    ;;
siguiente)
    team ask escritor cometa -m "Di en una frase qué hay en el worktree ahora."
    ;;
eventos)
    cat "$REPO/.agents/state/cometa/events.jsonl"
    ;;
down)
    id=$(thread)
    if [ -n "$id" ]; then codex delete --force "$id" || true; fi
    rm -rf "$DIR"
    ;;
*)
    sed -n '2,17p' "$0"
    exit 2
    ;;
esac
