---
name: equipo
description: Dirige el equipo multiagente v7 definido en .agents/team.toml (Opus fija un contrato numerado con las firmas y rutas que usan los tests, con la lógica de interacción en modelos puros y los criterios visuales, deja un esqueleto que compila e implementa cada parte con sus tests; antes, Codex Sol escribe pocos tests de aceptación, al menos uno por apartado, que deben compilar; titan-team formatea y comprueba tras cada turno que escribe; al final, una revisión de Sol con la compuerta y contexto limpio, y un evaluador visual de Opus nuevo con el kit de scripts/demo/; Luna solo para ediciones repetitivas). Úsala cuando el usuario pida "encargar al equipo", "pásalo por el equipo" o un desarrollo con tests de aceptación, revisión independiente y evaluación visual.
---

# Equipo multiagente v7: bucle del director

Tú eres el **director** (rol `director` de `.agents/team.toml`) y **también implementas**. Desde la v3 el trabajo se reparte por **contexto**, no por rol: quien hace una parte hace también sus tests, y los demás roles aportan lo que solo da un contexto distinto (tests desde la especificación y una revisión limpia). Origen: experimento de la fase 1 (la variante con Luna implementando gastó más Claude, tardó más y quedó peor), la fase 3 (v4: contrato antes que tests y evaluador nuevo en cada ronda), la ronda 0.5.0 (v5: sus carencias, en `docs/Tareas/Equipo v5.md`), la 0.6.0 (v6: `docs/Tareas/Equipo v6.md`) y la 0.7.0 (v7: `docs/Tareas/Equipo v7.md`).

Herramienta: `bin/team`, envoltorio de `titan-team` (crate `crates/titan-team`; formato de `team.toml`, salidas y eventos en `docs/Decisiones/ADR-0010` y en `bin/team doc`). Lee `bin/team roles` al empezar (y `bin/team check` si cambias el TOML): el TOML manda, no esta skill. Cada paso queda en `.agents/state/<id>/events.jsonl`, que el monitor de titan-agent muestra en vivo. Cada `team ask` es una llamada a la cuota de ChatGPT Plus: **no repitas llamadas**. Si la cuota se agota, para y avisa al usuario con la hora de reinicio.

**Tareas sobre el propio equipo** (cambian titan-team): fija antes la herramienta para no dirigir con la que estás escribiendo: `git fetch origin`, `bin/team pin origin/main` (compila titan-team de esa revisión en la carpeta del equipo y lo deja en `.agents/team-bin/`; `bin/team` lo usa desde entonces) y comprueba con `bin/team --version`. Al cerrar, `bin/team unpin`. Si la tarea añade claves a `team.toml`, el binario fijado no las entiende: cambia `.agents/team.toml` cuando ya no lo necesites para Sol y, antes de la revisión, fija tu commit con la compuerta en verde (`bin/team pin HEAD`). `TITAN_TEAM_BIN=<binario>` manda sobre los dos.

**Órdenes en la sesión.** En una sesión en segundo plano o en un worktree, el aislamiento rechaza las órdenes compuestas que nombran git (también `github` en una ruta o `git` dentro de un `-m`). Así que:
- **git, una orden por llamada**: sin `&&`, `;` ni tuberías alrededor;
- **los mensajes largos se escriben con la herramienta de ficheros** (Write), nunca con `printf` ni heredoc dentro de una orden compuesta, y se pasan con `git commit -F <fichero>` o `bin/team ask … --message-file <fichero>`. titan-team rechaza un mensaje vacío (exit 2), así que un fichero que no llegó a escribirse ya no se convierte en un `-m` vacío.

