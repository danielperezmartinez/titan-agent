---
Nombre: Chat de sesión con Codex y Claude Code
Estado: En curso
Resumen: 'Versión 0.8.0, después de la 0.7.0 (que trae la vista «conversación» y el campo de texto común): llenar la conversación con el chat, con menú `/` y `@`, en tres PR (titan-team por el app-server, chat de Codex, chat de Claude Code). En Codex, por el app-server compartido, con titan-team ejecutando sus turnos en el mismo app-server para que no haya dos escritores. En Claude Code, envío con `claude --resume` en un PTY del daemon, lectura de la transcripción y hooks al lanzar para responder permisos, preguntas y planes. Empieza con un spike con los CLI reales. La hace el equipo v7 (configuración de [[Equipo v7]]).'
Decisiones: 'Investigación y diseño con el usuario el 2026-11-17 (sesión «chat-sesion», lanzada desde control-3b): Codex y Claude a la vez; titan-team con cliente JSON-RPC propio (ADR-0013); rol ocupado → cola oficial; previa y decisión visual; ADR-0012 y ADR-0013 aceptadas por el usuario el mismo día; decisión visual aceptada tras revisar la previa; un solo campo de texto para «Nueva sesión» y el chat; menú `/`; diffs, permisos, preguntas y planes en el chat. Al diseñar la 0.7.0 (mismo día) el usuario decidió que el chat va en la 0.8.0, sobre la estructura de la 0.7.0. La versión se acuerda en cada PR.'
Bloqueada: []
Fecha de creación: 2026-11-17T23:30:00+01:00
Última modificación: 2026-11-19T21:10:00+01:00
---

# Chat de sesión con Codex y Claude Code

## Objetivo

Que el usuario hable con cualquier sesión desde un chat, sin romper la
comunicación del equipo y con un protocolo que sirva luego a Android:

- **Arquitectura**:
  [[Decisiones/ADR-0012 Chat de sesión sobre las vías oficiales de cada CLI|ADR-0012]].
- **Turnos de titan-team**:
  [[Decisiones/ADR-0013 Turnos de titan-team por el app-server de Codex|ADR-0013]].
- **Aspecto**:
  [[Decisiones visuales/Conversación de sesión y campo de texto común]].

Sustituye la vista de solo lectura y «tomar el control» de
[[Abrir los hilos del equipo en solo lectura desde la app]] y resuelve sus
hallazgos de la prueba manual.

**Cómo se trabaja**:

- con el equipo vigente, el v7 de [[Equipo v7]] (skill `equipo`), en la
  rama `feat/chat-sesion`, que ya trae la previa
  `design/preview/Conversacion.dc.html`. Lo que cambia la v7 para estas PR:
  - la especificación de cada PR lleva los títulos «Contrato», «Firmas» y
    «Criterios visuales» (la de la PR 1, «Sin interfaz»), o titan-team no
    lanza a Sol; los criterios visuales de las PR 2 y 3 salen de la
    decisión visual y de la previa, antes de implementar;
  - antes de Sol, un esqueleto con las firmas del contrato y cuerpos
    `todo!()` que compile (tipos de la conversación, variantes del
    protocolo…): sus tests deben compilar y, si no, su turno sale en rojo;
  - ningún apartado remite a otra regla («como el monitor», «la misma regla
    que X») sin dar su firma, y las rutas se describen por su significado
    («ruta absoluta»), no por su forma en Linux;
  - la PR 1 cambia titan-team: se dirige con titan-team fijado
    (`bin/team pin origin/main`) y, si añade claves a `team.toml`, se fija
    el commit propio en verde antes de la revisión (`bin/team pin HEAD`);
  - la prueba a mano de cada PR va completa en el mensaje al usuario (la
    orden, los pasos numerados y el resultado esperado), no solo en esta
    nota;
- **va después de la
  [[Versión 0.7.0 - Proyectos importados y monitor por sesión directora|0.7.0]]**,
  que deja hecha la vista «conversación» (borrador y sesión con terminal),
  el campo de texto común y el monitor por sesión directora; la rama hace
  rebase sobre `main` antes de empezar;
- el spike (parte 0) se puede hacer antes, porque no toca código de la app;
- **tres PR, en este orden**, con la versión que se acuerde en cada una:
  1. titan-team por el app-server (parte 1), que es independiente de la
     interfaz y deja un solo escritor por hilo;
  2. chat de Codex (partes 2 a 4 y 6 para Codex);
  3. chat de Claude Code (partes 2, 3, 5 y 6 para Claude).

## Parte 0. Spike con los CLI reales (antes del contrato)

**Reglas**:

- datos inventados, un `CODEX_HOME` y una carpeta de proyecto de prueba
  cuando se pueda, y sesiones de Claude creadas para el spike;
- nada de sesiones reales del usuario;
- resultados anotados en esta nota, en «Spike»;
- **lo que el spike contradiga de las ADR se decide con el usuario antes de
  seguir.**

