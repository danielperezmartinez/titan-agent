---
Nombre: "titan-agentd"
Tipo: "Servicio"
Área: "Daemon"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agentd/src/main.rs"
Entrada pública: "titan-agentd [--socket <ruta>] [--print-socket]; variables TITAN_AGENTD_SOCKET, TITAN_AGENTD_POLL_MS, TITAN_AGENTD_CLAUDE, TITAN_AGENTD_CODEX"
Resumen: "Daemon por usuario: sondea Claude Code y Codex, mantiene los PTY de los comandos oficiales de adjuntar con 2 MiB de historial, aplica stop con el comando oficial, lanza sesiones nuevas con las salvaguardas de ADR-0002 (claude --bg una sola vez, sin reintentos; la TUI de codex --remote unix:// en un PTY, unida al app-server compartido; marca launched_here; tokens de un solo uso y sin repetir el mismo encargo en 30 s; sesión provisional launch:<n> hasta que el CLI la registra) y sirve el protocolo de ADR-0007 en un socket Unix con un solo daemon por socket. Desde la fase 3 sigue en solo lectura los events.jsonl del equipo de las carpetas de las sesiones conocidas (teams) y la cuota de Codex una vez por minuto (quotas). Es el único componente que lanza procesos."
Última modificación: "2026-11-16T21:30:00+01:00"
---

# titan-agentd

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agentd/src/main.rs)
como fuente de verdad de su contrato detallado (estado compartido en
`hub.rs`, lanzamientos en `hub/launch.rs`, terminales en `hub/terminals.rs`,
fuentes en `claude.rs` y `codex.rs`, equipo en `teams.rs`). Decisiones en
[[Decisiones/ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]],
[[Decisiones/ADR-0007 Protocolo local de titan-agentd en NDJSON versionado]] y
[[Decisiones/ADR-0009 Descubrimiento de sesiones y jerarquía lanzada por]].
Tests de punta a punta con CLI falsos en `crates/titan-agentd/tests/daemon.rs`.
