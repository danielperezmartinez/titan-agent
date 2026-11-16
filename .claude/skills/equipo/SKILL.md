---
name: equipo
description: Dirige el equipo multiagente v3 definido en .agents/team.toml (Opus implementa cada parte con sus tests; antes, Codex Sol escribe pocos tests de aceptación desde la especificación; al final, una revisión de Sol con contexto limpio y un evaluador visual de Opus nuevo; Luna solo para trabajo mecánico). Úsala cuando el usuario pida "encargar al equipo", "pásalo por el equipo" o un desarrollo con tests de aceptación, revisión independiente y evaluación visual.
---

# Equipo multiagente v3: bucle del director

Tú eres el **director** (rol `director` de `.agents/team.toml`) y **también implementas**. La v3 reparte el trabajo por **contexto**, no por rol: quien hace una parte hace también sus tests, y los demás roles aportan lo que solo da un contexto distinto (tests desde la especificación y una revisión limpia). Origen: experimento de la fase 1 (la variante con Luna implementando gastó más Claude, tardó más y quedó peor) y la valoración de la nota «Equipo multiagente con mis suscripciones» de la bóveda de control.

Herramienta: `bin/team` (en la carpeta que contiene `.agents/`). Lee `bin/team roles` al empezar: el TOML manda, no esta skill. Cada `team ask` es una llamada a la cuota de ChatGPT Plus: **no repitas llamadas**. Si la cuota se agota, para y avisa al usuario con la hora de reinicio.

## 0. Especificación (tú, antes de nada)
Escribe la especificación en un fichero fuera del repo (`$CLAUDE_JOB_DIR/tmp/<id>.md`). Debe bastar por sí sola, porque los roles de Codex no ven esta conversación **ni la bóveda** (`project.hidden_paths`, `[codex].config`). Contiene:
- Objetivo y comportamiento visible, copiando lo necesario de la tarea, sus ADR y reglas.
- El **contrato público** que tocarán los tests: crate y módulo, nombres, firmas, mensajes de protocolo, códigos de error y cuándo salen, y en qué `crates/<crate>/tests/` van.
- **Nada de tu diseño interno** (estructuras privadas, cómo lo vas a resolver): los tests deben salir de la especificación, no de tu plan.

Si hay decisiones de producto abiertas, pregúntalas al usuario antes de gastar llamadas.

## 1. Tests de aceptación (Sol, medium)
```
bin/team new <id> --brief <fichero> --base <tu rama>
bin/team ask disenador_tests <id>
```
Lanza el `ask` en segundo plano (`run_in_background`, timeout ≥ `timeout_s`) y, mientras, escribe los criterios visuales (paso 2) o empieza por partes que los tests no tocan. Al terminar, trae su commit a tu rama (`git cherry-pick <commit>`) y anota el commit: **esos ficheros no los tocas tú**. Si crees que un test está mal, se lo devuelves con tu argumento (`bin/team ask disenador_tests <id> -m "…"`, mismo hilo) o lo consultas con el usuario. Al cerrar, `git diff <commit>..HEAD -- <ficheros de los tests>` debe salir vacío salvo correcciones hechas por Sol.

## 2. Criterios visuales (tú, antes de implementar)
Si la tarea tiene interfaz, escribe **4–5 criterios** concretos y comprobables frente a `design/preview/` y las decisiones visuales (p. ej. jerarquía y espaciado, tipografía, color y disciplina de color, estados e interacción, acabado). Guárdalos en la nota de la tarea antes de implementar: no se cambian para que el resultado pase.

## 3. Implementación (tú)
Implementa cada parte con **sus propios tests** (unitarios o de integración propios, en ficheros distintos de los de aceptación). Compuerta: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` en verde, y los tests de aceptación pasando sin tocarlos.

**Luna (`mecanico`) solo para trabajo mecánico sin criterio**: código repetitivo con un patrón ya fijado, o investigar una API en paralelo mientras implementas. **Nunca la interfaz** ni nada que exija decidir. Si no hay trabajo así, no se usa.

## 4. Revisión única (Sol, high, contexto limpio)
Una sola ronda (`max_review_rounds = 1`), en un **hilo nuevo** que solo ve la especificación y el diff:
```
bin/team new <id>-revision --brief <especificación> --base <commit base de la fase>
git -C .agents/worktrees/<id>-revision merge --ff-only <tu rama>
bin/team ask revisor <id>-revision -m "El diff a revisar es git diff <base>..HEAD -- crates bin .agents (léelo con git; puede ser largo)."
```
(`--diff` trunca a 60 000 caracteres: para una fase entera, que el revisor lo lea con git.) Tú **decides** cada hallazgo — aceptado, descartado o aplazado — y **anotas por qué** en la nota de la tarea. Corriges tú. Otra ronda solo con permiso del usuario.

## 5. Evaluador visual (subagente de Opus nuevo)
Con la interfaz terminada, captura la ventana real (p. ej. `spectacle -a -b -n -o <png>`, con datos de prueba, nunca sesiones reales del usuario) y lanza un subagente **nuevo** con la herramienta `Agent`, sin el contexto de la implementación: recibe las capturas, `design/preview/` (o capturas de la previa) y los criterios del paso 2, y puntúa cada criterio con lo que falla. Corriges y repites **como mucho 2 veces**.

## 6. Cierre
- Métricas para la nota de la tarea: tiempo, uso de Claude, `bin/team show <id>` (llamadas y tokens de Codex), número de tests (de aceptación y propios), hallazgos de revisión aceptados/descartados y resultado del evaluador visual.
- La rama se publica según `docs/README.md` → Publicación. **Push, PR, integrar y borrar worktrees los decide el usuario.** `bin/team close <id> [--delete-branch]` para los encargos de Codex.

## Reglas
- Los roles de Codex mantienen su sandbox (`workspace-write` o `read-only`); nunca `danger-full-access`.
- Una llamada por paso; no relances un paso que ha ido bien "por si acaso".
- Si un `team ask` falla (exit 1), lee el `.stderr` del turno en `.agents/state/<id>/` antes de reintentar; un reintento como mucho y luego al usuario.
- Variantes: si el usuario quiere otro reparto, se cambia en `.agents/team.toml` y aquí.
