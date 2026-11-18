---
Nombre: Equipo v7
Estado: En curso
Resumen: 'Corrige las carencias del equipo v6 anotadas en la 0.7.0/0.7.1: comprobación `build` obligatoria tras el turno de Sol sobre un esqueleto que compila (un test que no compila sale en rojo), secciones obligatorias del encargo (`brief_sections`: contrato, firmas y criterios visuales antes de implementar), pasos de la prueba a mano siempre en el mensaje al usuario, contrato sin «la misma regla que X» sin firma y con rutas según la plataforma, `shots` que reintenta las capturas rechazadas por el foco y HOME de la demo sin falso positivo de gitleaks. Deja la configuración v7 como la vigente.'
Decisiones: 'Encargo de la sesión de control control-3b (2026-11-18).'
Bloqueada: []
Fecha de creación: 2026-11-18T22:05:00+01:00
Última modificación: 2026-11-18T22:05:00+01:00
---

# Equipo v7

## Objetivo

Corregir las carencias del equipo v6 anotadas en
[[Versión 0.7.0 - Proyectos importados y monitor por sesión directora]] →
«Carencias del equipo» y dejar la **configuración v7** como la vigente, para
que la haga servir [[Chat de sesión con Codex y Claude Code]] (0.8.0).
Sustituye a la v6 de [[Equipo v6]].

Se lanzó desde la sesión de control `control-3b`; trabajo en la rama
`feat/equipo-v7`, que sale de `main` (con la 0.7.1 integrada).

## Carencias que corrige

Numeración de la nota de la 0.7.0 (C = carencia) y el ajuste:

| Carencia | Ajuste |
| --- | --- |
| C6. Un test de Sol no compilaba (`json!` con un entero que no cabe en `i32`) y sus `checks` no lo vieron | A1: esqueleto que compila antes de Sol, comprobación `build` y `checks_required = ["build"]` |
| C4. Criterios visuales de la B escritos tras implementar | A2: `brief_sections`: Sol no arranca sin «Contrato», «Firmas» y «Criterios visuales» en el encargo |
| C7. El usuario no veía los pasos de la prueba a mano | A3: la skill pide los pasos en el propio mensaje; la nota de la rama no le llega |
| C3. «La misma regla que X» sin la firma de X | A4: la skill lo prohíbe; Sol lo pregunta |
| C8. El contrato fijó una ruta absoluta con forma de Unix | A4: el contrato describe rutas y formatos por su significado, no por su forma en una plataforma |
| C2. Capturas rechazadas cuando el usuario usa el escritorio | A5: `shot` y `shots` reintentan las rechazadas por el foco |
| C5. Falso positivo de gitleaks por el HOME de la demo (`home/dev`) | A6: el HOME inventado se llama `casa/`; sin `gitleaks:allow` y sin tocar las reglas |

C1 (guion de teclas a medio escribir) ya se corrigió en el kit durante la
0.7.0.

## Diagnóstico de C6

Las `checks` de Sol son `fmt` y `test` (`cargo test --workspace`), sin
`checks_required`: sus tests se escriben antes de la implementación y fallan
siempre, así que el resultado era solo informativo. Además, el fallo concreto
**no podía verse** en su turno: `literal out of range for i32` no es un error
de tipos sino el lint `overflowing_literals` (denegado por defecto), y rustc no
ejecuta los lints mientras haya errores de resolución. Con la implementación
sin escribir, el test fallaba antes con `unresolved import` (los
`.checks.txt` de la PR B solo muestran E0432/E0599/E0026) y el lint salió
cuando el director implementó. Además, `cargo test` se para en el primer crate
que no compila, así que los tests de otros crates ni se compilaban.

Por eso no basta con mirar mejor la salida: hace falta que los tests de Sol
compilen en su turno. El director deja antes un **esqueleto** (las firmas del
contrato con cuerpos `todo!()`), y titan-team compila los tests sin
ejecutarlos (`build`) como comprobación obligatoria: un error de compilación
(un lint denegado incluido) deja el turno en rojo; un test que falla al
ejecutarse sigue siendo lo normal.

## Contrato (para los tests de aceptación)

### C1. Comprobación `build`

- `[project] build_cmd` es una clave opcional nueva (cadena): la orden que
  compila sin ejecutar (en este repo, `cargo test --workspace --no-run`).
