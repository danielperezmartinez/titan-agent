---
Nombre: "Daemon titan-agentd"
Tipo: "Comando CLI"
Área: "Daemon"
Feature: "Sesiones"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agentd/src/server.rs"
Entrada pública: "titan-agentd [--socket <ruta>] · titan_agentd::{Daemon, DaemonHandle, Config, run}"
Resumen: "Daemon sin interfaz, uno por usuario (lock de fichero junto al socket). Sirve el protocolo local por un socket Unix privado, descubre las sesiones con `claude agents --json` (2 s con suscriptores, 15 s sin ellos) y con un `codex app-server proxy` persistente, calcula el proyecto con git y mantiene un PTY por sesión adjuntada que sobrevive a los clientes. Colas acotadas por cliente, comandos con plazo, apagado acotado. Configurable con TITAN_AGENTD_SOCKET, TITAN_AGENTD_CLAUDE, TITAN_AGENTD_CODEX y TITAN_AGENTD_GIT (`off` desactiva Codex o git)."
Última modificación: "2026-11-14T21:07:32+01:00"
---

# Daemon titan-agentd

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agentd/src/server.rs)
como fuente de verdad de su contrato detallado.

Contrato en [[Decisiones/ADR-0007 Socket local y protocolo NDJSON versionado entre daemon e interfaz]] y [[Decisiones/ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]]. Tests de aceptación en `crates/titan-agentd/tests/` con CLI falsos.
