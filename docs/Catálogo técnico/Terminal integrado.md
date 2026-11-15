---
Nombre: "Terminal integrado"
Tipo: "Componente UI"
Área: "Interfaz"
Feature: "Terminal"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent/src/terminal.rs"
Entrada pública: "TerminalView (attach, attached, feed, closed) y keys::keystroke_bytes"
Resumen: "Vista GPUI del terminal de una sesión: emula con alacritty_terminal los bytes que manda titan-agentd, los pinta con la paleta ANSI de titan-ssh y JetBrains Mono, y manda al daemon lo que se teclea (secuencias xterm), lo que se pega (ctrl+shift+v, bracketed paste) y los cambios de tamaño. Rueda: historial o eventos al programa."
Última modificación: "2026-11-15T11:37:00+01:00"
---

# Terminal integrado

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent/src/terminal.rs)
como fuente de verdad de su contrato detallado. Decisión en
[[Decisiones/ADR-0008 Terminal integrado con portable-pty en el daemon y alacritty_terminal en la interfaz]].
Tests del teclado en `crates/titan-agent/src/keys.rs`.
