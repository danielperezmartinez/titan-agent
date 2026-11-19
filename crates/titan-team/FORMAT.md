# Formato de `.agents/team.toml` y de los eventos de titan-team

Referencia que imprime `titan-team doc`, para que un agente director monte o
cambie el equipo de un proyecto **solo editando `team.toml`**, sin tocar la
herramienta ni necesitar otra documentación. `titan-team check` lo valida y
explica cada error con su ruta y su código. (En el repositorio de titan-agent,
la fuente de verdad es la ADR-0010 de su bóveda; este texto la resume.)

## Dónde vive

- `<carpeta>/.agents/team.toml`. Se busca desde la carpeta actual hacia
  arriba, o se da con `--config <ruta>` (el fichero o una carpeta que contiene
  `.agents/team.toml`). Las rutas relativas parten de `<carpeta>` (la
  **carpeta base**, la que contiene `.agents/`).
- `titan-team init` crea en la carpeta actual un `team.toml` de partida, dos
  prompts (`.agents/prompts/tests.md` y `revisor.md`) y el esquema del informe
  del revisor (`.agents/schemas/review.json`). Nunca sobrescribe los prompts ni
  el esquema; `team.toml` solo con `--force`.
- `.agents/state/` y `.agents/worktrees/` no se versionan (añádelos a
  `.gitignore`): pueden contener salidas con datos del entorno.

## `team.toml` (versión 1)

### Raíz

| Clave | Tipo | Obligatoria | Por defecto |
| --- | --- | --- | --- |
| `version` | entero, debe ser `1` | sí | — |
| `name` | texto no vacío | sí | — |
| `state_dir` | ruta | no | `".agents/state"` |

### `[project]`

| Clave | Tipo | Por defecto | Qué es |
| --- | --- | --- | --- |
| `repo` | ruta | `"."` | Repositorio git del que salen los worktrees. |
| `base` | texto | `"main"` | Rama de la que sale cada encargo (`new --base` la cambia). |
| `worktrees_dir` | ruta | `".agents/worktrees"` | Un worktree por encargo. |
| `branch_prefix` | texto | `"encargo/"` | Prefijo de la rama de cada encargo. |
| `test_cmd` | texto | — | Orden de tests (`titan-team test`, `ask --test`, comprobación `test`). |
| `fmt_cmd` | texto | — | Formatea el worktree en el sitio (comprobación `fmt`), p. ej. `cargo fmt --all`. |
| `gate_cmd` | texto | — | Compuerta completa: formato, lints y tests (comprobación `gate`, `ask --gate`). |
| `build_cmd` | texto | — | Compila sin ejecutar, tests incluidos (comprobación `build`), p. ej. `cargo test --workspace --no-run`. |
| `test_timeout_s` | entero > 0 | `300` | Tope de los tests, en segundos. |
| `autocommit` | booleano | `false` | Commit tras cada turno de un rol que escribe. |
| `hidden_paths` | lista de patrones | `[]` | Lo que no se copia al worktree (sparse checkout); sigue en la rama. |
| `target_dir` | ruta (no vacía) | — | Carpeta de compilación del equipo, relativa a la carpeta que contiene `.agents/`. `test_cmd`, las comprobaciones y `ask --gate` la reciben como `CARGO_TARGET_DIR` en todos los encargos, y un rol `workspace-write` también (con la carpeta en `sandbox_workspace_write.writable_roots`). Sin ella, cada worktree compila en el suyo. |

### `[codex]`

| Clave | Tipo | Por defecto | Qué es |
| --- | --- | --- | --- |
| `config` | lista de `clave=valor` | `[]` | Cada una va en el `config` de `thread/start` y `thread/resume` de todos los turnos (la clave tal cual, el valor leído como TOML; p. ej. `project_doc_max_bytes=0` para que los roles no lean `AGENTS.md`). |

### `[flow]`

| Clave | Tipo | Por defecto | Qué es |
| --- | --- | --- | --- |
| `steps` | lista de roles | `[]` | Orden de paso del trabajo. |
| `review_role` | rol | — | Su turno abre una ronda de revisión. |
| `fix_role` | rol | — | Quién corrige tras la revisión (informativo). |
| `max_review_rounds` | entero ≥ 0 | `0` | Tope de rondas: superarlo exige `ask --force` (salida 3). |

