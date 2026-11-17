---
Nombre: Chat de sesión sobre las vías oficiales de cada CLI
Número: 12
Estado: Propuesta
Resumen: 'La app muestra cada sesión como una conversación con un campo de texto (el mismo componente que «Nueva sesión») en lugar del terminal en crudo, y entrega lo que escribe el usuario por la vía oficial de cada CLI: en Codex, el app-server compartido (turn/start con el hilo libre, la cola oficial thread/queue/add con el hilo ocupado, aprobaciones, preguntas, diffs y plan en el chat); en Claude Code, `claude --resume <id> "<texto>"` en un PTY del daemon para enviar, la transcripción local para leer y hooks oficiales puestos al lanzar (`--settings`) para responder permisos, preguntas y planes. El terminal sigue para lo que solo existe en el TUI. Ningún texto lo genera la app.'
Decisión: Chat de sesión sobre el app-server de Codex y, en Claude Code, sobre `claude --resume` en PTY, la transcripción local y hooks puestos al lanzar; con el rol del equipo ocupado, cola oficial; el terminal como respaldo.
Consecuencias: Desaparece el «active writer» con titan-team solo si titan-team ejecuta sus turnos en el mismo app-server (ADR-0013). La lectura del chat de Claude depende de un formato interno que puede cambiar en cualquier versión (el chat lo detecta y ofrece el terminal). Riesgo residual de términos de Anthropic igual al de ADR-0002. Protocolo del daemon ampliado de forma compatible (ADR-0007).
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-17T23:15:00+01:00
Última modificación: 2026-11-17T23:15:00+01:00
---

# ADR-0012 · Chat de sesión sobre las vías oficiales de cada CLI

## Contexto

Hoy cada sesión se ve en un terminal integrado con el CLI oficial
([[ADR-0008 Terminal integrado con portable-pty en el daemon y alacritty_terminal en la interfaz|ADR-0008]]).
Para hablar con un rol de Codex del equipo, el usuario abre su hilo con
`codex resume` y la app pasa a ser **escritora**: el siguiente
`titan-team ask` del director sale con «thread already has an active writer»
(salida 4, [[ADR-0010 Formato de team.toml y de los eventos del equipo|ADR-0010]]).
La 0.5.0 lo rodeó con una vista de solo lectura y «tomar el control»
([[Tareas/Abrir los hilos del equipo en solo lectura desde la app]]), pero la
prueba manual del usuario mostró que el flujo no se entiende
([[Tareas/Ronda de arreglos 0.5.0]], hallazgos del 2026-11-17).

El usuario propone un **chat**: escribe en un campo, ve las respuestas como
conversación y la app entrega el mensaje a la sesión sin romper la
comunicación entre agentes. Encaja además con la app de Android
([[Tareas/Fase 4 - Acceso desde Android en titan-ssh|Fase 4]]), donde un chat
es mucho más usable que un terminal.

La investigación se hizo el 2026-11-17 con Codex 0.169.0 y Claude Code
2.1.326, sobre la documentación oficial y el código abierto de Codex
(`openai/codex`, rama `main` y etiquetas de versión).

### Términos

