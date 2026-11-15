---
Nombre: "Campo de encargo"
Tipo: "Componente UI"
Área: "Interfaz"
Feature: "Nueva sesión"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent/src/brief.rs"
Entrada pública: "BriefInput (new, text, clear; eventos BriefEvent::Changed y Submit) y BriefText (modelo de edición sin GPUI)"
Resumen: "Campo de texto de varias líneas para GPUI, que no trae ninguno: texto y composición IME por el input handler de la plataforma, teclas de edición y movimiento (flechas, palabras con ctrl, inicio y fin, selección con mayúsculas y ratón, copiar, cortar, pegar), ajuste de línea con shape_text, crece de 4 a 12 líneas y después desplaza. ctrl+enter emite Submit. Nunca completa ni reescribe el texto."
Última modificación: "2026-11-15T20:40:00+01:00"
---

# Campo de encargo

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent/src/brief.rs)
como fuente de verdad de su contrato detallado. Sigue el ejemplo `input.rs`
de GPUI (sin crate de terceros, así que no hay ADR). Tests del modelo de
edición en el propio módulo.
