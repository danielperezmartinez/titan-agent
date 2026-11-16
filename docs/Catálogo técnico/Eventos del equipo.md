---
Nombre: "Eventos del equipo"
Tipo: "Protocolo"
Área: "Núcleo"
Feature: "Equipo"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-core/src/team.rs"
Entrada pública: "titan_agent_core::team::{TeamEvent, EventKind, parse_event, parse_event_line, TeamRun, Member, TeamMessage, FeedItem, EVENTS_VERSION, STATUS_BUSY, TEXT_LIMIT, DECISION_LIMIT, clip}"
Resumen: "Formato v1 de <state_dir>/<encargo>/events.jsonl (task_created, brief_sent, turn_started, activity, turn_finished con status ok/error/timeout/busy, report, decision, task_closed; director_session opcional en cualquiera). Lectores tolerantes: ignoran tipos y campos desconocidos y saltan líneas de una v mayor. TeamRun pliega los eventos de un encargo: miembros con estado, turnos (un busy no cuenta), hilo, tokens y acción; mensajes, feed, tramos de trabajo, ronda y la sesión del director. Sin E/S."
Última modificación: "2026-11-16T21:30:00+01:00"
---

# Eventos del equipo

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/team.rs)
como fuente de verdad de su contrato detallado. Decisión en
[[Decisiones/ADR-0010 Formato de team.toml y de los eventos del equipo]].
Los escribe [[Catálogo técnico/titan-team]] y los reenvía
[[Catálogo técnico/titan-agentd]] en `teams`.
