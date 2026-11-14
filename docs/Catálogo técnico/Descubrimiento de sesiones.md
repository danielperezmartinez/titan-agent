---
Nombre: "Descubrimiento de sesiones"
Tipo: "Modelo de datos"
Área: "Núcleo"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-core/src/discovery.rs"
Entrada pública: "titan_agent_core::discovery::{sessions_from_claude_agents, session_from_codex_thread, project_from_git_common_dir, fallback_project}"
Resumen: "Traduce sin E/S la salida de `claude agents --json` y los `Thread` de `codex app-server` a `Session` (estado, nombre, `attach`, `stop`, padre solo para subagentes de Codex), tolerando campos ausentes o desconocidos, y calcula el proyecto de una carpeta a partir de `git rev-parse --git-common-dir` o, sin git, quitando sufijos de worktree y subiendo hasta el `.git`."
Última modificación: "2026-11-14T19:50:00+01:00"
---

# Descubrimiento de sesiones

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/discovery.rs)
como fuente de verdad de su contrato detallado.

Correspondencias de estado en [[Decisiones/ADR-0007 Socket local y protocolo NDJSON versionado entre daemon e interfaz]]. Tests en `crates/titan-agent-core/tests/discovery.rs`.
