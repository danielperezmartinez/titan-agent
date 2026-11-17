---
Nombre: Equipo v5
Estado: En curso
Resumen: 'Corrige las carencias del equipo v4 anotadas en la ronda 0.5.0: titan-team ejecuta formato y tests tras los turnos de los roles que escriben y pasa la compuerta al revisor, la skill exige un contrato numerado con las firmas que usan los tests, los movimientos de código los hace el director con un script, hay un kit de demo versionado en scripts/demo/ con escenas y captura verificada por PID, y ask --allow autoriza una ruta en un turno. Deja la configuración v5 como la vigente.'
Decisiones: 'Ajustes acordados con el usuario desde la sesión de control (2026-11-17). Lo que toque ficheros que también cambia feat/v0.5-equipo-v4 espera a que la 0.5.0 se integre (rebase antes de la PR). Las comprobaciones no añaden tipos de evento: van como activity, warnings y status del turno, para no tocar titan_agent_core::team (lo cambia la 0.5.0).'
Bloqueada: []
Fecha de creación: 2026-11-17T14:52:00+01:00
Última modificación: 2026-11-17T14:52:00+01:00
---

# Equipo v5

## Objetivo

El usuario pausó los experimentos de comparación de variantes para detectar y
corregir las carencias del equipo multiagente. La
[[Ronda de arreglos 0.5.0]] (equipo v4) anotó diez, con su causa y su
propuesta, en su sección «Carencias del equipo». Esta tarea las corrige en
titan-team, la skill `equipo`, los prompts de los roles y `scripts/`, y deja
la **configuración v5** como la vigente (sustituye a la v4 de
[[Fase 3 - Herramienta del equipo en Rust y monitor]]). La próxima ronda
(0.6.0: instalador y actualización automática) la hará el equipo v5.

Se lanzó desde la sesión de control `control-3b`; trabajo en la rama
`feat/equipo-v5`, que sale de `main`.

## Carencias que corrige

Numeración de la nota de la 0.5.0 (C = carencia) y el ajuste acordado:

| Carencia | Ajuste |
| --- | --- |
| C1. Los tests de Sol no pasaban `cargo fmt` | A1: titan-team formatea tras el turno |
| C4. Luna entregó sin compilar | A1: compuerta tras el turno de Luna, que falla si no pasa |
| C10. El revisor no puede ejecutar nada y repite lo visual | A1: `ask --gate` le pasa la compuerta; el prompt le quita lo visual |
| C2. A Sol le faltaron las firmas | A2: la especificación lista las firmas que usan los tests |
| C3. Un apartado del contrato sin test | A2: contrato numerado, al menos un test por apartado |
| C5. Luna no compensa para mover código | A3: los movimientos los hace el director con `scripts/mover-lineas.sh` |
| C7. No hay entorno de demo reutilizable | A4: kit versionado en `scripts/demo/` |
| C8. Un bucle de zsh capturó la vista equivocada | A4: `shot` recibe el nombre de la escena y fija él sus variables |
| C9. El evaluador visual no prueba la interacción | A4: escenas por estado y lista de clics para el usuario |
| C6. Avisos de `forbidden_paths` que no aportan | A5: `ask --allow <patrón>` |

## Diseño

### A1. Comprobaciones tras el turno y compuerta para el revisor

En `team.toml` (formato v1: son claves opcionales, no cambia `version`):

- `[project] fmt_cmd` (orden que formatea en el sitio) y `gate_cmd` (la
  compuerta completa: formato, clippy y tests).
- `[roles.<n>] checks`: lista con `"fmt"`, `"test"` o `"gate"`. Tras un
  turno `ok` de ese rol, titan-team ejecuta esas órdenes en el worktree del
  encargo **antes del commit automático** (lo que formatee entra en el commit
  del turno) y guarda su salida en el estado.
- `[roles.<n>] checks_required`: si alguna falla, el turno cuenta como
  fallido (`status = "error"`, salida 1).
- `ask --gate`: ejecuta `gate_cmd` antes del turno y añade su salida al
  mensaje (para el revisor, que tiene el sandbox de solo lectura).

En este repo: Sol (tests) lleva `checks = ["fmt", "test"]` sin
`checks_required` (sus tests fallan hasta que haya implementación: la salida
es informativa); Luna, `checks = ["fmt", "gate"]` con
`checks_required = true`; el revisor recibe `--gate` y su prompt le dice que
no repita la compuerta ni evalúe lo visual.

Para no tocar `titan_agent_core::team` (lo cambia la 0.5.0), no hay tipos de
evento nuevos: cada comprobación emite un `activity` (`$ <orden>`), las que
fallan van a `warnings` del `turn_finished` y, con `checks_required`, el
`status` es `error`.

### A2. Especificación completa para Sol (skill `equipo`, paso 0)

- **Contrato numerado** (C1, C2…): cada apartado una garantía comprobable.
- **Firmas que usan los tests**: la firma completa (ruta del módulo, tipos,
  constructores) de todo lo que los tests construyen o llaman, **también lo
  que ya existe**; en un binario, la orden, sus variables de entorno, los
  ficheros que lee y escribe y el formato de los CLI falsos. Sol no lee
  código de producción: lo que no esté aquí no lo sabe.
