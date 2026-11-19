---
Nombre: Chat de sesión con Codex y Claude Code
Estado: En curso
Resumen: 'Versión 0.8.0, después de la 0.7.0 (que trae la vista «conversación» y el campo de texto común): llenar la conversación con el chat, con menú `/` y `@`, en tres PR (titan-team por el app-server, chat de Codex, chat de Claude Code). En Codex, por el app-server compartido, con titan-team ejecutando sus turnos en el mismo app-server para que no haya dos escritores. En Claude Code, envío con `claude --resume` en un PTY del daemon, lectura de la transcripción y hooks al lanzar para responder permisos, preguntas y planes. Empieza con un spike con los CLI reales. La hace el equipo v7 (configuración de [[Equipo v7]]).'
Decisiones: 'Investigación y diseño con el usuario el 2026-11-17 (sesión «chat-sesion», lanzada desde control-3b): Codex y Claude a la vez; titan-team con cliente JSON-RPC propio (ADR-0013); rol ocupado → cola oficial; previa y decisión visual; ADR-0012 y ADR-0013 aceptadas por el usuario el mismo día; decisión visual aceptada tras revisar la previa; un solo campo de texto para «Nueva sesión» y el chat; menú `/`; diffs, permisos, preguntas y planes en el chat. Al diseñar la 0.7.0 (mismo día) el usuario decidió que el chat va en la 0.8.0, sobre la estructura de la 0.7.0. La versión se acuerda en cada PR.'
Bloqueada: []
Fecha de creación: 2026-11-17T23:30:00+01:00
Última modificación: 2026-11-19T20:30:00+01:00
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
