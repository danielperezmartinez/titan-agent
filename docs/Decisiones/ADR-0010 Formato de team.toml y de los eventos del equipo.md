---
Nombre: Formato de team.toml y de los eventos del equipo
Número: 10
Estado: Aceptada
Resumen: 'titan-team (Rust) sustituye a bin/team con el mismo modelo: .agents/team.toml v1 validado (todos los errores a la vez, con ruta y código: parse_error, bad_version, unknown_key, missing_key, bad_type, unknown_value, unknown_role, missing_file) y órdenes roles/new/ask/show/diff/test/decide/close/check con códigos de salida 0/1/2/3. Cada encargo escribe <state_dir>/<encargo>/events.jsonl: una línea JSON por evento con v=1, ts_ms, task y type (task_created, brief_sent, turn_started, activity, turn_finished, report, decision, task_closed), textos acotados a 200/500 caracteres. El daemon lo lee y lo publica como teams (últimos 300 eventos de los encargos abiertos con actividad en 12 h) junto a quotas (cuota de Codex por account/rateLimits/read) en el evento sessions, sin subir la versión del protocolo.'
Decisión: Formato v1 de team.toml y de events.jsonl de titan-team tal como se fija en esta nota; el daemon lo publica en los campos aditivos teams y quotas del evento sessions.
Consecuencias: El monitor depende solo de events.jsonl, del estado de las sesiones y de quotas. Añadir tipos o campos opcionales no cambia v; quitar o cambiar el significado de algo sí. La cuota de Claude Code no aparece (no hay vía oficial apta para el daemon).
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-16T18:40:00+01:00
Última modificación: 2026-11-16T21:00:00+01:00
---

# ADR-0010 · Formato de team.toml y de los eventos del equipo

## Contexto

[[ADR-0005 Herramienta del equipo en Rust dentro de titan-agent]] decide
reescribir `bin/team` en Rust con un formato de eventos versionado que lea el
daemon para el [[Monitor del proyecto en tres vistas|monitor]]. La fase 3
([[Tareas/Fase 3 - Herramienta del equipo en Rust y monitor]]) pide además
que nada del equipo vaya fijo en el código: todo vive en `.agents/team.toml`,
con su formato documentado y validado para que un agente pueda montar el
equipo de otro proyecto solo con ese fichero. En la variante «equipo v4» del
experimento, el director fija este contrato **antes** de que Sol escriba los
tests de aceptación, que lo comprueban sin inventar detalles.

## Decisión

Este es el contrato (nombres, formatos, límites y códigos de error). Lo que no
aparece aquí es diseño interno.

### Parte 1. `titan-team` (crate `titan-team`, binario `titan-team`)

#### 1.1 Localización de la configuración

- `--config <ruta>` (opción global, antes de la orden): la ruta de un `team.toml` o de una carpeta que contiene `.agents/team.toml`.
- Sin `--config`: se busca `.agents/team.toml` en la carpeta actual y en sus antecesoras.
- La **carpeta base** es la que contiene `.agents/` (el padre del padre de `team.toml`). Todas las rutas relativas del fichero se resuelven desde ella.
- Sin configuración: error de uso (exit 2).

#### 1.2 Formato de `team.toml` (versión 1)

Claves de primer nivel:

| Clave | Tipo | Obligatoria | Por defecto |
| --- | --- | --- | --- |
| `version` | entero, debe ser `1` | sí | — |
| `name` | texto no vacío | sí | — |
| `state_dir` | texto (ruta) | no | `".agents/state"` |
| `[project]` | tabla | no | ver abajo |
| `[codex]` | tabla | no | — |
| `[flow]` | tabla | no | ver abajo |
| `[roles.<nombre>]` | tablas | al menos un rol | — |

`[project]`: `repo` (texto, `"."`), `base` (texto, `"main"`), `worktrees_dir` (texto, `".agents/worktrees"`), `branch_prefix` (texto, `"encargo/"`), `test_cmd` (texto, sin valor), `test_timeout_s` (entero > 0, `300`), `autocommit` (booleano, `false`), `hidden_paths` (lista de textos, `[]`).

`[codex]`: `config` (lista de textos `clave=valor`, `[]`): cada una se pasa como `-c <texto>` a todas las llamadas.

`[flow]`: `steps` (lista de nombres de rol, `[]`), `review_role` (nombre de rol, sin valor), `fix_role` (nombre de rol, sin valor), `max_review_rounds` (entero ≥ 0, `0`).