### `[roles.<nombre>]` (nombre: `[a-z0-9_]+`)

| Clave | Tipo / valores | Obligatoria | Por defecto |
| --- | --- | --- | --- |
| `cli` | `"codex"` · `"claude"` | sí | — |
| `model` | texto no vacío | sí | — |
| `effort` | `"minimal"` · `"low"` · `"medium"` · `"high"` · `"xhigh"` | no | — |
| `sandbox` | `"read-only"` · `"workspace-write"` | no | `"read-only"` |
| `launch` | booleano | no | `true` |
| `prompt_file` | ruta | si `cli = "codex"` y `launch = true` | — |
| `output_schema` | ruta a un esquema JSON | no | — |
| `allowed_paths` | lista de patrones | no | `[]` (aviso si el rol escribe fuera) |
| `forbidden_paths` | lista de patrones | no | `[]` (aviso si escribe dentro) |
| `checks` | lista de `"fmt"` · `"test"` · `"gate"` · `"build"` | no | `[]` (solo con `sandbox = "workspace-write"`) |
| `checks_required` | booleano o lista de comprobaciones de `checks` | no | `false` (cuáles hacen fallar el turno si fallan) |
| `brief_sections` | lista de textos no vacíos | no | `[]` (títulos que el encargo debe tener antes del primer turno del rol) |
| `config` | lista de `clave=valor` | no | `[]` (tras las de `[codex]`) |
| `timeout_s` | entero > 0 | no | `900` |
| `purpose` | texto | no | `""` |

- Un rol `cli = "claude"` es el **director** (la sesión de Claude Code que
  usa la herramienta) y lleva `launch = false`: titan-team solo lanza Codex, y
  nunca con `danger-full-access`.
- `allowed_paths` y `forbidden_paths`: comodines al estilo `fnmatch` sobre
  rutas relativas al repo (`*` cubre también `/`, `?`, `[abc]`, `[!abc]`);
  uno que termina en `/` cubre todo lo de debajo.
- `checks`: tras un turno `ok` de ese rol, titan-team ejecuta en el worktree,
  en ese orden y todas aunque falle una, las órdenes `[project] <nombre>_cmd`
  (tope `test_timeout_s`), **antes del commit automático**: lo que cambien
  (el formato) entra en el commit del turno. Su salida queda en
  `<encargo>/NN-<rol>.checks.txt` y en `checks` del historial; cada una que
  falla añade el aviso `comprobación <nombre> falló (exit N)`. Con
  `checks_required = true`, si alguna falla el turno queda `status = "error"`
  y `ask` sale con 1 (el commit se hace igual); con una lista (p. ej.
  `["build"]`), solo las de la lista lo hacen fallar y las demás solo avisan.
  Un turno que no acaba `ok` no ejecuta ninguna.
- `build`: para el rol que escribe los tests antes de la implementación. Sus
  tests fallan al ejecutarse (`test`), pero deben compilar contra el
  esqueleto que deja el director: `checks_required = ["build"]` deja en rojo
  el turno que no compila (también por un lint denegado, que rustc no
  comprueba mientras haya errores de resolución).
- `brief_sections`: antes de abrir el hilo del rol (su primer turno o
  `--new`), `ask` busca cada sección en los títulos Markdown del encargo
  (`#` a `######` y un espacio, con hasta 3 espacios delante; fuera de los
  bloques de código cercados): el título debe contenerla, sin distinguir
  mayúsculas. Si falta alguna, sale con 2 nombrándolas, sin lanzar Codex ni
  escribir eventos ni historial. Al continuar un hilo no se comprueba.
- `hidden_paths`: patrones de `git sparse-checkout` (`/AGENTS.md` es el de la
  raíz; `/docs/`, la carpeta entera).

### Errores de validación

Se informa de **todos** a la vez, cada uno con `path` (p. ej.
`roles.revisor.sandbox`), `code` y `message`:

| `code` | Cuándo |
| --- | --- |
| `parse_error` | el TOML no se puede leer |
| `bad_version` | `version` no es `1` |
| `unknown_key` | una clave que el formato no define |
| `missing_key` | falta una clave obligatoria, o la orden `[project] <nombre>_cmd` de una comprobación |
| `bad_type` | otro tipo, fuera de rango (`timeout_s = 0`) o un `config` sin `clave=valor` |
| `unknown_value` | un valor fuera de su lista, un rol `claude` con `launch = true` o `checks` en un rol `read-only` |
| `unknown_role` | `flow` nombra un rol que no existe |
| `missing_file` | `prompt_file` u `output_schema` no existen |

