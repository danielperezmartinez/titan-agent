---
Nombre: Herramienta del equipo en Rust dentro de titan-agent
Número: 5
Estado: Aceptada
Resumen: La herramienta que reparte el trabajo del equipo (encargar a un rol, continuar su hilo, revisar, rondas, worktree por encargo; el bin/team del prototipo) se reescribe en Rust como un CLI de este repo con su skill de Claude Code. Escribe sus eventos en un formato versionado que lee el daemon para el monitor. Mientras llega (fase 3), se usa el prototipo en Python copiado al repo.
Decisión: Reescribir la herramienta del equipo en Rust dentro de este repo (CLI + skill), con un formato de eventos versionado que consume titan-agentd.
Consecuencias: El monitor depende del formato de eventos del CLI. La lanza siempre el agente director por Bash, nunca la app. El adaptador que lanzaba claude -p queda fuera.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-14T15:27:18+01:00
Última modificación: 2026-11-14T15:27:18+01:00
---

# ADR-0005 · Herramienta del equipo en Rust dentro de titan-agent

## Contexto

El prototipo de la sandbox `multiagente` probó en real el flujo: `.agents/team.toml`
(roles con CLI, modelo, esfuerzo, sandbox, prompt y rutas permitidas; proyecto;
flujo y tope de rondas), `bin/team` (Python, `new/ask/show/diff/test/close`,
continúa hilos por `thread_id` con `codex exec` / `codex exec resume`, commit
por turno en un worktree por encargo, informe del revisor con esquema) y la skill
`equipo`. Es lo que genera los datos de «quién habla con quién» que muestra el
monitor ([[Monitor del proyecto en tres vistas]]).

## Decisión

- La herramienta vive **en este repo**, reescrita en **Rust** como un CLI (nombre
  provisional `titan-team`) con su skill de Claude Code. Mantiene el modelo del
  prototipo: equipo en `.agents/team.toml` del proyecto, prompts por rol,
  esquema del informe del revisor, estado en `.agents/state/` (no versionado).
- Escribe sus **eventos** (encargo enviado, turno empezado y terminado, informe,
  ronda, decisión) en un formato **versionado y documentado** que lee
  titan-agentd ([[ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]]).
- La ejecuta el **agente director** (Opus en Claude Code) por Bash; llama a los
  CLI oficiales (`codex exec`). La app no la lanza
  ([[ADR-0001 Torre de control y no orquestador]]). El adaptador `cli = "claude"`
  del prototipo (`claude -p`) queda fuera.
- **Hasta la fase 3** se usa el prototipo en Python copiado al repo, y con él se
  construye titan-agent (Opus dirige, Luna implementa y diseña los tests en
  hilos separados, Sol revisa).

## Alternativas consideradas

- **Dejarlo fuera, como hoy** — dos piezas que versionar por separado y un
  formato de estado sin dueño claro.
- **Posponerlo y monitorizar solo sesiones** — el monitor perdería las
  comunicaciones, que es lo que más interesa al usuario.

## Consecuencias

- Positivas: una sola pieza versionada; el formato de eventos tiene dueño; el
  propio desarrollo de titan-agent prueba el flujo del equipo en un repo real.
- Negativas / compromisos: reescritura en la fase 3; mientras tanto conviven el
  prototipo y el formato nuevo. Cada llamada a Codex gasta cuota de ChatGPT Plus
  (Sol tiene poca: 15–160 mensajes cada 5 h).