**Codex 0.169**:

1. `thread/queue/add` con el hilo libre (¿arranca ya?) y con un turno en
   curso (arranca al acabar, sin dirigirlo). `thread/queue/delete` y
   `thread/queue/changed`.
2. Dos clientes del mismo app-server sobre un hilo: los dos reciben las
   notificaciones; una aprobación la responde el primero y el otro recibe
   `serverRequest/resolved`.
3. Los parámetros exactos de `thread/start`, `thread/resume` y `turn/start`
   (sandbox, configuración, esfuerzo, `outputSchema`), y las formas de
   `thread/turns/list`, `item/tool/requestUserInput`, `collaborationMode`
   y `skills/list`.
4. Que un turno lanzado así por titan-team (prototipo) da el mismo
   resultado que `codex exec`: respuesta, informe con esquema, uso y
   actividad.

**Claude Code 2.1.x**:

5. `claude --resume <id> "texto"` en un PTY contra una sesión `--bg`:
   - libre y con un turno en curso (¿lo encola?);
   - cuándo se puede cerrar el PTY;
   - que cerrarlo no detiene la sesión.
6. Hooks puestos con `--settings`:
   - que la decisión de `PermissionRequest` evita el diálogo del TUI;
   - `PreToolUse` sobre `AskUserQuestion` con `updatedInput.answers`;
   - `ExitPlanMode`: aprobar y rechazar;
   - que el hook que termina sin decidir deja el diálogo;
   - el tope de espera.
7. La transcripción de una sesión de prueba: qué tipos de entrada hay para
   mensajes, razonamiento, herramientas (`Edit`, `Write`, `Bash`) y
   resultados. Con eso se escriben las fixtures a mano.
8. Escribir `/compact` en el PTY de `claude attach` sin Enter: queda escrito
   y el usuario lo lanza.

## Partes (el contrato numerado lo cierra el director tras el spike)

**Reglas comunes**:

- cada parte lleva sus firmas y al menos un test de aceptación de Sol;
- los nombres de abajo son la propuesta de las ADR, y el director los fija.

**1. titan-team por el app-server** (ADR-0013):

- cliente JSON-RPC con conexión al daemon compartido o servidor propio
  por stdio;
- turno nuevo y continuación, mapeo de notificaciones a `activity`,
  respuesta, esquema, uso, tiempo límite y `busy`;
- detección de turnos del usuario, commit y evento `user_turn`;
- `--dry-run`, y `FORMAT.md` y la skill `equipo` actualizados;
- tests con un app-server falso, reutilizando el de los tests del daemon.

**2. Núcleo**:

- tipos de la conversación: elementos (ampliar `TranscriptItem` o un tipo
  nuevo, con diff, plan y petición), estado del chat y cola;
- conversión de las notificaciones de Codex;
- parser tolerante de la transcripción de Claude, con «formato no
  reconocido» explícito;
- tipos de las peticiones al usuario y sus respuestas;
- el menú `/`: entradas por agente y la lista fija de comandos del CLI de
  Claude;
- el monitor lee el evento `user_turn`.

**3. Protocolo**, como ampliación compatible de ADR-0007:

- peticiones `chat_open`, `chat_close`, `chat_send`, `chat_unqueue`,
  `chat_interrupt`, `chat_answer`, `chat_menu` y `chat_command`;
- eventos `chat_item`, `chat_state` y `chat_request`;
- errores y su comportamiento con varios clientes;
- se anota en ADR-0007 en el mismo cambio.

**4. Daemon, Codex**:

- suscripción por chat abierto;
- `turn/start` o `thread/queue/add` según el estado, sin dirigir nunca un
  turno en curso;
- aprobaciones, preguntas, plan y elicitaciones;
- el menú (`model/list`, `skills/list`, `permissionProfile/list`…) y los
  comandos con método;
- `@` con `fuzzyFileSearch`.

**5. Daemon, Claude Code**:

- envío con `claude --resume` en un PTY propio;
- seguimiento de la transcripción;
- estado y `waitingFor` de `claude agents --json`;
- el subcomando `titan-agentd hook`: pregunta por el socket, espera solo
  si el chat de esa sesión está abierto, y si no, termina sin decidir;
- el `--settings` de los hooks en los lanzamientos de la app, visible en el
  comando exacto;
- el fragmento para los ajustes del usuario en la configuración, sin
  editarlos nunca.

**6. Interfaz**:

- el campo de texto y la vista «conversación» ya vienen de la 0.7.0; aquí
  se añade el menú `/` y `@` del campo;
- el cuerpo de la sesión pasa del terminal a la conversación, y sustituye a
  `thread_view.rs`;
- las peticiones al usuario, la cola, la línea de estado y `[t] terminal`;
- el clic en un rol del monitor o en una sesión del árbol abre la
  conversación.

## Criterios de finalización

