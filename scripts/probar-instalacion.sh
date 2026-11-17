#!/bin/sh
# Test the Linux tar.gz without touching the user's installation: extract it,
# run install.sh twice and uninstall in a temporary HOME, then check the result
# (docs/Decisiones/ADR-0011, §4). Used by CI.
# Usage: scripts/probar-instalacion.sh <titan-agent-<v>-linux-x86_64.tar.gz>
set -eu
[ $# -eq 1 ] || { echo "uso: probar-instalacion.sh <tar.gz>" >&2; exit 2; }
tarball=$(CDPATH='' cd -- "$(dirname -- "$1")" && pwd)/$(basename -- "$1")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/home" "$work/unpack"
tar -xzf "$tarball" -C "$work/unpack"
bundle=$(find "$work/unpack" -mindepth 1 -maxdepth 1 -type d)
version=$(basename "$bundle" | sed 's/^titan-agent-//; s/-linux-x86_64$//')
export HOME="$work/home"
unset XDG_DATA_HOME XDG_CONFIG_HOME
root="$HOME/.local/share/titan-agent"
desktop="$HOME/.local/share/applications/io.github.danielperezmartinez.titanagent.desktop"
fail() { echo "FALLO: $*" >&2; exit 1; }

"$bundle/install.sh" >/dev/null
[ "$(readlink "$root/current")" = "versions/$version" ] || fail "current no apunta a versions/$version"
for b in titan-agent titan-agentd titan-team titan-agent-launcher; do
  [ -x "$root/current/bin/$b" ] || fail "falta current/bin/$b"
done
[ "$(readlink "$HOME/.local/bin/titan-agent")" = "$root/titan-agent-launcher" ] || fail "enlace titan-agent"
[ "$(readlink "$HOME/.local/bin/titan-team")" = "$root/current/bin/titan-team" ] || fail "enlace titan-team"
grep -qx "Exec=$HOME/.local/bin/titan-agent" "$desktop" || fail "Exec del .desktop"
[ "$("$root/titan-agent-launcher" --version)" = "titan-agent-launcher $version" ] || fail "versión del lanzador"
before=$(cd "$HOME" && find . | sort | cksum)
"$bundle/install.sh" >/dev/null
[ "$(cd "$HOME" && find . | sort | cksum)" = "$before" ] || fail "install no es idempotente"
mkdir -p "$HOME/.config/titan-agent" && echo mio > "$HOME/.config/titan-agent/keep"
"$root/titan-agent-launcher" uninstall >/dev/null
[ ! -e "$root" ] && [ ! -e "$desktop" ] && [ ! -L "$HOME/.local/bin/titan-agent" ] || fail "uninstall dejó restos"
[ "$(cat "$HOME/.config/titan-agent/keep")" = mio ] || fail "uninstall tocó la configuración"
echo "instalación de $version: install, install otra vez y uninstall correctos"
