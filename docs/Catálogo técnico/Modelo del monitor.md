---
Nombre: "Modelo del monitor"
Tipo: "Modelo de datos"
Área: "Núcleo"
Feature: "Monitor"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-core/src/monitor.rs"
Entrada pública: "titan_agent_core::monitor::{Monitor, StateLog, Talk, Spark}"
Resumen: "Lo que muestran las tres vistas del monitor a partir de un TeamRun, las sesiones y la cuota: agentes (tú, director, roles) con su estado, acción y tiempo en ese estado; mensajes con el que está en vuelo y el avance del punto; tramos de trabajo y de te necesita; feed; franja de la mesa; ronda e histograma. Puro y con tests."
Última modificación: "2026-11-16T20:35:00+01:00"
---

# Modelo del monitor

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/monitor.rs)
como fuente de verdad de su contrato detallado. Tests en `monitor_tests.rs`.
