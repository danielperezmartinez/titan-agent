---
Nombre: "titan-agentd"
Tipo: "Servicio"
Área: "Daemon"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agentd/src/main.rs"
Entrada pública: "titan-agentd [--socket <ruta>] [--print-socket]; variables TITAN_AGENTD_SOCKET, TITAN_AGENTD_POLL_MS, TITAN_AGENTD_CLAUDE, TITAN_AGENTD_CODEX"
Resumen: "Daemon por usuario: sondea Claude Code y Codex, mantiene los PTY de los comandos oficiales de adjuntar con 2 MiB de historial, aplica stop con el comando oficial y sirve el protocolo de ADR-0007 en un socket Unix con un solo daemon por socket. Es el único componente que lanza procesos."
Última modificación: "2026-11-15T11:37:00+01:00"
---

# titan-agentd

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agentd/src/main.rs)
como fuente de verdad de su contrato detallado (estado compartido en
`hub.rs`, fuentes en `claude.rs` y `codex.rs`). Decisiones en
[[Decisiones/ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]],
[[Decisiones/ADR-0007 Protocolo local de titan-agentd en NDJSON versionado]] y
[[Decisiones/ADR-0009 Descubrimiento de sesiones y jerarquía lanzada por]].
Tests de punta a punta con CLI falsos en `crates/titan-agentd/tests/daemon.rs`.
