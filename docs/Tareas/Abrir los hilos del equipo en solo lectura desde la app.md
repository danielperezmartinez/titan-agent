---
Nombre: Abrir los hilos del equipo en solo lectura desde la app
Estado: En curso
Resumen: 'En la app, el clic en un rol del equipo abre su hilo en solo lectura (el daemon sirve la transcripción) y solo un «tomar el control» explícito y avisado lanza codex resume; mientras el hilo esté abierto así, el monitor marca el rol. Es la otra mitad del hilo ocupado de titan-team (salida 4), para la fase 4.'
Decisiones: 'Acordado al cerrar la fase 3 (2026-11-16): titan-team ya conserva el hilo y sale con 4 si está abierto en otro sitio; esta tarea evita que la app lo abra como escritor sin querer. Sin cola en titan-team. Ronda 0.5.0: el usuario eligió leer la transcripción por el app-server de Codex (thread/read y thread/items/list); petición transcript como ampliación compatible de ADR-0007. Integrado en la PR #8 (v0.5.0); faltan la prueba a mano y decidir los hallazgos de la prueba de «tomar el control» (nota de la ronda).'
Bloqueada: []
Fecha de creación: 2026-11-16T21:40:00+01:00
Última modificación: 2026-11-17T23:30:00+01:00
---

# Abrir los hilos del equipo en solo lectura desde la app

## Objetivo

Hoy, el clic en un rol de Codex en el monitor abre su sesión (`cx:<thread_id>`)
con `codex resume` en un PTY: la app pasa a ser **escritora** del hilo y, si el
director encarga a ese rol un turno mientras tanto, Codex lo rechaza («thread
already has an active writer»). Desde el cierre de la
[[Fase 3 - Herramienta del equipo en Rust y monitor|fase 3]], `titan-team`
conserva el hilo, escribe `turn_finished` con `status = "busy"` y sale con 4,
y la skill `equipo` hace que el director avise al usuario y repita cuando se lo
diga (contrato en
[[Decisiones/ADR-0010 Formato de team.toml y de los eventos del equipo]]).
Falta la otra mitad, en la app:

- **Clic en un rol del equipo → su hilo en solo lectura.** El daemon sirve la
  transcripción del hilo (petición nueva del protocolo, aditiva) y la interfaz
  la muestra sin abrir un escritor. Sirve también para los hilos que el daemon
  ya no lista (más de 20 min sin actualizarse), lo aplazado en la revisión de
  la fase 3.
- **«Tomar el control» explícito y avisado.** Solo esa acción lanza
  `codex resume` (con las salvaguardas de
  [[Decisiones/ADR-0002 Lanzar sesiones desde la app con salvaguardas]]),
  avisando de que el director no podrá encargar turnos a ese rol mientras el
  hilo siga abierto.
- **El monitor marca el rol** mientras su hilo está abierto como escritor en
  la app (y muestra el `busy` de un turno rechazado), para que se vea por qué
  el director espera.

Se decide en una ADR o en una ampliación de
[[Decisiones/ADR-0007 Protocolo local de titan-agentd en NDJSON versionado]]
antes de implementar: cómo lee el daemon la transcripción por una vía oficial,
sin leer credenciales ([[Decisiones/ADR-0001 Torre de control y no orquestador]]).

## Observado en la prueba manual del cierre de la fase 3 (2026-11-16)

El usuario probó la ventana de la versión 0.4.0 con un encargo real
(`fase-3-cierre-revision`, revisor de Codex con su hilo ya terminado):

- En el **grafo**, el cursor cambia a mano sobre el revisor, pero el clic no
  hace nada.
- En **carriles** y **mesa de control**, la tarjeta del revisor sale
  deshabilitada.
- Aparece un **error abajo a la izquierda** de la ventana (texto por
  apuntar en la siguiente prueba). Lo más probable: el hilo de `codex exec`
  no está en la lista de sesiones del daemon (los hilos de `codex exec` no se
  cargan en el app-server compartido), así que no hay sesión `cx:<thread_id>`
  que abrir.
