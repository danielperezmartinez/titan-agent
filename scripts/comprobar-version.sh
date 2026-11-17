#!/bin/sh
# Falla si la versión del Cargo.toml raíz no supera la de la rama principal
# o si es 1.0.0 o mayor.
# Lee [workspace.package] o, si no existe, [package]. Ignora las etiquetas de prerelease.
# Uso: comprobar-version.sh <sha base>
set -e
base_sha=$1
version() {
  awk '
    /^\[(workspace\.package|package)\]/ { s = 1; next }
    /^\[/ { s = 0 }
    s && /^[[:space:]]*version[[:space:]]*=/ { gsub(/^[^"]*"|".*$/, ""); print; exit }
  '
}
n() { echo "$1" | sed 's/[-+].*//' | awk -F. '{ printf "%d %d %d", $1, $2, $3 }'; }
actual=$(version < Cargo.toml)
[ -n "$actual" ] || { echo "No encuentro la versión en Cargo.toml." >&2; exit 1; }
# Nada de 1.x hasta que lo decida el usuario (docs/README.md, Publicación),
# tampoco cuando la base no tiene versión con la que comparar.
set -- $(n "$actual")
if [ "$1" -ge 1 ]; then
  echo "La versión $actual es 1.x o mayor: las versiones son 0.x hasta que el usuario decida la 1.0.0." >&2
  exit 1
fi
if ! git cat-file -e "$base_sha:Cargo.toml" 2>/dev/null; then
  echo "La rama principal aún no tiene Cargo.toml: no hay versión con la que comparar."
  exit 0
fi
base=$(git show "$base_sha:Cargo.toml" | version)
set -- $(n "$base") $(n "$actual")
if [ "$4" -gt "$1" ] || { [ "$4" -eq "$1" ] && { [ "$5" -gt "$2" ] || { [ "$5" -eq "$2" ] && [ "$6" -gt "$3" ]; }; }; }; then
  exit 0
fi
echo "La versión $actual debe superar la de la rama principal ($base)." >&2
exit 1
