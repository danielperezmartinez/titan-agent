#!/bin/sh
# Install titan-agent for this user, without sudo, in ~/.local
# (docs/Decisiones/ADR-0011, §4). It is idempotent: running it again changes nothing.
# To uninstall: ~/.local/share/titan-agent/titan-agent-launcher uninstall
set -e
exec "$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)/bin/titan-agent-launcher" install "$@"
