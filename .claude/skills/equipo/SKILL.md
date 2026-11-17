---
name: equipo
description: Dirige el equipo multiagente v5 definido en .agents/team.toml (Opus fija un contrato numerado con las firmas que usan los tests e implementa cada parte con sus tests; antes, Codex Sol escribe pocos tests de aceptación, al menos uno por apartado; titan-team formatea y comprueba tras cada turno que escribe; al final, una revisión de Sol con la compuerta y contexto limpio, y un evaluador visual de Opus nuevo con el kit de scripts/demo/; Luna solo para ediciones repetitivas). Úsala cuando el usuario pida "encargar al equipo", "pásalo por el equipo" o un desarrollo con tests de aceptación, revisión independiente y evaluación visual.
---

# Equipo multiagente v5: bucle del director

Tú eres el **director** (rol `director` de `.agents/team.toml`) y **también implementas**. Desde la v3 el trabajo se reparte por **contexto**, no por rol: quien hace una parte hace también sus tests, y los demás roles aportan lo que solo da un contexto distinto (tests desde la especificación y una revisión limpia). Origen: experimento de la fase 1 (la variante con Luna implementando gastó más Claude, tardó más y quedó peor), la fase 3 (v4: contrato antes que tests y evaluador nuevo en cada ronda) y la ronda 0.5.0 (v5: sus carencias, en `docs/Tareas/Equipo v5.md`).

Herramienta: `bin/team`, envoltorio de `titan-team` (crate `crates/titan-team`; formato de `team.toml`, salidas y eventos en `docs/Decisiones/ADR-0010` y en `bin/team doc`). Lee `bin/team roles` al empezar (y `bin/team check` si cambias el TOML): el TOML manda, no esta skill. Cada paso queda en `.agents/state/<id>/events.jsonl`, que el monitor de titan-agent muestra en vivo. Cada `team ask` es una llamada a la cuota de ChatGPT Plus: **no repitas llamadas**. Si la cuota se agota, para y avisa al usuario con la hora de reinicio.

## 0. Especificación (tú, antes de nada)
Escribe la especificación en un fichero fuera del repo (`$CLAUDE_JOB_DIR/tmp/<id>.md`). Debe bastar por sí sola, porque los roles de Codex no ven esta conversación **ni la bóveda** (`project.hidden_paths`, `[codex].config`) **ni leen código de producción**. Tiene tres secciones obligatorias:
- **Contrato numerado** (C1, C2…): cada apartado, una garantía comprobable del comportamiento público, cerrada por ti: nombres, formatos, límites, mensajes de protocolo, códigos de error y cuándo salen. Antes, el objetivo y el comportamiento visible, copiando lo necesario de la tarea, sus ADR y reglas. Cópialo a la nota de la tarea o a su ADR **antes** de lanzar a Sol.
- **Firmas que usan los tests**: la firma completa (`use` con la ruta del módulo, tipos, constructores, campos públicos) de **todo** lo que los tests tengan que construir o llamar, **también lo que ya existe** (cópialo del código; nunca «ver los tests existentes»), y en qué `crates/<crate>/tests/<fichero>.rs` van. Si se prueba un binario: la orden, sus variables de entorno, los ficheros que lee y escribe y el formato de los CLI falsos. Lo que no esté aquí, Sol no lo sabe.
- **Qué pedir a los tests**: pocos (5–8) y **al menos uno por apartado del contrato**; pídele una tabla apartado → test y lo que deje sin test, con su motivo.

**Nada de tu diseño interno** (estructuras privadas, cómo lo vas a resolver): los tests deben salir de la especificación, no de tu plan.

Si hay decisiones de producto abiertas, pregúntalas al usuario antes de gastar llamadas.

## 1. Tests de aceptación (Sol, medium)
```
bin/team new <id> --brief <fichero> --base <tu rama>
bin/team ask disenador_tests <id>
```
Lanza el `ask` en segundo plano (`run_in_background`, timeout ≥ `timeout_s` + `test_timeout_s`) y, mientras, escribe los criterios visuales (paso 2) o empieza por partes que los tests no tocan. Tras su turno, titan-team ejecuta sus `checks` (`fmt` y `test`): el formato entra en su commit y la salida de los tests queda en `.agents/state/<id>/NN-disenador_tests.checks.txt`. Que fallen es lo normal (no hay implementación); mira si alguno no compila por algo ajeno a lo que falta (una firma mal usada) y, si es así, devuélveselo. Comprueba su tabla: un apartado sin test se lo pides o lo anotas con el motivo. Al terminar, trae su commit a tu rama (`git cherry-pick <commit>`) y anota el commit: **esos ficheros no los tocas tú**. Si crees que un test está mal, se lo devuelves con tu argumento (`bin/team ask disenador_tests <id> -m "…"`, mismo hilo) o lo consultas con el usuario. Al cerrar, `git diff <commit>..HEAD -- <ficheros de los tests>` debe salir vacío salvo correcciones hechas por Sol.

## 2. Criterios visuales (tú, antes de implementar)
Si la tarea tiene interfaz, escribe **4–5 criterios** concretos y comprobables frente a `design/preview/` y las decisiones visuales (p. ej. jerarquía y espaciado, tipografía, color y disciplina de color, estados e interacción, acabado). Guárdalos en la nota de la tarea antes de implementar: no se cambian para que el resultado pase.

