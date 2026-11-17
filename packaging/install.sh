#!/bin/sh
# Instala titan-agent para este usuario, sin sudo, en ~/.local
# (docs/Decisiones/ADR-0011, §4). Es idempotente: repetirlo no cambia nada.
# Para desinstalar: ~/.local/share/titan-agent/titan-agent-launcher uninstall
set -e
exec "$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)/bin/titan-agent-launcher" install "$@"
