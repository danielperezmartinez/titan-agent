---
Nombre: "titan-team"
Tipo: "Comando CLI"
Área: "Equipo"
Feature: "Equipo"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-team/src/main.rs"
Entrada pública: "titan-team [--config RUTA] roles|check|init|doc|new|ask|show|diff|test|note|close (bin/titan-team desde el repo)"
Resumen: "Herramienta del equipo (ADR-0005): reparte encargos a los roles Codex de .agents/team.toml con codex exec y codex exec resume, en un worktree por encargo con hidden_paths fuera, commit por turno, avisos de rutas, tope de rondas (salida 3) e informe del revisor con esquema. Escribe el log de eventos de ADR-0010. Nunca lanza Claude Code. init, check y doc permiten montar el equipo de otro proyecto solo con team.toml."
Última modificación: "2026-11-16T20:35:00+01:00"
---

# titan-team

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-team/src/main.rs)
como fuente de verdad de su contrato detallado. Formato: `crates/titan-team/FORMAT.md`. Tests: `crates/titan-team/tests/cli.rs` (Codex falso).
