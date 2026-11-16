# Formato de `.agents/team.toml` y de los eventos de titan-team

Este texto es la referencia que imprime `titan-team doc`. Está pensado para que
un agente director monte o cambie el equipo de un proyecto **solo editando este
fichero**, sin tocar la herramienta. `titan-team check` lo valida: una clave o un
valor desconocido es un error que nombra la clave, la línea y los valores
válidos.

## Dónde vive

- `<carpeta>/.agents/team.toml`. `titan-team` lo busca desde la carpeta actual
  hacia arriba (o `--config RUTA`). Las rutas relativas del fichero parten de
  `<carpeta>` (la que contiene `.agents/`).
- `titan-team init` crea uno de partida en `./.agents/team.toml`.

## Claves

### Raíz

| Clave | Tipo | Por defecto | Qué es |
| --- | --- | --- | --- |
| `version` | entero | obligatorio | Versión del formato. Hoy, `1`. |
| `name` | texto | — | Nombre del equipo (sale en el monitor). |
| `state_dir` | ruta | `.agents/state` | Estado de cada encargo y el log de eventos. No se versiona. |

### `[project]`

| Clave | Tipo | Por defecto | Qué es |
| --- | --- | --- | --- |
| `repo` | ruta | `.` | Repositorio git del que salen los worktrees. |
| `base` | texto | `main` | Rama de la que sale cada encargo (`new --base` la cambia). |
| `worktrees_dir` | ruta | `.agents/worktrees` | Un worktree por encargo. No se versiona. |
| `branch_prefix` | texto | `encargo/` | Prefijo de la rama de cada encargo. |
| `test_cmd` | texto | — | Orden de tests (`titan-team test`, `ask --test`). |
| `test_timeout_s` | entero > 0 | `300` | Tope de los tests, en segundos. |
| `autocommit` | booleano | `false` | Commit tras cada turno de un rol que escribe. |
| `hidden_paths` | lista | `[]` | Rutas que no se copian al worktree (sparse checkout): instrucciones y bóveda del director. Siguen en la rama. |

### `[codex]`

| Clave | Tipo | Por defecto | Qué es |
| --- | --- | --- | --- |
| `config` | lista | `[]` | Opciones `-c clave=valor` para todas las llamadas a `codex exec` (p. ej. `project_doc_max_bytes=0` para que no lea `AGENTS.md`). |

### `[flow]`

| Clave | Tipo | Por defecto | Qué es |
| --- | --- | --- | --- |
| `steps` | lista de roles | `[]` | Orden de paso del trabajo (también el orden del monitor). |
| `review_role` | rol | — | Rol cuyo turno abre una ronda de revisión. |
| `fix_role` | rol | — | Quién corrige tras la revisión (informativo). |
| `max_review_rounds` | entero | `0` | Tope de rondas: superarlo exige `ask --force` (salida 3). |

### `[roles.<nombre>]`

El nombre usa minúsculas, cifras, `-` y `_`.

| Clave | Tipo | Por defecto | Qué es |
| --- | --- | --- | --- |
| `cli` | `codex` · `claude` | obligatorio | Un rol `claude` es el director (la sesión de Claude Code que usa la herramienta) y lleva `launch = false`. Solo se lanzan roles `codex`. |
| `model` | texto | obligatorio | Modelo (`gpt-6-luna`, `gpt-6.1-sol`, `opus`…). |
| `effort` | `none` · `minimal` · `low` · `medium` · `high` · `xhigh` | — | Esfuerzo de razonamiento. |
| `sandbox` | `read-only` · `workspace-write` | `read-only` | Nunca `danger-full-access`. |
| `prompt_file` | ruta | obligatorio si se lanza | Instrucciones del rol; van antes del encargo en el primer turno. |
| `output_schema` | ruta | — | Esquema JSON de la respuesta (el informe del revisor). |
| `allowed_paths` | lista | `[]` | Aviso si el rol toca algo fuera. |
| `forbidden_paths` | lista | `[]` | Aviso si el rol toca algo dentro. |
| `timeout_s` | entero > 0 | `900` | Tope de un turno. |
| `purpose` | texto | — | Para qué está el rol. |
| `label` | texto | el nombre | Nombre corto en el monitor (`tests`, `revisor`). |
| `launch` | booleano | `true` | `false` para el director. |
| `config` | lista | `[]` | Opciones `-c` propias, tras las de `[codex]`. |

Rutas de `allowed_paths`, `forbidden_paths` y `hidden_paths`: comodines de
shell (`*` dentro de una carpeta, `?`, `**` a través de carpetas); una ruta que
termina en `/` cubre todo lo que hay debajo; una `/` inicial se ignora.

## Órdenes

`titan-team --help`. En resumen: `new` crea el encargo (worktree, rama y estado);
`ask <rol> <encargo>` es un turno del rol, que continúa su hilo si ya tiene uno
(`codex exec resume`), hace commit de lo que escribió y avisa de rutas
prohibidas; `show`, `diff`, `test`; `note -m` anota una decisión del director;
`close` quita el worktree. La herramienta nunca lanza Claude Code.

## Log de eventos (versión 1)

`titan-team` añade una línea JSON por evento a `<state_dir>/events.jsonl`;
titan-agentd la lee para el monitor del proyecto. Campos comunes:

```json
{"v":1,"at_ms":1794850000000,"task":"mi-encargo","director":"<id de sesión de Claude Code>","type":"turn_started", ...}
```

| `type` | Campos propios |
| --- | --- |
| `task_created` | `branch`, `base`, `base_dir`, `team`, `roles` (nombre, `label`, `cli`, `model`, `effort`, `sandbox`, `purpose`, `launch`), `steps`, `review_role`, `max_review_rounds` |
| `turn_started` | `role`, `turn`, `resumed`, `thread_id` (si continúa), `round` (si es el revisor), `message` (primera línea del mensaje) |
| `action` | `role`, `text`: lo que hace el rol en su turno (una orden, una edición…), como mucho cada 2 s |
| `turn_finished` | `role`, `turn`, `status` (`ok` · `error` · `timeout`), `thread_id`, `duration_ms`, `usage` (`input_tokens`, `cached_input_tokens`, `output_tokens`), `commit`, `changed_files`, `warnings`, `summary` |
| `review` | `role`, `round`, `max_rounds`, `verdict`, `findings` |
| `note` | `text` |
| `task_closed` | — |

`director` es `CLAUDE_CODE_SESSION_ID` del entorno, si existe. Un lector ignora
las líneas de otra versión o de un `type` que no conoce, y los campos nuevos.
