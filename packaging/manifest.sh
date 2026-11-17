#!/bin/sh
# Write a version manifest, titan-agent-update.json (format 1,
# docs/Decisiones/ADR-0011, §3), from the artifacts in a directory.
# Usage: packaging/manifest.sh <versión> <carpeta de artefactos>
set -eu
[ $# -eq 2 ] || { echo "uso: manifest.sh <versión> <carpeta>" >&2; exit 2; }
version=$1 dir=$2
artifact() {
  file="$dir/titan-agent-$version-$1.$2"
  [ -f "$file" ] || return 0
  sha=$(sha256sum "$file" | cut -d' ' -f1)
  size=$(wc -c < "$file" | tr -d ' ')
  printf '{"target":"%s","name":"%s","sha256":"%s","size":%s}' "$1" "$(basename "$file")" "$sha" "$size"
}
linux=$(artifact linux-x86_64 tar.gz)
windows=$(artifact windows-x86_64 zip)
[ -n "$linux" ] || { echo "falta el tar.gz de Linux en $dir" >&2; exit 1; }
list=$linux
[ -z "$windows" ] || list="$list,$windows"
printf '{"format":1,"version":"%s","published":"%s","notes_url":"https://github.com/danielperezmartinez/titan-agent/releases/tag/v%s","artifacts":[%s]}\n' \
  "$version" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$version" "$list" > "$dir/titan-agent-update.json"
echo "$dir/titan-agent-update.json"