`check --json` imprime `{"ok": bool, "errors": [{"path", "code", "message"}]}`.

## Órdenes

`titan-team --help` las lista y `titan-team --version` (o `-V`) da la
versión. `new` crea el encargo (rama, worktree y estado);
`ask <rol> <encargo>` es un turno de un rol de Codex, que continúa su hilo si
ya tiene uno (`-m`/`-f` obligatorio entonces; `--new` empieza otro), ejecuta
las `checks` del rol, hace commit de lo que escribió y avisa de rutas.
`ask --gate` ejecuta antes `gate_cmd` (ya dentro del turno, con un `activity`
`$ <orden>`) y añade al mensaje `# Compuerta (exit N)`
con sus últimas 150 líneas (también en `NN-<rol>.gate.txt`): sirve para un rol
`read-only` que no puede ejecutarla. `ask --allow <patrón>` (repetible)
autoriza esas rutas en ese turno: no avisan por `allowed_paths` ni
`forbidden_paths`, y el historial guarda los patrones en `allowed`; `show`, `diff`, `test`; `decide -m`
anota una decisión del director; `close` quita el worktree. El mensaje del
director va con `-m TEXTO` o desde un fichero con `-f`/`--file`/`--message-file`
(nunca los dos); un mensaje vacío o solo con espacios es un error de uso
(salida 2) y no lanza el turno. `new`, `ask` y
`close` toman un bloqueo por encargo (otra orden a la vez: salida 2,
«ocupado»).

### Códigos de salida

| Código | Significado |
| --- | --- |
| 0 | bien |
| 1 | el turno falló o agotó su tiempo, o los tests fallaron (`test` devuelve el código de la orden de tests) |
| 2 | uso, configuración o estado (encargo inexistente, rol no lanzable, fichero que falta, encargo ocupado…) |
| 3 | se superó el tope de rondas de revisión sin `--force` |
| 4 | **hilo ocupado**: el hilo del rol tiene un turno en curso (por ejemplo, del usuario desde el chat de titan-agent) o lo escribe otro proceso de Codex («already has an active writer»). No se ejecutó nada: el rol **conserva su hilo** y su número de turnos, la ronda de revisión no se gasta y titan-team nunca abre un hilo nuevo por su cuenta. Hay que esperar y repetir la misma orden. |

### Cómo se ejecuta un turno (app-server de Codex)

Cada turno habla JSON-RPC con el **app-server** de Codex (ADR-0013):

- **Conexión**: si `<codex> app-server daemon version` da `"status":
  "running"` y un `socketPath`, titan-team se conecta a ese socket
  (WebSocket, el daemon compartido que usan el terminal de Codex y el chat de
  titan-agent); si no, lanza `<codex> app-server` en el worktree, por stdio
  (un objeto JSON por línea), solo durante el turno.
- **Peticiones**: un objeto JSON compacto por línea que empieza por
  `{"id":<n>,"method":"<método>"` (`id` desde 1). Primero `initialize`
  (`clientInfo.name = "titan-team"`, `capabilities.experimentalApi = true`)
  y la notificación `initialized`.
- **Hilo nuevo**: `thread/start` con `cwd` (el worktree), `model`, `sandbox`,
  `approvalPolicy: "never"` y `config` (el esfuerzo como
  `model_reasoning_effort`, los pares de `[codex].config` y del rol y, en un
  rol `workspace-write` con `target_dir`, `sandbox_workspace_write.writable_roots`
  y `shell_environment_policy.set.CARGO_TARGET_DIR`).
- **Hilo que continúa**: `thread/read` (si tiene un turno en curso, el turno
  es `busy`), `thread/turns/list` (turnos del usuario, abajo) y
  `thread/resume` con los mismos ajustes. «already has an active writer» es
  `busy`.
