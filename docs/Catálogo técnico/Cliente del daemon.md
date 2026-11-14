---
Nombre: "Cliente del daemon"
Tipo: "Servicio"
Área: "Interfaz"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent/src/client.rs"
Entrada pública: "titan_agent::client::{Connection, connect_or_spawn, default_socket_path, daemon_program}"
Resumen: "Conexión con titan-agentd para la interfaz: saludo con plazo global, peticiones bloqueantes desde cualquier hilo emparejadas por id, notificaciones en orden por un canal y cierre que libera las peticiones pendientes. `connect_or_spawn` arranca el daemon en su propio grupo de procesos si no responde."
Última modificación: "2026-11-14T21:07:32+01:00"
---

# Cliente del daemon

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent/src/client.rs)
como fuente de verdad de su contrato detallado.

Tests en `crates/titan-agent/tests/client.rs` con un daemon falso.