## 0. Especificación (tú, antes de nada)
Escribe la especificación en un fichero fuera del repo (`$CLAUDE_JOB_DIR/tmp/<id>.md`). Debe bastar por sí sola, porque los roles de Codex no ven esta conversación **ni la bóveda** (`project.hidden_paths`, `[codex].config`) **ni leen código de producción**. Tiene cuatro secciones obligatorias, cada una con su título Markdown (`## Contrato`, `## Firmas que usan los tests`, `## Qué pedir a los tests`, `## Criterios visuales`): titan-team no lanza el primer turno de Sol si al encargo le falta uno de los títulos de `brief_sections` en `team.toml` (salida 2, sin gastar la llamada).
- **Contrato numerado** (C1, C2…): cada apartado, una garantía comprobable del comportamiento público, cerrada por ti: nombres, formatos, límites, mensajes de protocolo, códigos de error y cuándo salen. **Todo fichero o recurso que nombre** (un icono, un script, un fichero de configuración o de datos) **lleva su ruta exacta** en el repo o en tiempo de ejecución: Sol no puede adivinarla. **Ningún apartado remite a otra regla sin dar su firma**: si dice «la misma regla que X» o «como hace X», da también la firma de X (y X entra en la sección de firmas), o Sol no puede probarlo (0.7.0: pidió la de `board`). **Rutas y formatos por su significado, no por su forma en una plataforma**: «ruta absoluta», no «empieza por `/`» (el CI también corre en Windows; 0.7.1: `project_add` rechazaba toda carpeta en Windows). Antes, el objetivo y el comportamiento visible, copiando lo necesario de la tarea, sus ADR y reglas. Cópialo a la nota de la tarea o a su ADR **antes** de lanzar a Sol.
- **Firmas que usan los tests**: la firma completa (`use` con la ruta del módulo, tipos, constructores, campos públicos) de **todo** lo que los tests tengan que construir o llamar, **también lo que ya existe** (cópialo del código; nunca «ver los tests existentes»), y en qué `crates/<crate>/tests/<fichero>.rs` van. Si se prueba un binario: la orden, sus variables de entorno, los ficheros que lee y escribe y el formato de los CLI falsos. Lo que no esté aquí, Sol no lo sabe.
- **Qué pedir a los tests**: pocos (5–8) y **al menos uno por apartado del contrato**; pídele una tabla apartado → test y lo que deje sin test, con su motivo.
- **Criterios visuales** (paso 2): los escribes aquí, antes de lanzar a Sol y por tanto antes de implementar. Si la tarea no toca la ventana, la sección lo dice («Sin interfaz»): así no se olvida (0.7.1: los de la PR B se escribieron tras implementar).

**Esqueleto que compila (antes de lanzar a Sol).** Con el contrato cerrado, escribe en tu rama **todas las firmas de la sección de firmas** que aún no existen —tipos, campos, variantes de enum, funciones y métodos— con cuerpos `todo!()` (y los brazos de `match` que pidan las variantes nuevas), sin lógica: que `cargo test --workspace --no-run` compile, y haz commit. El encargo de Sol sale de ese commit (`--base <tu rama>`). Así sus tests compilan contra el código real y la comprobación `build` de su turno es obligatoria (`checks_required = ["build"]`): un test que no compila —también por un lint denegado como `overflowing_literals`, que rustc no comprueba mientras falten importaciones— deja su turno en rojo. Lo que se prueba como binario (órdenes, variables, ficheros) no necesita esqueleto. Ningún `todo!()` llega a la PR: el revisor lo busca.

**Interfaz: lógica en modelos puros.** La interfaz no tiene tests de ventana. Toda lógica de interacción nueva (qué hace una tecla, cómo se edita un texto, listas y borradores, qué se muestra según el estado, reglas del árbol o de qué encargos tiene una sesión) va en **funciones o modelos puros sin GPUI de ventana** (en la librería `titan_agent`, `crates/titan-agent/src/lib.rs`, o en el núcleo), con sus firmas en el contrato y tests de aceptación de Sol; la vista solo pinta y enruta eventos. Así Sol prueba lo que antes solo veía el evaluador visual. El revisor lo comprueba.

**Nada de tu diseño interno** (estructuras privadas, cómo lo vas a resolver): los tests deben salir de la especificación, no de tu plan.

Si hay decisiones de producto abiertas, pregúntalas al usuario antes de gastar llamadas.

## 1. Tests de aceptación (Sol, medium)
```
bin/team new <id> --brief <fichero> --base <tu rama>
bin/team ask disenador_tests <id>
```
Lanza el `ask` en segundo plano (`run_in_background`, timeout ≥ `timeout_s` + 2 × `test_timeout_s`) y, mientras, empieza por partes que los tests no tocan. Tras su turno, titan-team ejecuta sus `checks` (`fmt`, `build` y `test`): el formato entra en su commit y la salida queda en `.agents/state/<id>/NN-disenador_tests.checks.txt`. Que los tests fallen al ejecutarse es lo normal (`todo!()`); **si `build` falla, el turno sale con 1**: lee el error en su `.checks.txt` y devuélveselo en el mismo hilo (si el fallo es tuyo, una firma que falta en el esqueleto, la añades y se lo dices). Comprueba su tabla: un apartado sin test se lo pides o lo anotas con el motivo. Al terminar, trae su commit a tu rama (`git cherry-pick <commit>`) y anota el commit: **esos ficheros no los tocas tú**. Si crees que un test está mal, se lo devuelves con tu argumento (`bin/team ask disenador_tests <id> --message-file <fichero>`, mismo hilo) o lo consultas con el usuario. Al cerrar, `git diff <commit>..HEAD -- <ficheros de los tests>` debe salir vacío salvo correcciones hechas por Sol.