- El director sí abre su sesión (la de Claude Code, unida por
  `director_session`).

Esta tarea debe resolverlo: el clic en cualquier rol del equipo abre su hilo
en solo lectura aunque el daemon no lo liste, y el grafo no muestra el cursor
de clic donde no hay acción.

## Decisión del usuario: cómo lee el daemon la transcripción

2026-11-16, en la sesión de la ronda 0.5.0: **por el app-server compartido de
Codex** (`thread/read` y `thread/items/list`, API oficial JSON-RPC, la misma
conexión que ya usa el daemon para listar, la cuota y detener). Opciones
descartadas: lo que guarda titan-team en `.agents/state/` (solo roles de
titan-team y solo lo que pasó por él), las dos con respaldo (más trabajo y dos
formatos) y el fichero de sesión de `~/.codex` (formato interno, no oficial).
Comprobado con Codex 0.169 sobre un hilo de prueba de esta ronda: leer así no
carga el hilo (`notLoaded` antes y después), así que no abre escritor; un hilo
inexistente da «thread not loaded: <id>» en `thread/read`, y
`thread/items/list` puede responder «not supported yet» (de ahí el respaldo
con `includeTurns`).

## Especificación (contrato, fijado antes de los tests)

Ronda de arreglos 0.5.0, variante equipo v4 (encargo `v05-hilos`). Contrato
cerrado por el director antes de que Sol escriba los tests de aceptación.

### Comportamiento visible

- Clic en un rol de Codex con hilo (en cualquiera de las tres vistas) → se
  abre su hilo en SOLO LECTURA en la zona central: la transcripción servida
  por el daemon, sin terminal y sin escritor. Funciona aunque el daemon no
  liste el hilo. Clic en el director → su sesión de Claude Code, como hoy.
- «Tomar el control» explícito, con confirmación que avisa de que el director
  no podrá encargar turnos a ese rol mientras el hilo siga abierto: solo
  entonces se abre `codex resume <thread>` (la sesión, como hoy), también
  para un hilo que el daemon no lista.
- El monitor marca el rol mientras su hilo está abierto como escritor en la
  app (un terminal vivo del daemon para `cx:<thread>`) y cuando su último
  turno fue rechazado por hilo ocupado (`busy`).

### Contrato público (lo que comprueban los tests de aceptación)

#### 1. Núcleo: `titan_agent_core::transcript` (crate `titan-agent-core`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptKind { User, Agent, Reasoning, Command, FileChange, Tool, Other }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptItem {
    /// Id del turno de Codex al que pertenece.
    pub turn: String,
    pub kind: TranscriptKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Elementos que el daemon pide de una vez (los más recientes).
pub const TRANSCRIPT_ITEMS: usize = 200;
/// Caracteres (no bytes) máximos del `text` de un elemento.
pub const TRANSCRIPT_TEXT_MAX: usize = 4000;

/// Página de `thread/items/list` pedida con `sortDirection: "desc"`:
/// `{"data":[{"turnId":"…","item":{…}}, …], "nextCursor": … }`, la más
/// reciente primero. Devuelve los elementos en orden cronológico (el más
/// antiguo primero) y `true` si hay más antiguos (`nextCursor` no nulo).
pub fn transcript_from_items_page(page: &serde_json::Value) -> (Vec<TranscriptItem>, bool);

