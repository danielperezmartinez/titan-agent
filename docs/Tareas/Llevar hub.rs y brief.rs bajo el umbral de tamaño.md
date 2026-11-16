---
Nombre: Llevar hub.rs y brief.rs bajo el umbral de tamaño
Estado: En curso
Resumen: Dos ficheros superaban las 800 líneas al fijar la regla de estructura del código (2026-11-16) — crates/titan-agentd/src/hub.rs (unas 1330) y crates/titan-agent/src/brief.rs (unas 920) — y hay que dividirlos o justificar su tamaño en su cabecera, sin cambios de comportamiento.
Decisiones: 'hub.rs se dividió en la fase 3 (variante equipo v4) y brief.rs en la ronda 0.5.0 (BriefText y sus tests a brief/text.rs), los dos moviendo código sin cambios de comportamiento. Falta integrar la PR de la ronda.'
Bloqueada: []
Fecha de creación: 2026-11-16T18:05:00+01:00
Última modificación: 2026-11-16T23:05:00+01:00
---

# Llevar hub.rs y brief.rs bajo el umbral de tamaño

## Objetivo

Aplicar a los ficheros que ya existían la regla
[[README#7. Estructura del código|7. Estructura del código]], fijada al cerrar
[[Tareas/Fase 2 - Nueva sesión con salvaguardas]], donde solo se separó la
interfaz (`app.rs`).

- `crates/titan-agentd/src/hub.rs`: estado del daemon, clientes, terminales,
  lanzamientos y su resolución en un solo fichero. Candidatos a módulo propio:
  los lanzamientos (salvaguardas, `claude --bg`, resolución de la sesión
  provisional) y los terminales (PTY e historial).
- `crates/titan-agent/src/brief.rs`: el campo de encargo (modelo de edición
  sin GPUI, elemento de GPUI y tests). Se puede separar el modelo
  (`BriefText`) del elemento, o justificar en su cabecera que es una sola
  pieza.

## Criterios de finalización

- Ningún fichero `.rs` del workspace pasa de 800 líneas sin justificación en
  su cabecera `//!`.
- Sin cambios de comportamiento: la misma batería de tests en verde, más fmt,
  clippy y gitleaks.

## Verificación

- **hub.rs (hecho en la fase 3, rama `feat/fase-3-equipo-v4`)**: Luna
  (`mecanico`, gpt-6-luna) movió el código tal cual a `hub.rs` (469 líneas,
  estado y enrutado), `hub/launch.rs` (607, lanzamientos) y
  `hub/terminals.rs` (274, terminales); solo cambian visibilidad e imports
  (comprobado comparando las líneas ordenadas antes y después). Los tests
  del daemon siguen en verde. Era necesario antes de que la fase 3 añadiera
  al daemon la lectura de eventos y la cuota.
- **brief.rs (ronda 0.5.0, rama `feat/v0.5-equipo-v4`)**: Luna
  (`mecanico`, gpt-6-luna, encargo `v05-brief-division`) movió el modelo
  `BriefText` y sus tests a `brief/text.rs` (233 líneas) y dejó en
  `brief.rs` el campo de GPUI (699 líneas tras la ronda), con
  `pub use text::BriefText`. Comparando las líneas ordenadas antes y después
  solo cambian la cabecera, `mod`/`pub use` y una línea en blanco. Luna no
  pudo compilar (el directorio de compilación estaba bloqueado por una
  compilación en paralelo) y quitó un `use std::ops::Range;` que `brief.rs`
  seguía necesitando; lo repuso el director (commit aparte). fmt, clippy y
  la batería completa en verde.
- Ningún `.rs` del workspace pasa de 800 líneas: los mayores son
  `terminal.rs` (710), `brief.rs` (699) y `monitor.rs` del núcleo (~680).

## Resultado

<Se rellena al completar.>