`[roles.<nombre>]` (nombre de rol: `[a-z0-9_]+`):

| Clave | Tipo / valores | Obligatoria | Por defecto |
| --- | --- | --- | --- |
| `cli` | `"codex"` o `"claude"` | sí | — |
| `model` | texto no vacío | sí | — |
| `effort` | `"minimal"`, `"low"`, `"medium"`, `"high"`, `"xhigh"` | no | sin valor |
| `sandbox` | `"read-only"` o `"workspace-write"` | no | `"read-only"` |
| `launch` | booleano | no | `true` |
| `prompt_file` | texto (ruta) | sí si `cli = "codex"` y `launch = true` | — |
| `output_schema` | texto (ruta a un esquema JSON) | no | — |
| `allowed_paths` | lista de patrones | no | `[]` |
| `forbidden_paths` | lista de patrones | no | `[]` |
| `config` | lista de textos `clave=valor` | no | `[]` |
| `timeout_s` | entero > 0 | no | `900` |
| `purpose` | texto | no | `""` |

Reglas añadidas: un rol `cli = "claude"` debe tener `launch = false` (la herramienta no lanza Claude Code). Los patrones de rutas usan comodines al estilo `fnmatch` sobre rutas relativas al repo; un patrón que termina en `/` cubre todo lo que hay debajo.

#### 1.3 Validación y `titan-team check`

Toda orden carga y valida el fichero antes de hacer nada. La validación informa de **todos** los errores a la vez. Cada error tiene `path` (ruta con puntos, p. ej. `roles.revisor.sandbox`; `""` para el fichero entero), `code` y `message` (en español). Códigos:

| `code` | Cuándo |
| --- | --- |
| `parse_error` | el TOML no se puede leer (sintaxis) |
| `bad_version` | `version` no es `1` |
| `unknown_key` | una clave que el formato no define (en cualquier tabla) |
| `missing_key` | falta una clave obligatoria |
| `bad_type` | el valor tiene otro tipo (p. ej. texto donde va un entero) o está fuera de rango (`timeout_s = 0`) |
| `unknown_value` | un valor fuera de su lista (`cli`, `effort`, `sandbox`, incluido `"danger-full-access"`) o un rol `claude` con `launch = true` |
| `unknown_role` | `flow.steps`, `flow.review_role` o `flow.fix_role` nombran un rol que no existe |
| `missing_file` | `prompt_file` u `output_schema` no existen |

`titan-team check [--json]`:

- Válido: exit 0. Sin `--json` imprime una línea que empieza por `team.toml válido`.
- Inválido: exit 2. Sin `--json`, una línea por error `<path>: <message> (<code>)` en stderr. Con `--json`, en stdout: `{"ok": false, "errors": [{"path": …, "code": …, "message": …}, …]}` (y `{"ok": true, "errors": []}` si es válido).
- Cualquier otra orden con un fichero inválido termina con exit 2 e imprime los errores en stderr.

#### 1.4 Órdenes

Códigos de salida comunes: **0** bien; **1** el turno del agente falló o agotó su tiempo (`ask`), o los tests fallaron (`test` devuelve el código del test); **2** error de uso, configuración o estado (encargo inexistente o repetido, rol inexistente o no lanzable, fichero que falta…); **3** se superó el tope de rondas de revisión sin `--force`. Los mensajes de error van en español a stderr.

