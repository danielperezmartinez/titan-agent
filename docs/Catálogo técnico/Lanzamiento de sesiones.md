---
Nombre: "Lanzamiento de sesiones"
Tipo: "Modelo de datos"
Área: "Núcleo"
Feature: "Nueva sesión"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-core/src/launch.rs"
Entrada pública: "titan_agent_core::launch::{LaunchSpec (validate, argv, command_line, session_name, provisional_session, matches_new_session), LaunchError::code, LaunchDefaults (parse, get), models, efforts, permissions, shell_quote, parse_backgrounded_id}"
Resumen: "Lo que el usuario elige en Nueva sesión (agente, carpeta, modelo, esfuerzo, permisos o sandbox y encargo literal) convertido en el argv exacto del CLI oficial (claude --bg -n … -- <encargo>; TUI de codex -m … -s … -- <encargo>) y en la línea que se muestra antes de lanzar (cd <carpeta> && …, citada POSIX). validate aplica las salvaguardas sin E/S (encargo no vacío, ≤ 32 000 bytes, sin NUL, ruta absoluta, opciones de listas cerradas sin bypassPermissions, dontAsk ni danger-full-access) con códigos estables. LaunchDefaults son los valores por defecto de [*]."
Última modificación: "2026-11-15T20:40:00+01:00"
---

# Lanzamiento de sesiones

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/launch.rs)
como fuente de verdad de su contrato detallado. Decisión en
[[Decisiones/ADR-0002 Lanzar sesiones desde la app con salvaguardas]].
Tests: unitarios en el propio módulo y de aceptación en
`crates/titan-agent-core/tests/launch.rs` (escritos por Sol desde la
especificación).
