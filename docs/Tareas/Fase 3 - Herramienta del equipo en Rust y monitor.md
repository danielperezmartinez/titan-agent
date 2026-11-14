---
Nombre: Fase 3 - Herramienta del equipo en Rust y monitor
Estado: Pendiente
Resumen: Reescribir en Rust la herramienta del equipo (titan-team, con su skill) con un formato de eventos versionado, hacer que el daemon lo lea, y construir el monitor del proyecto con sus tres vistas (grafo, carriles, mesa de control) en la zona central.
Decisiones: ''
Bloqueada: ["[[Fase 2 - Nueva sesión con salvaguardas]]"]
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-14T15:28:51+01:00
---

# Fase 3 - Herramienta del equipo en Rust y monitor

## Objetivo

- **`titan-team`** según
  [[Decisiones/ADR-0005 Herramienta del equipo en Rust dentro de titan-agent]]:
  mismas órdenes y modelo que el prototipo (`roles/new/ask/show/diff/test/close`,
  `team.toml`, hilos por `thread_id`, worktree y commit por turno, aviso de rutas
  prohibidas, tope de rondas, informe del revisor con esquema, `--json`), sin el
  adaptador `claude -p`. **ADR** con el formato de eventos. Sustituye al
  prototipo en Python, que se retira del repo.
- **Daemon**: lee los eventos y los une a las sesiones para saber quién habla con
  quién y en qué ronda va cada encargo.
- **Monitor** de [[Decisiones visuales/Monitor del proyecto en tres vistas]]:
  la cabecera del proyecto en el árbol lo abre; barra con
  `[grafo] [carriles] [mesa de control]`; animaciones (punto que viaja, carriles
  que corren, línea ASCII, spinner). Previa: `MonitorA/B/C.dc.html` y el estado
  `inicio="monitor"` de `Main.dc.html` en `design/preview/`.
- La cuota solo si [[Investigar la cuota restante de cada CLI]] confirma que se
  puede obtener; si no, la tarjeta de la mesa de control va sin ella.

## Criterios de finalización

- Un encargo real del equipo se ve en las tres vistas en tiempo real.
- El prototipo en Python ya no está en el repo.

## Verificación

## Resultado
