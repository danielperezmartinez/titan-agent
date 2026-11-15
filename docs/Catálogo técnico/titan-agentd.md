---
Nombre: "titan-agentd"
Tipo: "Servicio"
Área: "Daemon"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agentd/src/main.rs"
Entrada pública: "titan-agentd [--socket <ruta>] [--print-socket]; variables TITAN_AGENTD_SOCKET, TITAN_AGENTD_POLL_MS, TITAN_AGENTD_CLAUDE, TITAN_AGENTD_CODEX"
Resumen: "Daemon por usuario: sondea Claude Code y Codex, mantiene los PTY de los comandos oficiales de adjuntar con 2 MiB de historial, aplica stop con el comando oficial y sirve el protocolo de ADR-0007 en un socket Unix con un solo daemon por socket. Es el único componente que lanza procesos: `launch` aplica las salvaguardas de ADR-0002 (encargo del usuario no vacío, comando mostrado idéntico, un clic un lanzamiento, sin reintentos), ejecuta `claude --bg` o abre el TUI de `codex --remote unix://` en un PTY propio, pide un sondeo inmediato y vincula ese PTY con su hilo; marca launched_here en lo que lanzó."
Última modificación: "2026-11-15T20:48:00+01:00"
---

# titan-agentd

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agentd/src/main.rs)
como fuente de verdad de su contrato detallado (estado compartido en
`hub.rs`, fuentes en `claude.rs` y `codex.rs`). Decisiones en
[[Decisiones/ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]],
[[Decisiones/ADR-0007 Protocolo local de titan-agentd en NDJSON versionado]] y
[[Decisiones/ADR-0009 Descubrimiento de sesiones y jerarquía lanzada por]].
Tests de punta a punta con CLI falsos en `crates/titan-agentd/tests/daemon.rs`.

Lanzamientos en `Hub::launch` (fase 2); el sondeo espera en `wait_poll`, que
un lanzamiento despierta con `poke`.
