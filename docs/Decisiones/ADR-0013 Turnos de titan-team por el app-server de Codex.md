---
Nombre: Turnos de titan-team por el app-server de Codex
Número: 13
Estado: Propuesta
Resumen: 'titan-team deja de lanzar `codex exec` y ejecuta cada turno con un cliente JSON-RPC propio contra el app-server de Codex: el daemon compartido si está en marcha (el mismo al que se conectan titan-agentd y el chat) o, si no, un `codex app-server` propio por stdio. Así el hilo de cada rol tiene un solo escritor y el usuario puede hablar con él desde el chat sin que el director reciba «active writer». Antes de cada turno detecta los turnos que el usuario tuvo con el rol, los registra (commit y evento `user_turn`) y se los cuenta al director. Sustituye solo cómo se lanza el turno en ADR-0010 (pasos 3 a 5 y 7 de 1.5); team.toml y events.jsonl no cambian salvo el tipo nuevo.'
Decisión: titan-team ejecuta los turnos de los roles de Codex por el app-server (thread/start o thread/resume, turn/start y sus notificaciones) en lugar de con `codex exec`, y detecta y registra los turnos del usuario con el rol.
Consecuencias: No hay que esperar a `codex exec --remote` (0.171) ni añadir dependencias, porque es la API que ya usa el daemon. Más código en titan-team (cliente, mapeo de notificaciones, esquema de salida, tiempo límite) y una API marcada como experimental que cambia entre versiones de Codex. Lo comprueban tests con un app-server falso.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-17T23:20:00+01:00
Última modificación: 2026-11-17T23:20:00+01:00
---

# ADR-0013 · Turnos de titan-team por el app-server de Codex

## Contexto

[[ADR-0010 Formato de team.toml y de los eventos del equipo|ADR-0010]] (1.5)
ejecuta cada turno de un rol con `codex exec` o `codex exec resume`. Ese
proceso arranca **su propio** app-server. El candado «active writer» de
Codex es por proceso (`CODEX_HOME/thread-writer-locks/<id>.lock`), así que
si el hilo está cargado en otro proceso, el turno sale con `busy`
(salida 4). Eso pasa con el daemon compartido de Codex, al que se conectan
el TUI y el chat de
[[ADR-0012 Chat de sesión sobre las vías oficiales de cada CLI|ADR-0012]].
Las fuentes y el detalle están en ADR-0012.

Para que el usuario hable con un rol sin romper el equipo, los dos tienen
que pasar por **el mismo** app-server. Había tres caminos, y el usuario los
valoró el 2026-11-17 pidiendo no depender de terceros si había otra opción:

