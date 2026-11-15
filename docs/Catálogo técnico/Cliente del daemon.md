---
Nombre: "Cliente del daemon"
Tipo: "Servicio"
Área: "Interfaz"
Feature: "Base"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent/src/client.rs"
Entrada pública: "titan_agent::client::{DaemonClient, Incoming, socket_from_env, spawn_daemon}"
Resumen: "Cliente sin GPUI: conecta al socket (arrancando titan-agentd si no responde), saluda con plazo absoluto, envía peticiones por una cola FIFO que nunca bloquea y entrega respuestas y eventos por un canal asíncrono."
Última modificación: "2026-11-15T13:00:00+01:00"
---

# Cliente del daemon

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent/src/client.rs)
como fuente de verdad de su contrato detallado.
