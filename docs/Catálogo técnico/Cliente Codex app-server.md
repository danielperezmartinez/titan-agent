---
Nombre: "Cliente Codex app-server"
Tipo: "Servicio"
Área: "Daemon"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agentd/src/discovery.rs"
Entrada pública: "titan_agentd::discovery::{CodexClient, CodexError}"
Resumen: "Cliente JSON-RPC por líneas de `codex app-server` genérico sobre BufRead/Write: `initialize` + `initialized`, `loaded_threads` (thread/loaded/list paginado y thread/read) e `interrupt` (thread/turns/list y turn/interrupt). Ignora notificaciones y no responde peticiones del servidor (podrían ser aprobaciones de otra sesión)."
Última modificación: "2026-11-14T21:07:32+01:00"
---

# Cliente Codex app-server

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agentd/src/discovery.rs)
como fuente de verdad de su contrato detallado.

Servirá también para la cuota de Codex (`account/rateLimits/read`, ver [[Tareas/Investigar la cuota restante de cada CLI]]). Tests en `crates/titan-agentd/tests/codex_acceptance.rs`.