## 2. Criterios visuales (tú, en la especificación)
Si la tarea tiene interfaz, escribe **4–5 criterios** concretos y comprobables frente a `design/preview/` y las decisiones visuales (p. ej. jerarquía y espaciado, tipografía, color y disciplina de color, estados e interacción, acabado), en la sección `## Criterios visuales` de la especificación (paso 0) y en la nota de la tarea. Como el primer turno de Sol no arranca sin esa sección, quedan escritos antes de implementar y el encargo de `.agents/state/<id>/encargo.md` lo demuestra. No se cambian para que el resultado pase. Una tarea con varias PR escribe los de cada PR en la especificación de esa PR.

## 3. Implementación (tú)
Implementa cada parte con **sus propios tests** (unitarios o de integración propios, en ficheros distintos de los de aceptación). Compuerta: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` en verde, y los tests de aceptación pasando sin tocarlos.

**Movimientos de código: los haces tú**, con `scripts/mover-lineas.sh <origen> <desde> <hasta> <destino> [<tras>]` (mueve el bloque sin cambiarlo y comprueba que no se pierde ni sobra ninguna línea); luego ajustas los `use` y la visibilidad. No se los encargues a Luna: en la 0.5.0 gastó 293 k tokens de entrada en mover un bloque y hubo que arreglarlo igual.

**Luna (`mecanico`) solo para ediciones repetitivas** con un patrón ya fijado en muchos sitios, o investigar una API en paralelo mientras implementas. **Nunca la interfaz** ni nada que exija decidir. Si no hay trabajo así, no se usa. Su turno pasa la compuerta (`checks = ["fmt", "gate"]` con `checks_required`): si no pasa, el turno sale como fallido (exit 1) y lees su `.checks.txt`. Si el encargo la autoriza a tocar una ruta de su `forbidden_paths`, dilo con `--allow <patrón>` en ese `ask` (sin avisos que no aportan). Su sandbox no abre PTY: si dice «tests bloqueados por el sandbox», no es un fallo; lo que cuenta es la compuerta de titan-team, que corre fuera.

## 4. Revisión única (Sol, high, contexto limpio)
Una sola ronda (`max_review_rounds = 1`), en un **hilo nuevo** que solo ve la especificación, el diff y la compuerta. **Antes, tu propia compuerta en verde** (fmt, clippy y tests) sobre el commit que le pasas: un fallo que ya podías ver gasta su única ronda en un bloqueante.
```
bin/team new <id>-revision --brief <especificación> --base <commit base de la fase>
git -C .agents/worktrees/<id>-revision merge --ff-only <tu rama>
bin/team ask revisor <id>-revision --gate --message-file <mensaje>.md
```
El mensaje (escrito con Write) dice qué diff revisar: «El diff a revisar es `git diff <base>..HEAD -- crates bin .agents scripts` (léelo con git; puede ser largo)». `--gate` ejecuta `project.gate_cmd` (formato, clippy y tests) en su worktree y le pasa la salida: el revisor tiene el sandbox de solo lectura y no puede ejecutarla. Su prompt le dice que no evalúe lo visual (es del evaluador del paso 5). Lánzalo en segundo plano: la compuerta compila en un worktree nuevo, pero en la carpeta de compilación del equipo (`project.target_dir`), que ya calentaron los `checks` de Sol, así que solo recompila los crates del workspace. **En una tarea sobre el propio equipo** (titan-team, esta skill, los prompts o `team.toml`), añade esta skill al final de la especificación del revisor (`cat .claude/skills/equipo/SKILL.md >> <especificación>`): su worktree no trae `.claude/` y la skill es la mitad del cambio. (`--diff` trunca a 60 000 caracteres: para una fase entera, que el revisor lo lea con git.) Tú **decides** cada hallazgo — aceptado, descartado o aplazado — y **anotas por qué** en la nota de la tarea. Corriges tú. Otra ronda solo con permiso del usuario.

## 5. Evaluador visual (subagente de Opus nuevo en cada ronda)
Con la interfaz terminada, captura la ventana real con el **kit de demo** (`scripts/demo/demo.sh`; datos inventados, daemon privado, nunca sesiones reales del usuario):
```
cargo build -p titan-agent -p titan-agentd
scripts/demo/demo.sh up
scripts/demo/demo.sh shots grafo hilo hilo-largo     # o shots, para todas; demo.sh escenas las lista
scripts/demo/demo.sh down
```
`shot` fija él las variables de la escena y solo guarda la captura si KWin confirma, antes y después, que la ventana activa es la nuestra (su PID) y mide lo esperado; si no, no deja fichero. **Una escena por estado** que cambie tu tarea (vacío, con datos, largo, error, menú abierto, **estrecho**, **con texto escrito**…): si falta, añádela a `scene_env` en `demo.sh` (y sus datos a `datos.py`) en el mismo cambio. Una escena puede fijar:
- `TITAN_AGENT_DEMO_SIZE=<ancho>x<alto>`: el tamaño de su ventana (`shot` comprueba que la captura mida eso; `act-estrecho` es el ejemplo);
- `TITAN_AGENT_DEMO_KEYS=<guion>`: teclas que la ventana aplica 1,5 s después de abrirse, como si se pulsaran: texto, `{enter}`, `{shift-enter}`, `{tab}`, `{up}`, `{escape}`, `{ctrl-n}`, `{pause 500}`, `{{`/`}}` (sintaxis en `crates/titan-agent/src/demo.rs`; ejemplo: `nueva-teclas`). Un guion inválido cierra la ventana con el error en `ui.log`;
- datos de proyectos: `datos.py` escribe `config/projects.json` (ADR-0014) con un proyecto sin sesiones, uno con la carpeta borrada y `web-notas` sin importar.

**Flujos reales además de escenas fijas.** Si un estado depende de un servicio (el actualizador, el daemon, el app-server de Codex…), la escena fija con su variable de estado no basta: añade también un **kit de punta a punta** en `scripts/demo/<flujo>.sh` que monte el servicio real con datos inventados (como `actualizacion.sh`; `demo.sh flujos` los lista) y úsalo en la «Prueba a mano».

Lanza un subagente **nuevo en cada ronda** con la herramienta `Agent`, sin el contexto de la implementación ni el de rondas anteriores: recibe las capturas, `design/preview/` (o capturas de la previa) y los criterios del paso 2, y puntúa cada criterio con lo que falla. `shot` apaga los estilos de hover (`TITAN_AGENT_DEMO_NO_HOVER`), así que una fila resaltada en una captura es de la vista, no del puntero. Corriges y repites **como mucho 2 veces**.

`shot` no guarda una captura si otra ventana tiene el foco (el usuario está usando el escritorio): `shot` y `shots` la reintentan solas (`TITAN_DEMO_RETRIES`, 2 por defecto, tras `TITAN_DEMO_RETRY_WAIT` s, 15 por defecto); si aún falta alguna, repite solo esas más tarde.

El evaluador no puede probar la interacción (clics, hover, cursor, foco, scroll). Si la tarea la cambia, escribe en su nota una sección **«Prueba a mano»**: una frase con qué se prueba y por qué, la orden que abre la ventana (`scripts/demo/demo.sh probar <escena>`, que arranca la demo y la para al cerrar la ventana) y una **lista numerada de pasos, cada uno con su resultado esperado** («2. Clic en el nodo de Sol → se abre su hilo en solo lectura, con lo más reciente abajo»).

**Los pasos van siempre en el propio mensaje al usuario** (o en el que envíes a la sesión de control para él): la orden, la lista numerada y el resultado esperado de cada paso, tal cual, no un resumen ni un «están en la nota». El usuario abre la bóveda de `main`, y la nota de la rama no le llega hasta que se integra (0.7.1: no veía los de la PR B). Tiene que saber qué está probando y cuándo algo falla.

## 6. Cierre
- Si queda una prueba a mano, el mensaje final al usuario la trae completa (paso 5), aunque ya esté en la nota.
- Métricas para la nota de la tarea: tiempo, uso de Claude, `bin/team show <id>` (llamadas y tokens de Codex), número de tests (de aceptación y propios), hallazgos de revisión aceptados/descartados, resultado del evaluador visual y **carencias del equipo** encontradas (con su causa y una propuesta), para la siguiente versión del equipo.
- La rama se publica según `docs/README.md` → Publicación. Si el usuario autoriza publicar una versión, lanza «Publicar versión» **cuando el CI de `main` sobre el commit integrado esté en verde** (`gh run list --branch main --commit <sha>`), no justo al integrar (0.7.1: el workflow se negó y al usuario le llegó el correo de «build failed»). **Push, PR, integrar y borrar worktrees los decide el usuario.** `bin/team close <id> [--delete-branch]` para los encargos de Codex.

## Reglas
- Los roles de Codex mantienen su sandbox (`workspace-write` o `read-only`); nunca `danger-full-access`.
- Una llamada por paso; no relances un paso que ha ido bien "por si acaso".
- Si un `team ask` falla (exit 1), lee el `.stderr` del turno en `.agents/state/<id>/` antes de reintentar; un reintento como mucho y luego al usuario.
- **Hilo ocupado (exit 4)**: el hilo de ese rol está abierto en otro sitio (normalmente el usuario lo abrió en titan-agent con `codex resume`). No se ejecutó nada y el rol conserva su hilo. **No uses `--new`** ni abras otro hilo: avisa al usuario («el hilo de <rol> está abierto en titan-agent; ciérralo o dime cuándo») y espera. Cuando te diga que ya está, repite **la misma orden**.
- Variantes: si el usuario quiere otro reparto, se cambia en `.agents/team.toml` y aquí.
