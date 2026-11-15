---
Nombre: "Tipos de sesión"
Tipo: "Modelo de datos"
Área: "Núcleo"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agent-core/src/session.rs"
Entrada pública: "titan_agent_core::session::{AgentCli, SessionState, StateCounts, SessionKey, SessionKind, Project, Session}"
Resumen: "AgentCli (ClaudeCode, Codex) con su ejecutable oficial (`claude`/`codex`), su etiqueta de pastilla (`cl`/`cx`), su nombre visible y la búsqueda exacta por ejecutable. SessionState (Working, NeedsYou, Waiting, Finished { failed }) con su marcador ASCII (`[>]`, `[?]`, `[+]`, `[x]`) y si sigue viva. StateCounts cuenta los estados vivos para los contadores de la cabecera. Lógica pura, sin E/S. Desde la fase 1 incluye el modelo serializable (serde) que comparten daemon e interfaz: SessionKey (cli + id), SessionKind, Project (nombre + raíz) y Session (nombre, estado, carpeta, proyecto, modelo corto, sesión padre, inicio, estado desde y comando oficial de adjuntar)."
Última modificación: "2026-11-15T13:00:00+01:00"
---

# Tipos de sesión

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/session.rs)
como fuente de verdad de su contrato detallado. Sus tests de aceptación están en
`crates/titan-agent-core/tests/session.rs`.

Marcadores y estados según
[[Decisiones visuales/Lenguaje visual heredado de titan-ssh]]; los contadores son
los de la cabecera de [[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]].
Primer encargo hecho con el equipo (fase 0): tests de Sol, implementación de Luna
y revisión de Sol.
