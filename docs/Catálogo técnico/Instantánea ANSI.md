---
Nombre: "Instantánea ANSI"
Tipo: "Servicio"
Área: "Daemon"
Feature: "Terminal"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agentd/src/snapshot.rs"
Entrada pública: "titan_agentd::snapshot::snapshot_ansi"
Resumen: "Genera desde la rejilla de un `Term` de alacritty_terminal la secuencia ANSI que reproduce la pantalla visible en un emulador nuevo del mismo tamaño: celdas con colores y atributos, caracteres anchos y combinantes, cursor, modos (pantalla alternativa, cursor de aplicación, pegado entre corchetes) y la plantilla final del cursor. Sin historial."
Última modificación: "2026-11-14T21:07:32+01:00"
---

# Instantánea ANSI

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agentd/src/snapshot.rs)
como fuente de verdad de su contrato detallado.

Ver [[Decisiones/ADR-0008 Terminal con alacritty_terminal en el daemon y en la interfaz]]. Tests de ida y vuelta en `crates/titan-agentd/tests/snapshot_acceptance.rs`.