- `titan-team roles [--json]`: lista los roles (tabla legible, o `{"roles": {...}, "flow": {...}}` con `--json`).
- `titan-team new <encargo> --brief <fichero> [--base <rev>]`: `<encargo>` cumple `[a-z0-9-]+` y tiene de 1 a 64 caracteres. Crea la rama `<branch_prefix><encargo>` desde `<base>` (por defecto `project.base`) en un worktree `<worktrees_dir>/<encargo>`; si `project.hidden_paths` no está vacío, deja esas rutas fuera del worktree con un sparse checkout (siguen en la rama). Copia el brief a `<state_dir>/<encargo>/encargo.md`, guarda el estado en `<state_dir>/<encargo>.json` y escribe el evento `task_created`. Un encargo que ya existe: exit 2.
- `titan-team ask <rol> <encargo> [-m <texto> | -f <fichero>] [--diff] [--test] [--new] [--force] [--dry-run] [--json]`: un turno de un rol de Codex (ver 1.5). Un rol con `launch = false` o `cli = "claude"`: exit 2. Continuar un hilo existente exige `-m` o `-f` (si no, exit 2). `--new` empieza un hilo nuevo aunque el rol ya tenga uno. `--diff` añade al prompt el diff del worktree respecto al commit base (truncado a 60 000 caracteres, con aviso). `--test` añade la salida de `project.test_cmd`. `--dry-run` imprime el comando y el prompt y no ejecuta nada ni escribe eventos. `--json` imprime un objeto JSON con la entrada del historial más `text` (respuesta del agente) y `parsed` (el informe si el rol tiene `output_schema`, o `null`).
- `titan-team show <encargo> [--json]`: estado del encargo (con `--json`, el JSON del estado).
- `titan-team diff <encargo> [--stat]`: `git diff <commit base>..HEAD` del worktree.
- `titan-team test <encargo>`: ejecuta `project.test_cmd` en el worktree; imprime `exit: <código>` y la salida; devuelve ese código.
- `titan-team decide <encargo> -m <texto>`: registra una decisión del director (evento `decision`). Texto vacío: exit 2.
- `titan-team close <encargo> [--delete-branch]`: quita el worktree (y la rama con `--delete-branch`), marca el estado como cerrado y escribe `task_closed`.

#### 1.5 Un turno (`ask`)

1. Prompt de un hilo nuevo: el contenido de `prompt_file`, `\n\n# Encargo\n\n`, el brief del encargo y, si hay mensaje, `\n\n# Mensaje del director\n\n<mensaje>`. Prompt de un hilo que continúa: `# Mensaje del director\n\n<mensaje>`.
2. Ronda de revisión: si el rol es `flow.review_role`, el turno es la ronda `rondas hechas + 1`; si eso supera `max_review_rounds` y no hay `--force`, exit 3 sin ejecutar nada ni escribir eventos.
3. El ejecutable de Codex es el de la variable `TITAN_TEAM_CODEX` o, si no está, `codex` del PATH. Se ejecuta **sin shell**, con el prompt por stdin, en el worktree, con este argv:
   - Hilo nuevo: `exec --json -o <fichero de salida> -m <model> [-c model_reasoning_effort=<effort>] [-c <cada [codex].config>] [-c <cada config del rol>] -s <sandbox> -C <worktree> [--output-schema <ruta absoluta>] -`
   - Hilo que continúa: `exec resume --json -o <fichero de salida> -m <model> [-c model_reasoning_effort=<effort>] [-c …config…] -c sandbox_mode="<sandbox>" [--output-schema <ruta absoluta>] <thread_id> -`
4. Lo que se lee de Codex: su stdout es JSONL. `{"type":"thread.started","thread_id":…}` (o cualquier línea con `thread_id`) da el hilo; `{"type":"turn.completed","usage":{"input_tokens","cached_input_tokens","output_tokens"}}` suma el uso; `{"type":"turn.failed","error":{"message"}}` y `{"type":"error","message"}` marcan error; `{"type":"item.completed","item":{"type":"agent_message","text"}}` es la última respuesta si no existe el fichero `-o`. El fichero `-o`, si existe, es la respuesta final. `{"type":"item.started","item":{…}}` produce eventos `activity` (ver Parte 2). Las líneas que no son JSON se ignoran.
5. Estado del turno: `ok`; `error` si el proceso sale con código ≠ 0 o hubo un error en su JSONL; `timeout` si supera `timeout_s` (se mata el proceso).
6. Tras el turno, si el rol no es `read-only` y `project.autocommit = true`: `git add -A` y, si hay cambios, un commit en el worktree con el mensaje `team: <rol> turno <n>` (si el repo no tiene identidad de git, se usa `team` / `team@localhost`); se anotan el hash corto y los ficheros cambiados. Avisos: un fichero cambiado fuera de `allowed_paths` (si la lista no está vacía) da `Fichero fuera de allowed_paths: <ruta>`; uno dentro de `forbidden_paths`, `Fichero dentro de forbidden_paths: <ruta>`; un rol `read-only` que deja cambios sin confirmar, `El rol read-only dejó cambios en el árbol`.
7. Salidas guardadas en `<state_dir>/<encargo>/`: `NN-<rol>.jsonl` (stdout crudo), `NN-<rol>.stderr`, y `NN-<rol>.md` o `NN-<rol>.json` (respuesta; `.json` si el rol tiene `output_schema`), con `NN` el número de entrada del historial con dos cifras.
8. Exit 0 si el estado es `ok`, 1 si no.

---

