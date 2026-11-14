---
Nombre: "Codificación del teclado"
Tipo: "Modelo de datos"
Área: "Núcleo"
Feature: "Terminal"
Estado: "Vigente"
Ámbito: "Compartido"
Fuente: "crates/titan-agent-core/src/keys.rs"
Entrada pública: "titan_agent_core::keys::{encode_key, encode_paste, Key, Modifiers, KeyModes}"
Resumen: "Convierte teclas con modificadores en los bytes que espera un PTY xterm (control, alt con ESC, flechas normales o de aplicación, CSI con modificadores, F1–F12) y el pegado con o sin corchetes, sin dependencias de GPUI."
Última modificación: "2026-11-14T19:50:00+01:00"
---

# Codificación del teclado

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/keys.rs)
como fuente de verdad de su contrato detallado.

Ver [[Decisiones/ADR-0008 Terminal con alacritty_terminal en el daemon y en la interfaz]]. Tests en `crates/titan-agent-core/tests/keys.rs`.
