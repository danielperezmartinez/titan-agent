---
Nombre: "Daemon titan-agentd"
Tipo: "Servicio"
Área: "Daemon"
Feature: "Terminal"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agentd/src/server.rs"
Entrada pública: "titan_agentd::server::{Daemon, DaemonConfig}; titan_agentd::source::SessionSource; binario titan-agentd [--socket RUTA] [--fixture FICHERO]"
Resumen: "Daemon único por usuario: socket Unix, colas acotadas por cliente, terminales PTY compartidos entre clientes que sobreviven al cierre de la ventana (instantánea ANSI + salida, en ese orden), descubrimiento real de Claude Code y Codex solo con suscritos, parada con el comando oficial y apagado ordenado. Con --fixture sirve sesiones inventadas para demos y tests."
Última modificación: "2026-11-15T13:00:00+01:00"
---

# Daemon titan-agentd

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agentd/src/server.rs)
como fuente de verdad de su contrato detallado. Piezas: `pty.rs` (proceso en PTY), `snapshot.rs` (emulador e instantánea), `discover.rs`, `codex.rs` y `ws.rs` (fuentes reales). Decisión en [[Decisiones/ADR-0008 Terminal con alacritty_terminal en el daemon y en la interfaz]].