**Con Codex**:

- con un rol del equipo libre y con el chat abierto, `titan-team ask` a ese
  rol funciona sin `busy`;
- un mensaje del usuario mientras el rol trabaja queda en cola y arranca al
  acabar el turno;
- el siguiente `ask` del director informa del turno del usuario
  (`user_turn`), con su commit.

**Con Claude Code**:

- un mensaje a una sesión `--bg` llega como turno del usuario y su
  respuesta aparece en el chat;
- con los hooks, un permiso y una pregunta se responden desde el chat;
- sin el chat abierto, el diálogo sale en el terminal.

**En la interfaz**:

- el menú `/` funciona en las dos sesiones;
- tras lanzar desde el borrador, el cuerpo de la sesión muestra la
  conversación y no el terminal;
- con una transcripción de formato desconocido, el chat lo dice y ofrece el
  terminal.

**Proceso**:

- revisión de Sol y evaluación visual del equipo v7;
- prueba manual del usuario con pasos numerados y el resultado esperado de
  cada uno;
- comprobaciones de «Comprobaciones y compuerta de "Hecha"» en verde;
- PR integrada con la versión acordada.

## PR 1 · Contrato (titan-team por el app-server)

Fijado por el director tras el spike, antes de los tests de Sol (encargo
`v08-pr1`, especificación completa en `.agents/state/v08-pr1/encargo.md`).
Sin interfaz.

**C1. Conexión.**
- El ejecutable es el de siempre: `TITAN_TEAM_CODEX` o `codex` del PATH.
- Antes del turno, titan-team ejecuta, sin shell,
  `<codex> app-server daemon version`. Si su salida estándar es un objeto
  JSON con `"status": "running"` y una cadena `socketPath`, y la plataforma
  admite sockets de dominio Unix, se conecta a ese socket (WebSocket sobre
  el socket, un mensaje de texto por petición o respuesta).
- En cualquier otro caso (otro `status`, salida que no es JSON, la orden
  falla, o no se puede conectar al socket) lanza `<codex> app-server` sin
  shell, con la carpeta de trabajo en el worktree del encargo, la entrada y
  la salida estándar por tuberías y el mismo entorno que hoy recibía `codex
  exec` (incluido `CARGO_TARGET_DIR` para un rol `workspace-write` con
  `project.target_dir`). Por stdio se intercambia **un objeto JSON por
  línea**. Ese servidor solo vive durante el turno: al acabar se cierra (y
  se mata su grupo de procesos si no sale).
- Si `<codex>` no se puede ejecutar, `ask` sale con 2 como hoy (mensaje que
  contiene `no está disponible`), sin escribir eventos.

**C2. Forma de las peticiones.** Cada petición es **un objeto JSON compacto
en una línea**, que empieza exactamente por `{"id":<n>,"method":"<método>"`
(luego `,"params":{…}}`), con `id` entero que empieza en 1 y crece de uno
en uno por conexión. Las notificaciones de titan-team no llevan `id`. Ningún
texto lleva saltos de línea sin escapar.

**C3. Saludo.** Primera petición: `initialize` con `params`
`{"clientInfo": {"name": "titan-team", "version": "<versión de titan-team>"},
"capabilities": {"experimentalApi": true}}`; tras su respuesta, la
notificación `{"method":"initialized"}`.

**C4. Hilo nuevo** (primer turno del rol en el encargo, o `--new`):
`thread/start` con `params`:
- `cwd`: la ruta absoluta del worktree del encargo;
- `model`: el `model` del rol; `sandbox`: el `sandbox` del rol;
  `approvalPolicy`: `"never"`;
- `config`: un objeto con
  - `model_reasoning_effort`: el `effort` del rol (solo si lo tiene);
  - cada par `clave=valor` de `[codex].config` y luego del `config` del rol:
    la clave tal cual (con sus puntos) y el valor leído como valor TOML
    (`"high"` → `"high"`, `5` → `5`, `true` → `true`, `["a"]` → `["a"]`); si
    no es TOML válido, la cadena tal cual. Un par posterior con la misma
    clave sustituye al anterior;
  - para un rol `workspace-write` con `project.target_dir`:
    `sandbox_workspace_write.writable_roots`: `[<ruta absoluta de
    target_dir>]` y `shell_environment_policy.set`: `{"CARGO_TARGET_DIR":
    "<ruta absoluta de target_dir>"}`.
- El hilo del rol es `result.thread.id`.

**C5. Hilo que continúa** (el rol ya tiene `thread_id` y no hay `--new`):
1. `thread/read {"threadId": <id>}`: si `result.thread.status.type` es
   `"active"`, el turno es **`busy`** (C9) y no se envía nada más;
2. `thread/turns/list {"threadId": <id>, "limit": 50}` (para C8);
3. `thread/resume` con `threadId` y los mismos `model`, `sandbox`,
   `approvalPolicy` y `config` de C4 (sin `cwd`).
