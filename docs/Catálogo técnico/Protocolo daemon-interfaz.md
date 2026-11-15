---
Nombre: "Protocolo daemon-interfaz"
Tipo: "Protocolo"
Área: "Núcleo"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agent-core/src/protocol.rs"
Entrada pública: "titan_agent_core::protocol::{ClientMessage, Request, ServerMessage, Response, Event, read_frame, encode_line}; titan_agent_core::paths::socket_path"
Resumen: "Protocolo NDJSON versionado (PROTOCOL_VERSION 1) entre titan-agentd y sus clientes: peticiones con id (hello, subscribe, open_terminal, input, resize, detach, close_terminal, stop_session, shutdown), respuestas o errores con ese id y eventos (sessions, terminal_snapshot, terminal_output, terminal_exited); bytes en base64; líneas de 8 MiB como máximo. Incluye la ruta del socket por usuario. Base del acceso remoto de la fase 4."
Última modificación: "2026-11-15T13:00:00+01:00"
---

# Protocolo daemon-interfaz

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/protocol.rs)
como fuente de verdad de su contrato detallado. Decisión en [[Decisiones/ADR-0007 Protocolo NDJSON versionado entre titan-agentd y sus clientes]].