- `[roles.<n>] checks` admite un cuarto valor, `"build"`, que ejecuta
  `project.build_cmd`, igual que las otras tres (en el worktree del encargo,
  en el orden de la lista, tras un turno `ok`, antes del commit automático, con
  un evento `activity` cuyo texto es `$ <orden>` antes de ejecutarla, su salida
  en `.agents/state/<encargo>/NN-<rol>.checks.txt` bajo la cabecera
  `## build · exit N` y su ejecución en `checks` del historial con
  `name = "build"`).
- Un rol que usa `"build"` sin `project.build_cmd` es un error de
  configuración: `titan-team check` sale con 1 y el error tiene
  `path = "project.build_cmd"` y `code = "missing_key"` (como ya pasa con las
  otras tres).
- Un valor fuera de `fmt`, `test`, `gate`, `build` sigue siendo
  `unknown_value` en `roles.<n>.checks`.

### C2. `checks_required` como lista

`[roles.<n>] checks_required` admite, además de un booleano, una **lista de
nombres de comprobación**:

- `true`: el turno falla si falla cualquiera de sus comprobaciones (como hoy).
- `false` o ausente: ninguna lo hace fallar (como hoy).
- Lista, p. ej. `checks_required = ["build"]`: el turno falla solo si falla
  alguna de las de la lista; las demás que fallen solo avisan. Una lista vacía
  equivale a `false`.
- «El turno falla» significa lo de hoy: `status = "error"` en el historial y
  en el evento `turn_finished`, y `ask` sale con 1. El commit automático del
  turno se hace igual (los ficheros del rol quedan en su rama) y todas las
  comprobaciones de la lista `checks` se ejecutan aunque falle una.
- Cada comprobación fallida, obligatoria o no, sigue dando el aviso
  `comprobación <nombre> falló (exit N)`.
- Errores de `titan-team check` (sale con 1):
  - un nombre de la lista que no está en `checks` de ese rol →
    `path = "roles.<n>.checks_required"`, `code = "unknown_value"`;
  - un valor que no es ni booleano ni lista de cadenas (p. ej. `"yes"` o
    `[1]`) → `path = "roles.<n>.checks_required"`, `code = "bad_type"`.

### C3. `brief_sections`: secciones obligatorias del encargo

`[roles.<n>] brief_sections` es una clave opcional nueva: lista de cadenas no
vacías (p. ej. `["Contrato", "Firmas", "Criterios visuales"]`). Si no es una
lista de cadenas o alguna está vacía (o solo con espacios):
`path = "roles.<n>.brief_sections"`, `code = "bad_type"`.

Cuando `ask <rol> <encargo>` va a **abrir un hilo nuevo** de ese rol (su
primer turno en el encargo, o con `--new`), comprueba antes el encargo (el
fichero que se copió con `new --brief`): cada sección de la lista debe
aparecer en **un título Markdown**:

- un título es una línea que, quitando hasta 3 espacios iniciales, empieza por
  entre 1 y 6 `#` seguidos de un espacio; su texto es lo que va detrás, sin
  espacios a los lados;
- una sección está si el texto de algún título la **contiene**, sin
  distinguir mayúsculas de minúsculas (`## C. Contrato numerado` cubre
  `Contrato`; `### CRITERIOS VISUALES` cubre `Criterios visuales`);
- un texto que no es título (un párrafo que dice «Contrato») no cuenta, y
  tampoco una línea dentro de un bloque de código cercado (entre dos líneas
  que empiezan por ```` ``` ````).

Si falta alguna: `ask` sale con 2, escribe en stderr un mensaje que contiene
`falta` y el nombre de cada sección que falta tal como está en la lista, y
**no** lanza Codex, no escribe ningún evento en `events.jsonl` ni añade nada
al historial. Pasa igual con `--dry-run`. Al continuar un hilo (`ask` con
`-m`/`-f` y un hilo ya abierto, sin `--new`) no se comprueba. Un rol sin
`brief_sections` no comprueba nada (como hoy).

### C4. Documentación

`titan-team doc` (que imprime `crates/titan-team/FORMAT.md`) nombra
`build_cmd` y `brief_sections`.

### C5. Fuera de los tests (contexto)

El kit de demo (`scripts/demo/demo.sh`) reintenta las capturas rechazadas por
el foco y su HOME inventado deja de llamarse `home/`. Es un script de shell
contra KWin: no lleva tests de aceptación.