- Si `thread/resume` o `turn/start` responden un error JSON-RPC cuyo
  `message` contiene `already has an active writer` (sin distinguir
  mayúsculas), el turno es **`busy`** (C9).
- Cualquier otro error JSON-RPC en `initialize`, `thread/start`,
  `thread/read`, `thread/turns/list`, `thread/resume` o `turn/start` deja el
  turno en `error`, con el mensaje del error en el `.stderr` del turno.

**C6. El turno.**
- `turn/start` con `params` `{"threadId": <id>, "input": [{"type": "text",
  "text": <prompt>}]}` y, si el rol tiene `output_schema`, `"outputSchema":
  <el contenido JSON de ese fichero>`. El turno es `result.turn.id`.
- Solo cuentan las notificaciones (mensajes con `method` y sin `id`) cuyo
  `params.threadId` es el del hilo y cuyo `params.turnId` (o
  `params.turn.id`) es el del turno:
  - `item/started` produce un evento `activity` (como mucho 200 por turno,
    texto de 200 caracteres como mucho) según `params.item.type`:
    - `commandExecution` → `$ <command>`, quitando la envoltura de shell
      (`/usr/bin/bash -lc 'cargo test'` → `$ cargo test`; la regla de hoy:
      si el programa es `sh`, `bash` o `zsh` con `-lc` o `-c`, se toma el
      guion y se le quitan unas comillas simples o dobles exteriores);
    - `fileChange` → `edita <rutas>`: las `path` de `item.changes`, unidas
      por `, `, relativas al worktree cuando están dentro;
    - `mcpToolCall` → `mcp_tool_call`; `webSearch` → `web_search`;
    - cualquier otro tipo no produce evento;
  - `item/completed` con `item.type = "agentMessage"`: su `item.text` es la
    respuesta del turno (gana el último);
  - `thread/tokenUsage/updated`: el `usage` del turno es la **suma** de
    `tokenUsage.last` de todas las del turno (`inputTokens` →
    `input_tokens`, `cachedInputTokens` → `cached_input_tokens`,
    `outputTokens` → `output_tokens`);
  - `turn/completed` cierra el turno: `turn.status = "completed"` → `ok`;
    cualquier otro (`failed`, `interrupted`) → `error`, con
    `turn.error.message` (si lo hay) en el `.stderr` del turno.
- Si la conexión se cierra (o el servidor propio sale) antes de
  `turn/completed`, el turno es `error`.
- **Peticiones del servidor** (mensajes con `id` y `method`, p. ej.
  aprobaciones o preguntas): titan-team **no las responde** ni las rechaza.
- La respuesta se guarda como hoy: `NN-<rol>.md`, o `NN-<rol>.json` si el
  rol tiene `output_schema` (y entonces se interpreta como hoy para
  `report`).

**C7. Tiempo límite y cierre.**
- Si el turno no completa en `timeout_s` del rol: `turn/interrupt
  {"threadId", "turnId"}`, se espera como mucho 10 s a su `turn/completed` y
  el estado es `timeout` (aunque llegue `interrupted`). Salida 1, como hoy.
- Al acabar (bien, mal o por tiempo), si hubo hilo: `thread/unsubscribe
  {"threadId"}` y se cierra la conexión; el servidor propio se cierra.
- **Salidas del turno** en `<state_dir>/<encargo>/`:
  - `NN-<rol>.jsonl`: cada mensaje recibido del servidor que lleva
    `method` (notificaciones y peticiones del servidor) y cuyo
    `params.threadId`, si lo tiene, es el del hilo, uno por línea, en orden
    de llegada;
  - `NN-<rol>.stderr`: el stderr del servidor propio (vacío con el daemon)
    y los mensajes de error de C5 y C6.

**C8. Turnos del usuario con un rol.**
- El historial (`<state_dir>/<encargo>.json`, `history[]`) guarda en cada
  entrada un campo nuevo **`turn_id`**: el `turn.id` de Codex del turno, o
  `null` si no llegó a empezar (p. ej. `busy`). Las entradas antiguas no lo
  tienen y valen `null`.
- Al continuar un hilo (C5, paso 2), con los turnos de `thread/turns/list`
  (`result.data`, **del más reciente al más antiguo**): se recorren desde el
  más reciente hasta el primero cuyo `id` sea el `turn_id` de alguna entrada
  del historial de ese rol; los recorridos antes de él son **turnos del
  usuario**. Si ninguno coincide (historial sin `turn_id`), no hay turnos del
  usuario.