/// Resultado de `thread/read` con `includeTurns: true`:
/// `{"thread":{…,"turns":[{"id":"…","items":[{…}, …]}, …]}}`, en orden
/// cronológico. Devuelve los últimos `TRANSCRIPT_ITEMS` elementos en orden
/// cronológico y `true` si se descartó alguno más antiguo.
pub fn transcript_from_thread(read: &serde_json::Value) -> (Vec<TranscriptItem>, bool);
```

Conversión de cada elemento de Codex (`item.type`), igual en las dos
funciones:

| `item.type` | `kind` | `text` | `detail` |
| --- | --- | --- | --- |
| `userMessage` | `user` | los `text` de las entradas de `content` con `"type":"text"`, unidos con `\n` | — |
| `agentMessage` | `agent` | `text` | — |
| `reasoning` | `reasoning` | las cadenas de `summary` unidas con `\n` | — |
| `commandExecution` | `command` | `command` | `"código N"` si `exitCode` es un número; si no, nada |
| `fileChange` | `file_change` | los `path` de `changes` unidos con `, ` | — |
| `mcpToolCall` | `tool` | `server.tool` (`"{server}.{tool}"`) | — |
| `dynamicToolCall` | `tool` | `tool` | — |
| `webSearch` | `tool` | `"búsqueda: {query}"` | — |
| `plan` | `other` | `text` | — |
| cualquier otro tipo | `other` | el propio tipo (p. ej. `contextCompaction`) | — |

- Un elemento sin `item.type` (o sin objeto `item`) se omite. Un elemento
  cuyo `text` queda vacío (p. ej. `reasoning` sin `summary`, `userMessage`
  sin texto) se omite.
- Un `text` de más de `TRANSCRIPT_TEXT_MAX` caracteres se corta a ese número
  de caracteres y se le añade `…` (U+2026).
- `turn` es `turnId` en la página de `thread/items/list` y el `id` del turno
  en `thread/read`.

#### 2. Protocolo (`titan_agent_core::protocol`, versión 1, ampliación aditiva)

Petición nueva `Request::Transcript { session: String }`, en el cable
`{"id":7,"method":"transcript","params":{"session":"cx:<thread>"}}`. Respuesta
`TranscriptResult`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptResult {
    pub session: String,          // la clave pedida
    pub items: Vec<TranscriptItem>, // orden cronológico, como mucho TRANSCRIPT_ITEMS
    pub more: bool,               // hay elementos más antiguos que no se mandan
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,      // `cwd` del hilo según thread/read
}
```

Errores (`{"id":…,"error":{"code":…,"message":…}}`):

- `not_codex`: la clave no empieza por `cx:` (o no tiene id detrás).
- `codex_unavailable`: el app-server de Codex no está en marcha o no responde
  (`codex app-server daemon version` no da `"status":"running"` con
  `socketPath`, o falla la conexión). También en Windows.
- `not_found`: `thread/read` del hilo devuelve error (Codex 0.169 responde
  «thread not loaded: <id>» a un hilo que no existe). El mensaje incluye el de
  Codex.
- `codex_error`: cualquier otro error de Codex al leer los elementos.

El daemon, para `cx:<thread>`: `thread/read` `{"threadId":…}` (sin turnos)
para comprobar que existe y sacar `cwd`; luego `thread/items/list`
`{"threadId":…,"limit":200,"sortDirection":"desc"}`; si esa llamada da error
(Codex 0.169 responde `thread/items/list is not supported yet` a algunos
hilos), `thread/read` `{"threadId":…,"includeTurns":true}`. Nunca llama a
`thread/resume`, `turn/start` ni a nada que escriba. Responde sin bloquear al
resto de clientes.

#### 3. Adjuntar un hilo que el daemon no lista

`Request::Attach { session: "cx:<thread>", … }` (existente) también funciona
cuando la sesión no está en la lista del daemon: el daemon la resuelve con
`thread/read` en el app-server y la abre con el mismo comando que a una sesión
de Codex listada (`codex resume <thread>` en la carpeta `cwd` del hilo, o en la
carpeta personal si ya no existe). Si Codex tampoco la conoce, el error sigue
siendo `unknown_session`.

#### 4. Monitor (`titan_agent_core::monitor` y `titan_agent_core::team`)

