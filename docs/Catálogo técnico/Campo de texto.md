---
Nombre: "Campo de texto"
Tipo: "Componente UI"
Área: "Interfaz"
Feature: "Base"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent/src/text_area.rs"
Entrada pública: "TextArea::new(placeholder, min_rows, max_rows, cx), text(), clear(cx); eventos TextAreaEvent::{Changed, Submit}"
Resumen: "Campo de texto de varias líneas para GPUI (que no trae ninguno), sin dependencias nuevas: sigue el ejemplo input de GPUI (EntityInputHandler, así que teclas muertas e IME funcionan) y añade ajuste de línea, crecimiento entre min_rows y max_rows, desplazamiento vertical que sigue al cursor, selección con ratón y mayúsculas, palabras con ctrl, inicio/fin, copiar, cortar y pegar. Enter inserta salto de línea y ctrl+enter emite Submit. Nunca reescribe lo escrito. Estilo de titan-ssh: 13 px, interlineado 19 px, cursor y selección en accent."
Última modificación: "2026-11-15T20:48:00+01:00"
---

# Campo de texto

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent/src/text_area.rs)
como fuente de verdad de su contrato detallado. Lo usa
[[Catálogo técnico/Cuadro de nueva sesión]]. Tests de los pasos por carácter y
palabra en el propio fichero. No se eligió un crate (no hizo falta ADR): los
de terceros fijan otras versiones de GPUI.