- Si hay alguno, **antes** de `brief_sent`:
  - si `project.autocommit` es `true` y el rol es `workspace-write`, se hace
    commit de todo lo que haya sin confirmar en el worktree (como el commit
    de un turno: `git add -A`, y nada si no hay cambios) con el mensaje
    exacto `team: turnos del usuario con <rol>`;
  - se escribe el evento **`user_turn`** con `role`, `thread_id`, `turns`
    (cuántos), `text` (el texto del primer elemento `userMessage` del turno
    del usuario **más antiguo**: la concatenación de sus `content[]` de
    `type = "text"`, cortada a 200 caracteres con `…`), `commit` (el hash
    corto, o `null`) y `changed_files` (las rutas del commit, o `[]`);
  - la salida de `ask` lo cuenta **antes** de la respuesta: en texto, una
    línea que empieza por `Turnos del usuario con <rol>: <N>`; con `--json`,
    el campo `user_turns`: `{"turns": N, "text": …, "commit": …}`. Sin turnos
    del usuario, la línea no sale y `user_turns` es `null`.
- `user_turn` es un tipo nuevo del log de eventos v1 (`v` sigue en 1).

**C9. Hilo ocupado (`busy`).** Mismo contrato que hoy, con textos nuevos:
salida 4, nada se ejecuta (ni `checks` ni commit), el rol conserva su
`thread_id` y su número de turnos, la ronda de revisión no se gasta, no hay
reintentos ni hilo nuevo, y `turn_finished` sale con `status = "busy"` y
`text = "hilo de <rol> ocupado"`. En stderr, una línea que empieza por
`El hilo de <rol> está ocupado`. Ocurre por C5 (estado `active` o «active
writer»).

**C10. `--dry-run`.** No ejecuta `<codex>` (tampoco `daemon version`) ni se
conecta. Imprime en stdout:
- una línea `Peticiones JSON-RPC:` y, debajo, una línea por petición que se
  enviaría con la forma de C2: `initialize`, luego `thread/start` (hilo
  nuevo) o `thread/resume` (hilo que continúa; sin `thread/read` ni
  `thread/turns/list`), y `turn/start` con el prompt completo;
- después, como hoy, `--- prompt ---` y el prompt.
Sigue sin efectos: ni eventos, ni historial, ni `checks`.

**C11. Documentación.** `titan-team doc` (imprime
`crates/titan-team/FORMAT.md`) nombra `app-server`, `user_turn` y `turn_id`,
y ya no dice que el turno se ejecuta con `codex exec`.

## PR 1 · Resultado (rama `feat/v0.8.0`, versión 0.8.0)

**Hecho**: titan-team ejecuta los turnos por el app-server (`crates/titan-team/src/appserver.rs`,
`codex.rs`, `user_turns.rs`), turnos del usuario con commit y evento
`user_turn`, `turn_id` en el historial, `FORMAT.md` y la skill `equipo`
actualizados, y el kit `scripts/demo/equipo-appserver.sh` para probarlo con
el Codex real. El monitor ignora `user_turn` hasta la PR 2.

**Tests**: 8 de aceptación de Sol en `crates/titan-team/tests/appserver_acceptance.rs`
(turno 1, commit `d17fda2`; turno 2 solo cambió un `.last()` que clippy
rechazaba), 10 propios en `tests/appserver.rs` (el falso, el daemon por
WebSocket, la suma de `usage`, `busy` sin turnos del usuario repetidos y las
regresiones de la revisión) y unitarios en `codex/tests.rs`, `user_turns.rs`
y `appserver.rs`. El app-server falso compartido es `tests/support/mod.rs`.
Los tests antiguos (`cli.rs`, `equipo_v5/v6/v7`, `phase3`) los pasó Luna al
falso nuevo (2 turnos).

**Revisión de Sol** (high, `--gate`, hilo nuevo): `changes_requested`, 8
mayores y 2 menores. Decisiones del director:

| Hallazgo | Decisión |
| --- | --- |
| El plazo no vence con un servidor que no para de escribir | Aceptado: se comprueba antes de cada lectura; bandeja de mensajes acotada (10 000) |
| `initialize` fallido deja vivo el servidor propio | Aceptado: se cierra y su stderr va al error |
| `--gate` se ejecuta con el hilo ocupado | Aceptado: con `busy` no se ejecuta nada |
| `busy` en `turn/start` tras registrar los turnos del usuario | Aceptado en parte: `thread/resume` va antes de anunciar el turno (así `busy` por «active writer» no registra nada) y, con `--gate`, se relee el estado justo antes de `turn/start` para no dirigir un turno del usuario que empiece durante la compuerta. Queda una ventana de milisegundos sin cubrir |
| Un Codex no ejecutable escribe eventos | Aceptado: salida 2, «no está disponible», sin eventos |
| La interrupción podía esperar 20 s | Aceptado: un solo plazo de 10 s; `thread/unsubscribe` como mucho 3 s |
| Si falla el commit de los turnos del usuario, se sigue | Aceptado: el turno se detiene (salida 2) para no mezclar sus cambios con los del rol |
| El servidor propio que sale con un hijo vivo no se detecta | Aceptado: se vigila su salida mientras se espera |
| El `.jsonl` pierde mensajes de la preparación y del cierre | Aceptado: se vuelcan al cerrar |
| `daemon version` con salida ≠ 0 | Aceptado |

