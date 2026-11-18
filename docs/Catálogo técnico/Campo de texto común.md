---
Nombre: "Campo de texto común"
Tipo: "Componente UI"
Área: "Interfaz"
Feature: "Nueva sesión"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent/src/app/composer.rs"
Entrada pública: "Composer (new, text, clear, last_sent; eventos ComposerEvent::Changed, Submit y Blurred) y el modelo puro titan_agent::composer::{ComposerText (press), EditKey, edit_key, Effect, rows_shown, MIN_ROWS, MAX_ROWS, normalize_paste, Drafts}"
Resumen: "El campo de texto de la vista «conversación» (antes «Campo de encargo», solo de Nueva sesión), al estilo de Claude Code Desktop: Enter envía o lanza, Shift+Enter salto de línea, Tab, listas «- » que continúan con Shift+Enter y acaban en un elemento vacío, Tab y Shift+Tab sangran un elemento, ↑ con el campo vacío recupera el último enviado, Esc quita la selección y luego el foco, crece de 2 a 8 líneas y desplaza, y al pegar conserva los saltos. Lo que hace cada tecla, el tamaño y los borradores por conversación están en un modelo puro con tests; el componente GPUI pinta y enruta teclas, IME y ratón. Nunca completa ni reescribe el texto."
Última modificación: "2026-11-18T03:00:00+01:00"
---

# Campo de texto común

Después de descubrir esta pieza en el catálogo, consulta
[el componente](../../crates/titan-agent/src/app/composer.rs) y
[su modelo](../../crates/titan-agent/src/composer.rs) como fuente de verdad de
su contrato detallado. Sigue el ejemplo `input.rs` de GPUI (sin crate de
terceros, así que no hay ADR). Decisión visual:
[[Decisiones visuales/Conversación de sesión y campo de texto común]]. Tests
de aceptación en `crates/titan-agent/tests/composer_acceptance.rs` y propios en
el módulo del modelo. El menú `/` y `@` llega con el chat (0.8.0).