- `team::Member` gana `pub busy: bool`: `true` si el último `turn_finished`
  del rol tiene `status = "busy"` y no ha empezado otro turno después
  (`turn_started` lo pone a `false`).
- `monitor::Agent` gana:
  - `pub thread: Option<String>`: el `thread_id` del rol de Codex, AUNQUE el
    daemon no liste su sesión (`None` para el usuario y para roles que aún
    no tienen hilo; para el director de Claude Code, `None`).
  - `pub writer_open: bool`: su hilo está abierto como escritor en la app.
  - `pub busy: bool`: el de su `Member`.
  `Agent.session` no cambia (la sesión listada que se abre al hacer clic en
  el director).
- `monitor::board(task, run, sessions, now_ms)` no cambia de firma y deja
  `writer_open = false`. Método nuevo:
  `impl Board { pub fn mark_writers(&mut self, terminals: &[protocol::TerminalRef]) }`
  pone `writer_open = true` en cada agente con `thread = Some(t)` si hay un
  `TerminalRef` con `session == "cx:" + t`, y `false` en los demás.

#### Dónde van los tests

- Núcleo: `crates/titan-agent-core/tests/` (transcripción, protocolo,
  monitor). Los eventos de `events.jsonl` y el uso de `TeamTask`, `run_of`,
  `board` son los que ya usan los tests existentes de esa carpeta (léelos
  para ver el formato).
- Daemon: `crates/titan-agentd/tests/`. Los tests existentes arrancan el
  binario `titan-agentd` con CLI falsos (scripts de shell) por las variables
  `TITAN_AGENTD_CLAUDE`/`TITAN_AGENTD_CODEX` y un socket propio (léelos). Para
  Codex hace falta un app-server falso: el `codex` falso responde a
  `app-server daemon version` con
  `{"status":"running","socketPath":"<ruta>"}` y el test sirve WebSocket en
  ese socket Unix con `tungstenite` (ya es dependencia de desarrollo de
  `titan-agentd`), contestando JSON-RPC (`initialize`, `thread/loaded/list`,
  `thread/list`, `thread/read`, `thread/items/list`,
  `account/rateLimits/read`; el daemon también manda la notificación
  `initialized` sin `id`).


## Criterios visuales (escritos antes de implementar)

1. **Barra de la vista de solo lectura**, con el lenguaje de la barra de
   sesión: etiqueta `cx` en cian, nombre del rol en negrita `ink`, «rol ·
   modelo · encargo» 12 px `mute`, una pastilla «solo lectura» con borde
   hairline y texto `mute`, y a la derecha el botón `[>] tomar el control`
   (borde hairline, `accent` al pasar); bajo ella una hairline. El cuerpo no
   tiene cursor de terminal ni acepta escritura.
2. **Transcripción** sobre `terminal-bg` en JetBrains Mono 13 px: cada
   elemento con su prefijo en una columna fija (`tú ›`/director en violeta
   `cl` para los mensajes de usuario, el modelo del rol en cian `cx` para el
   agente, `$` y la orden con su `código N` en `success` si es 0 y en
   `danger` si no, `[~]` y las rutas para ficheros, razonamiento en `mute`),
   turnos separados por una hairline con «turno N», lo más reciente abajo y
   visible al abrir; se desplaza con la rueda; «hay elementos anteriores» en
   `mute` arriba si `more`.
3. **Confirmación de «tomar el control»**: una franja con borde `warning`
   sobre `surface` explica que se abrirá `codex resume <id>` y que, mientras
   siga abierto, el director no podrá encargar turnos a ese rol (titan-team
   saldrá con 4), con `[ok] tomar el control` y `[esc] cancelar`. Nada se abre
   sin confirmar.
4. **Monitor**: los roles con acción muestran la mano y responden en las tres
   vistas (ninguna tarjeta deshabilitada si hay hilo); los que no tienen
   acción no muestran la mano. Un rol con el hilo abierto como escritor
   muestra «abierto en la app» en `warning` (y su caja o tarjeta con borde
   `warning`); un turno `busy` muestra «hilo ocupado».
