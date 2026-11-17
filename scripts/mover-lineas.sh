#!/bin/sh
# Moves a block of lines from one file to another, unchanged, and checks
# that no line was lost or added. For pure code moves (splitting a file that
# passed the size limit), which the director does with this script instead
# of handing them to an agent.
#
# Usage: scripts/mover-lineas.sh <origen> <desde> <hasta> <destino> [<tras>]
#   Moves lines <desde>..<hasta> (1-based, both included) of <origen> to
#   <destino>: after its line <tras> (0 = at the start) or, without <tras>,
#   at the end. <destino> is created if it does not exist. Afterwards fix
#   the imports and visibility by hand: they are not "pure" moves.
set -eu

die() { echo "mover-lineas: $*" >&2; exit 1; }

[ $# -eq 4 ] || [ $# -eq 5 ] || die "uso: mover-lineas.sh <origen> <desde> <hasta> <destino> [<tras>]"
src=$1 from=$2 to=$3 dst=$4 after=${5:-}
case "$from$to$after" in *[!0-9]*) die "las líneas son números" ;; esac
[ -f "$src" ] || die "no existe $src"
[ "$from" -ge 1 ] && [ "$from" -le "$to" ] || die "rango no válido: $from..$to"
total=$(wc -l <"$src")
[ "$to" -le "$total" ] || die "$src tiene $total líneas; no llega a la $to"
# -ef: the same file on disk, also through a symbolic or hard link.
if [ -e "$dst" ] && [ "$src" -ef "$dst" ]; then die "origen y destino son el mismo fichero"; fi
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
# A missing destination is an empty file until every check has passed.
if [ -e "$dst" ]; then cat "$dst" >"$tmp/old"; else : >"$tmp/old"; fi
dst_total=$(wc -l <"$tmp/old")
[ -z "$after" ] || [ "$after" -le "$dst_total" ] || die "$dst tiene $dst_total líneas; no hay línea $after"

cat "$src" "$tmp/old" | sort >"$tmp/before"
sed -n "${from},${to}p" "$src" >"$tmp/block"
sed "${from},${to}d" "$src" >"$tmp/src"
if [ -z "$after" ]; then
  cat "$tmp/old" "$tmp/block" >"$tmp/dst"
elif [ "$after" -eq 0 ]; then
  cat "$tmp/block" "$tmp/old" >"$tmp/dst"
else
  { head -n "$after" "$tmp/old"; cat "$tmp/block"; tail -n +"$((after + 1))" "$tmp/old"; } >"$tmp/dst"
fi
cat "$tmp/src" "$tmp/dst" | sort >"$tmp/after"
cmp -s "$tmp/before" "$tmp/after" || die "las líneas no cuadran; no se cambió nada"
cat "$tmp/src" >"$src"
cat "$tmp/dst" >"$dst"
echo "movidas $((to - from + 1)) líneas de $src:$from-$to a $dst${after:+ (tras la línea $after)}"
