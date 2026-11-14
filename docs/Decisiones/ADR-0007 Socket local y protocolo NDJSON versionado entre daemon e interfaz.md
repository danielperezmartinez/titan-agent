---
Nombre: Socket local y protocolo NDJSON versionado entre daemon e interfaz
Número: 7
Estado: Aceptada
Resumen: titan-agentd escucha en un socket Unix privado del usuario (en Windows, una named pipe, más adelante) y habla con sus clientes con JSON por líneas (NDJSON) con la forma de JSON-RPC sin el campo jsonrpc, igual que codex app-server. La primera petición es hello con la versión del protocolo (1). Las sesiones viajan como lista completa en cada cambio y el terminal como bytes en base64, empezando por una instantánea de la pantalla.
Decisión: Socket Unix en $XDG_RUNTIME_DIR/titan-agent/agentd.sock, NDJSON con peticiones, respuestas y notificaciones al estilo JSON-RPC, protocolo 1 con hello obligatorio, sesiones como lista completa y terminal en base64 con instantánea inicial.
Consecuencias: Cualquier cliente (la interfaz, titan-ssh en la fase 4 por stdio) habla el mismo formato. Cambiar un mensaje de forma incompatible exige subir el número de protocolo. La interfaz no lanza CLI ni lee sus ficheros; todo pasa por el daemon.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-14T19:40:00+01:00
Última modificación: 2026-11-14T19:40:00+01:00
---

# ADR-0007 · Socket local y protocolo NDJSON versionado entre daemon e interfaz

## Contexto

[[ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]] separa el daemon
de la interfaz y deja el formato del protocolo para la fase 1
([[Tareas/Fase 1 - Daemon, árbol de sesiones y terminal integrado]]). Requisitos:
pocos mensajes, fácil de depurar a mano, que lo hable titan-ssh por SSH en la
fase 4 y que lleve el flujo de bytes de los terminales.

Fuentes de datos verificadas el 2026-11-14 (Claude Code 2.1.325, codex-cli
0.164.0 con app-server 0.169.0):

- `claude agents --json` (documentado «for scripting»): array con `id` (solo
  sesiones background), `pid` (si el proceso vive), `cwd`, `kind`
  (`background`/`interactive`), `startedAt` (epoch ms), `sessionId`, `name`,
  `state` (`working`/`blocked`/`done`/`stopped`, solo background) y `status`
  (`busy`/`idle`). `claude attach` solo admite sesiones background. Ningún
  campo dice quién lanzó a quién. Cuesta ~0,2 s de CPU por llamada.
- Codex: las sesiones vivas son los hilos cargados en su daemon compartido
  (`codex app-server proxy` habla JSON-RPC por stdio con él): `thread/loaded/list`,
  `thread/read` y las notificaciones `thread/status/changed`, `thread/started` y
  `thread/closed`. `Thread` trae `id`, `name`, `preview`, `cwd`, `createdAt` (s),
  `model`, `status` (`notLoaded`/`idle`/`systemError`/`active` con
  `activeFlags` `waitingOnApproval`/`waitingOnUserInput`) y `parentThreadId`
  (solo subagentes de Codex). No hay método para detener un hilo; sí
  `turn/interrupt`.

## Decisión

### Transporte

- **Linux**: socket Unix en `$XDG_RUNTIME_DIR/titan-agent/agentd.sock`; sin
  `XDG_RUNTIME_DIR`, en `<tmp>/titan-agent-<uid>/agentd.sock`. La carpeta se crea
  con permisos `0700`. `--socket <ruta>` o `TITAN_AGENTD_SOCKET` lo cambian.
- **Un daemon por usuario**: al arrancar, si el socket existe y acepta conexión,
  el daemon nuevo termina con error; si no la acepta (fichero huérfano), lo borra
  y escucha.
- **Windows**: named pipe `\\.\pipe\titan-agentd-<usuario>`, cuando se aborde
  Windows (no entra en la fase 1).
- **Remoto (fase 4)**: el mismo protocolo por stdio.

### Formato

- **NDJSON**: un objeto JSON UTF-8 por línea terminada en `\n`, como mucho
  4 MiB por línea (una línea mayor es error de protocolo y cierra la conexión).
- Forma de JSON-RPC sin el campo `jsonrpc`, como `codex app-server`:
  - petición (cliente → daemon): `{"id": 1, "method": "…", "params": {…}}`;
  - respuesta: `{"id": 1, "result": {…}}` o
    `{"id": 1, "error": {"code": 3, "message": "…"}}`;
  - notificación (daemon → cliente): `{"method": "…", "params": {…}}`.
- Campos desconocidos se ignoran al leer (compatibilidad hacia delante); los
  nombres de campo van en `snake_case`.

### Versión y saludo

- `PROTOCOL_VERSION = 1`. La primera petición es `hello`
  `{"protocol": 1, "client": {"name": "…", "version": "…"}}` → `{"protocol": 1,
  "daemon": {"name": "titan-agentd", "version": "…"}}`. Otra versión → error 1 y
  cierre; cualquier otro método antes de `hello` → error 2.
- Un cambio incompatible de un mensaje sube el número de protocolo; añadir un
  campo opcional o un método nuevo no.

### Códigos de error

`1` protocolo no soportado · `2` falta `hello` · `3` sesión desconocida ·
`4` operación no disponible para esa sesión (no adjuntable, sin detener) ·
`5` parámetros no válidos · `6` método desconocido · `7` fallo interno (con
mensaje).

### Métodos (protocolo 1)