## 3. Implementación (tú)
Implementa cada parte con **sus propios tests** (unitarios o de integración propios, en ficheros distintos de los de aceptación). Compuerta: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` en verde, y los tests de aceptación pasando sin tocarlos.

**Movimientos de código: los haces tú**, con `scripts/mover-lineas.sh <origen> <desde> <hasta> <destino> [<tras>]` (mueve el bloque sin cambiarlo y comprueba que no se pierde ni sobra ninguna línea); luego ajustas los `use` y la visibilidad. No se los encargues a Luna: en la 0.5.0 gastó 293 k tokens de entrada en mover un bloque y hubo que arreglarlo igual.

**Luna (`mecanico`) solo para ediciones repetitivas** con un patrón ya fijado en muchos sitios, o investigar una API en paralelo mientras implementas. **Nunca la interfaz** ni nada que exija decidir. Si no hay trabajo así, no se usa. Su turno pasa la compuerta (`checks = ["fmt", "gate"]` con `checks_required`): si no pasa, el turno sale como fallido (exit 1) y lees su `.checks.txt`. Si el encargo la autoriza a tocar una ruta de su `forbidden_paths`, dilo con `--allow <patrón>` en ese `ask` (sin avisos que no aportan).

## 4. Revisión única (Sol, high, contexto limpio)
Una sola ronda (`max_review_rounds = 1`), en un **hilo nuevo** que solo ve la especificación, el diff y la compuerta:
```
bin/team new <id>-revision --brief <especificación> --base <commit base de la fase>
git -C .agents/worktrees/<id>-revision merge --ff-only <tu rama>
bin/team ask revisor <id>-revision --gate -m "El diff a revisar es git diff <base>..HEAD -- crates bin .agents scripts (léelo con git; puede ser largo)."
```
`--gate` ejecuta `project.gate_cmd` (formato, clippy y tests) en su worktree y le pasa la salida: el revisor tiene el sandbox de solo lectura y no puede ejecutarla. Su prompt le dice que no evalúe lo visual (es del evaluador del paso 5). Lánzalo en segundo plano: la compuerta compila en un worktree nuevo. (`--diff` trunca a 60 000 caracteres: para una fase entera, que el revisor lo lea con git.) Tú **decides** cada hallazgo — aceptado, descartado o aplazado — y **anotas por qué** en la nota de la tarea. Corriges tú. Otra ronda solo con permiso del usuario.

## 5. Evaluador visual (subagente de Opus nuevo en cada ronda)
Con la interfaz terminada, captura la ventana real con el **kit de demo** (`scripts/demo/demo.sh`; datos inventados, daemon privado, nunca sesiones reales del usuario):
```
cargo build -p titan-agent -p titan-agentd
scripts/demo/demo.sh up
scripts/demo/demo.sh shots grafo hilo hilo-largo     # o shots, para todas; demo.sh escenas las lista
scripts/demo/demo.sh down
```
`shot` fija él las variables de la escena y solo guarda la captura si KWin confirma, antes y después, que la ventana activa es la nuestra (su PID) y mide lo esperado; si no, no deja fichero. **Una escena por estado** que cambie tu tarea (vacío, con datos, largo, error, menú abierto…): si falta, añádela a `scene_env` en `demo.sh` (y sus datos a `datos.py`) en el mismo cambio. Lanza un subagente **nuevo en cada ronda** con la herramienta `Agent`, sin el contexto de la implementación ni el de rondas anteriores: recibe las capturas, `design/preview/` (o capturas de la previa) y los criterios del paso 2, y puntúa cada criterio con lo que falla (dile que un resaltado de hover puede deberse a dónde quedó el puntero, no a la vista). Corriges y repites **como mucho 2 veces**.

El evaluador no puede probar la interacción (clics, hover, cursor, foco, scroll). Si la tarea la cambia, escribe en su nota una **lista corta de clics** («Prueba a mano»: qué pulsar y qué debe pasar) y pide al usuario que la pruebe con `scripts/demo/demo.sh up && scripts/demo/demo.sh probar <escena>`.

## 6. Cierre
- Métricas para la nota de la tarea: tiempo, uso de Claude, `bin/team show <id>` (llamadas y tokens de Codex), número de tests (de aceptación y propios), hallazgos de revisión aceptados/descartados, resultado del evaluador visual y **carencias del equipo** encontradas (con su causa y una propuesta), para la siguiente versión del equipo.
- La rama se publica según `docs/README.md` → Publicación. **Push, PR, integrar y borrar worktrees los decide el usuario.** `bin/team close <id> [--delete-branch]` para los encargos de Codex.

## Reglas
- Los roles de Codex mantienen su sandbox (`workspace-write` o `read-only`); nunca `danger-full-access`.
- Una llamada por paso; no relances un paso que ha ido bien "por si acaso".
- Si un `team ask` falla (exit 1), lee el `.stderr` del turno en `.agents/state/<id>/` antes de reintentar; un reintento como mucho y luego al usuario.
- **Hilo ocupado (exit 4)**: el hilo de ese rol está abierto en otro sitio (normalmente el usuario lo abrió en titan-agent con `codex resume`). No se ejecutó nada y el rol conserva su hilo. **No uses `--new`** ni abras otro hilo: avisa al usuario («el hilo de <rol> está abierto en titan-agent; ciérralo o dime cuándo») y espera. Cuando te diga que ya está, repite **la misma orden**.
- Variantes: si el usuario quiere otro reparto, se cambia en `.agents/team.toml` y aquí.
