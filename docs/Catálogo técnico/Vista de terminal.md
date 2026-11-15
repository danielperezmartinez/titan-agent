---
Nombre: "Vista de terminal"
Tipo: "Componente UI"
Área: "Interfaz"
Feature: "Terminal"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent/src/terminal_view.rs"
Entrada pública: "titan_agent::terminal_view::{TerminalView, TerminalViewEvent}; titan_agent::terminal_model::TerminalModel"
Resumen: "Terminal GPUI: TerminalModel (alacritty_terminal) alimentado con instantánea y salida, pintado en una rejilla medida con JetBrains Mono embebida y la paleta ANSI de titan-ssh; foco, teclado, pegado y rueda emiten Input y Resized para que la ventana los envíe al daemon."
Última modificación: "2026-11-15T13:00:00+01:00"
---

# Vista de terminal

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent/src/terminal_view.rs)
como fuente de verdad de su contrato detallado. El modelo está en `crates/titan-agent/src/terminal_model.rs`; ejemplo aislado en `crates/titan-agent/examples/terminal_demo.rs`.
