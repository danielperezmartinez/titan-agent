---
Nombre: "titan-agent-launcher"
Tipo: "Comando CLI"
Área: "Interfaz"
Feature: "Base"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-launcher/src/main.rs"
Entrada pública: "titan-agent-launcher [install | uninstall | --version]; sin orden, arranca la versión instalada"
Resumen: "Lanzador estable de la instalación por usuario (ADR-0011 §4 y §7). install copia su carpeta a versions/<v> (temporal y rename, idempotente), apunta current, actualiza la copia estable del lanzador, crea ~/.local/bin/titan-agent y titan-team y el .desktop (StartupWMClass = app_id). uninstall lo quita todo menos la configuración. Sin orden: con update/pending.json cuenta los intentos y al tercer arranque vuelve a la versión anterior (bad.json, rolled-back.json, para el daemon malo por su pid, notify-send) y ejecuta current/bin/titan-agent con TITAN_AGENT_LAUNCHER."
Última modificación: "2026-11-17T20:40:00+01:00"
---

# titan-agent-launcher

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-launcher/src/main.rs)
como fuente de verdad de su contrato detallado. Tests de aceptación en `crates/titan-agent-launcher/tests/launcher_acceptance.rs` (C6, C7) y del empaquetado en `scripts/probar-instalacion.sh` (CI).
