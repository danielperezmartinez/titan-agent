---
Nombre: "Cuota de los CLI"
Tipo: "Servicio"
Área: "Daemon"
Feature: "Monitor"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agentd/src/quota.rs"
Entrada pública: "titan_agent_core::quota::{Quota, parse_codex_rate_limits, parse_claude_rate_limits, quota_for}"
Resumen: "Cuota restante por vías oficiales: Codex con account/rateLimits/read del app-server compartido cada minuto; Claude Code desde el puente de la statusline en <state>/titan-agent/claude-rate-limits.json, si el usuario lo monta. Va en quotas del snapshot. No lee credenciales."
Última modificación: "2026-11-16T20:35:00+01:00"
---

# Cuota de los CLI

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agentd/src/quota.rs)
como fuente de verdad de su contrato detallado. Investigación en [[Tareas/Investigar la cuota restante de cada CLI]].