| Método | Parámetros | Resultado |
| --- | --- | --- |
| `hello` | `protocol`, `client` | `protocol`, `daemon` |
| `sessions/list` | — | `sessions: [Session]` |
| `sessions/subscribe` | — | `sessions: [Session]`; después, notificaciones `sessions/changed` |
| `terminal/open` | `session`, `cols`, `rows` | `{}`; después, notificaciones `terminal/output` |
| `terminal/input` | `session`, `data` (base64) | `{}` |
| `terminal/resize` | `session`, `cols`, `rows` | `{}` |
| `terminal/close` | `session` | `{}` (deja de recibir salida; el PTY sigue) |
| `session/stop` | `session` | `{}` |

Notificaciones:

- `sessions/changed` `{"sessions": [Session]}`: la **lista completa** cada vez
  que cambia algo (son pocas sesiones; evita diffs).
- `terminal/output` `{"session": id, "data": base64}`. La primera tras
  `terminal/open` es la **instantánea** de la pantalla actual (ver
  [[ADR-0008 Terminal con alacritty_terminal en el daemon y en la interfaz]]).
- `terminal/exited` `{"session": id, "code": n | null}`: el proceso del terminal
  terminó y el PTY se descarta.

Un PTY por sesión. Si ya existe, `terminal/open` lo reutiliza (y lo redimensiona);
varios clientes pueden recibir la misma salida y el tamaño es el del último
`terminal/open` o `terminal/resize`.

### Sesión

```json
{
  "id": "claude:748e2953",
  "cli": "claude",
  "name": "escritorio: titan-ssh",
  "cwd": "/ruta/al/proyecto/.claude/worktrees/x",
  "project": { "root": "/ruta/al/proyecto", "name": "proyecto" },
  "state": "working",
  "parent": null,
  "pid": 1234,
  "started_at": 1792599639991,
  "model": null,
  "attach": ["claude", "attach", "748e2953"],
  "stop": "stop"
}
```

- `id`: `claude:<id>` (background), `claude:<sessionId>` (interactiva sin `id`)
  o `codex:<threadId>`.
- `state` ∈ `working` · `needs_you` · `waiting` · `finished` · `failed`.
- `attach`: el comando que el daemon ejecuta en el PTY, o `null` si la sesión
  no se puede adjuntar (interactivas de Claude). La interfaz lo muestra tal cual.
- `stop` ∈ `"stop"` (Claude: `claude stop <id>`) · `"interrupt"` (Codex:
  `turn/interrupt` del turno activo, sin descargar el hilo) · `null`.

Correspondencias de estado:

| Origen | `state` |
| --- | --- |
| Claude `state` `working` | `working` |
| Claude `state` `blocked` | `needs_you` |
| Claude `state` `done` con `pid` | `waiting` |
| Claude `state` `done` sin `pid`, o `stopped` | `finished` |
| Claude `state` `failed` / `error` | `failed` |
| Claude sin `state` (interactiva) o `state` desconocido: `status` `busy` | `working` |
| … `status` `idle`, desconocido o ausente | `waiting` |
| Codex `active` sin `activeFlags` | `working` |
| Codex `active` con algún `activeFlags` | `needs_you` |
| Codex `idle` | `waiting` |
| Codex `systemError` | `failed` |
| Codex `notLoaded` o desconocido | `finished` |

### Descubrimiento (daemon)

- **Claude**: `claude agents --json` cada 2 s mientras haya algún cliente
  suscrito y cada 15 s si no (≈0,2 s de CPU por llamada). Campos ausentes o
  desconocidos se toleran.
- **Codex**: un proceso `codex app-server proxy` con `initialize`,
  notificación `initialized`, `thread/loaded/list` y `thread/read` de cada hilo,
  más las notificaciones de estado; se reconcilia con un sondeo cada 5 s. Si
  Codex no está o el proxy falla, el daemon sigue solo con Claude y lo reintenta
  cada 30 s.
- **Anidamiento**: solo por `parentThreadId` de Codex. Claude va plano: no se
  adivina quién lanzó a quién (si el árbol se equivoca, miente). La relación de
  los encargos del equipo llega en la fase 3.
- **Proyecto**: carpeta padre de
  `git -C <cwd> rev-parse --path-format=absolute --git-common-dir`, que agrupa
  bajo su repo los worktrees en cualquier ruta (los de Claude Code, los de
  `bin/team` y los de Codex con `--worktree`). Si git falla: se quita un sufijo
  `/.claude/worktrees/<x>` o `/.agents/worktrees/<x>` y se sube hasta el ancestro
  con `.git`; sin repo, la propia carpeta. `name` es el último componente de
  `root`. El daemon lo cachea por carpeta.

## Alternativas consideradas

- **JSON-RPC 2.0 estricto** — descartado por poco: el campo `jsonrpc` no aporta
  y `codex app-server` ya usa la forma sin él.
- **Protocolo binario o con marcos de longitud** — más eficiente para el
  terminal, pero peor de depurar; base64 en local es asumible.
- **Diffs de la lista de sesiones** — innecesarios con decenas de sesiones.
- **Vigilar ficheros internos de los CLI** en vez de `claude agents --json` —
  descartado: formato no documentado.

## Consecuencias

- Positivas: un solo formato para la interfaz y para titan-ssh; se depura con
  `socat`; los CLI se consultan solo por sus vías documentadas
  ([[ADR-0001 Torre de control y no orquestador]]).
- Negativas / compromisos: base64 añade un 33 % al flujo del terminal; el sondeo
  de `claude agents --json` cuesta CPU (por eso baja a 15 s sin clientes);
  `codex app-server` es experimental y su protocolo cambia entre versiones.
