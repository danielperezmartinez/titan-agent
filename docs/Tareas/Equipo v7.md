---
Nombre: Equipo v7
Estado: En curso
Resumen: 'Corrige las carencias del equipo v6 anotadas en la 0.7.0/0.7.1: comprobación `build` obligatoria tras el turno de Sol sobre un esqueleto que compila (un test que no compila sale en rojo), secciones obligatorias del encargo (`brief_sections`: contrato, firmas y criterios visuales antes de implementar), pasos de la prueba a mano siempre en el mensaje al usuario, contrato sin «la misma regla que X» sin firma y con rutas según la plataforma, `shots` que reintenta las capturas rechazadas por el foco y HOME de la demo sin falso positivo de gitleaks. Deja la configuración v7 como la vigente.'
Decisiones: 'Encargo de la sesión de control control-3b (2026-11-18); al cerrar la 0.7 añadió las carencias 8 y 9. Versión propuesta 0.7.2 (patch), pendiente del usuario. Revisión de Sol: ok, sin hallazgos.'
Bloqueada: []
Fecha de creación: 2026-11-18T22:05:00+01:00
Última modificación: 2026-11-18T22:35:00+01:00
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
| C9. «Publicar versión» lanzado justo tras integrar, con el CI de `main` en marcha: se negó y llegó un correo de «build failed» | A7: la skill y el README piden esperar al CI de `main` en verde, y el workflow espera (40 min como mucho) a los checks que aún corren en vez de fallar |

C1 (guion de teclas a medio escribir) ya se corrigió en el kit durante la
0.7.0. C8 y C9 las añadió la sesión de control al cerrar la 0.7 (nota cerrada
en `main`, 256d8ba).

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

## Configuración v7 (vigente)

Sustituye a la v6 de [[Equipo v6]]. El reparto no cambia; lo nuevo va
marcado:

1. El director (Opus) escribe la especificación con el contrato numerado,
   las firmas que usan los tests y la ruta exacta de todo fichero o recurso
   que nombre, con la lógica de interacción en modelos puros. **Lleva los
   títulos «Contrato», «Firmas» y «Criterios visuales»** (o «Sin interfaz»):
   sin ellos titan-team no lanza a Sol (`brief_sections`), así que los
   criterios visuales se escriben antes de implementar (nuevo). **Ningún
   apartado remite a otra regla sin su firma, y las rutas y formatos se
   describen por su significado, válidos en Linux y Windows** (nuevo). En las
   tareas sobre el propio equipo dirige con titan-team fijado y, si añade
   claves a `team.toml`, fija su commit en verde antes de usarlas (nuevo).
2. **Esqueleto que compila** (nuevo): antes de Sol, las firmas del contrato
   con cuerpos `todo!()`, en la rama de la que sale su encargo.
3. Sol (gpt-6.1-sol, medium) escribe pocos tests de aceptación, al menos uno
   por apartado. Tras su turno, titan-team formatea, **compila (`build`,
   obligatoria: un test que no compila deja el turno en rojo)** y ejecuta los
   tests (informativo) (nuevo).
4. El director implementa cada parte con sus tests; movimientos de código con
   `scripts/mover-lineas.sh`; git de una orden por llamada y mensajes por
   fichero.
5. Luna (gpt-6-luna) solo para ediciones repetitivas; su turno pasa la
   compuerta o cuenta como fallido.
6. Una revisión final de Sol (high) en un hilo nuevo, con la compuerta, sin
   evaluar lo visual; **comprueba también que no quede ningún `todo!()` del
   esqueleto** (nuevo).
7. Evaluador visual: un subagente de Opus nuevo en cada ronda (máximo 2), con
   el kit de demo; **`shot` y `shots` reintentan las capturas rechazadas por
   el foco** (nuevo).
8. **La prueba a mano va completa en el mensaje al usuario** (la orden, los
   pasos numerados y el resultado esperado de cada uno), además de en la
   nota (nuevo).
9. **«Publicar versión» se lanza con el CI de `main` en verde** sobre el
   commit integrado; el workflow espera a un CI que aún corre (nuevo).

Fuente de verdad del reparto: `.agents/team.toml` y la skill `equipo`.

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
  configuración: `titan-team check` sale con 2 (como hoy) y el error tiene
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
- Errores de `titan-team check` (sale con 2, como hoy):
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


## Tests de aceptación (Sol)

Encargo `equipo-v7`, `crates/titan-team/tests/equipo_v7_acceptance.rs`, 7
tests (C1–C4; C5 sin test, por KWin). Commits `656239b` y `c8d6ce5` en la
rama (turnos 1 y 2), sin tocar después. El turno 2 corrigió un código de
salida que el contrato daba mal (ver «Carencias nuevas», 1). Ese turno ya
corrió con la v7: `fmt ok · build ok · test exit 101` (sin implementación en
su worktree), turno en verde.

## Revisión de Sol (high, con `--gate`)

Hilo nuevo (`equipo-v7-revision`) sobre `8763dd9`, con la especificación, lo
demás del diff y la skill tras el cambio, y titan-team fijado en el commit de
la implementación (`bin/team pin HEAD`). Compuerta en verde. Veredicto
**`ok`, sin hallazgos ni preguntas**.