- **Qué pedir a los tests**: al menos un test por apartado del contrato. Sol
  responde con una tabla apartado → test, y lo que deja sin test, con su
  motivo.

### A3. Movimientos de código por el director

Un movimiento puro (llevar un bloque a otro fichero sin cambiarlo) lo hace el
director con `scripts/mover-lineas.sh`, que mueve un rango de líneas y
comprueba que el conjunto de líneas no cambia. Luna queda para ediciones
repetitivas en muchos sitios con un patrón fijado.

### A4. Kit de demo en `scripts/demo/`

- `datos.py`: datos inventados (un HOME falso con sesiones de Claude, un
  proyecto `titan-ssh` con un equipo en marcha y otro terminado, hilos de
  Codex, uno largo, y cuota inventada).
- `appserver.py`: app-server de Codex falso (JSON-RPC sobre WebSocket en un
  socket Unix) y `bin/claude`, `bin/codex`: CLI falsos.
- `demo.sh up|escenas|ui|shot|shots|probar|down`: arranca un daemon privado
  (nunca el socket del usuario), abre la ventana en una escena con nombre,
  captura solo si KWin confirma que la ventana activa es la nuestra (PID,
  antes y después de capturar) y del tamaño esperado, y abre la ventana para
  que el usuario pruebe los clics.
- La skill pide una escena por estado de la interfaz que cambie (incluidos
  los largos) y, en la nota de la tarea, una **lista corta de clics** para
  que el usuario pruebe la interacción.

### A5. `ask --allow <patrón>`

Repetible. En ese turno, los ficheros que casan con algún patrón no generan
avisos de `forbidden_paths` ni de `allowed_paths`; el historial guarda los
patrones autorizados.

## Contrato de titan-team (para los tests de aceptación)

Ampliación compatible de
[[Decisiones/ADR-0010 Formato de team.toml y de los eventos del equipo|ADR-0010]].

- **C1. Claves nuevas y su validación.** `[project] fmt_cmd` y `gate_cmd`
  (texto, sin valor por defecto). `[roles.<n>] checks` (lista de textos, cada
  uno `"fmt"`, `"test"` o `"gate"`; `[]`) y `checks_required` (booleano;
  `false`). Errores, todos a la vez como el resto: un valor fuera de la lista
  → `unknown_value` en `roles.<n>.checks`; una comprobación cuya orden no
  está configurada → `missing_key` en `project.fmt_cmd`, `project.test_cmd`
  o `project.gate_cmd`; `checks` no vacío en un rol `read-only` (también por
  defecto) → `unknown_value` en `roles.<n>.checks`; tipo equivocado →
  `bad_type`.
- **C2. Comprobaciones tras un turno `ok`.** Solo si el turno de Codex acaba
  con `status = "ok"` y el rol tiene `checks`. Se ejecutan en el worktree,
  en el orden de la lista, con el shell del sistema y el tope
  `project.test_timeout_s` (exit 124 al agotarlo); se ejecutan todas aunque
  falle una. Van antes del commit automático: lo que cambien entra en el
  commit del turno y en `changed_files`. Antes de cada una, un evento
  `activity` del rol y el turno con `text = "$ <orden>"`. Su salida se guarda
  en `<state_dir>/<encargo>/NN-<rol>.checks.txt` (mismo `NN` que el resto
  de ficheros del turno): por comprobación, una línea
  `## <nombre> · exit <código>`, la línea `$ <orden>` y sus últimas 400
  líneas. La entrada del historial (`<state_dir>/<encargo>.json`) y la salida
  de `ask --json` llevan `checks`: lista ordenada de
  `{"name", "cmd", "exit", "duration_s"}`. Cada una que falla (exit ≠ 0)
  añade el aviso `comprobación <nombre> falló (exit <código>)` a `warnings`
  (historial y `turn_finished`). Sin `checks_required`, el turno sigue `ok` y
  `ask` sale con 0; con `checks_required = true` y alguna fallida, `status`
  es `"error"` (historial y `turn_finished`) y `ask` sale con 1, con el
  commit hecho igualmente. Un turno `error`, `timeout` o `busy`, o
  `--dry-run`, no ejecuta ninguna, no crea el fichero y no lleva `checks`
  (o la lleva vacía).
- **C3. `ask --gate`.** Antes de lanzar Codex ejecuta `project.gate_cmd` en
  el worktree (shell, tope `test_timeout_s`; ya con `turn_started` escrito y
  un `activity` `$ <orden>`, para que el monitor muestre la espera —ampliación
  hecha tras los tests de Sol, que la siguen cumpliendo—) y añade al final del mensaje que
  recibe Codex por la entrada estándar el bloque
  `# Compuerta (exit <código>)` seguido de las últimas 150 líneas de su
  salida; lo guarda también en `NN-<rol>.gate.txt`. Que falle no impide el
  turno. Sin `gate_cmd`: salida 2, sin lanzar nada ni escribir eventos. Con
  `--dry-run` no se ejecuta y el mensaje lleva `# Compuerta` con
  `(no se ejecuta en --dry-run)`. Se combina con `--diff` y `--test` (en ese
  orden: diff, tests, compuerta).
