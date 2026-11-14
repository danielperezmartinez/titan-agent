---
Nombre: Fase 1 - Daemon, árbol de sesiones y terminal integrado
Estado: Pendiente
Resumen: Primera versión usable - titan-agentd descubre las sesiones de Claude Code y Codex y mantiene los terminales; la interfaz GPUI muestra la ventana principal con el árbol de sesiones por proyecto y, al elegir una, su terminal con el CLI oficial en el que escribe el usuario.
Decisiones: ''
Bloqueada: ["[[Fase 0 - Preparar el workspace y el equipo del prototipo]]"]
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-14T15:28:51+01:00
---

# Fase 1 - Daemon, árbol de sesiones y terminal integrado

## Objetivo

- **Daemon** según [[Decisiones/ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]]:
  - sesiones de Claude Code con `claude agents --json` (campos vistos el
    2026-11-14: `id`, `pid`, `cwd`, `kind` interactive/background,
    `startedAt`, `sessionId`, `name`, `status`/`state`); comprobar el
    formato con la versión instalada y no depender de campos no documentados sin
    tolerar su ausencia;
  - sesiones de Codex con `codex agents` / el daemon `codex app-server`
    (JSON-RPC; `codex app-server generate-json-schema` da los tipos);
  - jerarquía «lanzada por» para anidar sesiones (de dónde sale: investigarlo; si
    no hay dato fiable, sin anidar en esta fase);
  - PTY de `claude attach <id>` y `codex resume <id>` que sobreviven al cierre
    de la ventana; detener una sesión con el comando oficial;
  - socket local y protocolo versionado: **ADR nueva** con el formato.
- **Interfaz GPUI**: ventana principal de
  [[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]] con los
  tokens de [[Decisiones visuales/Lenguaje visual heredado de titan-ssh]]:
  cabecera con contadores, columna izquierda (filtro y árbol por proyecto con
  estado y pastilla de agente), zona central con el terminal de la sesión
  seleccionada y el panel de detalle. El terminal se apoya en el precedente de
  Zed (`alacritty_terminal` sobre GPUI): **ADR** con los crates elegidos.
- Linux primero; Windows en esta fase si el coste es razonable (PTY con ConPTY,
  named pipe).

## Criterios de finalización

- Con varias sesiones reales de Claude Code y Codex abiertas, el árbol las muestra
  con su estado actualizado en segundos.
- Elegir una sesión muestra su terminal y se puede escribir en ella; cerrar y
  reabrir la ventana conserva el terminal.
- Ninguna parte lee credenciales de las CLI ni envía texto propio a un modelo
  (ADR-0001).
- Comprobaciones de la compuerta en verde.

## Verificación

## Resultado
