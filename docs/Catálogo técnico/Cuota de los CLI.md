---
Nombre: "Cuota de los CLI"
Tipo: "Servicio"
Área: "Daemon"
Feature: "Monitor"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-core/src/quota.rs"
Entrada pública: "titan_agent_core::quota::{Quota, parse_codex_rate_limits, shortest_window, LOW_REMAINING_PERCENT}"
Resumen: "Cuota de Codex por la vía oficial: titan-agentd pide account/rateLimits/read al codex app-server compartido al conectar y como mucho una vez por minuto, y publica en quotas del evento sessions una Quota por ventana (límite, minutos, porcentaje usado, reinicio). Si falla, la cuota desaparece; no se inventa. Claude Code no tiene vía oficial apta para el daemon: su tarjeta dice «cuota: sin dato». No lee credenciales."
Última modificación: "2026-11-16T21:30:00+01:00"
---

# Cuota de los CLI

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/quota.rs)
como fuente de verdad de su contrato detallado (la petición, en
`crates/titan-agentd/src/codex.rs`). Investigación en
[[Tareas/Investigar la cuota restante de cada CLI]]; contrato en
[[Decisiones/ADR-0010 Formato de team.toml y de los eventos del equipo]].
