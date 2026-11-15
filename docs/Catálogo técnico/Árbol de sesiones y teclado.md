---
Nombre: "Árbol de sesiones y teclado"
Tipo: "Servicio"
Área: "Núcleo"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-core/src/tree.rs"
Entrada pública: "titan_agent_core::tree::{build_tree, TreeRow, since_label}; titan_agent_core::keys::{encode_key, encode_paste}"
Resumen: "build_tree agrupa por proyecto, anida por sesión padre con ramas ASCII, filtra por nombre, agente, modelo o proyecto y marca los proyectos con equipo en marcha; since_label da el 'estado desde'. keys codifica pulsaciones (nombres de tecla de GPUI) y pegados en bytes xterm, con modo de cursor de aplicación y pegado entre corchetes."
Última modificación: "2026-11-15T13:00:00+01:00"
---

# Árbol de sesiones y teclado

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/tree.rs)
como fuente de verdad de su contrato detallado. El teclado está en `crates/titan-agent-core/src/keys.rs`.
