---
Nombre: "Árbol de sesiones"
Tipo: "Modelo de datos"
Área: "Núcleo"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agent-core/src/tree.rs"
Entrada pública: "titan_agent_core::tree::{project_tree, NavRow, build_rows, TreeRow, matches}; titan_agent_core::projects (projects.json v1); titan_agent_core::time::ago"
Resumen: "Convierte la lista de sesiones en las filas de la columna izquierda: cabecera por proyecto (nombre, contador, equipo en vivo) y sesiones ordenadas (vivas primero, más recientes arriba) con las lanzadas por otra anidadas y su rama ASCII (|- y `-). Filtra por nombre, agente, modelo o proyecto conservando los antepasados. Desde la 0.7.1, project_tree (ADR-0014) pone primero los proyectos importados por última actividad (con Empty si no tienen sesiones y su marca missing) y después Others con los grupos de carpetas no importadas; build_rows queda para un daemon sin proyectos. ago da el «hace 3 min»."
Última modificación: "2026-11-18T10:00:00+01:00"
---

# Árbol de sesiones

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/tree.rs)
como fuente de verdad de su contrato detallado. Aspecto en
[[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]].
Tests: `crates/titan-agent-core/tests/tree.rs`.