- **Turno**: `turn/start` con el prompt como entrada de texto y
  `outputSchema` si el rol tiene `output_schema`. `item/started` da los
  `activity`; el último `agentMessage` es la respuesta; la suma de
  `thread/tokenUsage/updated.tokenUsage.last` es el `usage`; `turn/completed`
  cierra el turno (`completed` → `ok`; si no, `error`). Al pasar `timeout_s`,
  `turn/interrupt` y estado `timeout`. Al final, `thread/unsubscribe`.
  titan-team no responde a las peticiones del servidor (aprobaciones,
  preguntas).
- **Salidas** en `<state_dir>/<encargo>/`: `NN-<rol>.jsonl` (los mensajes del
  servidor para el hilo, uno por línea), `NN-<rol>.stderr` (el stderr del
  servidor propio y los errores), y `NN-<rol>.md` o `.json` (la respuesta).
- **`--dry-run`** imprime `Peticiones JSON-RPC:` con `initialize`, la
  petición del hilo y `turn/start`, y luego `--- prompt ---` y el prompt; no
  ejecuta Codex.

### Turnos del usuario con un rol

Cada entrada del historial guarda `turn_id`, el id de Codex de su turno
(`null` si no llegó a empezar). Al continuar un hilo, los turnos de
`thread/turns/list` más recientes que el último con un `turn_id` del rol son
**turnos del usuario** (desde el chat o el terminal de Codex). Si los hay,
antes del turno se hace commit de lo que dejaron (rol `workspace-write` con
`autocommit`; mensaje `team: turnos del usuario con <rol>`), se escribe
`user_turn` y la salida de `ask` lo dice antes de la respuesta (una línea
`Turnos del usuario con <rol>: N …`, o el campo `user_turns` `{turns, text,
commit}` con `--json`; `null` si no hay).

## Log de eventos (versión 1)

`<carpeta base>/<state_dir>/<encargo>/events.jsonl`: una línea JSON por
evento, solo añadida al final. titan-agentd lo lee para el monitor.

Campos comunes: `v` (`1`), `ts_ms` (milisegundos Unix), `task`, `type` y,
opcional, `director_session`: el `CLAUDE_CODE_SESSION_ID` de la sesión de
Claude Code que ejecutó la orden (falta si no corre dentro de Claude Code).
El monitor une el director a esa sesión exacta; sin ella, a la sesión de
Claude Code más activa de la carpeta base.

| `type` | Campos | Cuándo |
| --- | --- | --- |
| `task_created` | `base`, `base_commit`, `branch`, `worktree`, `team` `{name, director, roles: [{name, cli, model, effort, sandbox}], steps, review_role, max_review_rounds}` | `new` |
| `brief_sent` | `from`, `to`, `turn`, `resumed`, `text` | al empezar `ask` |
| `turn_started` | `role`, `turn`, `cli`, `model`, `effort`, `sandbox`, `thread_id` (`null` si es nuevo), `review_round` | antes de lanzar Codex |
| `activity` | `role`, `turn`, `text` (`$ <orden>`, `edita <rutas>`, `mcp_tool_call`, `web_search`) | durante el turno, como mucho 200 |
| `turn_finished` | `role`, `turn`, `status` (`ok` · `error` · `timeout` · `busy`), `duration_ms`, `thread_id`, `usage` `{input_tokens, cached_input_tokens, output_tokens}`, `commit`, `changed_files`, `warnings`, `text` | al terminar |
| `user_turn` | `role`, `thread_id`, `turns`, `text` (el primer mensaje del usuario, 200 caracteres como mucho), `commit`, `changed_files` | antes de `brief_sent`, si hubo turnos del usuario con el rol |
| `report` | `role`, `review_round`, `verdict`, `findings` `{blocker, major, minor, nit}`, `questions` | tras el turno de un rol con `output_schema` |
| `decision` | `by`, `text` | `decide` |
| `task_closed` | `delete_branch` | `close` |

- `status = "busy"`: el hilo estaba ocupado (salida 4). `thread_id` es el
  que el rol ya tenía, `text` dice `hilo de <rol> ocupado` y el turno no
  cuenta como hecho.
- `text` de `brief_sent`, `activity` y `turn_finished`: como mucho 200
  caracteres; `decision.text`, 500. Lo que pasa se corta con `…`.
- Los lectores ignoran tipos y campos desconocidos y saltan líneas mal
  formadas o de una `v` mayor. Añadir tipos o campos opcionales no cambia `v`.