### Parte 2. Formato de eventos v1

Fichero: `<carpeta base>/<state_dir>/<encargo>/events.jsonl`. Solo lo escribe `titan-team`, añadiendo al final; **cada evento es una línea JSON** escrita de una vez. Nunca se reescribe ni se trunca.

Campos comunes de cada línea: `v` (entero, `1`), `ts_ms` (entero, milisegundos Unix), `task` (nombre del encargo) y `type`. Los demás campos dependen del tipo:

| `type` | Campos | Cuándo |
| --- | --- | --- |
| `task_created` | `base`, `base_commit`, `branch`, `worktree` (relativo a la carpeta base), `team`: `{name, director, roles: [{name, cli, model, effort, sandbox}], steps, review_role, max_review_rounds}` | `new`. `director` es el nombre del primer rol con `cli = "claude"` (o `null`); `roles` en el orden del fichero; `effort`, `sandbox` y `review_role` pueden ser `null` |
| `brief_sent` | `from` (el director o `null`), `to` (rol), `turn`, `resumed` (bool), `text` | al empezar `ask`, antes del turno |
| `turn_started` | `role`, `turn`, `cli`, `model`, `effort`, `sandbox`, `thread_id` (o `null` si es nuevo), `review_round` (entero si el rol es `review_role`; si no, `null`) | justo antes de lanzar Codex |
| `activity` | `role`, `turn`, `text` | por cada `item.started` de Codex de tipo `command_execution` (`text` = `$ <command>`), `file_change` (`text` = `edita <rutas separadas por coma>`), `mcp_tool_call` o `web_search` (`text` = el tipo); como mucho 200 por turno |
| `turn_finished` | `role`, `turn`, `status` (`ok`/`error`/`timeout`), `duration_ms`, `thread_id`, `usage` `{input_tokens, cached_input_tokens, output_tokens}`, `commit` (o `null`), `changed_files`, `warnings`, `text` | al terminar el turno |
| `report` | `role`, `review_round`, `verdict`, `findings` `{blocker, major, minor, nit}` (recuentos), `questions` (recuento) | tras `turn_finished` de un rol con `output_schema` cuya respuesta es JSON con `verdict` |
| `decision` | `by` (el director o `null`), `text` | `decide` |
| `task_closed` | `delete_branch` (bool) | `close` |

Textos:

- `brief_sent.text`: la primera línea no vacía del mensaje del director; sin mensaje, `encargo inicial`.
- `turn_finished.text`: con informe, `<verdict> · <n> hallazgos` (n = total de findings; `1 hallazgo` en singular, ajuste del 2026-11-16 tras la evaluación visual); si no, la primera línea no vacía de la respuesta; si no hay respuesta, el estado.
- Límites: `text` de `brief_sent`, `activity` y `turn_finished` como mucho **200 caracteres** (no bytes); `decision.text` **500**. Lo que pase se corta y termina en `…` (contando dentro del límite).

Orden de un turno: `brief_sent`, `turn_started`, `activity`*, `turn_finished`, `report`?

Compatibilidad: los lectores ignoran tipos y campos desconocidos y saltan líneas mal formadas o con `v` mayor que el que conocen. Añadir tipos o campos opcionales no cambia `v`; quitar o cambiar el significado de algo sí.

---

### Parte 3. Modelo del equipo en el núcleo (`titan_agent_core::team`)

Funciones puras (sin E/S), públicas:

```rust
pub const EVENTS_VERSION: u32 = 1;

/// Parses one event. None for malformed JSON, a `v` greater than
/// EVENTS_VERSION, a missing common field or an unknown `type`.
pub fn parse_event(value: &serde_json::Value) -> Option<TeamEvent>;
pub fn parse_event_line(line: &str) -> Option<TeamEvent>;

pub struct TeamEvent { pub ts_ms: i64, pub task: String, /* + el tipo con sus campos: diseño libre */ }

impl TeamRun {
    /// None if there is no task_created event.
    pub fn from_events(events: &[TeamEvent]) -> Option<TeamRun>;
}

pub struct TeamRun {
    pub task: String,
    pub director: Option<String>,   // team.director of task_created
    pub review_round: u32,          // last review_round of a turn_started (0 if none)
    pub max_review_rounds: u32,
    pub closed: bool,               // a task_closed was seen
    pub members: Vec<Member>,       // one per team.roles entry, same order
    pub messages: Vec<TeamMessage>, // chronological
    pub last_ts_ms: i64,            // ts_ms of the last event
}

pub struct Member {
    pub role: String,
    pub cli: AgentCli,               // titan_agent_core::session::AgentCli
    pub model: String,
    pub effort: Option<String>,
    pub working: bool,               // a turn_started without its turn_finished
    pub turns: u32,                  // finished turns
    pub thread_id: Option<String>,   // last one seen in turn_started/turn_finished
    pub tokens: u64,                 // sum of input_tokens + output_tokens of its finished turns
    pub activity: Option<String>,    // text of the last activity of the current turn; None if not working
    pub since_ms: Option<i64>,       // ts_ms of its last turn_started or turn_finished
}

pub struct TeamMessage { pub at_ms: i64, pub from: String, pub to: String, pub text: String }
```

