---
Nombre: "Protocolo de titan-agentd"
Tipo: "Protocolo"
Área: "Núcleo"
Feature: "Base"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agent-core/src/protocol.rs"
Entrada pública: "titan_agent_core::protocol::{ClientMessage, Request, ServerMessage, Event, LaunchResult, LaunchRef, PROTOCOL_VERSION, encode_line, decode_line, encode_bytes, decode_bytes, socket_path}"
Resumen: "Mensajes NDJSON v1 entre titan-agentd y sus clientes: peticiones {id, method, params} (hello, subscribe, attach, input, resize, detach, close_terminal, stop, open_folder, launch {token, spec, command} sin campos desconocidos), respuestas {id, result | error} (id null si la línea no se pudo leer) y eventos {event, params} (sessions con launches, teams y quotas, output, terminal_closed). Bytes de terminal en base64. socket_path resuelve la ruta del socket por usuario. Sin E/S."
Última modificación: "2026-11-16T21:30:00+01:00"
---

# Protocolo de titan-agentd

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/protocol.rs)
como fuente de verdad de su contrato detallado. Decisión en
[[Decisiones/ADR-0007 Protocolo local de titan-agentd en NDJSON versionado]].
Tests: `crates/titan-agent-core/tests/protocol.rs` y, de punta a punta,
`crates/titan-agentd/tests/daemon.rs`.
Lanzamientos (fase 2): `crates/titan-agent-core/tests/launch.rs` y
`crates/titan-agentd/tests/launch.rs`.
Equipo y cuota (fase 3): `teams` (encargos con los eventos de
[[Catálogo técnico/Eventos del equipo]], más `created` y `totals`) y `quotas`
([[Catálogo técnico/Cuota de los CLI]]), vacíos por defecto al leer y sin
cambiar `PROTOCOL_VERSION`; contrato en
[[Decisiones/ADR-0010 Formato de team.toml y de los eventos del equipo]].