- **C4. `ask --allow <patrón>`.** Repetible, mismos comodines que
  `allowed_paths`. En ese turno, un fichero cambiado que casa con algún
  patrón no genera avisos de `forbidden_paths` ni de `allowed_paths`; los
  demás, sí. La entrada del historial lleva `allowed` con los patrones (no
  aparece si no hay). `--allow` sin valor: salida 2.
- **C5. Documentación.** `titan-team doc` describe `fmt_cmd`, `gate_cmd`,
  `checks`, `checks_required`, `--gate` y `--allow`.

## Configuración v5 (vigente)

Sustituye a la v4 (la de la fase 3 y la 0.5.0). Igual que la v4 salvo lo
marcado como nuevo:

1. El director (Opus) escribe la especificación con el **contrato numerado y
   las firmas completas que usan los tests** (nuevo) y la copia a la nota o
   ADR antes de lanzar a Sol.
2. Sol (gpt-6.1-sol, medium) escribe pocos tests de aceptación, **al menos uno
   por apartado** (nuevo), que el director no toca; titan-team **los formatea
   y ejecuta los tests tras su turno** (nuevo, informativo).
3. El director implementa cada parte con sus tests. **Los movimientos de
   código los hace él con `scripts/mover-lineas.sh`** (nuevo).
4. Luna (gpt-6-luna) solo para ediciones repetitivas; **su turno pasa la
   compuerta o cuenta como fallido** (nuevo). `--allow` para las rutas que el
   encargo autoriza (nuevo).
5. Una revisión final de Sol (high) en un hilo nuevo, **con la salida de la
   compuerta y sin evaluar lo visual** (nuevo).
6. Evaluador visual: un subagente de Opus nuevo en cada ronda (máximo 2), con
   **capturas del kit `scripts/demo/` por escena y estado** (nuevo); el
   usuario prueba a mano la **lista de clics** de la nota (nuevo).

Los roles de Codex mantienen su sandbox. Fuente de verdad del reparto:
`.agents/team.toml` y la skill `equipo`.

## Coordinación con la 0.5.0

`feat/v0.5-equipo-v4` (sin integrar) cambia, entre otros, `Cargo.toml`,
`Cargo.lock`, `titan_agent_core::team`, la interfaz y `docs/README.md`. Esta
rama no toca esos ficheros hasta que la 0.5.0 esté en `main`: entonces se
hace `git rebase origin/main`, se sube la versión y se abre la PR.

## Criterios de finalización

- A1–A5 implementados, con los tests de aceptación de Sol y los propios.
- La skill `equipo`, los prompts y `.agents/team.toml` describen la v5.
- El kit de demo captura la ventana real con datos inventados.
- fmt, clippy `-D warnings`, tests y gitleaks en verde; PR integrada por
  squash con la versión acordada con el usuario.

## Carencias nuevas encontradas

Encontradas al hacer esta tarea con el equipo (v4 y, desde la revisión, v5):

1. **Los datos de la demo dentro de un worktree se agrupan bajo el repo.**
   `.claude/worktrees/` y `.agents/worktrees/` marcan la raíz del proyecto,
   así que un `~/dev/titan-ssh` falso bajo `.previa/` salía como el proyecto
   `titan-agent`. *Corregido en el kit*: los datos van fuera del repo.
2. **Las capturas podían mostrar el nombre del usuario.** La línea «se
   ejecutará» de nueva sesión imprime la carpeta absoluta, y con los datos
   bajo `HOME` salía `/home/<usuario>/…`. *Corregido en el kit*: los datos
   viven en `$XDG_RUNTIME_DIR`. *Propuesta para la app*: abreviar `HOME` como
   `~` también en esa línea.
3. **El hover depende de dónde esté el puntero.** La ventana se abre bajo el
   puntero y una fila del árbol sale resaltada en algunas capturas; sin
   simular entrada no se controla. *Propuesta*: que el evaluador visual lo
   sepa (va en la skill) o una variable de la app que desactive el hover en
   las capturas.
4. **El revisor no ve la skill ni la bóveda** (`hidden_paths`), y en una
   tarea sobre el propio equipo la skill es la mitad del cambio. *Propuesta*:
   en esas tareas, copiar la skill al brief del revisor.
5. **Cada encargo compila desde cero** (cada worktree tiene su `target/`):
   las `checks` de Sol y el `--gate` del revisor pagan un build completo con
   GPUI. *Propuesta*: medirlo en la 0.6.0 y, si pesa, un `CARGO_TARGET_DIR`
   compartido por los encargos (sin compartirlo con el del director, para no
   volver a la espera del bloqueo de Cargo de la 0.5.0).

## Verificación

<Se rellena al completar.>

## Métricas

Inicio: 2026-11-17 14:45.

## Resultado

<Se rellena al completar.>
