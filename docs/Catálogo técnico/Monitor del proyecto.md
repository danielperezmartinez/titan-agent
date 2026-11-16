---
Nombre: "Monitor del proyecto"
Tipo: "Componente UI"
Área: "Interfaz"
Feature: "Monitor"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent/src/app/monitor.rs"
Entrada pública: "Clic en la cabecera de un proyecto del árbol; TITAN_AGENT_MONITOR=grafo|carriles|mesa (vista inicial, para capturas)"
Resumen: "Monitor en la zona central: barra con proyecto, encargo, ronda, [*] en vivo y los botones grafo, carriles y mesa de control (una vista por módulo en app/monitor/graph.rs, lanes.rs y desk.rs). Sin equipo trabajando, lista las sesiones sueltas. Clic en un agente abre su sesión (cx:<thread_id> para los roles de Codex). Se redibuja cada 100 ms solo mientras está en pantalla; la ventana de carriles (36 s) se cambia desde la vista."
Última modificación: "2026-11-16T21:30:00+01:00"
---

# Monitor del proyecto

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent/src/app/monitor.rs)
como fuente de verdad de su contrato detallado. Decisión visual en
[[Decisiones visuales/Monitor del proyecto en tres vistas]]; datos de
[[Catálogo técnico/Modelo del monitor]].
