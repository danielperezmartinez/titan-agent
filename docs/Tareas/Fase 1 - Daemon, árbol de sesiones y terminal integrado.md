---
Nombre: Fase 1 - Daemon, árbol de sesiones y terminal integrado
Estado: En curso
Resumen: Primera versión usable - titan-agentd descubre las sesiones de Claude Code y Codex y mantiene los terminales; la interfaz GPUI muestra la ventana principal con el árbol de sesiones por proyecto y, al elegir una, su terminal con el CLI oficial en el que escribe el usuario.
Decisiones: '[[Decisiones/ADR-0007 Socket local y protocolo NDJSON versionado entre daemon e interfaz]] · [[Decisiones/ADR-0008 Terminal con alacritty_terminal en el daemon y en la interfaz]] · Solo anidan los subagentes de Codex (parentThreadId), Claude va plano · Proyecto por git rev-parse --git-common-dir · Windows fuera de la fase 1 · Las interactivas de Claude se muestran sin terminal'
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-14T21:51:49+01:00
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

## Plan

Decisiones tomadas al empezar (2026-11-14), con la sesión de diseño:

- Protocolo, transporte, sesiones, descubrimiento y proyecto:
  [[Decisiones/ADR-0007 Socket local y protocolo NDJSON versionado entre daemon e interfaz]].
- Terminal (crates, instantánea al reabrir, render propio):
  [[Decisiones/ADR-0008 Terminal con alacritty_terminal en el daemon y en la interfaz]].
- Jerarquía «lanzada por»: solo los subagentes de Codex (`parentThreadId`).
  Claude no da el dato y no se adivina; los encargos del equipo, en la fase 3.
- Interactivas de Claude y acciones de parar: ampliación de
  [[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]].
- Windows: fuera de esta fase (el crate ya trae ConPTY; falta la named pipe y
  probarlo).

Encargos del equipo (skill «equipo»), en la rama `feat/fase-1`:

1. `nucleo-protocolo` — `titan-agent-core`: tipos del protocolo, sesión,
   correspondencias de Claude y Codex, proyecto, árbol y filtro, teclado.
2. `daemon` — `titan-agentd`: socket, descubrimiento, PTY e instantánea.
3. `interfaz` — `titan-agent`: cliente del daemon y ventana principal.

## Criterios de finalización

- Con varias sesiones reales de Claude Code y Codex abiertas, el árbol las muestra
  con su estado actualizado en segundos.
- Elegir una sesión muestra su terminal y se puede escribir en ella; cerrar y
  reabrir la ventana conserva el terminal.
- Ninguna parte lee credenciales de las CLI ni envía texto propio a un modelo
  (ADR-0001).
- Comprobaciones de la compuerta en verde.

## Verificación

Estado a 2026-11-14 en la rama local `feat/fase-1` (sin subir; versión 0.2.0
provisional, pendiente de acordar con el usuario):

- **Compuerta en local**: `cargo fmt --all -- --check` y
  `cargo clippy --workspace --all-targets -- -D warnings` limpios;
  `cargo test --workspace`: 108 tests en verde (con un `TMPDIR` propio, ver
  abajo); tests de `bin/team` en verde; gitleaks sobre los commits de la rama
  sin hallazgos y todos con el correo noreply. Falta el CI de la PR.
- **Daemon con datos reales** (Claude Code 2.1.325): `titan-agentd` en un
  socket temporal lista las sesiones de `claude agents --json` con su estado,
  agrupa los worktrees bajo su repo y deja las interactivas sin `attach`; un
  segundo daemon en el mismo socket se rechaza; en reposo, 0 % de CPU y ~5 MB
  de memoria. Con el proxy de Codex sin responder (sandbox de la sesión del
  director), Claude sigue listándose y el vigilante limpia el proxy.
- **Terminal**: probado de extremo a extremo con un CLI falso en los tests de
  aceptación (PTY compartido que sobrevive a la desconexión del cliente,
  instantánea que repone la pantalla al reabrir, entrada con eco, fin del
  proceso). **No** se ha adjuntado a una sesión real para no interferir con las
  del usuario.
- **No verificado**: la ventana GPUI no se ha visto en pantalla (la sesión del
  director no tiene display) y Codex con su daemon real (el proxy no responde
  dentro del sandbox del director). Los dos criterios de finalización con
  sesiones reales quedan para la prueba manual del usuario.
- **Fragilidad conocida**: el test
  `discovery_lists_subscribes_and_preserves_previous_claude_list_on_failure`
  falla si existe un `.git` en un ancestro de la carpeta temporal (p. ej. un
  `/tmp/.git` creado por otro proceso), porque el respaldo sin git sube hasta
  él. Con un `TMPDIR` limpio pasa.

Prueba manual pendiente (usuario):

1. `cargo build --workspace && ./target/debug/titan-agent` (la interfaz
   arranca el `titan-agentd` que está a su lado si no está en marcha) con varias
   sesiones de Claude Code y de Codex abiertas: el árbol las muestra con su
   estado en pocos segundos.
2. Elegir una sesión background de Claude: su terminal aparece y se escribe en
   él. Cerrar la ventana y volver a abrirla: el terminal conserva la pantalla.
3. Una interactiva de Claude muestra el aviso en `mute`; `[x] detener` y
   `[-] interrumpir` actúan sobre la sesión adecuada.
4. JetBrains Mono no está instalada en el equipo de desarrollo: sin ella, GPUI
   usa su fuente de respaldo.

## Resultado

Encargos del equipo (skill «equipo»), integrados en `feat/fase-1`:

| Encargo | Qué | Llamadas | Rondas de revisión |
| --- | --- | --- | --- |
| `nucleo-protocolo` | `titan-agent-core`: protocolo, descubrimiento, proyecto, árbol, teclado | Sol 3 · Luna 3 | 2 (la tercera comprobación la hizo el director) |
| `daemon` | `titan-agentd`: socket, descubrimiento, PTY, instantánea, apagado | Sol 4 · Luna 8 | 3 (tope; los últimos 4 arreglos los verificó el director) |
| `interfaz` | `titan-agent`: cliente, tema, emulador, teclado y ventana | Sol 4 · Luna 5 | 3 (tope; los últimos 2 arreglos los verificó el director) |

Decisiones tomadas durante los encargos (del director):

- Concurrencia del daemon: cola acotada por cliente (no se escribe en sockets
  con locks tomados), un mutex por terminal, descubrimiento de Claude y Codex en
  hilos separados, comandos con plazo, espera con `rustix::event::poll`, lock de
  fichero `<socket>.lock` para un daemon por socket y apagado acotado.
  Dependencia nueva: `rustix` (ya transitiva) con `process` y `event`.
- Interfaz: cola FIFO única hacia el daemon, generación por apertura, rejilla
  medida con la fuente y «desde» como tiempo relativo («hace N min»).
- Los roles de Codex no pueden usar sockets Unix en su sandbox: el director
  pasa esos tests fuera (ver
  [[Tareas/Aislar a los roles de Codex de las instrucciones del repo]]).

Mejoras anotadas para después: borrar el socket al recibir SIGTERM (hoy queda
huérfano y el siguiente arranque lo reemplaza); scrollback y selección de
texto; Windows.