**Hallado en la prueba con el Codex real** (antes de la revisión): un turno
del usuario se registraba aunque luego el hilo saliera `busy`, y se habría
contado dos veces. Corregido (resume antes de anunciar) y con test.

**Verificación** (local, 2026-11-19): `cargo fmt --all --check`, `cargo
clippy --workspace --all-targets -- -D warnings` y `cargo test --workspace`
en verde (250 tests). Con el Codex real (daemon compartido 0.170, `gpt-6-luna`
low, repo de prueba, hilos borrados al acabar): turno nuevo por el daemon con
`CARGO_TARGET_DIR` en las órdenes del rol; turno del usuario por el mismo
daemon → el siguiente `ask` lo cuenta, hace commit y escribe `user_turn`;
turno del usuario en curso → `ask` sale con 4 sin ejecutar nada.

**Métricas (spike + PR 1)**: de 20:05 a 21:10, unos 65 min. Claude (esta
sesión, Opus): 176 respuestas, 178 llamadas, 169 k de salida, 0,38 M de
escritura en caché y 47 M de lectura. Codex:

| Encargo | Rol | Entrada (caché) | Salida | Tiempo |
| --- | --- | --- | --- | --- |
| spike | luna low, 9 turnos cortos | ~18 k c/u | — | — |
| `v08-pr1` t1 | Sol, tests | 276 k (234 k) | 9,8 k | 238 s |
| `v08-pr1` t2 | Sol, lint | 555 k (507 k) | 10,4 k | 36 s |
| `v08-pr1-tests` t1 | Luna, tests antiguos | 1,02 M (0,98 M) | 4,3 k | 198 s |
| `v08-pr1-tests` t2 | Luna, tests antiguos | 2,32 M (2,25 M) | 9,0 k | 181 s |
| `v08-pr1-revision` | Sol, revisión high | 796 k (698 k) | 7,4 k | 219 s |

Claude del spike: unos 12 turnos de haiku.

### Carencias del equipo (PR 1)

1. **El primer turno de Luna se quedó a medias** (3 de 5 ficheros, 1 M de
   entrada): paró al ver que la compuerta fallaba por un lint en un test de
   Sol que no podía tocar. *Causa*: la comprobación `build` de Sol compila
   pero no pasa clippy, así que el lint llegó a la compuerta de Luna.
   *Propuesta*: que `build_cmd` del equipo sea `cargo clippy --workspace
   --all-targets -- -D warnings` (compila los tests y además pasa los
   lints).
2. **Orden en segundo plano con tope**: Sol se lanzó con un tope de 10 min
   en la orden de fondo, menor que su `timeout_s` más las comprobaciones.
   Salió bien por poco. *Ajuste aplicado*: los turnos largos se lanzan
   desacoplados (`setsid nohup`) y se espera con un monitor. *Propuesta*:
   decirlo en la skill.
3. **Luna gasta mucha entrada en ediciones repetitivas largas** (3,3 M en
   dos turnos para 5 ficheros de tests). Aun así ahorra cuota de Claude.
   *Propuesta*: nada por ahora; anotarlo si se repite.
4. **Un fallo de diseño solo salió con el CLI real** (turno del usuario
   contado dos veces con `busy`): el contrato fijaba el orden de las
   peticiones pero no qué pasa con lo ya registrado si después sale
   `busy`. *Propuesta*: en contratos con efectos antes de una petición que
   puede fallar, un apartado que diga qué efectos quedan.

### Prueba a mano (PR 1)

Sin ventana: se prueba con el Codex real (unas pocas llamadas cortas de
luna). Desde la carpeta del repo:

1. `scripts/demo/equipo-appserver.sh up` → compila titan-team, crea un repo
   de prueba y el primer turno del rol `escritor` sale `estado: ok`, con la
   respuesta `hecho` y un commit con `cometa.txt`; al final, `Hilo del rol:
   <id>`.
2. `scripts/demo/equipo-appserver.sh hilo` → imprime `cd … && codex resume
   <id>`. Ejecútala en otro terminal → se abre el terminal de Codex con el
   hilo del rol.
3. En ese terminal escribe «Crea el fichero usuario.txt con el texto hola» y
   espera a que acabe → Codex crea el fichero.
4. Con el terminal de Codex abierto, `scripts/demo/equipo-appserver.sh
   siguiente` → primero `Turnos del usuario con escritor: 1 («Crea el
   fichero usuario.txt…»), commit <hash>` y luego el turno 2 `ok`. **No sale
   «active writer».**
5. En el terminal de Codex pide algo largo («Ejecuta sleep 30 y responde
   ok») y, mientras trabaja, `… siguiente` → sale con 4 y `El hilo de
   escritor está ocupado…`; no se ejecuta nada.
