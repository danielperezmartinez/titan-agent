#!/bin/sh
# Print the [workspace.package] version from the current directory's Cargo.toml
# if it is publishable: only 0.x.y, with no pre-release or build metadata
# (docs/Decisiones/ADR-0011, §2). Otherwise, exit with 1. Used by the workflow
# «Publish version».
# Usage: version-publicable.sh
set -e
[ -f Cargo.toml ] || { echo "No hay Cargo.toml en $(pwd)." >&2; exit 1; }
v=$(awk '
  /^\[workspace\.package\]/ { s = 1; next }
  /^\[/ { s = 0 }
  s && /^[[:space:]]*version[[:space:]]*=/ { gsub(/^[^"]*"|".*$/, ""); print; exit }
' Cargo.toml)
if printf '%s\n' "$v" | grep -Eq '^0\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'; then
  printf '%s\n' "$v"
  exit 0
fi
echo "La versión «$v» no se puede publicar: solo 0.x.y (nada de 1.x sin decisión del usuario, ni pre-releases)." >&2
exit 1
