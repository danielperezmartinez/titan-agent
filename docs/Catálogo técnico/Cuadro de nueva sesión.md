---
Nombre: "Cuadro de nueva sesión"
Tipo: "Componente UI"
Área: "Interfaz"
Feature: "Nueva sesión"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent/src/new_session.rs"
Entrada pública: "NewSessionView (apply_defaults, set_projects, launched, failed) con el evento NewSessionEvent::Launch; SettingsView y load_defaults/save_defaults en settings.rs; piezas chip, menu_list y menu_option"
Resumen: "Zona central «Nueva sesión» de la ventana principal: título, «en [#] proyecto con agente», cuadro de encargo vacío con borde accent, botones que abren una lista corta (agente, modelo, esfuerzo, permisos o sandbox, proyecto o [d] otra carpeta), [ok] lanzar (activo solo con encargo y un lanzamiento en curso como mucho) y la línea «se ejecutará» con el comando exacto. Emite un lanzamiento por clic; el error del daemon se muestra con [x] y no se reintenta. `[*]` abre SettingsView, que guarda los valores por defecto en el fichero de configuración propio de titan-agent."
Última modificación: "2026-11-15T20:48:00+01:00"
---

# Cuadro de nueva sesión

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent/src/new_session.rs)
como fuente de verdad de su contrato detallado (configuración en
`crates/titan-agent/src/settings.rs`). Diseño en
[[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]] (previa:
`design/preview/Main.dc.html` con `inicio="nueva"`) y salvaguardas en
[[Decisiones/ADR-0002 Lanzar sesiones desde la app con salvaguardas]].
Usa [[Catálogo técnico/Campo de texto]] y [[Catálogo técnico/Lanzamiento de sesiones]].

Variables de arranque para capturas (no lanzan nada): `TITAN_AGENT_NEW`,
`TITAN_AGENT_SETTINGS`, `TITAN_AGENT_MENU=agent|model|effort|access|project`,
`TITAN_AGENT_FOLDER=<carpeta>` y `TITAN_AGENT_CONFIG=<fichero>`.
