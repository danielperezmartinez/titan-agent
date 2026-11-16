---
Nombre: "Monitor del proyecto"
Tipo: "Componente UI"
Área: "Interfaz"
Feature: "Monitor"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent/src/app/monitor.rs"
Entrada pública: "Selección de proyecto en el árbol; TITAN_AGENT_MONITOR_VIEW, TITAN_AGENT_LANES_WINDOW"
Resumen: "Monitor en la zona central: barra con proyecto, encargo, ronda, [*] en vivo y los botones grafo, carriles y mesa de control (monitor_graph.rs, monitor_lanes.rs, monitor_desk.rs). Sin equipo activo, lista las sesiones sueltas. Clic en un agente abre su sesión. Anima a 20 fps solo mientras se ve."
Última modificación: "2026-11-16T20:35:00+01:00"
---

# Monitor del proyecto

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent/src/app/monitor.rs)
como fuente de verdad de su contrato detallado. Decisión visual en [[Decisiones visuales/Monitor del proyecto en tres vistas]].
