---
Nombre: "Modelo del monitor"
Tipo: "Modelo de datos"
Área: "Núcleo"
Feature: "Monitor"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-core/src/monitor.rs"
Entrada pública: "titan_agent_core::monitor::{latest_team, run_of, board, director_of, directors, TaskId, team_project, Board, Agent, Message, Span, Bar, MESSAGE_LIVE_MS}"
Resumen: "El tablero que dibujan las tres vistas del monitor a partir de un TeamTask del daemon, su TeamRun y las sesiones: tú, el director (la sesión exacta que nombran los eventos con director_session; si no, la sesión de Claude Code más activa de la carpeta del equipo) y los roles con su estado, acción y tiempo en ese estado; mensajes con el que está en vuelo; tramos, histograma, titular y feed. La misma regla al revés (directors) da los encargos abiertos de cada sesión directora, más reciente primero, para su [m] monitor y el [*] del árbol (0.7.0). Puro y con tests."
Última modificación: "2026-11-18T03:00:00+01:00"
---

# Modelo del monitor

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/monitor.rs)
como fuente de verdad de su contrato detallado. Parte de
[[Catálogo técnico/Eventos del equipo]]; lo dibuja
[[Catálogo técnico/Monitor del proyecto]].
