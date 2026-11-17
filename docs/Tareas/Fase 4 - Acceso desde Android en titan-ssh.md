---
Nombre: Fase 4 - Acceso desde Android en titan-ssh
Estado: Planificando
Resumen: Controlar las sesiones del equipo desde Android como un plugin de titan-ssh que se conecta por SSH al daemon titan-agentd y habla su protocolo; se planifica con la sesión de titan-ssh cuando el escritorio esté terminado.
Decisiones: ''
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-17T23:30:00+01:00
---

# Fase 4 - Acceso desde Android en titan-ssh

## Objetivo

Según [[Decisiones/ADR-0003 Rust con GPUI en escritorio y Android dentro de titan-ssh]]
y [[Decisiones/ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]]: un
plugin de titan-ssh (su ADR-0032: plugins incluidos que se activan) que abre una
sesión SSH al equipo, habla el protocolo del daemon por stdio y muestra el árbol
de sesiones, su terminal y el monitor con el lenguaje visual de titan-ssh. El
trabajo en titan-ssh se hace en su repo y con su bóveda.

## Notas para planificarla (2026-11-17)

- **Ya hay vías oficiales para hablar desde Android** con las sesiones
  locales:
  - Remote Control, con la app de Claude (suscripción; protocolo no
    público);
  - `codex remote-control`, con la app de ChatGPT (relay de OpenAI).
  
  Ver
  [[Decisiones/ADR-0012 Chat de sesión sobre las vías oficiales de cada CLI|ADR-0012]].
- **Lo que aporta el plugin de titan-ssh**: el monitor y el chat de
  [[Chat de sesión con Codex y Claude Code]] por el mismo protocolo del
  daemon (eventos `chat_*`), que está pensado para ir por stdio.

## Criterios de finalización

- Se planifica al terminar la fase 3, con una tarea en la bóveda de titan-ssh.

## Verificación

## Resultado
