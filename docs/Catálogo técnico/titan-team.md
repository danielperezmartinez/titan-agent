---
Nombre: "titan-team"
Tipo: "Comando CLI"
Área: "Equipo"
Feature: "Equipo"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-team/src/main.rs"
Entrada pública: "titan-team [--config RUTA] init|doc|check|roles|new|ask|show|diff|test|decide|close; titan-team --version; ask -m|-f|--message-file; bin/team desde este repo (TITAN_TEAM_BIN, bin/team pin [REV] y unpin); TITAN_TEAM_CODEX; salidas 0/1/2/3/4"
Resumen: "Herramienta del equipo (ADR-0005, formato en ADR-0010): reparte encargos a los roles de Codex de .agents/team.toml con codex exec y codex exec resume, sin shell y con tiempo límite que mata su grupo de procesos, en un worktree por encargo con hidden_paths fuera; bloqueo por encargo, commit por turno, avisos de rutas, tope de rondas (salida 3) e informe del revisor con esquema. Tras el turno, sus checks (fmt, test, gate, build; checks_required, booleano o lista) y, antes del primer turno de un rol, las secciones obligatorias del encargo (brief_sections, salida 2). Hilo abierto en otro sitio: conserva el thread_id, turn_finished busy y salida 4, sin abrir otro hilo. Escribe events.jsonl v1 con la sesión del director (CLAUDE_CODE_SESSION_ID). Nunca lanza Claude Code. init y doc (FORMAT.md embebido) permiten montar el equipo de otro proyecto sin la bóveda."
Última modificación: "2026-11-18T22:45:00+01:00"
---

# titan-team

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-team/src/main.rs)
como fuente de verdad de su contrato detallado. Formato para otros proyectos:
`crates/titan-team/FORMAT.md` (lo imprime `titan-team doc`) y plantillas de
`init` en `crates/titan-team/template/`. Decisiones en
[[Decisiones/ADR-0005 Herramienta del equipo en Rust dentro de titan-agent]] y
[[Decisiones/ADR-0010 Formato de team.toml y de los eventos del equipo]].
Tests: `crates/titan-team/tests/phase3_acceptance.rs`,
`equipo_v5_acceptance.rs`, `equipo_v6_acceptance.rs` (prueba también
`bin/team`) y `equipo_v7_acceptance.rs` (aceptación), y
`crates/titan-team/tests/cli.rs` (Codex falso). La usa la skill `equipo`.
