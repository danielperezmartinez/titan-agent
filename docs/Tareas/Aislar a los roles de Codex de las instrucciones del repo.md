---
Nombre: Aislar a los roles de Codex de las instrucciones del repo
Estado: En curso
Resumen: Cada llamada de bin/team a Codex (Luna y Sol) entraba con AGENTS.md, el catálogo de skills del usuario y el protocolo de la bóveda, que el rol leía por su cuenta (130–195k tokens de entrada por llamada; Luna llegó a marcar una tarea). Ahora cada llamada lleva solo su prompt, el encargo y el código, con opciones de codex exec en team.toml y un worktree sin docs/ ni punteros de arranque.
Decisiones: Aislamiento en dos capas, ambas en .agents/team.toml - [codex].config (opciones -c por llamada) y project.hidden_paths (sparse checkout del worktree del encargo). Se descarta desactivar herramientas de Codex (navegador, imagen, búsqueda web) porque con ellas desaparece también el shell.
Bloqueada: []
Fecha de creación: 2026-11-14T19:20:00+01:00
Última modificación: 2026-11-14T21:10:00+01:00
---

# Aislar a los roles de Codex de las instrucciones del repo

## Objetivo

Decisión del usuario antes de la [[Tareas/Fase 1 - Daemon, árbol de sesiones y terminal integrado|fase 1]]:
los roles de Codex del equipo (Luna implementa, Sol diseña tests y revisa) no
deben cargar las instrucciones del repo (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`)
ni seguir el protocolo de la bóveda. Cada llamada lleva solo su prompt
(`.agents/prompts/`), el encargo y el contexto que el rol necesite del código.
La bóveda la mantiene el director.

Motivos: cada llamada entraba con 130–195k tokens de entrada (suma de todas sus peticiones al modelo; cuota
de ChatGPT Plus, y la de Sol se gasta el doble al tener dos roles) y Luna llegó
a marcar una tarea de la bóveda por su cuenta.

## Diagnóstico (codex-cli 0.164.0)

Contexto que `codex exec` añade a cada petición, visto en los rollouts de las
llamadas de la fase 0:

| Bloque | Tamaño aprox. | Origen |
| --- | --- | --- |
| Instrucciones base de Codex | ~18–21k caracteres | fijo |
| `<skills_instructions>` | ~5–21k caracteres | skills del usuario en `~/.codex` |
| `<multi_agent_role>` | ~2,4k caracteres | función multiagente de Codex |
| `AGENTS.md` del worktree | ~1–2k caracteres | puntero de arranque del repo |

El coste grande no es el `AGENTS.md` en sí, sino lo que provoca: el rol lee
`docs/README.md`, `Inicio`, los `.base` y notas de la bóveda antes de trabajar,
y todo eso se reenvía en cada petición siguiente del mismo turno.

## Solución

Ambas piezas son configuración de `.agents/team.toml`; `bin/team` no tiene nada
fijo:

1. **`[codex].config`**: lista de pares `clave=valor` que `bin/team` pasa como
   `-c` en cada llamada, nueva o reanudada (un rol puede añadir los suyos con
   `config`):
   - `project_doc_max_bytes=0` y `project_doc_fallback_filenames=[]`: Codex no
     carga `AGENTS.md` ni ficheros de proyecto alternativos.
   - `skills.include_instructions=false`: sin el catálogo de skills del usuario.
2. **`project.hidden_paths`**: `bin/team new` deja fuera del worktree del
   encargo `/AGENTS.md`, `/CLAUDE.md`, `/GEMINI.md`, `/docs/` y `/.claude/` con
   un *sparse checkout* (`git sparse-checkout set --no-cone`). Los ficheros
   siguen en la rama y el commit de cada turno (`git add -A`) no los borra; el
   rol no puede leer ni editar la bóveda. Efecto secundario: git activa
   `extensions.worktreeConfig` en la configuración local del repo.
3. Prompts del diseñador de tests y del implementador: «todo lo que necesitas
   está en este prompt, el encargo y el código; tu copia no trae `docs/` ni
   `AGENTS.md`».

Probado y descartado: desactivar `image_generation`, `browser_use`,
`computer_use`, `in_app_browser` y `web_search` bajaba la llamada mínima a
~8,6k tokens, pero el rol se quedaba **sin shell** (respondía sin leer el
fichero, inventando). `features.multi_agent=false` no quita el bloque
`<multi_agent_role>`; no se usa.

Consecuencia para el director (skill «equipo»): las reglas del proyecto que el
rol necesite (idioma, datos inventados, ADR que afecten al encargo) van en el
encargo o ya están en su prompt.

## Criterios de finalización

- Una llamada de `bin/team` no carga `AGENTS.md` ni las skills y el worktree del
  encargo no tiene `docs/` ni los punteros de arranque.
- Tokens medidos antes y después.
- Tests de `bin/team` en verde y compuerta del CI en verde en la PR.

## Verificación

Medidas del 2026-11-14 con `gpt-6-luna`, tokens de entrada de toda la llamada:

| Llamada | Antes | Después |
| --- | --- | --- |
| Mínima (`Responde solo "OK"`, sin herramientas, esfuerzo low) | 17 522 | 12 336 |
| Tarea pequeña de lectura (`codex exec` directo, esfuerzo low) | 88 725 (leyó README, Inicio, catálogo) | 25 494 (leyó solo el fichero) |
| `bin/team ask implementador` con el mismo encargo de lectura (esfuerzo medium) | 77 362 · 11,8 s | 28 156 · 6,9 s |

- `python3 -m unittest discover -s bin -p 'test_*.py'`: 15 tests en verde
  (nuevos: opciones globales y por rol, en hilos nuevos y reanudados; worktree
  sin `AGENTS.md`/`CLAUDE.md`/`docs/` y commit que conserva los ocultos).
- PR: pendiente de autorización del usuario para el push.

## Resultado

Observaciones de la fase 1 (2026-11-14), primera fase con el aislamiento:

- **Sockets Unix**: el sandbox `workspace-write` de Codex no deja crearlos ni
  conectarse a ellos (`PermissionDenied`), así que los roles no pueden
  ejecutar los tests del daemon ni del cliente. El director los pasa fuera del
  sandbox (`bin/team test` o `--test` en la revisión) y envía los fallos. No se
  ha abierto la red a los roles: sería decisión del usuario
  (`sandbox_workspace_write.network_access` en `config` del rol).
- **Coste de los hilos largos**: cada turno reanudado reenvía todo el hilo en
  cada petición al modelo. El implementador del daemon llegó a ~26M tokens de
  entrada acumulados en 8 turnos (casi todo en caché). Para rondas de corrección
  grandes conviene un hilo nuevo (`bin/team ask … --new`) con un mensaje que
  baste por sí solo.