## Verificación

Rama `feat/equipo-v7` (sobre `main` 256d8ba), en local el 2026-11-18:

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` y `cargo test --workspace` en verde: **230 tests**, con los 7 de
  aceptación de Sol y 2 propios nuevos (títulos del encargo en `ask.rs`;
  `checks_required` en lista, `build_cmd` y `brief_sections` en
  `config/tests.rs`). Hooks de gitleaks en cada commit.
- `bin/team check` valida el `team.toml` v7; el titan-team de `main` lo
  rechaza (claves nuevas), como se esperaba.
- Kit de demo (`demo.sh shots inicio borrador-proyecto
  resumen-sin-directora`, PID comprobado por `shot`, datos inventados): las
  tres capturas salen con el HOME `casa/` (el borrador muestra
  `cd ~/dev/recetas`) y no queda ningún proceso de la demo.
  `demo.sh borrador-proyecto` ya no lleva `gitleaks:allow` y gitleaks pasa.
- Reintento de `shot`: la función `shot_retry` probada con un `shot` falso
  (0–3 rechazos por foco y fallos de otro tipo): reintenta solo los de foco,
  como mucho `TITAN_DEMO_RETRIES` veces.
- Espera de «Publicar versión»: el bucle probado con un `gh` falso (checks
  en verde, uno que aún no existe y luego corre y acaba en verde, y uno
  `skipped`, que falla). No se ha ejecutado en GitHub: se verá en la
  primera publicación tras integrar.
- Versión propuesta: **0.7.2** (patch), pendiente del acuerdo del usuario.
- Sin push ni PR: pendientes de la autorización del usuario.

## Métricas

- **Tiempo**: de 21:50 a 22:35 del 2026-11-18, unos **45 min**, sin
  esperas al usuario (la versión y la PR quedan para él).
- **Uso de Claude** (transcript, Opus 5.5) hasta la revisión: unas 90
  respuestas y 100 llamadas a herramientas, unos **65 k tokens de salida,
  0,2 M de escritura en caché y 15 M de lectura de caché**. Sin subagentes:
  no hay cambios en la ventana (las capturas, solo para comprobar el kit).
- **Codex**: 3 llamadas (más una que el titan-team fijado rechazó sin lanzar
  nada), 0,74 M de entrada (0,64 M en caché) y 13,4 k de salida.

  | Encargo | Rol | Entrada (caché) | Salida | Tiempo |
  | --- | --- | --- | --- | --- |
  | `equipo-v7`, turno 1 | Sol, tests | 70 k (58 k) | 5,0 k | 113 s (+700 s de sus `checks`, compilación en frío) |
  | `equipo-v7`, turno 2 | Sol, corrección | 188 k (175 k) | 5,3 k | 26 s (+19 s de `checks`) |
  | `equipo-v7-revision` | Sol, revisión high con `--gate` | 480 k (402 k) | 3,1 k | 103 s |

- **Luna**: no se usó; no hubo ediciones repetitivas.
- **Hallazgos**: ninguno.

## Carencias nuevas encontradas

1. **El contrato dio un código de salida de memoria**: decía que `titan-team
   check` sale con 1 con una configuración inválida; sale con 2. Sol lo
   preguntó y costó un segundo turno corto. *Corregido en la skill*: los
   códigos de salida y mensajes de lo que ya existe se copian del código o
   de `FORMAT.md`, como las firmas.
2. **El titan-team fijado rechaza el `team.toml` con claves nuevas**: la
   segunda llamada a Sol salió con 2 sin lanzar nada (no gastó cuota).
   *Corregido en la skill*: cambiar `team.toml` cuando ya no se necesite el
   binario fijado, o fijar el propio commit en verde (`bin/team pin HEAD`).
   *Propuesta*: que `bin/team pin` ejecute `check` con el binario recién
   fijado y avise si no valida el `team.toml` actual.
3. **Los primeros `checks` de Sol tardaron 700 s**: la carpeta de
   compilación del equipo (`.agents/target`) es de cada checkout y la de
   este worktree estaba fría. Es el coste esperado de la v5.1; el resto de
   la ronda ya fue en caliente. *Propuesta*: ninguna por ahora; anotarlo
   si se repite.
4. **La espera del workflow de publicación no tiene test automático** (es
   shell dentro del YAML). *Propuesta*: si vuelve a cambiar, sacarla a
   `scripts/` con un test con un `gh` falso.

## Resultado

Configuración v7 vigente (sección «Configuración v7»), en la rama
`feat/equipo-v7`: comprobación `build` y `checks_required` en lista,
`brief_sections` en titan-team (con `FORMAT.md`, la plantilla y ADR-0010),
el `team.toml`, la skill y los prompts de Sol y del revisor de la v7,
`shot`/`shots` que reintentan las capturas rechazadas por el foco, el HOME
`casa/` de la demo y la espera al CI en «Publicar versión». La 0.8.0
([[Chat de sesión con Codex y Claude Code]]) queda preparada para el equipo
v7. Pendiente: acordar la versión (propuesta 0.7.2) y la PR.