Mensajes: cada `brief_sent` da un mensaje `from → to` con su `text` (si `from` es `null`, `from = "tú"`); cada `turn_finished` da uno `role → director` con su `text` (si no hay director, `role → "tú"`).

Cuota (`titan_agent_core::quota`):

```rust
pub struct Quota {
    pub agent: AgentCli,
    pub limit: String,          // limitId, e.g. "codex"
    pub window_mins: u32,       // 300 = 5 h, 10080 = one week
    pub used_percent: f64,      // 0–100, used (not remaining)
    pub resets_at_ms: Option<i64>,
}
/// Reads the result of `account/rateLimits/read` of `codex app-server`.
pub fn parse_codex_rate_limits(result: &serde_json::Value) -> Vec<Quota>;
```

Entrada (ejemplo con datos inventados; los campos que no se usan pueden faltar o haber más):

```json
{"rateLimits": {"limitId": "codex", "primary": {"usedPercent": 3, "windowDurationMins": 300, "resetsAt": 1794866535},
                "secondary": {"usedPercent": 19, "windowDurationMins": 10080, "resetsAt": 1795194824}},
 "rateLimitsByLimitId": {"codex": {"limitId": "codex", "primary": {…}, "secondary": {…}}}}
```

Reglas: se leen las entradas de `rateLimitsByLimitId` (o `rateLimits` si no existe ese mapa); por cada una, `primary` y `secondary` si están, en ese orden; una ventana sin `usedPercent` o sin `windowDurationMins` se salta; `resetsAt` viene en **segundos** y se guarda en milisegundos; la clave del mapa da `limit` si falta `limitId`. Una entrada que no es un objeto da `[]`, nunca un pánico. Ningún otro dato (cuenta, créditos) pasa a `Quota`.

---

### Parte 4. Daemon `titan-agentd`

#### 4.1 Protocolo (añadidos compatibles; `PROTOCOL_VERSION` sigue en 1)

El evento `sessions` (`{"event":"sessions","params":{sessions, sources, terminals, launches, …}}`) gana dos campos, siempre presentes al enviarlo y vacíos por defecto al leerlo:

- `teams`: lista de `TeamTask` `{project_root, folder, task, events}`:
  - `folder`: la carpeta base del equipo (la que contiene `.agents/team.toml`).
  - `project_root`: el proyecto al que pertenece `folder` (misma regla que las sesiones: un worktree `…/.claude/worktrees/<x>` o `…/.agents/worktrees/<x>` pertenece a la carpeta que lo contiene).
  - `task`: nombre del encargo (la carpeta de `<state_dir>`).
  - `events`: los objetos JSON de `events.jsonl` tal cual, en el orden del fichero, **los últimos 300** como mucho; sin las líneas que no son JSON.
- `quotas`: lista de `Quota` (Parte 3) serializada con los nombres de los campos en `snake_case` (`agent`, `limit`, `window_mins`, `used_percent`, `resets_at_ms`).

#### 4.2 Qué encargos lee

- Carpetas candidatas: el `cwd` y el `project_root` de cada sesión conocida. Si una tiene `.agents/team.toml`, se lee su `state_dir` (por defecto `.agents/state`; si el fichero no se puede leer, el por defecto) y cada `<state_dir>/<encargo>/events.jsonl`.
- Un encargo aparece en `teams` si **no está cerrado** (ningún `task_closed`) y su último evento tiene menos de **12 horas** (`ts_ms`).
- Cambios en los ficheros llegan a los clientes en el siguiente sondeo (intervalo `TITAN_AGENTD_POLL_MS`, 2000 ms por defecto), como cualquier cambio del snapshot.
- El daemon solo lee: nunca escribe ni borra nada en `.agents/`.