6. Cuando acabe, `… siguiente` → cuenta 1 turno del usuario (el del paso 5)
   y el turno sale `ok`.
7. `scripts/demo/equipo-appserver.sh eventos` → cada `user_turn` va antes de
   su `brief_sent`.
8. Sal del terminal de Codex y `scripts/demo/equipo-appserver.sh down` →
   borra el hilo de prueba en Codex y la carpeta.

## Spike

Hecho el 2026-11-19 por la sesión de la 0.8.0 (Opus), con Codex CLI 0.169.0
(el daemon compartido corría la **0.170.0**) y Claude Code **2.1.328**.
Modelos baratos (Codex `gpt-6-luna` con esfuerzo `low`, Claude `haiku`),
una carpeta de proyecto de prueba con datos inventados, y hilos y sesiones
propios, **borrados al acabar** (5 hilos de Codex con `thread/delete`; 3
sesiones de Claude paradas y sus ficheros borrados). Ninguna sesión del
usuario tocada.

### Codex (app-server, WebSocket sobre el socket del daemon compartido)

1. **Cola** (`thread/queue/*`, pide `experimentalApi: true` en
   `initialize.capabilities`):
   - `thread/queue/add {threadId, clientUserMessageId, input}` →
     `{queuedSubmission: {id, input, clientUserMessageId}}`;
   - con el hilo **libre** arranca ya (`thread/queue/changed` ×2 y
     `turn/started`); con un turno en curso **espera** y arranca como turno
     nuevo al acabar, sin dirigirlo;
   - `thread/queue/list {threadId}` → `{data: [QueuedSubmission]}`;
     `thread/queue/delete {threadId, queuedSubmissionId}` → `{deleted: true}`;
   - `thread/queue/changed {threadId}` no trae la cola: hay que pedir
     `list`.
   - El `userMessage` del turno que sale de la cola lleva
     `clientId = clientUserMessageId`.
2. **Dos clientes** del mismo app-server: tras `thread/resume`, los dos
   reciben todas las notificaciones del hilo y **las dos peticiones** del
   servidor (mismo `id`). Responde el primero y el otro recibe
   `serverRequest/resolved {threadId, requestId}`; una respuesta tardía del
   segundo no rompe nada. `thread/unsubscribe` → `{status: "unsubscribed"}`.
   **Un hilo sin turnos no se puede reanudar** desde otro cliente («no
   rollout found»).
3. **Parámetros (0.169/0.170)**:
   - `thread/start {cwd, model, sandbox: "read-only"|"workspace-write",
     approvalPolicy: "never"|"on-request"|"untrusted", config: {clave:
     valor}}` (el esfuerzo va como `config.model_reasoning_effort` o
     `turn/start.effort`) → `{thread: {id, path, status…}}`;
   - `thread/resume {threadId, model?, sandbox?, approvalPolicy?, config?,
     cwd?}`; con el hilo cargado en otro proceso responde el error
     `thread <id> already has an active writer` (comprobado con un
     `codex app-server` por stdio frente al daemon);
   - `turn/start {threadId, input: [{type: "text", text}], effort?,
     outputSchema?, collaborationMode?}` → `{turn: {id, status:
     "inProgress"}}`;
   - `thread/turns/list {threadId, limit?, cursor?, sortDirection?}` →
     `{data: [Turn], nextCursor}`, **del más reciente al más antiguo**; cada
     `Turn {id, status, items (resumen: userMessage y agentMessage),
     startedAt, completedAt}`;
   - `item/tool/requestUserInput` (servidor → cliente, en modo plan):
     `{threadId, turnId, itemId, questions: [{id, header, question, isOther,
     isSecret, options: [{label, description}]}], isBlocking}`; respuesta
     `{answers: {<id>: {answers: ["…"]}}}`. Funcionó.
   - `collaborationMode/list` → `{data: [{name, mode: "plan"|"default",
     model, reasoning_effort}]}`; en `turn/start`, `collaborationMode: {mode,
     settings: {model, reasoning_effort}}`.
   - `skills/list {cwds}` → `{data: [{cwd, skills: [{name, description…}]}]}`;
     `model/list` → `{data: [{id, displayName, description,
     supportedReasoningEfforts…}]}`; `permissionProfile/list {cwd}` →
     `{data: [{id: ":read-only"…}]}`; `fuzzyFileSearch {query, roots}` →
     `{files: [{root, path, file_name, score}]}`.
   - Aprobación de ficheros: `item/fileChange/requestApproval {threadId,
     turnId, itemId, reason, grantRoot}`; el diff está en el `fileChange`
     del `item/started` previo (`changes[].diff`, `kind.type`), y
     `turn/diff/updated {diff}` trae el del turno. Respuesta `{decision:
     "accept"|"acceptForSession"|"decline"|"cancel"}`.
