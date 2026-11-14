---
Nombre: "Modelo del terminal de la interfaz"
Tipo: "Modelo de datos"
Área: "Interfaz"
Feature: "Terminal"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent/src/terminal_model.rs"
Entrada pública: "titan_agent::terminal_model::{TerminalModel, StyledLine, Span}"
Resumen: "Emulador local de la interfaz sobre alacritty_terminal: recibe los bytes del daemon y entrega la pantalla como líneas de tramos con colores ya resueltos con la paleta de titan-ssh (negrita que aclara, inverso, dim, oculto), el cursor y los modos de teclado."
Última modificación: "2026-11-14T21:07:32+01:00"
---

# Modelo del terminal de la interfaz

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent/src/terminal_model.rs)
como fuente de verdad de su contrato detallado.

Paleta en [[Decisiones visuales/Lenguaje visual heredado de titan-ssh]]. Tests en `crates/titan-agent/tests/terminal_model.rs`.