#### 4.3 Cuota

- Codex: el daemon pide `account/rateLimits/read` con `{"excludeResetCreditDetails": true}` al `codex app-server` compartido al conectar y luego como mucho una vez por minuto, y publica el resultado de `parse_codex_rate_limits` en `quotas`. Si falla, la cuota de Codex desaparece de `quotas` (no se inventa).
- Claude Code no expone su cuota por una vía oficial apta para el daemon: no aparece en `quotas`.

---

### Parte 5. Monitor (interfaz; sin tests de aceptación, solo como contexto)

Al pulsar un proyecto en el árbol, el centro muestra su monitor: barra con `[#] proyecto`, «encargo X · ronda r de max · N agentes», `[*] en vivo` y tres botones `grafo`, `carriles`, `mesa de control`. El encargo mostrado es el más reciente de `teams` de ese proyecto. Si no hay ninguno, el centro dice «Sin equipo trabajando en <proyecto>» y lista las sesiones sueltas. Los datos salen de `TeamRun::from_events`, del estado de la sesión del director (una sesión de Claude Code cuyo `cwd` es `folder`; si «te necesita», el monitor lo muestra como mensaje al usuario) y de `quotas`. Clic en un agente: abre su sesión (`cx:<thread_id>` para los roles de Codex).

### Ampliaciones compatibles tras la revisión (2026-11-16, antes de integrar)

Salen de la revisión de Sol de la fase 3 y no cambian `v` ni la versión del
protocolo:

- **Opciones por orden**: cada orden rechaza con exit 2, antes de hacer nada,
  las opciones y argumentos que no admite (por ejemplo `close --dry-run`).
- **Bloqueo por encargo**: `new`, `ask` y `close` toman
  `<state_dir>/<encargo>.lock`; si otra orden lo tiene, exit 2 al momento
  («ocupado»).
- **Nada se ejecuta antes de tiempo**: el tope de rondas se comprueba antes de
  cualquier otra cosa, y `--dry-run` no ejecuta `test_cmd`.
- **Persistencia**: si no se pueden escribir `brief_sent` o `turn_started`,
  Codex no se lanza (exit 2). Una vez lanzado, el turno se registra entero
  aunque falle un paso posterior (guardar salidas, el commit, un evento), y
  esos fallos dan exit 2 con su explicación; un commit fallido queda además
  como aviso del turno.
- **`test` imprime la salida completa**; el prompt de `ask --test` lleva sus
  últimas 150 líneas.
- **`config` de `[codex]` y de cada rol**: cada entrada debe ser
  `clave=valor` (si no, `bad_type`).
- **`TeamTask` gana dos campos opcionales**: `created` (el evento
  `task_created`, aunque ya no esté entre los últimos 300) y `totals` (turnos
  terminados y tokens de cada rol en todo el fichero). Sin ellos, un encargo
  con más de 300 eventos perdía su equipo en el monitor.
- **El daemon solo interpreta eventos válidos**: reenvía cualquier objeto JSON,
  pero solo un evento válido de su versión cierra el encargo, suma a
  `totals` o fija su hora; la caducidad de 12 h usa el último evento del
  fichero, no el máximo.

## Alternativas consideradas

- **Derivar el monitor del estado `<encargo>.json` del prototipo** — descartado:
  se reescribe en cada turno, no tiene tiempos por evento ni lo que pasa durante
  un turno, y su forma no estaba documentada.
- **Que el daemon siga a `codex exec` por el app-server** — descartado: los
  hilos de `codex exec` no se cargan en el app-server compartido, así que su
  estado en vivo no se ve; el registro de la herramienta sí lo tiene.
- **Mandar a la interfaz un resumen ya calculado en el daemon** — descartado
  por ahora: el daemon reenvía los eventos tal cual y el núcleo los resume
  (`TeamRun`), así el resumen se prueba sin E/S y el daemon no cambia cuando
  cambia el monitor.

## Consecuencias

- Positivas: un registro con dueño y versión; el monitor muestra quién habla
  con quién con tiempos reales y la acción en curso de los roles de Codex; un
  `team.toml` erróneo se explica con su ruta y su código.
- Negativas / compromisos: un fichero por encargo que crece sin límite mientras
  el encargo vive (el daemon solo reenvía los últimos 300 eventos); el director
  de Claude Code no tiene eventos propios más allá de los de la herramienta y
  `decide`; la cuota de Claude Code queda fuera mientras no haya vía oficial.
