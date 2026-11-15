---
Nombre: "Protocolo de titan-agentd"
Tipo: "Protocolo"
Área: "Núcleo"
Feature: "Base"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agent-core/src/protocol.rs"
Entrada pública: "titan_agent_core::protocol::{ClientMessage, Request, ServerMessage, Event, LaunchResult, provisional_key, PROTOCOL_VERSION, encode_line, decode_line, encode_bytes, decode_bytes, socket_path}"
Resumen: "Mensajes NDJSON v1 entre titan-agentd y sus clientes: peticiones {id, method, params} (hello, subscribe, attach, input, resize, detach, close_terminal, stop, open_folder y launch {spec, shown, click, cols, rows} → LaunchResult {agent, cli_id?, terminal?, command}), respuestas {id, result | error} y eventos {event, params} (sessions, output, terminal_closed). Un TUI lanzado figura como sesión provisional `launch:<terminal>` hasta que aparece su hilo. Bytes de terminal en base64. socket_path resuelve la ruta del socket por usuario. Sin E/S."
Última modificación: "2026-11-15T20:48:00+01:00"
---

# Protocolo de titan-agentd

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/protocol.rs)
como fuente de verdad de su contrato detallado. Decisión en
[[Decisiones/ADR-0007 Protocolo local de titan-agentd en NDJSON versionado]].
Tests: `crates/titan-agent-core/tests/protocol.rs` y, de punta a punta,
`crates/titan-agentd/tests/daemon.rs`.

`launch` (fase 2) se añadió sin cambiar la versión, como permite ADR-0007
(solo añade). Errores de `launch`: `empty_brief`, `bad_brief`,
`invalid_option`, `command_mismatch`, `missing_click`, `no_folder`,
`duplicate_launch`, `launch_failed`, `pty_failed`, `spawn_failed`.
