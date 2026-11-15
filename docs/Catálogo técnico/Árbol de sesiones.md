---
Nombre: "Árbol de sesiones"
Tipo: "Modelo de datos"
Área: "Núcleo"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agent-core/src/tree.rs"
Entrada pública: "titan_agent_core::tree::{build_rows, TreeRow, matches}; titan_agent_core::time::ago"
Resumen: "Convierte la lista de sesiones en las filas de la columna izquierda: cabecera por proyecto (nombre, contador, equipo en vivo) y sesiones ordenadas (vivas primero, más recientes arriba) con las lanzadas por otra anidadas y su rama ASCII (|- y `-). Filtra por nombre, agente, modelo o proyecto conservando los antepasados. ago da el «hace 3 min»."
Última modificación: "2026-11-15T11:45:00+01:00"
---

# Árbol de sesiones

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/tree.rs)
como fuente de verdad de su contrato detallado. Aspecto en
[[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]].
Tests: `crates/titan-agent-core/tests/tree.rs`.
