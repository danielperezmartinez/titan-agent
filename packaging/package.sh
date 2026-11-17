#!/bin/sh
# Empaqueta una versión de titan-agent para Linux (docs/Decisiones/ADR-0011,
# §1): titan-agent-<v>-<target>/ con bin/ (los cuatro binarios), share/ e
# install.sh, en un tar.gz.
# Uso: packaging/package.sh <versión> <target> <carpeta de binarios> <salida>
#   p. ej. packaging/package.sh 0.6.0 linux-x86_64 target/release dist
# Imprime la ruta del tar.gz.
set -eu
[ $# -eq 4 ] || { echo "uso: package.sh <versión> <target> <binarios> <salida>" >&2; exit 2; }
version=$1 target=$2 bins=$3 out=$4
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
name="titan-agent-$version-$target"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/$name/bin" "$work/$name/share" "$out"
for b in titan-agent titan-agentd titan-team titan-agent-launcher; do
  [ -f "$bins/$b" ] || { echo "falta $bins/$b" >&2; exit 1; }
  cp "$bins/$b" "$work/$name/bin/$b"
  chmod 0755 "$work/$name/bin/$b"
done
cp "$here/install.sh" "$work/$name/install.sh"
chmod 0755 "$work/$name/install.sh"
cp "$here/LEEME.txt" "$work/$name/share/LEEME.txt"
# Entradas ordenadas, sin dueño ni grupo del equipo que empaqueta.
tar --sort=name --owner=0 --group=0 --numeric-owner --format=gnu \
  -C "$work" -czf "$out/$name.tar.gz" "$name"
echo "$out/$name.tar.gz"
