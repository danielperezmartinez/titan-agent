---
Nombre: "Árbol de sesiones"
Tipo: "Modelo de datos"
Área: "Núcleo"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-core/src/tree.rs"
Entrada pública: "titan_agent_core::tree::{build_tree, matches_filter, ProjectGroup, TreeRow}"
Resumen: "Agrupa las sesiones por proyecto (orden por nombre), anida por `parent` con hermanas por fecha de inicio, genera las ramas ASCII (`|- `, `` `- ``, `|  `) y los contadores por grupo, y aplica el filtro `[/]` por nombre, agente, modelo o proyecto. Resistente a ciclos."
Última modificación: "2026-11-14T19:50:00+01:00"
---

# Árbol de sesiones

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/tree.rs)
como fuente de verdad de su contrato detallado.

Lo pinta la columna izquierda de [[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]]. Tests en `crates/titan-agent-core/tests/tree.rs`.