- esperar a `codex exec --remote` (PR #61707, hoy solo en la 0.171 alpha);
- un cliente JSON-RPC propio en titan-team;
- no tocar titan-team y aceptar el `busy` mientras el chat tenga el hilo.

## Decisión

Sustituye los pasos 3, 4, 5 y 7 de «1.5 Un turno (`ask`)» de ADR-0010. El
resto de ADR-0010 sigue vigente: `team.toml`, órdenes, prompts, rondas,
commits por turno, avisos de rutas, eventos y códigos de salida.

### Conexión

- **Ejecutable**: el de `TITAN_TEAM_CODEX` o `codex` del PATH, como hoy.
- **Con el daemon compartido** (`codex app-server daemon version` da
  `"status":"running"` con `socketPath`): titan-team se conecta a ese
  socket.
- **Sin él**: lanza `codex app-server` por stdio, sin shell y en el
  worktree, solo durante el turno. Sin daemon nadie más puede tener el hilo
  cargado por esa vía.
- **Saludo**: `initialize` con `clientInfo {name: "titan-team", version}`.
  Los métodos experimentales que se usen piden `experimentalApi: true`.

### Un turno

1. **Hilo nuevo**: `thread/start` en el worktree, con el modelo, el
   sandbox, la política de aprobación `never` y la configuración que hoy va
   por `-c` (esfuerzo, `[codex].config` y la `config` del rol).
2. **Hilo que continúa**: primero `thread/read` (estado) y
   `thread/turns/list` (turnos del usuario, ver abajo), y luego
   `thread/resume` con los mismos ajustes.
   - Si el hilo tiene un turno en curso, o Codex responde «already has an
     active writer», el turno es `busy` con el contrato de ADR-0010:
     salida 4, el rol conserva su `thread_id`, no se gasta la ronda y no
     hay reintentos ni cola.
3. **`turn/start`**: el prompt de siempre como entrada de texto y, si el
   rol tiene `output_schema`, el esquema como `outputSchema`.
4. **Notificaciones del turno**, filtradas por hilo y turno:
   - `item/started` de órdenes, cambios de ficheros, herramientas MCP y
     búsquedas web produce los mismos eventos `activity` que hoy;
   - el último `agentMessage` completado es la respuesta;
   - `thread/tokenUsage/updated` da el `usage`;
   - `turn/completed` cierra el turno con su estado (`completed` es `ok`;
     `failed` o `interrupted` son `error`).
5. **Tiempo límite**: al superar `timeout_s`, `turn/interrupt` y estado
   `timeout`.
6. **Al terminar**: `thread/unsubscribe`, y se cierra el servidor propio si
   lo hubo.
7. **Peticiones del servidor** (aprobaciones, preguntas): con la política
   `never` no deberían llegar. titan-team **no las responde**: si el usuario
   tiene el chat abierto, contesta él; si no, el turno acaba por tiempo
   límite.
8. **Salidas en `<state_dir>/<encargo>/`**:
   - `NN-<rol>.jsonl` guarda las notificaciones del turno, una por línea,
     en lugar del stdout de `exec`;
   - `NN-<rol>.stderr` guarda el stderr del servidor propio (vacío con el
     daemon compartido);
   - `.md` y `.json` no cambian.
9. **`--dry-run`** imprime el prompt y las peticiones JSON-RPC que se
   enviarían, sin conectar.

### Turnos del usuario con un rol

- **Detección**: el historial de titan-team guarda el id de turno de Codex
  de cada entrada (campo nuevo y opcional). Antes de continuar un hilo, los
  turnos de `thread/turns/list` posteriores al último suyo que no estén en
  el historial son **turnos del usuario** (desde el chat o el TUI).
- **Si los hay**:
  - en un rol que escribe con `autocommit`, se hace commit de lo que haya
    sin confirmar en el worktree con el mensaje
    `team: turnos del usuario con <rol>`;
  - se escribe un evento nuevo `user_turn` con `role`, `thread_id`,
    `turns` (número) y `text` (el primer mensaje del usuario, acotado a 200
    caracteres), y `commit` y `changed_files` como en `turn_finished`. Es
    un tipo aditivo: `v` sigue en 1 (regla de ADR-0010);
  - la salida de `ask` los cuenta antes de la respuesta: una línea en texto
    y el campo `user_turns` con `--json`.
- **El director se entera por su propia herramienta.** La app no envía nada
  al director.

## Alternativas consideradas

- **Esperar a `codex exec --remote`**: es un cambio mínimo, añadir
  `--remote` al argv. Descartada porque deja la tarea bloqueada sin fecha
  (0.171 en alpha), no está verificado que respete sandbox, carpeta,
  esquema y modelo, y es una dependencia nueva.
- **No tocar titan-team**: descartada. El `busy` seguiría mientras el chat
  tenga el hilo (más 60 s), y no está verificado qué pasa con un mensaje en
  cola mientras `exec` tiene el hilo: podría arrancarlo su propio servidor
  justo antes de salir.
- **Que el daemon de titan-agent ejecute los turnos en nombre de
  titan-team**: descartada. titan-team no depende del daemon (reglas de
  dependencia del README) y el daemon no lanza turnos (ADR-0001 y ADR-0005).

## Consecuencias

- **Positivas**:
  - un solo escritor por hilo, y el usuario puede hablar con un rol sin
    bloquear al director;
  - titan-team ve el estado real del hilo antes de lanzar;
  - el monitor y el chat pueden ver el turno de un rol en vivo;
  - no hay dependencia nueva: la API es la del daemon (listar, cuota,
    transcripción, detener);
  - la cola del chat no compite con titan-team.
- **Negativas / compromisos**:
  - más código en titan-team: cliente, mapeo de notificaciones, esquema y
    tiempo límite;
  - la API del app-server está marcada como experimental y cambia entre
    versiones;
  - `FORMAT.md` de titan-team y los tests de aceptación de ADR-0010 que
    comprueban el argv de `exec` se reescriben contra un app-server falso.
- **Por verificar en el spike** de
  [[Tareas/Chat de sesión con Codex y Claude Code]]: los nombres exactos de
  los parámetros de `thread/start` y `turn/start` en 0.169 (sandbox,
  configuración, esfuerzo, esquema) y el formato de `thread/turns/list`.
