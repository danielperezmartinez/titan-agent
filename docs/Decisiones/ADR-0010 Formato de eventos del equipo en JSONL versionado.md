---
Nombre: Formato de eventos del equipo en JSONL versionado
Número: 10
Estado: Aceptada
Resumen: 'titan-team escribe cada paso de un encargo (creado, turno empezado, acción en curso, turno terminado, informe del revisor, nota del director, cerrado) como una línea JSON con versión v=1 en <state_dir>/events.jsonl, junto al estado del encargo. titan-agentd lo encuentra desde las carpetas de las sesiones (.agents/team.toml), lo lee de forma incremental, lo une a las sesiones por el id de sesión del director (CLAUDE_CODE_SESSION_ID) y el thread_id de cada rol, y lo manda en el snapshot del protocolo (teams). El monitor lo dibuja.'
Decisión: Un log de eventos JSONL por equipo, append-only, con versión en cada línea y lectores tolerantes; unión a las sesiones por ids, no por carpetas, con la carpeta como respaldo.
Consecuencias: titan-team no depende del daemon ni el daemon de team.toml (solo lee su state_dir). Un formato incompatible es v=2 y los lectores ignoran lo que no conocen. El log puede crecer: el daemon guarda las últimas 20 000 líneas por equipo y solo informa de encargos con actividad en las últimas 24 h.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-16T19:20:00+01:00
Última modificación: 2026-11-16T19:20:00+01:00
---

# ADR-0010 · Formato de eventos del equipo en JSONL versionado

## Contexto

[[ADR-0005 Herramienta del equipo en Rust dentro de titan-agent]] pide que la
herramienta del equipo escriba sus eventos en un formato versionado y
documentado que lea titan-agentd, para que el monitor
([[Decisiones visuales/Monitor del proyecto en tres vistas]]) muestre quién habla
con quién y en qué ronda va cada encargo. Hasta ahora el daemon solo deducía la
jerarquía por carpetas ([[ADR-0009 Descubrimiento de sesiones y jerarquía lanzada por]]).
La regla de dependencias dice que `titan-team` no depende del daemon: escribe en
disco y el daemon lee.

## Decisión

- **Dónde**: `<state_dir>/events.jsonl` (por defecto `.agents/state/`, no
  versionado), un fichero por equipo para todos sus encargos. Solo se añaden
  líneas (`O_APPEND`, una escritura por línea).
- **Línea**: un objeto JSON con `v` (versión del formato, hoy `1`), `at_ms`
  (hora Unix en ms), `task`, `director` (el `CLAUDE_CODE_SESSION_ID` de quien
  ejecutó la herramienta, si existe) y `type` con sus campos:
  `task_created` (rama, base, carpeta base, equipo, roles con su CLI, modelo,
  esfuerzo, sandbox y etiqueta, flujo y tope de rondas), `turn_started` (rol,
  turno, si continúa el hilo, `thread_id`, ronda, primera línea del mensaje),
  `action` (lo que hace el rol durante el turno, leído en vivo de `codex exec
  --json`, como mucho cada 2 s), `turn_finished` (estado, `thread_id`,
  duración, tokens, commit, ficheros, avisos y un resumen corto),
  `review` (ronda, veredicto y hallazgos), `note` (decisión del director,
  `titan-team note`) y `task_closed`. Referencia completa:
  `crates/titan-team/FORMAT.md` (`titan-team doc`) y
  `titan_agent_core::team::events`.
- **Lectores tolerantes**: se ignoran las líneas de otra versión, los `type`
  desconocidos, los campos nuevos y la última línea si está a medias. Un cambio
  incompatible sube `v`.
- **Daemon**: cada segundo busca `.agents/team.toml` desde la carpeta de cada
  sesión (o el dueño de `.agents/worktrees/<encargo>`), lee del log solo lo
  añadido, lo pliega en un `TeamRun` por encargo y lo une a las sesiones:
  director por su id (`cl:<id>`) o, si no aparece, por la sesión viva de Claude
  Code en la carpeta base; cada rol por su `thread_id` (`cx:<id>`). Va en el
  snapshot `sessions` del protocolo como `teams`, campo nuevo con valor por
  defecto (no cambia la versión del protocolo,
  [[ADR-0007 Protocolo local de titan-agentd en NDJSON versionado]]).
- **Cuota**: va en el mismo snapshot (`quotas`), de las vías de
  [[Tareas/Investigar la cuota restante de cada CLI]].

## Alternativas consideradas

- **Que el daemon lea el `<encargo>.json` del estado** — es una foto, no una
  historia: no dice cuándo se envió cada mensaje ni qué hace el rol ahora.
- **Un socket del daemon al que `titan-team` mande los eventos** — rompe la
  regla de que la herramienta no depende del daemon; sin daemon se perderían.
- **Un fichero por encargo** — más ficheros que vigilar sin ganar nada; el
  `task` de cada línea ya separa los encargos.
- **Unir solo por carpetas, como en ADR-0009** — falla cuando el director
  trabaja en un worktree propio (`.claude/worktrees/…`) o hay varias sesiones de
  Claude en la misma carpeta; los ids lo resuelven.

## Consecuencias

- Positivas: el monitor sabe quién encarga qué, a quién responde cada rol, qué
  hace en este momento y en qué ronda va la revisión, con un retraso de ~1 s.
  El formato tiene dueño y tests en ambos lados.
- Negativas / compromisos: el log crece con cada encargo (unas pocas líneas por
  turno); no se rota todavía. La acción en curso depende de los eventos JSON de
  `codex exec`, que pueden cambiar: si cambian, solo se pierde esa línea.
