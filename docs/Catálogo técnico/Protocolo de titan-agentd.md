---
Nombre: "Protocolo de titan-agentd"
Tipo: "Protocolo"
Área: "Núcleo"
Feature: "Base"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agent-core/src/protocol.rs"
Entrada pública: "titan_agent_core::protocol::{ClientMessage, Request, ServerMessage, Event, LaunchResult, LaunchRef, PROTOCOL_VERSION, encode_line, decode_line, encode_bytes, decode_bytes, socket_path, socket_path_named, socket_name, PID_FILE, UpdateStatus, UpdateState}"
Resumen: "Mensajes NDJSON v1 entre titan-agentd y sus clientes: peticiones {id, method, params} (hello, subscribe, attach, input, resize, detach, close_terminal, stop, open_folder, launch {token, spec, command} sin campos desconocidos), respuestas {id, result | error} (id null si la línea no se pudo leer) y eventos {event, params} (sessions con launches, teams y quotas, output, terminal_closed, update). Desde la 0.6.0, peticiones aditivas update_status, update_check, update_apply {force}, update_settings {auto} y update_dismiss y el evento update con UpdateStatus (ADR-0011 §6), sin subir PROTOCOL_VERSION. Bytes de terminal en base64. socket_path resuelve la ruta del socket por usuario; socket_name da agentd-dev.sock a las builds de debug y agentd.sock a las de release. Sin E/S."
Última modificación: "2026-11-17T20:40:00+01:00"
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
Actualizaciones (0.6.0): contrato C8 de
[[Tareas/Versión 0.6.0 - Instalador y actualización automática]];
tests `crates/titan-agentd/tests/update_acceptance.rs` y
`crates/titan-agentd/tests/update_e2e.rs`.
