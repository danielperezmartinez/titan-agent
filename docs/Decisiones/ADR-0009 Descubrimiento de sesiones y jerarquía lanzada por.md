---
Nombre: Descubrimiento de sesiones y jerarquía lanzada por
Número: 9
Estado: Aceptada
Resumen: 'titan-agentd sondea cada 2 s claude agents --json y el daemon compartido de Codex (WebSocket sobre su socket Unix, que da codex app-server daemon version): hilos cargados más los actualizados en los últimos 20 min. Para el modelo de Claude lee solo la cola del transcript de cada sesión. Anida un hilo bajo otro con parentThreadId de Codex y un hilo de Codex bajo la sesión de Claude cuya carpeta contiene su worktree .agents/worktrees/<encargo> (la herramienta del equipo). Las sesiones de Claude no se anidan: el CLI no dice quién las lanzó.'
Decisión: Sondeo de las vías oficiales (claude agents --json, codex app-server por WebSocket) con un mapeo de estados fijado y dos reglas de anidado fiables; sin anidar sesiones de Claude.
Consecuencias: La lista refleja cambios en unos 2 s. Si los CLI cambian sus campos, solo cambia core::discovery (con sus tests). Cuando la herramienta del equipo escriba eventos (fase 3) habrá un dato directo de quién encarga a quién.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-15T11:36:00+01:00
Última modificación: 2026-11-15T11:36:00+01:00
---

# ADR-0009 · Descubrimiento de sesiones y jerarquía lanzada por

## Contexto

La fase 1 pide el árbol de sesiones de Claude Code y Codex con su estado en
segundos, y averiguar de dónde sale la jerarquía «lanzada por» (si no hay dato
fiable, sin anidar). Comprobado el 2026-11-15 con Claude Code 2.1.325 y Codex
CLI 0.164 (daemon 0.169):

- `claude agents --json` devuelve las sesiones activas con `id` (solo las de
  segundo plano), `sessionId`, `pid`, `cwd`, `kind`, `startedAt`, `name`,
  `status` (`busy`/`idle`) y `state` (`working`, `blocked`, `done`, `stopped`).
  No trae modelo ni padre.
- El daemon de Codex escucha en un socket Unix que indica
  `codex app-server daemon version` (`socketPath`) y habla JSON-RPC sobre
  WebSocket. `codex app-server proxy` no reenvía nada si su entrada es una
  tubería. `thread/list` y `thread/loaded/list` dan los hilos con `status`,
  `model`, `reasoningEffort`, `source` y `parentThreadId` (solo subagentes).

## Decisión

- **Sondeo cada 2 s** (`TITAN_AGENTD_POLL_MS`), una instantánea nueva solo si
  algo cambió. Fuentes con error se muestran en la interfaz (`sources`).
- **Claude Code**: `claude agents --json`. Estados: `state = blocked` → `[?]`;
  `done` / `stopped` → `[x]`; `failed` / `error` → `[x]` con error; si no,
  `status = busy` → `[>]` y `idle` → `[+]`. Se adjuntan (`claude attach <id>`) y
  se detienen (`claude stop <id>`) solo las de segundo plano; las interactivas
  viven en otro terminal. El modelo sale de los últimos 256 KiB del transcript
  `~/.claude/projects/<carpeta>/<sessionId>.jsonl` (lo permite ADR-0004); nada
  más se lee de `~/.claude`.
- **Codex**: cliente WebSocket propio (tungstenite) que solo hace `initialize`,
  `thread/loaded/list`, `thread/list` (60 más recientes), `thread/read` y, al
  pulsar `[x] detener`, `thread/turns/list` + `turn/interrupt`. Se muestran los
  hilos cargados y los actualizados en los últimos 20 min (para que un encargo
  del equipo recién terminado siga bajo su director). Estados: `active` con
  `waitingOnApproval`/`waitingOnUserInput` → `[?]`; `active` → `[>]`; `idle` →
  `[+]`; `systemError` → `[x]` con error; no cargado → `[x]`.
- **Jerarquía**, de más a menos fiable:
  1. `parentThreadId` de Codex.
  2. Un hilo de Codex cuya carpeta está en `<carpeta>/.agents/worktrees/<encargo>`
     cuelga de la sesión viva de Claude Code cuya carpeta es `<carpeta>`: es
     donde la herramienta del equipo crea los worktrees de sus encargos. Si el
     encargo empieza con el prompt del equipo («Eres el IMPLEMENTADOR…»), el rol
     y el nombre (`implementador · <encargo>`) salen de ahí.
  3. Las sesiones de Claude Code no se anidan.
- **Proyecto**: la carpeta de la sesión sin el tramo `/.claude/worktrees/…` ni
  `/.agents/worktrees/…`; el nombre es su último componente.

## Alternativas consideradas

- **`codex app-server proxy` por stdio** — descartado: no funciona con la entrada
  por tubería en la versión probada.
- **Notificaciones de Codex en lugar de sondeo** — aplazado: el sondeo es simple
  y suficiente; las notificaciones (`thread/status/changed`) servirán para el
  monitor de la fase 3.
- **Anidar sesiones de Claude por carpeta o por hora de inicio** — descartado: no
  es fiable (varias sesiones comparten carpeta).

## Consecuencias

- Positivas: solo vías oficiales; el árbol muestra quién encarga a quién en el
  caso que importa (director y equipo); toda la interpretación está en
  `titan_agent_core::discovery`, probada con datos inventados.
- Negativas / compromisos: el sondeo lanza `claude agents --json` cada 2 s (unos
  0,2 s de CPU). Un hilo del equipo lanzado desde una sesión ya cerrada queda
  suelto. La regla 2 depende de la disposición de carpetas de la herramienta del
  equipo; si cambia en la fase 3, cambia esta regla.
