---
Nombre: "Protocolo local"
Tipo: "Protocolo"
Área: "Núcleo"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agent-core/src/protocol.rs"
Entrada pública: "titan_agent_core::protocol::{Message, ClientRequest, DaemonNotification, Session, encode_line, decode_line, LineReader}"
Resumen: "Mensajes del protocolo 1 entre titan-agentd y sus clientes (NDJSON con forma JSON-RPC sin `jsonrpc`): peticiones, respuestas y notificaciones tipadas, `Session` con proyecto, estado, `attach` y `stop`, códigos de error, `encode_line`/`decode_line` con validación estricta (id u64, method string, params objeto) y `LineReader` con límite de 4 MiB por línea. Lo usará también titan-ssh en la fase 4."
Última modificación: "2026-11-14T19:50:00+01:00"
---

# Protocolo local

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/protocol.rs)
como fuente de verdad de su contrato detallado.

Contrato en [[Decisiones/ADR-0007 Socket local y protocolo NDJSON versionado entre daemon e interfaz]]. Tests en `crates/titan-agent-core/tests/protocol.rs` y en el propio módulo.
