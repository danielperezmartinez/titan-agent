---
Nombre: "Lanzamiento de sesiones"
Tipo: "Modelo de datos"
Área: "Núcleo"
Feature: "Nueva sesión"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agent-core/src/launch.rs"
Entrada pública: "titan_agent_core::launch::{LaunchSpec, LaunchError, LaunchDefaults, AgentDefaults, LaunchedTui, pick_launched_thread, session_name, shell_quote, parse_claude_bg_output, models, access_modes, EFFORTS}"
Resumen: "LaunchSpec es lo que el usuario elige en «Nueva sesión» (agente, modelo, esfuerzo, permisos o sandbox, carpeta y encargo). validate rechaza el encargo vacío, con NUL o de más de 100 KiB y cualquier opción fuera de las ofrecidas; argv da el comando oficial con el encargo intacto como último argumento (`claude --bg -n … --model … --effort … --permission-mode …` o `codex --remote unix:// -m … -c model_reasoning_effort=… -s …`, con `--` si empieza por guion); command_line lo muestra exacto, citado para un shell y con `~`. LaunchDefaults son los valores por defecto de `[*]`, saneados al leerlos. pick_launched_thread encuentra el hilo que abrió un TUI de Codex lanzado. Funciones puras, sin E/S."
Última modificación: "2026-11-15T20:48:00+01:00"
---

# Lanzamiento de sesiones

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/launch.rs)
como fuente de verdad de su contrato detallado. Salvaguardas en
[[Decisiones/ADR-0002 Lanzar sesiones desde la app con salvaguardas]]; quien
lanza es [[Catálogo técnico/titan-agentd]] (método `launch` de
[[Catálogo técnico/Protocolo de titan-agentd]]).
Tests: `crates/titan-agent-core/tests/launch.rs` y, de punta a punta con CLI
falsos, `crates/titan-agentd/tests/daemon.rs`.