5. **Acabado**: solo colores de los tokens; nada cortado ni solapado a
   1440×900; los errores (Codex no disponible, hilo no encontrado) salen en el
   cuerpo de la vista en `mute`, no como aviso abajo a la izquierda.

## Criterios de finalización

- Clic en un rol del equipo en el monitor: se ve su hilo y no se abre ningún
  escritor (un `titan-team ask` a ese rol a la vez funciona).
- «Tomar el control» pide confirmación, avisa y abre `codex resume`; mientras
  tanto el monitor marca el rol, y un `ask` a ese rol sale con 4.
- Tests del protocolo nuevo y del daemon con CLI falsos.

## Verificación

Rama `feat/v0.5-equipo-v4` (sin push), 2026-11-16:

- **Tests de aceptación de Sol** (encargo `v05-hilos`, sin tocar desde sus
  commits): `crates/titan-agent-core/tests/transcript_acceptance.rs` (4:
  conversión, corte y orden en las dos funciones, protocolo, `busy`),
  `crates/titan-agentd/tests/transcript_acceptance.rs` (2: hilo no listado
  leído sin ninguna llamada que escriba, con y sin respaldo, y los cuatro
  errores) y `crates/titan-agent-core/tests/monitor_transcript_acceptance.rs`
  (1: `thread`, `mark_writers` y `busy`; segundo turno de Sol, porque en el
  primero no tenía las firmas del monitor). Los 7 pasan.
- **Tests propios**: 3 de `transcript.rs` y `crates/titan-agentd/tests/attach_unlisted.rs`
  (adjuntar un hilo no listado abre `codex resume`; Codex que no lo conoce da
  `unknown_session`; una lectura cortada por el app-server da
  `codex_unavailable`).
- fmt, clippy y `cargo test --workspace` en verde (130 tests en la rama).
- **Codex real (0.169), sobre un hilo de prueba de esta ronda**:
  `thread/read` y `thread/items/list` no cargan el hilo (`notLoaded` antes y
  después) y devuelven los elementos del turno; un hilo inexistente da
  «thread not loaded».
- **Ventana real con datos inventados** (daemon en un socket propio, CLI
  falsos y un app-server falso; ninguna sesión real): el hilo de un rol que
  el daemon no lista se lee en solo lectura; la confirmación de «tomar el
  control»; las tres vistas con «abierto en la app» (un terminal del daemon
  para `cx:<hilo>` de un hilo no listado, abierto por la resolución nueva) y
  «hilo ocupado». Capturas en `.previa/v05-equipo-v4/capturas/` (ignorada por
  git). El PID de la ventana activa se comprobó antes y después de cada
  captura.
- **Pendiente del usuario**: probar los clics (no hay inyección de ratón) y
  un `titan-team ask` con el hilo abierto en solo lectura (debe funcionar) y
  tras «tomar el control» (debe salir con 4). Preparado:
  `.previa/v05-equipo-v4/probar-ventana.sh` (CLI reales, socket propio).

## Resultado

- **Publicación**: PR #8 integrada con squash en `main` el 2026-11-17, commit `b1237e8`, versión **0.5.0** sin etiqueta. Checks `Cambios`, `Fugas`, `Verificar` y `Windows` en verde. GitHub rechazó `--author-email` con el correo noreply, así que el squash sale con el correo principal de la cuenta.

- **Núcleo**: `transcript` (tipos y conversión de los elementos de Codex),
  `TranscriptResult` y `Request::Transcript` en el protocolo; `Member.busy`;
  `Agent.thread`, `writer_open`, `busy` y `Board::mark_writers`.
- **Daemon**: `hub/threads.rs` responde `transcript` en su propio hilo y
  resuelve los hilos no listados para `attach`; `codex.rs` separa los errores
  de transporte (`codex_unavailable`) de las respuestas de error de Codex.
