---
Nombre: "Eventos del equipo"
Tipo: "Protocolo"
Área: "Núcleo"
Feature: "Equipo"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-core/src/team/events.rs"
Entrada pública: "titan_agent_core::team::{config::TeamConfig, events::{TeamEvent, EventKind, parse_line}, run::TeamRun}"
Resumen: "Formato v1 de <state_dir>/events.jsonl (task_created, turn_started, action, turn_finished, review, note, task_closed) con lectores tolerantes; TeamConfig valida team.toml de forma estricta; TeamRun pliega los eventos de un encargo y lo une a las sesiones (director por su id, roles por thread_id). Sin E/S."
Última modificación: "2026-11-16T20:35:00+01:00"
---

# Eventos del equipo

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/team/events.rs)
como fuente de verdad de su contrato detallado. Decisión en [[Decisiones/ADR-0010 Formato de eventos del equipo en JSONL versionado]].
