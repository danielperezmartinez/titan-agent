---
Nombre: "Descubrimiento de sesiones"
Tipo: "Servicio"
Área: "Núcleo"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-core/src/discovery.rs"
Entrada pública: "titan_agent_core::discovery::{parse_claude_agents, claude_session, parse_codex_thread, codex_session, project_for, short_model}"
Resumen: "Funciones puras que convierten la salida de `claude agents --json` y los hilos de `codex app-server` en Session, deducen el proyecto de una carpeta (cortando worktrees) y el nombre corto del modelo (opus, luna, sol) incluso desde el transcript de Claude Code. Toleran campos ausentes o desconocidos."
Última modificación: "2026-11-15T13:00:00+01:00"
---

# Descubrimiento de sesiones

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/discovery.rs)
como fuente de verdad de su contrato detallado.
