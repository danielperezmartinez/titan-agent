#!/bin/sh
# Falla si la versión del Cargo.toml raíz no supera la de la rama principal.
# Lee [workspace.package] o, si no existe, [package]. Ignora las etiquetas de prerelease.
# Uso: comprobar-version.sh <sha base>
set -e
version() {
  awk '
    /^\[(workspace\.package|package)\]/ { s = 1; next }
    /^\[/ { s = 0 }
    s && /^[[:space:]]*version[[:space:]]*=/ { gsub(/^[^"]*"|".*$/, ""); print; exit }
  '
}
if ! git cat-file -e "$1:Cargo.toml" 2>/dev/null; then
  echo "La rama principal aún no tiene Cargo.toml: no hay versión con la que comparar."
  exit 0
fi
base=$(git show "$1:Cargo.toml" | version)
actual=$(version < Cargo.toml)
[ -n "$actual" ] || { echo "No encuentro la versión en Cargo.toml." >&2; exit 1; }
n() { echo "$1" | sed 's/[-+].*//' | awk -F. '{ printf "%d %d %d", $1, $2, $3 }'; }
set -- $(n "$base") $(n "$actual")
if [ "$4" -gt "$1" ] || { [ "$4" -eq "$1" ] && { [ "$5" -gt "$2" ] || { [ "$5" -eq "$2" ] && [ "$6" -gt "$3" ]; }; }; }; then
  exit 0
fi
echo "La versión $actual debe superar la de la rama principal ($base)." >&2
exit 1