- **Interfaz**: `app/thread_view.rs` (vista de solo lectura, refresco cada
  3 s mientras se ve, confirmación de «tomar el control», sesión provisional
  del hilo hasta que el daemon lo lista); el clic de las tres vistas usa un
  destino común (`monitor::target`): hilo de un rol de Codex o sesión del
  director, y sin mano donde no hay acción; marcas `[w] abierto en la app`
  (borde ámbar) y `[!] hilo ocupado`.

### Revisión de Sol (una ronda, contexto limpio)

`v05-revision`, gpt-6.1-sol high, hilo nuevo con la especificación de las
tres tareas, §7 y el diff `41a2821..HEAD`: `changes_requested`, **2 mayores
y 3 menores**. Decisiones:

- **Aceptado (mayor)**: con la conexión perdida, una petición `transcript`
  pendiente dejaba la vista esperando para siempre. Ahora se descarta al
  desconectar (el error sale en el cuerpo) y se pide otra al reconectar. Sin
  test: es estado de GPUI y la interfaz no tiene tests de ventana.
- **Aceptado (mayor)**: un corte del app-server tras `initialize` salía como
  `codex_error`; el contrato pide `codex_unavailable`. Test nuevo.
- **Aceptado (menor)**: con el hilo abierto y un turno rechazado a la vez
  solo se veía «abierto en la app»; ahora «abierto en la app · hilo ocupado».
- **Aceptado (menor)**: carriles no tenía borde de aviso; ahora una barra
  ámbar de 2 px a la izquierda, como la selección del árbol.
- **Aceptado en parte (menor)**: el color de la orden. El `$` toma el color
  del resultado (verde si `código 0`, rojo si no); el texto de la orden
  sigue en `body` para que se lea.

### Evaluación visual (subagente de Opus nuevo en cada ronda)

- Ronda 1: **7,5/10** (barra 8, transcripción 7,5, confirmación 7, monitor
  7,5, acabado 7,5). Corregido: botón de 30 px y marcado mientras la franja
  está abierta; miga `[#] proyecto ›` y el proyecto marcado en el árbol;
  franja con margen inferior, orden en `ink` y «cancelar» legible; párrafos
  y elementos separados; aire bajo «hay elementos anteriores»; leyendas del
  grafo y de carriles. Descartado: «disenador» sin ñ (es el id del rol en
  `team.toml`) y «[busy]» (es el estado del contrato de ADR-0010).
- Ronda 2: **8/10** (barra 8, transcripción 8, confirmación 9, monitor 8,
  acabado 8,5). Corregido (última ronda): ritmo vertical, marcas propias
  `[w]`/`[!]` para no confundirse con `[?] te necesita`, texto del agente
  en `body` y del director en `mute`, «pensando» para el razonamiento y «al
  rol revisor». Aplazado: el estado del rol en la barra de solo lectura.
  Descartado: el alto de las tarjetas de la mesa y la flecha del feed (no
  son de esta rama).

## Sustituida por el chat de sesión (2026-11-17)

La vista de solo lectura y «tomar el control» pasan a ser el chat de
[[Chat de sesión con Codex y Claude Code]]
([[Decisiones/ADR-0012 Chat de sesión sobre las vías oficiales de cada CLI|ADR-0012]]
y [[Decisiones/ADR-0013 Turnos de titan-team por el app-server de Codex|ADR-0013]]):

- el usuario habla con el rol por el app-server compartido;
- titan-team ejecuta sus turnos en el mismo app-server, así que ya no hay
  escritor doble;
- con el rol ocupado, el mensaje va a la cola oficial de Codex.

Los hallazgos de «tomar el control» de la prueba manual (ver
[[Ronda de arreglos 0.5.0]]) los resuelve esa tarea, que tiene un solo
camino de entrada. Esta tarea se cierra cuando aquella se integre. Hasta
entonces sigue pendiente su prueba a mano.