- **Anthropic.**
  - Permitido de forma explícita: «Nor does it prevent an end user from
    signing in to the unmodified Claude Code binary with their own Claude
    subscription»
    (<https://code.claude.com/docs/en/legal-and-compliance>).
  - Prohibido:
    - «Anthropic does not permit third-party developers to offer Claude.ai
      login into their own applications, or to route requests through
      Free, Pro, Max, or Ultra plan credentials on behalf of their users»
      (misma página);
    - el Agent SDK con el login de claude.ai en productos de terceros
      (<https://code.claude.com/docs/en/agent-sdk/overview>);
    - el acceso «through automated or non-human means, whether through a
      bot, script, or otherwise» salvo con API key (Consumer Terms §3.7,
      <https://www.anthropic.com/legal/consumer-terms>).
  - **Ambiguo**: una app local que reenvía al binario oficial el texto que
    escribe el usuario y le muestra la respuesta. Es el mismo riesgo
    residual que ya acepta
    [[ADR-0002 Lanzar sesiones desde la app con salvaguardas|ADR-0002]], y
    se acota con sus salvaguardas.
  - No se encontró ningún cambio fechado en 2026 en estos textos.
- **OpenAI.**
  - El app-server es la interfaz para clientes propios: «Codex app-server
    is the interface Codex uses to power rich clients (for example, the
    Codex VS Code extension)… Use it when you want a deep integration
    inside your own product» (<https://learn.chatgpt.com/docs/app-server>;
    las páginas de developers.openai.com/codex redirigen ahí).
  - La restricción es para servicios comerciales u hospedados: «App-server
    authentication has never been permitted for commercial or hosted
    services». Una app local y personal con el login del propio usuario
    encaja.
  - **No verificado**: el texto actual de los Terms of Use de openai.com
    (la página respondió 403).

### Codex: vía oficial completa

- **Hablar con un hilo** (<https://learn.chatgpt.com/docs/app-server>):
  - `thread/resume` suscribe la conexión al hilo y reengancha un turno en
    curso;
  - `turn/start {threadId, input:[{type:"text",text}]}` envía el mensaje;
  - las notificaciones `turn/started`, `item/started`,
    `item/agentMessage/delta`, `item/completed`, `turn/completed` y
    `thread/status/changed` dan la conversación en vivo;
  - `turn/interrupt` detiene el turno (el daemon ya lo usa).
- **`turn/start` con un turno en curso lo dirige** (steer) desde la 0.148
  (PR #38275, test `turn_start_steers_active_turn_and_returns_active_turn_id`).
- **Cola oficial** desde la 0.149 (PR #39092; `codex-rs/ext/queue/src/service.rs`):
  - métodos `thread/queue/add|list|update|delete|reorder|start` y la
    notificación `thread/queue/changed` (experimentales), y la orden
    `codex queue`;
  - se guarda en el SQLite de Codex y arranca el mensaje como turno nuevo
    **cuando el hilo queda libre**; nunca dirige un turno en curso;
  - tras una interrupción no envía nada solo;
  - funciona aunque otro proceso tenga el hilo.
- **«Active writer» es un candado de fichero por proceso**
  (`CODEX_HOME/thread-writer-locks/<id>.lock`,
  `codex-rs/rollout/src/writer_lock.rs`):
  - todos los clientes de un mismo app-server lo comparten;
  - el TUI usa el daemon compartido por defecto (PR #46088);
  - `codex exec` (0.169) arranca su propio servidor y por eso choca;
  - el hilo se descarga 60 s después de irse su último suscriptor;
  - `codex exec --remote` (PR #61707) solo existe desde la 0.171.0-alpha.8.
- **En el chat cabe todo**:
  - diffs: el elemento `fileChange` trae `changes[].diff` y
    `turn/diff/updated` trae el diff del turno;
  - aprobaciones (`item/commandExecution/requestApproval` e
    `item/fileChange/requestApproval`): gana la primera respuesta
    (`app-server/src/outgoing_message.rs`) y las demás pantallas cierran el
    aviso con `serverRequest/resolved`;
  - preguntas: `item/tool/requestUserInput`;
  - plan: `turn/plan/updated` y elementos `plan`;
  - elicitaciones MCP: `mcpServer/elicitation/request`.
- **Comandos `/`**: son del cliente (`codex-rs/tui/src/slash_command.rs`) y
  casi todos tienen método: `model/list` y los campos de `turn/start`,
  `permissionProfile/list`, `collaborationMode/list`,
  `thread/compact/start`, `review/start`, `thread/fork`,
  `thread/name/set`, `skills/list` con el elemento de entrada `skill`, y
  `fuzzyFileSearch` con el elemento `mention`. No hay un método que liste
  los comandos.

### Claude Code: no hay vía oficial equivalente

- **Enviar a una sesión en marcha**: `claude --resume <id> "texto"` manda el
  prompt a la sesión en segundo plano y se adjunta. Necesita un TTY: con E/S
  redirigida «sends nothing and exits with status 1», y no envía prompts que
  empiecen por `/` o `!` (<https://code.claude.com/docs/en/sessions>,
  <https://code.claude.com/docs/en/agent-view>).
- **Leer**:
  - no hay lectura estructurada oficial de una sesión existente;
  - la transcripción `~/.claude/projects/<proyecto>/<id>.jsonl` existe,
    pero «The entry format is internal to Claude Code and changes between
    versions» (sessions);
  - `claude logs <id>` da solo texto.
- **Estado**: `claude agents --json` da `status` y, con `status = waiting`,
  `waitingFor` (`permission prompt`, `input needed`, `sandbox request`,
  `worker request`, `dialog open`) (agent-view).
- **Hooks** (<https://code.claude.com/docs/en/hooks>):
  - `PermissionRequest` decide `allow` o `deny` sobre una petición de
    permiso;
  - `PreToolUse` sobre `AskUserQuestion` da las respuestas con
    `updatedInput.answers`, y sobre `ExitPlanMode` aprueba el plan
    («AskUserQuestion and ExitPlanMode require user interaction»);
  - `Elicitation` responde a MCP;
  - un hook que sale sin decisión deja el flujo normal;
  - los hooks se configuran al lanzar con `--settings`, y una sesión en
    segundo plano conserva sus flags al reabrirla.
- **Skills sin barra**: «To run a skill directly, put its name at the start
  of your message» (<https://code.claude.com/docs/en/skills>).
- **No hay forma oficial de listar los comandos** de una sesión existente
  (solo el Agent SDK, que no se puede usar con la suscripción), ni de
  ejecutar un comando `/` desde fuera.
- **Diffs**: la entrada de `Edit` trae `old_string`/`new_string` y la de
  `Write` el contenido (<https://code.claude.com/docs/en/tools-reference>).
- **Descartados**:
  - **Channels** (<https://code.claude.com/docs/en/channels>): research
    preview; un canal propio exige `--dangerously-load-development-channels`
    con un diálogo en cada lanzamiento, y el mensaje llega como evento del
    canal, no como turno del usuario.
  - **Mensajes entre sesiones**
    (<https://code.claude.com/docs/en/cross-session-messaging>): «Claude
    Code tells B's Claude that the message came from another session, not
    from you».
  - `claude -p`, `stream-json` y el Agent SDK, por los términos
    ([[ADR-0001 Torre de control y no orquestador|ADR-0001]]).

### Móvil

Ya hay vías oficiales para hablar desde Android con las sesiones locales:

- **Remote Control**: la app de Claude; solo con suscripción y protocolo no
  público (<https://code.claude.com/docs/en/remote-control>).
- **`codex remote-control`**: la app de ChatGPT, por el relay de OpenAI
  (<https://learn.chatgpt.com/docs/remote-connections>).

titan-agent no las sustituye. Su chat sirve para el monitor y los roles del
equipo, y el mismo protocolo del daemon lo llevará a titan-ssh.

## Decisión

### 1. Una vista «conversación» por sesión

- **Una sola vista**: «Nueva sesión» y el chat son la misma vista
  «conversación», con un campo de texto común
  ([[Decisiones visuales/Conversación de sesión y campo de texto común]]).
- **Lo que aparece**: mensajes del usuario y del agente (con streaming),
  razonamiento plegado, órdenes, cambios con su diff, estado del turno,
  cola, peticiones al usuario y el menú `/`.
- **El terminal no desaparece**: `[t] terminal` lo abre con el CLI oficial,
  como hoy.

### 2. Codex: por el app-server compartido

- **Abrir y cerrar**: el daemon suscribe la conexión con `thread/resume`
  mientras el chat está abierto y la suelta con `thread/unsubscribe`.
- **Enviar**:
  - con el hilo libre, `turn/start`;
  - con un turno en curso, `thread/queue/add`: se ve «en cola» y se puede
    quitar con `thread/queue/delete`;
  - **la app nunca dirige un turno en curso**, sea del director o del
    propio usuario.
- **Peticiones al usuario**: aprobaciones, preguntas, plan y elicitaciones
  se responden en el chat.
- **Comandos `/`**: el menú se monta con los métodos de arriba.
- **Requisito**: para que no haya dos escritores, titan-team ejecuta sus
  turnos en el mismo app-server
  ([[ADR-0013 Turnos de titan-team por el app-server de Codex|ADR-0013]]).
- **Turnos del usuario con un rol**: los detecta titan-team antes de su
  siguiente turno y se los cuenta al director (ADR-0013). La app no avisa a
  nadie por su cuenta.

### 3. Claude Code: envío por el CLI, lectura de la transcripción y hooks

- **Enviar**:
  - el daemon ejecuta `claude --resume <id> "<texto del usuario>"` en un PTY
    propio, sin shell, y cierra ese PTY cuando la sesión ha recibido el
    mensaje (la sesión sigue en segundo plano);
  - solo para sesiones en segundo plano de `claude agents --json`; las
    interactivas abiertas en otro terminal se leen pero no se escriben;
  - el texto que empieza por `/` o `!` no se envía: el chat ofrece el
    terminal.
- **Leer**:
  - el daemon sigue la transcripción local (ya lee su final para el modelo)
    y un parser tolerante del núcleo la convierte a elementos de
    conversación;
  - si no reconoce el formato, el chat lo dice y ofrece el terminal; nunca
    inventa;
  - los tests usan transcripciones escritas a mano.
- **Responder desde el chat** (permisos, preguntas y planes):
  - con hooks oficiales puestos al lanzar con `--settings`: un subcomando
    nuevo, `titan-agentd hook`, atiende `PermissionRequest`, `PreToolUse`
    (`AskUserQuestion`, `ExitPlanMode`) y `Elicitation`;
  - el hook pregunta al daemon y espera la respuesta del chat;
  - **si el chat de esa sesión no está abierto, o se agota el tiempo,
    termina sin decidir** y el diálogo sale en el terminal como siempre;
  - nunca decide solo.
- **Qué sesiones llevan los hooks**:
  - las que lanza la app los llevan siempre, y se ven en el comando exacto
    (ADR-0002);
  - para las lanzadas fuera, la configuración de la app muestra el
    fragmento de hooks para que el usuario lo añada a sus ajustes si
    quiere. **La app no edita nunca los ajustes de Claude Code.**
- **Comandos `/`**:
  - las **skills** (de las carpetas de skills del proyecto y del usuario)
    se envían como mensaje con su nombre al principio, sin barra;
  - los **comandos propios del CLI** (`/compact`, `/model`, `/clear`…)
    abren el terminal debajo del chat con el comando escrito, y el usuario
    pulsa Enter allí.

### 4. Salvaguardas (las de ADR-0002, aplicadas a cada envío)

1. Todo texto que llega a un modelo lo escribe el usuario. La app no genera,
   completa ni traduce mensajes; aprobar un plan de Codex es escribir el
   siguiente mensaje.
2. Cada envío o respuesta sale de una acción del usuario (Enter o clic).
   Nada de reintentos, programaciones ni envíos disparados por eventos. La
   cola la vacía Codex, que es su función oficial.
3. Binario oficial sin modificar. Nunca se leen credenciales: de
   `~/.claude` solo la transcripción de la sesión abierta y las carpetas de
   skills.
4. Lo que se ejecuta se ve: el comando de envío de Claude y el `--settings`
   de los hooks.
5. Los hooks solo transmiten la elección del usuario.

### 5. Protocolo del daemon

Es una ampliación compatible de
[[ADR-0007 Protocolo local de titan-agentd en NDJSON versionado|ADR-0007]]
(sin subir `PROTOCOL_VERSION`), que se fija en la tarea
[[Tareas/Chat de sesión con Codex y Claude Code]] y se anota en ADR-0007 al
integrarla:

- **Peticiones**:
  - `chat_open` y `chat_close {session}`;
  - `chat_send {session, text}` → `{started | queued, id}`;
  - `chat_unqueue` y `chat_interrupt`;
  - `chat_answer {session, request, answer}`;
  - `chat_menu {session}` y `chat_command {session, command, args}`.
- **Eventos**: `chat_item`, `chat_state` y `chat_request`.

El mismo protocolo servirá a titan-ssh.

## Alternativas consideradas

- **Seguir con el terminal y «tomar el control»** (0.5.0): descartada por el
  usuario; no resuelve el escritor doble y no sirve en el móvil.
- **Codex con `codex queue` desde un PTY**: descartada. Es la misma cola,
  pero sin conversación estructurada ni peticiones al usuario.
- **Dirigir el turno en curso** (`turn/start` o `turn/steer`) cuando el rol
  trabaja: descartada por el usuario, porque cambiaría el encargo del
  director sin que lo sepa. Se usa la cola oficial.
- **Claude por channels**: descartada (research preview, flag
  «dangerously» con diálogo, y el mensaje no es un turno del usuario).
- **Claude por mensajes entre sesiones**: descartada (llegan como de otra
  sesión y no cuentan como consentimiento).
- **Claude con `-p`, `stream-json` o el Agent SDK**: descartada por los
  términos (ADR-0001).
- **Claude escribiendo en el PTY de `claude attach`** como si se tecleara:
  descartada para los mensajes, porque el estado del TUI (menús, diálogos)
  es invisible para la app y un Enter podría elegir otra cosa.
  `claude --resume` es la vía documentada. Se mantiene solo para dejar
  escrito un comando `/`, sin pulsar Enter.
- **Claude con el hook `Stop` (`last_assistant_message`) en lugar de la
  transcripción**: descartada como vía principal, porque solo da el texto
  final y solo en las sesiones lanzadas con él. Queda como respaldo si la
  transcripción deja de leerse.
- **Esperar a `codex exec --remote`**: ver ADR-0013.

## Consecuencias

- **Positivas**:
  - un solo camino para hablar con cualquier sesión;
  - adiós al «active writer» con el equipo, junto con ADR-0013;
  - permisos, preguntas, planes y diffs en el chat;
  - el protocolo queda listo para Android;
  - la vista de solo lectura y «tomar el control» de la 0.5.0 se sustituyen
    por el chat.
- **Negativas / compromisos**:
  - el chat de Claude depende de un formato interno de la transcripción,
    que puede romperse en cualquier versión. Se detecta, se avisa y se usa
    el terminal hasta adaptar el parser;
  - los diálogos de Claude solo se responden desde el chat en sesiones con
    los hooks;
  - la cola y varios métodos de Codex están marcados como experimentales y
    pueden cambiar entre versiones (el daemon ya vive con eso);
  - el riesgo residual de los Consumer Terms §3.7 es el de ADR-0002 y hay
    que revisar los términos si cambian.
- **Por verificar** en el spike de la tarea:
  - `claude --resume` con un turno en curso, y que cerrar su PTY no detenga
    la sesión;
  - que la decisión de un hook evite el diálogo del TUI;
  - rechazar un plan con `ExitPlanMode`;
  - el tope de espera de los hooks;
  - las formas de 0.169 de `collaborationMode` y `requestUserInput`;
  - la cola con el hilo libre y ocupado, y dos clientes en el mismo
    app-server.