4. **Un turno por el app-server da lo mismo que `codex exec`**: respuesta
   (último `agentMessage` con `phase: "final_answer"`), esquema (con
   `outputSchema` el texto es el JSON), uso (`thread/tokenUsage/updated
   {tokenUsage: {total, last: {inputTokens, cachedInputTokens,
   outputTokens…}}}`) y actividad (`item/started` con `commandExecution
   {command, commandActions}`, `fileChange`…). `turn/completed {turn:
   {status: "completed"|"failed"|"interrupted", error}}`. `codex
   app-server` por stdio habla JSON por líneas.

### Claude Code 2.1.328

5. **`claude --resume <sessionId> "texto"` en un PTY** contra una sesión
   `--bg`:
   - **necesita el `sessionId` completo** (el id corto de `claude agents`
     abre el selector «No sessions match»);
   - con la sesión **libre**, la transcripción recibe en ~1 s una entrada
     `user` con `promptSource: "queued"` y `origin.kind: "human"`, y empieza
     el turno;
   - con un turno **en curso**, la transcripción recibe `queue-operation
     {operation: "enqueue", content}` y luego un adjunto `queued_command`; el
     mensaje **se absorbe a mitad del turno** (`queue-operation remove`,
     `reason: "absorbed_mid_turn"`): Claude lo lee junto al siguiente
     resultado de herramienta, como cuando el usuario escribe en el TUI con
     el turno en marcha. **No hay cola de turnos: dirige el turno en curso.**
   - El PTY se puede soltar en cuanto aparece la entrada (Ctrl+Z, que
     desconecta y sale con 0, o cerrar el maestro): la sesión sigue en
     segundo plano (mismo PID, `idle`).
6. **Hooks con `--settings`**:
   - `PermissionRequest` con `{"hookSpecificOutput": {"hookEventName":
     "PermissionRequest", "decision": {"behavior": "allow"}}}` evita el
     diálogo (Write y Bash); con `"deny"` y `message`, la herramienta falla
     con ese mensaje («Denied by PermissionRequest hook»). Entrada:
     `{session_id, cwd, permission_mode, hook_event_name, tool_name,
     tool_input, permission_suggestions}`.
   - `PreToolUse` sobre `AskUserQuestion` con `permissionDecision: "allow"` y
     `updatedInput` = la entrada más `answers: {<pregunta>: <etiqueta>}`:
     responde sin diálogo («Your questions have been answered…»).
   - `ExitPlanMode`: **rechazar funciona** (`PreToolUse` con
     `permissionDecision: "deny"` y el motivo, que Claude recibe). **Aprobar
     no**: `PreToolUse allow` pasa a un `PermissionRequest`, y ni `allow` ni
     `allow` con `updatedPermissions: [{type: "setMode"…}]` quitan el diálogo
     del plan, que sigue en el TUI (`waiting` · `permission prompt`).
   - Un hook que acaba sin salida deja el diálogo: `claude agents --json`
     pasa a `status: "waiting"`, `waitingFor: "permission prompt"`.
   - **Tope de espera**: el `timeout` del propio hook en los ajustes (en
     segundos). Al agotarse, sigue el flujo normal (diálogo; en
     `ExitPlanMode`, el `PermissionRequest`).
   - `Elicitation`: **no probado** (no había un servidor MCP de prueba).
7. **Transcripción** (`~/.claude/projects/<carpeta>/<sessionId>.jsonl`):
   - mensajes: `type: "user"` con `message.content` texto (prompt) o lista
     con `tool_result {tool_use_id, content, is_error}`; `type: "assistant"`
     con `message.content` lista de `text`, `thinking` (texto vacío y firma)
     y `tool_use {id, name, input}` (`Write {file_path, content}`, `Bash
     {command, description}`, `AskUserQuestion {questions}`, `ExitPlanMode
     {plan, planFilePath}`);
   - fin de turno: `type: "system", subtype: "turn_duration"`;
   - la cola: `queue-operation` y `attachment.type = "queued_command"`;
   - **mucho más que no es conversación** y no se lee nunca: `attachment`
     con el prompt de sistema, instrucciones, el correo y la organización
     del usuario, `api-request*`, `session-owner`, `file-history-snapshot`…
     El parser solo toma `user`, `assistant`, `system/turn_duration` y la
     cola; las fixtures se escriben a mano.
8. **`/compact` en el PTY de `claude attach` sin Enter**: queda escrito en el
   campo con su menú de comandos abierto, sin lanzarse.

### Lo que contradice o precisa las ADR (a decidir con el usuario)

- **Claude con un turno en curso**: `claude --resume` no encola un turno
  nuevo, dirige el turno en marcha (ADR-0012 solo prohíbe dirigir en Codex).
- **Aprobar un plan de Claude** desde el chat no es posible con hooks
  (ADR-0012 §3 lo daba por hecho); rechazarlo sí.
- Detalle sin decisión: el envío necesita el `sessionId` completo (lo da
  `claude agents --json`).

## Verificación

## Resultado
