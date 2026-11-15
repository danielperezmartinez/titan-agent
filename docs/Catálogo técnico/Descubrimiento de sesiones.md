---
Nombre: "Descubrimiento de sesiones"
Tipo: "Servicio"
Área: "Núcleo"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-core/src/discovery.rs"
Entrada pública: "titan_agent_core::discovery::{parse_claude_agents, codex_session, link_sessions, last_model_in_transcript}; titan_agent_core::project::{project_root, project_name, team_worktree, tilde}"
Resumen: "Traduce lo que dicen los CLI oficiales a Session: la salida de claude agents --json, los Thread de codex app-server y la cola de un transcript de Claude (modelo). link_sessions quita duplicados, anida por parentThreadId y por worktree del equipo (.agents/worktrees/<encargo> bajo la carpeta del director) y rellena el rol. project agrupa por proyecto plegando los worktrees. Funciones puras, sin E/S."
Última modificación: "2026-11-15T11:37:00+01:00"
---

# Descubrimiento de sesiones

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/discovery.rs)
como fuente de verdad de su contrato detallado. Mapeo de estados y reglas de
anidado en [[Decisiones/ADR-0009 Descubrimiento de sesiones y jerarquía lanzada por]].
Tests: `crates/titan-agent-core/tests/discovery.rs` (datos inventados).
