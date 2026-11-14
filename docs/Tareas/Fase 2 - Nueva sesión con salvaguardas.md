---
Nombre: Fase 2 - Nueva sesión con salvaguardas
Estado: Pendiente
Resumen: Botón Nueva sesión en la columna izquierda y cuadro de encargo en el centro con los valores por defecto y botones para cambiar agente, modelo, esfuerzo, permisos o sandbox y proyecto; muestra el comando exacto y lanza el CLI oficial desde el daemon con las cinco salvaguardas de ADR-0002.
Decisiones: ''
Bloqueada: ["[[Fase 1 - Daemon, árbol de sesiones y terminal integrado]]"]
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-14T15:28:51+01:00
---

# Fase 2 - Nueva sesión con salvaguardas

## Objetivo

- Interfaz: la parte «Nueva sesión» de
  [[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]]
  (previa: estado `inicio="nueva"` de `design/preview/Main.dc.html`).
  GPUI no trae campo de texto: resolverlo (ADR si se elige un crate).
- Daemon: lanzar Claude Code con `claude --bg -n <nombre> --model … --effort …
  --permission-mode … "<encargo>"` en la carpeta del proyecto, y Codex por su vía
  oficial (TUI de `codex` en un PTY o `thread/start` de `codex app-server`).
- Las **cinco salvaguardas** de
  [[Decisiones/ADR-0002 Lanzar sesiones desde la app con salvaguardas]]: el texto
  lo escribe el usuario; un clic, un lanzamiento (sin programar, repetir ni
  encadenar); binario oficial sin modificar y sin leer credenciales; el comando
  exacto visible antes de lanzar; Codex por su vía oficial. Con tests que lo
  comprueben donde se pueda (p. ej. el daemon rechaza un lanzamiento sin texto
  del usuario o duplicado).
- Los valores por defecto se configuran en `[*]` (configuración).

## Criterios de finalización

- Lanzar una sesión de cada CLI desde la app: aparece en el árbol, queda
  seleccionada y su terminal funciona.
- Las salvaguardas están cubiertas por tests o, si no se puede, verificadas a
  mano y anotadas aquí.

## Verificación

## Resultado
