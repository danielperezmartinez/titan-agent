---
Nombre: Versión 0.7.0 - Proyectos importados y monitor por sesión directora
Estado: En curso
Resumen: 'Dos PR para el equipo, en orden. A: la vista «conversación» (borrador con el campo de texto común, que sustituye a «Nueva sesión»; sesión con su barra y el terminal) y el monitor por sesión directora con `[m]`. B: proyectos importados en el daemon (ADR-0014) con `+` por proyecto, grupo «otras carpetas» y resumen del proyecto. Después de la ronda «Equipo v6». El chat va después, en la 0.8.0 (tarea «Chat de sesión con Codex y Claude Code»).'
Decisiones: 'Diseño con el usuario el 2026-11-17 (sesión «chat-sesion», encargo de control-3b): 0.7.0 = estructura (proyectos, conversación sin chat, monitor por directora) y chat en 0.8.0; lista de proyectos en el daemon; sesiones de carpetas no importadas en «otras carpetas»; la cabecera del proyecto abre un resumen; para elegir proyecto en el borrador basta el selector [d]. ADR-0014 y la decisión visual de la ventana aceptadas por el usuario el 2026-11-18, tras ver la previa. Versión de cada PR: se acuerda en ella (propuesta: A 0.7.0 y B 0.7.1, y una release cuando estén las dos). PR A: 0.7.0 (minor), acordada con el usuario el 2026-11-18.'
Bloqueada: []
Fecha de creación: 2026-11-17T23:45:00+01:00
Última modificación: 2026-11-18T02:40:00+01:00
---

# Versión 0.7.0 - Proyectos importados y monitor por sesión directora

## Objetivo

Estos son los comentarios del usuario al usar la 0.6.0:

- proyectos como en Claude Code Desktop (importar, verlos sin sesiones,
  quitar, `+` por proyecto);
- el monitor por sesión directora y no por proyecto, porque con dos equipos
  en un proyecto no se sabe qué grafo se ve;
- la base de la vista «conversación», que el chat de la 0.8.0 llenará.

Las decisiones están en:

- [[Decisiones/ADR-0014 Proyectos importados en el daemon|ADR-0014]];
- [[Decisiones visuales/Ventana principal con proyectos importados y monitor por sesión directora]];
- [[Decisiones visuales/Conversación de sesión y campo de texto común]].

Previas:

- `design/preview/Proyectos.dc.html`, en la rama `feat/v0.7-proyectos`, que
  es de donde salen las dos PR;
- `design/preview/Conversacion.dc.html`, del borrador y del campo, que está
  en `feat/chat-sesion`. La PR A la trae con `git cherry-pick` de su commit
  («Add the session conversation preview»).

## Plan y orden

| Orden | Trabajo | Depende de | Versión (propuesta) |
| --- | --- | --- | --- |
| 0 | Hecha (PR #12, 0.6.1). Ronda [[Equipo v6]]: corrige las carencias de [[Versión 0.6.0 - Instalador y actualización automática]] → «Carencias del equipo», con las 7 a 9 añadidas para esta versión | — | la suya |
| 1 | **PR A**: conversación y monitor por sesión directora | 0 | 0.7.0 |
| 2 | **PR B**: proyectos importados y resumen del proyecto | A | 0.7.1 |
| 3 | Release 0.7.x, con autorización del usuario | A y B | — |
| 4 | [[Chat de sesión con Codex y Claude Code]] (0.8.0, tres PR) | A (y B para la estructura final) | 0.8.x |

- Cada PR la hace el [[Equipo v6]] con su encargo (skill `equipo`): un
  contrato numerado con firmas y rutas, tests de aceptación de Sol,
  revisión de Sol, evaluador visual con el kit de `scripts/demo/` y una
  prueba manual del usuario con pasos numerados.
- **Lo que la v6 trae para esta versión**:
  - la lógica de interacción nueva (edición del campo, reglas del árbol con
    dos grupos, qué encargos tiene una directora) va en modelos puros con
    firmas en el contrato, para que Sol la pruebe; la vista solo pinta;
  - escenas con texto escrito por el guion de teclas
    (`TITAN_AGENT_DEMO_KEYS`, p. ej. `nueva-teclas`), con otro tamaño de
    ventana (`TITAN_AGENT_DEMO_SIZE`) y un `projects.json` de demo
    (proyecto sin sesiones, carpeta borrada, `web-notas` sin importar) que
    lee el daemon de la demo cuando B lo implemente;
  - ninguna PR de esta versión cambia titan-team, así que no hace falta
    `bin/team pin` (sí en la 0.8.0, que lo reescribe).
- B empieza cuando A está integrada (rebase sobre `main`).

## PR A · Conversación y monitor por sesión directora

1. **Campo de texto común** (`app/composer.rs`):
   - las teclas de la decisión visual: Enter envía o lanza, Shift+Enter
     hace salto de línea, Tab, listas «- » y su continuación, ↑ para
     recuperar, borrador por sesión, Esc y crecer hasta 8 líneas;
   - la lógica de edición va en un **modelo puro con tests**. GPUI no tiene
     tests de ventana, así que la vista solo pinta;
   - el menú `/` y `@` no entra aquí: es de la 0.8.0, porque necesita el
     daemon del chat.
2. **Vista «conversación»** (sustituye a `app/new_session.rs`):
   - **borrador**: ajustes, comando exacto y el campo; Enter lanza con las
     salvaguardas de ADR-0002;
   - **sesión**: barra de sesión y el terminal de hoy en el cuerpo, con el
     panel de detalle;
   - al lanzar, pasa de borrador a sesión sin cambiar de pantalla;
   - `ctrl+n` y `[+] Nueva sesión` abren el borrador.
3. **Monitor por sesión directora**:
   - en el núcleo, una función que da los encargos abiertos de una sesión
     (regla de ADR-0010 al revés: `director_session` o la regla de la
     carpeta base) y otra que dice si una sesión es directora;
   - en la interfaz, `[m] monitor` en la barra de una directora, el
     selector de encargo si hay más de uno, la miga
     `[#] proyecto › directora` y el `[*]` en la fila de la directora en el
     árbol;
   - mientras no llegue B, la cabecera del proyecto sigue abriendo el
     monitor del encargo más reciente, como hoy.
4. **Escenas del kit de demo**:
   - borrador con texto escrito y una lista;
   - sesión directora con `[m]` y monitor con dos encargos;
   - dos equipos en el mismo proyecto.

## PR B · Proyectos importados y resumen del proyecto

1. **Daemon** (ADR-0014):
   - `projects.json` v1, con lectura tolerante y escritura atómica;
   - la importación la primera vez y al lanzar en una carpeta nueva;
   - `project_add` y `project_remove`, y el campo `projects` del evento
     `sessions`;
   - las raíces importadas como carpetas candidatas de los encargos;
   - ampliación anotada en ADR-0007 y ADR-0010 en el mismo cambio.
2. **Núcleo**: el árbol con proyectos importados (con y sin sesiones, orden
   por actividad, carpeta no encontrada) y el grupo «otras carpetas»; el
   filtro igual en los dos.
3. **Interfaz**:
   - el título `PROYECTOS` con `[+] importar` (selector `[d]`);
   - el `+` por proyecto, que abre el borrador con el proyecto puesto;
   - el grupo «otras carpetas» con `[+] importar`;
   - el **resumen del proyecto** al pulsar su cabecera: acciones, equipos con
     `[m]` y sesiones. Sustituye al monitor por proyecto.
4. **Escenas del kit de demo**: un `projects.json` de demo con un proyecto
   sin sesiones, uno con la carpeta borrada, sesiones en otras carpetas y el
   resumen de un proyecto con dos equipos.

## PR A · Contrato (para los tests de aceptación)

### A.C1. Regla de la sesión directora (núcleo)

Módulo `titan_agent_core::monitor` (`crates/titan-agent-core/src/monitor.rs`).

```rust
/// One open team task, by its team folder and name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId { pub folder: String, pub task: String }

pub fn director_of<'a>(task: &TeamTask, run: &TeamRun, sessions: &'a [Session]) -> Option<&'a Session>;
pub fn directors(teams: &[TeamTask], sessions: &[Session]) -> std::collections::BTreeMap<String, Vec<TaskId>>;
```

**`director_of`**: la sesión que dirige el encargo (regla de ADR-0010, la que
ya usa el monitor):

1. Si `run.director` es `None` (equipo sin rol director): `None`.
2. Si `run.director_session` es `Some(id)` y hay en `sessions` una sesión con
   `key == format!("cl:{id}")`: esa.
3. Si no (sin `director_session`, o no está en la lista): entre las sesiones
   con `agent == AgentCli::ClaudeCode` y `cwd` igual a `task.folder` (sin
   contar las barras `/` finales de ninguno de los dos), la de menor rango
   de estado: `NeedsYou` (0) < `Working` (1) < `Waiting` (2) < `Finished`
   (3); a igual rango, la de `state_since_ms` mayor (`None` cuenta como 0);
   y a igualdad total, la primera en el orden de `sessions`. Ninguna: `None`.

**`directors`**: para cada encargo de `teams` cuyo resumen
`run_of(task)` existe (hay `task_created`) y **no está cerrado**
(`run.closed == false`, es decir, sin `task_closed`), su directora
(`director_of(task, &run, sessions)`). Devuelve un mapa clave de la sesión
directora (`Session::key`) → sus encargos como
`TaskId { folder: task.folder.clone(), task: task.task.clone() }`, **del más
reciente al más antiguo** por `run.last_ts_ms` (la marca del último evento del
fichero en orden), y a igualdad por nombre de encargo ascendente. Una sesión sin
encargos abiertos no aparece. Un encargo sin directora no aparece.

El monitor (`board`) usa la misma regla, así que un encargo y su directora son
los mismos en el monitor, en `[m]` y en el `[*]` del árbol.

### A.C2. Cuerpo de una sesión: terminal o monitor (interfaz, puro)

Módulo `titan_agent::conversation` (`crates/titan-agent/src/conversation.rs`).
`tasks` es siempre la lista de encargos abiertos de esa sesión tal como la da
`directors` (más reciente primero; vacía si no es directora).

```rust
use titan_agent_core::monitor::TaskId;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SessionBody {
    #[default]
    Terminal,
    Monitor { task: TaskId },
}

pub fn toggle_monitor(body: &SessionBody, tasks: &[TaskId]) -> SessionBody;
pub fn pick_task(body: &SessionBody, task: &TaskId, tasks: &[TaskId]) -> SessionBody;
pub fn settle(body: &SessionBody, tasks: &[TaskId]) -> SessionBody;
```

- **`toggle_monitor`** (`[m]`): `Terminal` con `tasks` no vacía →
  `Monitor { task: tasks[0] }`; `Terminal` con `tasks` vacía → `Terminal` (una
  sesión que no dirige nada no tiene monitor); `Monitor { .. }` → `Terminal`.
- **`pick_task`** (selector de encargo): si `task` está en `tasks` →
  `Monitor { task }` (aunque el cuerpo fuera `Terminal`); si no, devuelve
  `body` sin cambios.
- **`settle`** (tras cada snapshot del daemon): `Terminal` → `Terminal`;
  `Monitor { task }` con `task` en `tasks` → igual; con `task` fuera de
  `tasks` y `tasks` no vacía → `Monitor { task: tasks[0] }`; con `tasks`
  vacía → `Terminal`.

### A.C3. Teclas del campo de texto (interfaz, puro)

Módulo `titan_agent::composer` (`crates/titan-agent/src/composer.rs`).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKey { Enter, ShiftEnter, Tab, ShiftTab, Up, Escape }

pub fn edit_key(keystroke: &gpui::Keystroke) -> Option<EditKey>;
```

`gpui::Keystroke` es el de GPUI (dependencia `gpui` del crate, disponible en
sus tests):

```rust
pub struct Keystroke { pub modifiers: Modifiers, pub key: String, pub key_char: Option<String> }
pub struct Modifiers { pub control: bool, pub alt: bool, pub shift: bool, pub platform: bool, pub function: bool }
// Keystroke y Modifiers implementan Default.
```

Según `key` y los modificadores (`function` no cuenta; `key_char` no cuenta):

| `key` | modificadores | resultado |
| --- | --- | --- |
| `enter` | ninguno, o `control` y/o `platform` (sin `shift` ni `alt`) | `Some(Enter)` |
| `enter` | solo `shift` | `Some(ShiftEnter)` |
| `tab` | ninguno | `Some(Tab)` |
| `tab` | solo `shift` | `Some(ShiftTab)` |
| `up` | ninguno | `Some(Up)` |
| `escape` | ninguno | `Some(Escape)` |
| cualquier otro caso (otras teclas, otras combinaciones, p. ej. `alt-enter`, `ctrl-shift-enter`, `shift-up`) | | `None` |

### A.C4. Modelo de edición del campo (interfaz, puro)

Mismo módulo `titan_agent::composer`. `ComposerText` es el modelo de texto que
ya existía para el campo de «Nueva sesión» (antes privado), ahora público,
con un método nuevo `press`:

```rust
/// Text and selection; offsets are UTF-8 byte offsets on char boundaries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComposerText {
    pub text: String,
    pub selection: std::ops::Range<usize>,
    pub reversed: bool,
    /// IME composition in progress.
    pub marked: Option<std::ops::Range<usize>>,
}

impl ComposerText {
    pub fn cursor(&self) -> usize;              // selection.start si reversed, si no selection.end
    pub fn move_to(&mut self, offset: usize);   // selección vacía en offset (acotado a text.len())
    pub fn replace(&mut self, range: Option<std::ops::Range<usize>>, new: &str); // como teclear: sustituye range (o la selección) y deja el cursor tras lo insertado
    pub fn press(&mut self, key: EditKey, last_sent: Option<&str>) -> Effect;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// The key did nothing (the text and the selection are unchanged).
    Nothing,
    /// The text or the selection changed.
    Edited,
    /// Enter: the view launches (or sends) this text. The model does not clear it.
    Submit(String),
    /// Escape without a selection: the field gives up the focus.
    Blur,
    /// Not the model's: the view handles the key itself (the text is unchanged).
    Unhandled,
}
```

Definiciones: la **línea** del cursor es la línea (separada por `\n`) que
contiene `cursor()`. Una línea es un **elemento de lista** si empieza por cero
o más tabuladores `\t` (su **sangría**) seguidos de `- ` (guion y espacio); su
**texto** es lo que sigue a `- `. «Selección vacía» es
`selection.start == selection.end`. Tras un `Edited`, la selección queda vacía
en el cursor que se indica, `reversed == false` y `marked == None`.

`press(key, last_sent)`:

- **`Enter`**:
  - con `marked` en curso (`Some`) → `Unhandled` (la composición IME la cierra
    la plataforma);
  - si `text.trim()` está vacío → `Nothing`;
  - si no → `Submit(text.clone())`, sin cambiar el texto ni la selección.
- **`ShiftEnter`**: primero, si la selección no está vacía, se borra (como al
  teclear). Luego, con el cursor `c`:
  - si la línea es un elemento de lista **con texto vacío** (la línea es
    exactamente sangría + `- `) y `c` está al final de la línea → la línea
    entera (sangría incluida) pasa a ser vacía; el cursor queda donde empezaba
    la línea; no se inserta salto. `Edited`;
  - si la línea es un elemento de lista y `c` está en o después del final de
    su `- ` → se inserta `"\n" + sangría + "- "` en `c`; el cursor queda tras
    lo insertado. `Edited`;
  - en otro caso → se inserta `"\n"` en `c`. `Edited`.
- **`Tab`**:
  - con selección vacía y la línea siendo un elemento de lista → se inserta
    `\t` al principio de la línea; el cursor avanza 1 (sigue en el mismo
    sitio del texto). `Edited`;
  - en otro caso → la selección (o nada) se sustituye por `\t`; cursor tras él.
    `Edited`.
- **`ShiftTab`**: con selección vacía y la línea siendo un elemento de lista
  con sangría (al menos un `\t`) → se quita el primer `\t` de la línea; el
  cursor retrocede 1 (sin pasar del principio de la línea). `Edited`. En
  cualquier otro caso → `Nothing`.
- **`Up`**: si `text` está vacío y `last_sent` es `Some(s)` con `s` no vacío →
  `text = s`, cursor al final. `Edited`. En otro caso → `Unhandled` (la vista
  sube una línea visual).
- **`Escape`**: con selección no vacía → la selección pasa a vacía en
  `cursor()`. `Edited`. Con selección vacía → `Blur`.

### A.C5. Tamaño, pegado y borradores del campo (interfaz, puro)

Mismo módulo `titan_agent::composer`.

```rust
pub const MIN_ROWS: usize = 2;
pub const MAX_ROWS: usize = 8;
pub fn rows_shown(visual_lines: usize) -> usize;
pub fn normalize_paste(text: &str) -> String;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drafts { /* privado */ }
impl Drafts {
    pub fn save(&mut self, key: &str, text: &str);
    pub fn draft(&self, key: &str) -> &str;
    pub fn record_sent(&mut self, key: &str, text: &str);
    pub fn last_sent(&self, key: &str) -> Option<&str>;
}
```

- `rows_shown(n)` = `n` acotado a `MIN_ROWS..=MAX_ROWS` (0 y 1 → 2; 5 → 5;
  9 o más → 8).
- `normalize_paste`: cada `"\r\n"` y cada `"\r"` suelto pasan a `"\n"`; nada
  más cambia (los `\t`, los espacios y los saltos se conservan).
- `Drafts` guarda un borrador por clave de conversación (una cadena
  cualquiera: clave de sesión o la del borrador de «nueva sesión»):
  - `save(k, t)`: el borrador de `k` pasa a ser `t`; `save(k, "")` lo quita;
  - `draft(k)`: el último guardado para `k`, o `""` si no hay;
  - `record_sent(k, t)`: `t` pasa a ser el último enviado de `k` **y** el
    borrador de `k` se vacía;
  - `last_sent(k)`: el último `t` de `record_sent(k, t)`, o `None`.
  - Las claves no se mezclan entre sí.

Firmas completas y formato de los eventos: especificación del encargo `v07a` (fuera del repo); los tests van en `crates/titan-agent-core/tests/director_acceptance.rs`, `crates/titan-agent/tests/conversation_acceptance.rs` y `crates/titan-agent/tests/composer_acceptance.rs`.

## PR A · Criterios visuales (evaluador)

Frente a `design/preview/Conversacion.dc.html` (estado 1, borrador) y
`design/preview/Proyectos.dc.html` (estado 2, monitor desde la directora), y
las decisiones visuales de la conversación y de la ventana. No se cambian para
que el resultado pase.

1. **Borrador**: barra superior `[+] Nueva sesión en [#] proyecto` con
   `[esc] cerrar` a la derecha; zona vacía «la conversación empieza aquí» en
   `mute`; abajo, en este orden, los chips de ajustes, la línea del comando
   exacto (`$ …` en `mute`) y el campo; nada centrado en mitad de la pantalla
   como el cuadro antiguo.
2. **Campo**: fondo `surface`, borde `hairline` (`accent` con el foco), radio
   4 px; texto de varias líneas con la lista «- » alineada; ayuda en `stone`
   «enter lanzar · shift+enter nueva línea · «- » lista»; crece con el texto
   sin pasar de 8 líneas.
3. **Barra de sesión**: nombre en `ink` negrita, estado con su color, agente y
   modelo con el color del agente, carpeta en `mute`; a la derecha `[m]
   monitor` (solo en una directora) y `[t] terminal`, con el activo resaltado
   (borde `accent`); misma altura que la barra del monitor.
4. **Monitor desde la directora**: miga `[#] proyecto › directora`; con dos
   encargos, selector `encargo nombre [v]` (y su menú en
   `surface-elevated`/`hairline-strong`); `[<] conversación` para volver; las
   tres vistas sin cambios.
5. **Árbol**: `[*]` en `success` junto al nombre de cada sesión directora (dos
   directoras en el mismo proyecto → dos `[*]`), y ya no en la cabecera del
   proyecto; ninguna fila se rompe a 1000 px de ancho.

## PR A · Revisión de Sol (high, con `--gate`)

Hilo nuevo (`v07a-revision`) sobre el commit `1338744`, con la
especificación y la compuerta (en verde). Veredicto `changes_requested`:
C1–C5 bien en los modelos puros; fallos en la integración y en el kit.

| Hallazgo | Severidad | Decisión |
| --- | --- | --- |
| `lanzar.py` (el `claude --bg` falso) escribía el temporal siguiendo un enlace que ya estuviera | mayor | Aceptado: temporal exclusivo con `mkstemp`, rutas comprobadas dentro de `TITAN_DEMO_DIR` y probado con un enlace hacia fuera |
| El comando mostrado abreviaba el encargo (como la previa), y no era el que se lanza | mayor | Aceptado: ADR-0002 manda sobre la previa; se muestra el comando exacto, con el marcador solo con el campo vacío |
| Lanzar desde el monitor de una directora dejaba el cuerpo en «monitor» en la sesión provisional | mayor | Aceptado: el cuerpo vuelve al terminal y el selector se cierra en toda entrada a una sesión. No se extrae a un modelo: son dos asignaciones en el único sitio que no pasa por `select_session` |
| `Blurred` no pedía repintar: el foco cambiaba en el siguiente refresco | menor | Aceptado: `cx.notify()` |
| El texto del selector de encargos estaba en la vista y contaba mal la ronda antes de empezar | menor | Aceptado: `titan_agent::conversation::task_hint`, con tests; cuenta como el monitor (`ronda 1 de N` antes de la primera) |

## PR A · Evaluador visual

**Ronda 1** (subagente de Opus nuevo, 13 capturas): borrador 8, campo 6,
barra de sesión 8, monitor 6, árbol 10. Correcciones aplicadas:

- la miga decía `titan-ssh › titan-ss…` en las dos directoras: el nombre va
  sin el prefijo del proyecto (`crumb_name`, puro y con test) y tiene
  prioridad de ancho; cede antes la ronda y luego el selector;
- los menús (encargos y ajustes del borrador) empujaban el contenido: ahora
  flotan sobre él;
- chips con su etiqueta (`esfuerzo high`, `permisos manual`), `[+]` del
  borrador en `ink`, barra del borrador de 40 px como la de sesión, zona
  vacía en `mute` y el selector sin caja cuando solo hay un encargo;
- «modelo ?» tras lanzar en la demo: el `claude --bg` falso no escribía
  transcripción; ahora sí (con el modelo elegido);
- capturas del guion a medio escribir: la app escribe en `ui.log` cuándo
  acaba el guion y `shot` lo espera (carencia 1, resuelta en el kit).

**Ronda 2** (otro subagente nuevo): borrador 9, campo 9, barra de sesión 9,
monitor 6, árbol 10. El 6 del monitor salía de una captura de
`directora-monitor` anterior al último ajuste de la miga (solo se habían
repetido las otras dos); repetida, la miga sale entera y la ronda con «…»,
igual con el selector abierto y cerrado. Aplicado además: el texto de la
conversación vacía, centrado en su zona. Aplazado, sin más rondas (máximo 2):
anclar el menú de los chips al chip que lo abre (hoy flota sobre el borde
izquierdo de la fila) y las capturas del campo sin foco y con más de 8 líneas,
que quedan para la prueba a mano (pasos 6 y 7).

Fuera de los criterios y no aplicado: el panel de detalle corta una línea sin
«…» (ya pasaba en la 0.6) y el texto del error de la escena `nueva-error` (es
el dato inventado de la escena).

## PR A · Prueba a mano

Qué se prueba: lo que el evaluador visual no puede ver, que es el campo de
texto con el teclado (Enter, Shift+Enter, listas, Tab, ↑ y Esc), el paso de
borrador a sesión al lanzar y el `[m]` de cada sesión directora con dos
equipos en el mismo proyecto. Todo con datos inventados, un daemon privado y
un `claude` falso: no se lanza ningún CLI de verdad ni se toca la app
instalada.

Orden (desde la carpeta del repo, con la rama de la PR compilada:
`cargo build -p titan-agent -p titan-agentd`):

```
scripts/demo/demo.sh probar nueva
```

1. Se abre la ventana en el borrador → barra «[+] Nueva sesión en [#]
   titan-ssh» con «[esc] cerrar» a la derecha. Abajo están los ajustes, la
   línea «$ cd … claude --bg …» y el campo con un encargo de ejemplo.
2. Clic al final del texto del campo, Shift+Enter y escribe «- primero» →
   hay un salto de línea y no se lanza nada.
3. Shift+Enter → la línea nueva empieza ya por «- ».
4. Tab → ese elemento se sangra. Shift+Tab → vuelve a su sitio.
5. Escribe «segundo» y pulsa Shift+Enter dos veces → la segunda vez se va la
   viñeta vacía y se acaba la lista (queda una línea vacía, sin «- »).
6. Sigue metiendo líneas con Shift+Enter hasta pasar de 8 → el campo crece
   hasta 8 líneas y luego desplaza (la rueda del ratón lo mueve).
7. Selecciona una palabra con el ratón y pulsa Esc → se quita la selección.
   Esc otra vez → el campo pierde el foco (sin cursor y con el borde gris).
   Esc una tercera vez → se cierra el borrador y vuelve lo que había antes.
8. ctrl+n → vuelve el borrador con tu texto. Enter → se lanza: sin cambiar
   de pantalla, la vista pasa a la sesión nueva («[>] trabajando», terminal
   del `claude` falso), que aparece en el árbol bajo titan-ssh.
9. ctrl+n → el borrador está vacío. ↑ → aparece el encargo que acabas de
   lanzar. Esc dos veces para cerrarlo sin lanzar.
10. Clic en «titan-ssh: sftp renombrar» (lleva `[*]` en el árbol) → barra con
    `[m] monitor` y `[t] terminal` (este resaltado) y su terminal debajo.
11. Clic en `[m] monitor` → el terminal se cambia por el monitor: miga
    «[#] titan-ssh › titan-ssh: sftp renombrar», selector «encargo
    sftp-renombrar [v]», «ronda 1 de 1», `[*] en vivo`, las tres vistas y
    `[<] conversación`. `[m]` queda resaltado.
12. Clic en el selector → menú con sftp-renombrar y sftp-enlaces. Elige
    sftp-enlaces → el monitor pasa a ese encargo (revisor «[w]» o «hilo
    ocupado»).
13. Clic en `[<] conversación` → vuelve el terminal. `[m]` otra vez y luego
    `[t] terminal` → también vuelve.
14. Clic en «titan-ssh: sincronizar hosts» (el otro `[*]` del mismo
    proyecto) y en `[m]` → su propio monitor, el encargo
    sincronizar-hosts con Sol escribiendo tests, sin `[v]` (solo lleva uno).
15. En ese monitor, clic en el nodo de opus (la directora) → vuelve a su
    conversación, con el terminal.
16. Clic en «repasar el changelog» → su barra no tiene `[m]` (no dirige
    nada).
17. Clic en la cabecera «[#] titan-ssh» del árbol → el monitor del proyecto
    como en la 0.6 (el encargo más reciente). Cambia en la PR B.
18. Cierra la ventana → la demo se para sola.

## PR B · Contrato (para los tests de aceptación)

### B.C1. Fichero `projects.json` (núcleo, puro)

Módulo nuevo `titan_agent_core::projects` (`crates/titan-agent-core/src/projects.rs`).

```rust
pub const PROJECTS_FILE: &str = "projects.json";
pub const PROJECTS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImportedProject { pub root: String, pub added_ms: i64 }

pub fn parse_projects(text: &str) -> Result<Vec<ImportedProject>, String>;
pub fn render_projects(projects: &[ImportedProject]) -> String;
pub fn add_project(projects: &mut Vec<ImportedProject>, path: &str, now_ms: i64) -> String;
pub fn remove_project(projects: &mut Vec<ImportedProject>, root: &str) -> bool;
pub fn initial_projects(sessions: &[Session], now_ms: i64) -> Vec<ImportedProject>;
```

Formato v1: `{"version": 1, "projects": [{"root": "/ruta/absoluta", "added_ms": 1763400000000}]}`.

- **`parse_projects`**: `Ok` solo si el texto es un objeto JSON con `version`
  igual a 1 y `projects` un array de objetos con `root` (cadena) y
  `added_ms` (entero). Los campos de más se ignoran. Cualquier otra cosa
  (JSON inválido, sin `version`, otra versión, `projects` ausente o que no es
  array, una entrada sin `root` o `added_ms` o con otro tipo) → `Err` con un
  texto no vacío. Cada `root` se devuelve sin las `/` finales (salvo que sea
  `/`). Si dos entradas tienen el mismo `root` (tras quitar las `/` finales),
  se queda la primera. Orden: el del fichero.
- **`render_projects`**: el JSON v1 de la lista, terminado en `\n`. Para
  toda lista `l` válida (roots sin `/` final y sin repetir),
  `parse_projects(&render_projects(&l)) == Ok(l)`.
- **`add_project`**: `root = project_root(path)` (de
  `titan_agent_core::project`: quita las `/` finales y, si la ruta está dentro
  de `/.claude/worktrees/` o `/.agents/worktrees/`, la corta antes de ese
  tramo; p. ej. `/p/.claude/worktrees/x/sub` → `/p`). Si ya hay un proyecto
  con ese `root`, no cambia nada; si no, añade
  `ImportedProject { root, added_ms: now_ms }` **al final**. Devuelve `root`
  en los dos casos.
- **`remove_project`**: quita el proyecto cuyo `root` es igual a `root` sin
  sus `/` finales. Devuelve si quitó alguno.
- **`initial_projects`**: un `ImportedProject` por cada `project_root`
  distinto de `sessions`, con `added_ms = now_ms`, ordenados por `root`
  ascendente.

### B.C2. Protocolo (núcleo)

En `titan_agent_core::protocol` (`crates/titan-agent-core/src/protocol.rs`).
Ampliación compatible: `PROTOCOL_VERSION` sigue siendo 1.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInfo { pub root: String, pub name: String, pub added_ms: i64, pub missing: bool }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAddResult { pub root: String }

// Request (serde: tag = "method", content = "params", snake_case), variantes nuevas:
ProjectAdd { path: String },     // {"id":N,"method":"project_add","params":{"path":"/a/b"}}
ProjectRemove { root: String },  // {"id":N,"method":"project_remove","params":{"root":"/a"}}

// Event::Sessions (serde: {"event":"sessions","params":{…}}) gana dos campos opcionales:
#[serde(default, skip_serializing_if = "Option::is_none")]
projects: Option<Vec<ProjectInfo>>,
#[serde(default, skip_serializing_if = "Option::is_none")]
projects_error: Option<String>,
```

- `project_add` → resultado `{"root": "<root>"}` (un `ProjectAddResult`).
  Errores (`{"id":N,"error":{"code":…,"message":…}}`): `not_a_folder` si
  `path` no es absoluta (no empieza por `/`), no existe o no es una carpeta;
  `config_error` si no se pudo escribir el fichero (o si el fichero existente
  no se podía leer: ver C3). Importar uno que ya está no es un error.
- `project_remove` → resultado `{}`. Nunca toca la carpeta. Si no estaba, no
  es un error (también `{}`). Con el fichero ilegible: `config_error`.
- En `sessions`, el daemon envía siempre `projects` (la lista, en el orden del
  fichero), con `name` = `project_name(root)` (último tramo de la ruta) y
  `missing` = la carpeta `root` no existe o no es una carpeta. Un cliente que
  recibe `projects` ausente (daemon antiguo) no sabe de proyectos importados.
- Un mensaje `sessions` antiguo sin los campos nuevos sigue decodificando
  (`projects: None`, `projects_error: None`).

### B.C3. Comportamiento del daemon

Fichero: `<carpeta de configuración>/projects.json`, donde la carpeta es
`$TITAN_AGENT_CONFIG_DIR` si está definida y no vacía (los tests la usan),
si no `$XDG_CONFIG_HOME/titan-agent`, si no `$HOME/.config/titan-agent`. Solo
el daemon lo lee y escribe; si la carpeta no existe, la crea al escribir.

1. **Arranque**: lee el fichero. Si no existe, lista vacía y **sin**
   `projects_error`.
2. **Primera vez**: si el fichero no existía al arrancar, en cuanto las dos
   fuentes de sesiones (Claude Code y Codex) han informado una vez cada una
   (con éxito o con error), escribe el fichero con
   `initial_projects(sesiones conocidas)` y lo envía en `projects`. Solo una
   vez por arranque.
3. **Ilegible** (`parse_projects` da `Err`): el fichero **no se sobrescribe
   nunca** en ese arranque (ni en la primera vez, ni en `project_add` o
   `project_remove`, que dan `config_error`); `projects` es `[]` y
   `projects_error` lleva un texto no vacío.
4. **Escritura**: a un fichero temporal en la misma carpeta y renombrado
   sobre `projects.json` (nunca queda a medias).
5. **`project_add` / `project_remove`**: C1 sobre la lista, escritura, y un
   `sessions` nuevo con la lista cambiada a todos los suscritos (si cambió).
6. **Lanzar** (`launch`) en una carpeta cuyo `project_root` no está
   importado la importa (`add_project`) cuando el lanzamiento pasa las
   salvaguardas.
7. **Equipos**: la raíz de cada proyecto importado (que exista) es también
   una carpeta candidata donde buscar `.agents/team.toml`, así que sus
   encargos abiertos llegan en `teams` aunque no haya ninguna sesión.
8. La lista sobrevive a reiniciar el daemon (se lee del fichero).

### B.C4. Árbol con proyectos importados (núcleo, puro)

En `titan_agent_core::tree` (`crates/titan-agent-core/src/tree.rs`), junto a
`build_rows` (que no cambia).

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavRow {
    Project { root: String, name: String, count: usize, imported: bool, missing: bool },
    Session { key: String, depth: usize, branch: &'static str },
    /// An imported project without sessions shown under it.
    Empty { root: String },
    /// Title of the group of projects that are not imported.
    Others,
}

pub fn project_tree(sessions: &[Session], projects: &[ProjectInfo], teams: &[TeamTask], filter: &str) -> Vec<NavRow>;
```

- Una sesión es de un proyecto importado si su `project_root` es igual a su
  `root` (sin `/` finales en ninguno de los dos).
- **Importados** primero: cada proyecto de `projects` es una fila
  `Project { imported: true, missing: <su missing>, name: <su name>, root }`
  seguida de sus filas de sesión, con las mismas reglas que `build_rows`
  (una sesión cuyo `parent` está en la lista cuelga de él con `depth + 1` y
  rama `|-` o `` `- `` (el último hijo); las de primer nivel van con
  `depth 0` y rama `""`; orden: vivas antes que terminadas, luego
  `started_at_ms` más reciente, luego nombre). `count` = número de filas
  de sesión que lleva debajo. Si no lleva ninguna, va seguida de
  `Empty { root }`.
- **Orden de los importados**: por actividad, de más reciente a más
  antigua. La actividad de un proyecto es el mayor de: `started_at_ms` y
  `state_since_ms` de sus sesiones, y el `ts_ms` del **último** evento de
  `events` de cada `TeamTask` con `project_root` igual a su `root`. Los que
  no tienen actividad van después, por `name` en minúsculas y luego `root`.
- **Otras carpetas**: si alguna sesión no es de un proyecto importado, va
  `Others` y después un grupo por `project_root` (fila
  `Project { imported: false, missing: false, name: <project de la
  sesión>, root }` y sus sesiones, como arriba), ordenados por `name` en
  minúsculas y luego `root`. Sin ninguna, no hay `Others`.
- **Filtro** (`filter`, sin distinguir mayúsculas, tras quitar espacios de
  los extremos; vacío = todo), igual en los dos grupos: un proyecto se
  muestra entero (todas sus sesiones, o `Empty`) si su `name` contiene el
  filtro; si no, se muestra solo si alguna sesión coincide (por nombre,
  agente, modelo o proyecto, como `build_rows`), con las sesiones que
  coinciden y sus antepasados; si no, no aparece. `Empty` solo aparece con
  el filtro vacío o si el nombre del proyecto coincide. `Others` solo si
  queda algún grupo.

### B.C5. Equipos de un proyecto para su resumen (núcleo, puro)

En `titan_agent_core::monitor` (junto a `board`, `director_of`, `TaskId`).

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct TeamLine {
    pub id: TaskId,
    /// Key of its director session, when known (`director_of`).
    pub director: Option<String>,
    pub state: SessionState,
    pub headline: String,
    pub round: u32,
    pub max_rounds: u32,
    pub last_ts_ms: i64,
}

pub fn project_teams(root: &str, teams: &[TeamTask], sessions: &[Session], now_ms: i64) -> Vec<TeamLine>;
```

- Una línea por encargo **abierto** (`run_of(task)` existe y no está
  cerrado) con `task.project_root` igual a `root` (sin `/` finales).
- `id = TaskId { folder: task.folder, task: task.task }`;
  `director = director_of(task, &run, sessions).map(|s| s.key.clone())`.
- Con `b = board(task, &run, sessions, now_ms)`: `state` = `NeedsYou` si
  algún agente de `b.agents` está en `NeedsYou`; si no, `Working` si alguno
  está en `Working`; si no, `Waiting`. `headline = b.headline(now_ms)`,
  `round = b.round`, `max_rounds = b.max_rounds`.
- `last_ts_ms = run.last_ts_ms`. Orden: `last_ts_ms` descendente, luego
  nombre del encargo ascendente.

### B.C6. Carpetas del borrador (interfaz, puro)

En `titan_agent::conversation` (`crates/titan-agent/src/conversation.rs`).

```rust
pub fn folder_choices(projects: &[ProjectInfo], sessions: &[Session], picked: &[String]) -> Vec<(String, String)>;
```

Las carpetas que ofrece el menú de proyecto del borrador, como
`(carpeta, nombre)`, sin repetir carpeta (comparando sin `/` finales):

1. los proyectos importados que no faltan (`missing == false`), por `name`
   en minúsculas y luego `root`;
2. los `project_root` de las sesiones que no son de un importado, con el
   `project` de la sesión como nombre, por nombre en minúsculas y luego
   carpeta;
3. las carpetas de `picked` (elegidas con `[d]`) que no estén ya, en su
   orden, con `project_name(carpeta)` como nombre.

Firmas completas, formato de los eventos y cómo se prueba el daemon: especificación del encargo `v07b` (fuera del repo). Tests en `crates/titan-agent-core/tests/projects_acceptance.rs`, `crates/titan-agentd/tests/projects_acceptance.rs` y `crates/titan-agent/tests/folders_acceptance.rs`.

## PR B · Criterios visuales (evaluador)

Frente a `design/preview/Proyectos.dc.html` (estados 1 y 3) y la decisión
visual de la ventana. Escritos al acabar la implementación (no antes, como
pide la skill: carencia 4) y no se cambian para que el resultado pase.

1. **Árbol**: título `PROYECTOS` con `[+] importar` en `accent`; cada
   importado `[#] nombre`, contador y `+` al pasar o con la cabecera
   seleccionada; sin sesiones, una línea `mute` «sin sesiones · [+] nueva»;
   carpeta borrada, `[!]` en `warning` y «[!] carpeta no encontrada · [x]
   quitar».
2. **Otras carpetas**: título `OTRAS CARPETAS` en `mute` al final; cada grupo
   con `[+] importar`; sus cabeceras más apagadas que las importadas; nada
   se rompe a 1000 px.
3. **Resumen**: `[#] nombre` (`ink`, negrita) y la carpeta en `mute`;
   acciones `[+] nueva sesión`, `[d] abrir carpeta` y `[x] quitar`
   (`danger`), o `[+] importar`; «EQUIPOS · N» con una línea por encargo
   (estado en su color, encargo, ronda, directora o «sin sesión directora»,
   actividad en `mute` y `[m] monitor`); «SESIONES · N vivas» con su estado
   y desde cuándo; una columna, sin paneles.
4. **Borrador desde el `+`**: el proyecto puesto en la barra y en el chip.
5. **Monitor sin directora**: miga `[#] proyecto › encargo`, «sin sesión
   directora» y `[<] resumen`; las tres vistas sin cambios.

## PR B · Revisión de Sol (high, con `--gate`)

Hilo nuevo (`v07b-revision`) sobre el commit `9ad078e`, con la
especificación y la compuerta (en verde). Veredicto `changes_requested`:
la protección del fichero ilegible, el renombrado atómico y la importación
al lanzar están bien; fallos en la primera importación, en el árbol y en la
vista del resumen.

| Hallazgo | Severidad | Decisión |
| --- | --- | --- |
| `first_discovery` dejaba la lista cambiada aunque no se pudiera escribir: los clientes veían proyectos sin guardar y un `project_add` posterior daba éxito sin escribir | mayor | Aceptado: la primera importación es una transacción como las demás (se queda solo si se escribió); test propio con la carpeta de configuración bloqueada |
| Un hijo cuyo padre es de otra raíz colgaba del proyecto del padre (y su propio proyecto salía sin sesiones o no salía en «otras carpetas») | mayor | Aceptado: la jerarquía se arma dentro de cada raíz; el hijo de otra raíz empieza su grupo. Test propio con la raíz importada y sin importar |
| Qué sesiones lista el resumen y a dónde lleva el `[m]` de un equipo estaban en la vista | mayor | Aceptado: `summary_sessions` y `team_open` en `titan_agent::conversation`, con tests (también con un daemon sin proyectos) |
| `folder_choices` no quitaba un importado repetido (`/a` y `/a/`) | menor | Aceptado, con test |
| `projects_error` y los rechazos de importar o quitar no se veían | menor | Aceptado: salen en `danger` bajo el título del árbol, con cualquier selección |

Su pregunta (¿y si lanzar pasa las salvaguardas pero no se puede importar?):
el lanzamiento sigue y el fallo va al log del daemon. Importar es una
comodidad; no vale la pena negar un lanzamiento que el usuario ya pidió y vio.

## PR B · Evaluador visual

**Ronda 1** (subagente de Opus nuevo, 6 capturas): árbol 7, otras carpetas 9,
resumen 7, borrador 9, monitor sin directora 8,5. Aplicado: el contador del
árbol es el de sesiones vivas (como el resumen); `[!]` en el sitio del
contador en una carpeta borrada; la directora en `accent` (pulsable) y
«directora» en `mute`; la actividad alineada a la derecha antes de `[m]`;
`[+] nueva sesión` con borde `accent` y `[x] quitar` con borde `danger`; el
resumen limitado a 880 px; «sin sesión directora» en `mute`; el `+` en una
cajita con borde. No aplicado: anclar el chip del borrador al `+` con borde
`accent` y marcar el proyecto en el árbol mientras el borrador está abierto
(el botón «Nueva sesión» ya lo marca).

## PR B · Prueba a mano

Qué se prueba: la interacción con los proyectos importados (hover del `+`,
importar, quitar, lanzar en una carpeta nueva y abrir el monitor desde el
resumen), que el evaluador no puede ver. Datos inventados y un daemon privado
con su propio `projects.json`: no toca la lista de la app instalada.

```
scripts/demo/demo.sh probar inicio
```

1. Se abre la ventana → `PROYECTOS [+] importar`; titan-ssh con sus
   sesiones; recetas con «sin sesiones · [+] nueva»; borrado con `[!]` y
   «carpeta no encontrada · [x] quitar»; al final `OTRAS CARPETAS` con
   web-notas y su `[+] importar`.
2. Pasa el ratón por la cabecera de recetas → aparece un `+` a la derecha.
   Clic en el `+` → borrador «Nueva sesión en [#] recetas», con recetas en
   el chip de proyecto.
3. Esc dos veces → se cierra el borrador.
4. Clic en la cabecera de titan-ssh → su resumen: carpeta, `[+] nueva
   sesión`, `[d] abrir carpeta`, `[x] quitar`; «EQUIPOS · 3 encargos
   abiertos» (sincronizar-hosts, sftp-renombrar y sftp-enlaces, cada uno con
   su directora) y «SESIONES · 4 vivas».
5. `[m] monitor` de sincronizar-hosts → se abre la sesión «sincronizar
   hosts» con el monitor de ese encargo.
6. Clic en la cabecera de recetas → su resumen con limpieza-docs y «sin
   sesión directora». Su `[m] monitor` → el monitor ahí mismo, con la miga
   «[#] recetas › encargo limpieza-docs» y `[<] resumen`.
7. `[<] resumen` → vuelve el resumen.
8. En web-notas (otras carpetas), `[+] importar` → pasa a `PROYECTOS` y
   desaparece `OTRAS CARPETAS`.
9. Clic en la cabecera de web-notas y `[x] quitar` → vuelve a `OTRAS
   CARPETAS` con sus sesiones; la carpeta no se toca.
10. En borrado, `[x] quitar` → desaparece del árbol.
11. `[+] importar` del título → selector de carpetas; elige cualquier
    carpeta (solo cambia la lista de esta demo) → aparece en `PROYECTOS`
    con «sin sesiones».
12. ctrl+n, chip de proyecto → «[d] elegir otra carpeta…», elige otra
    carpeta que no esté en la lista, escribe un encargo y Enter → se lanza
    (con el `claude` falso) y su proyecto aparece importado en `PROYECTOS`.
13. Cierra la ventana → la demo se para sola.

## Criterios de finalización

**PR A**:

- Enter lanza desde el borrador y Shift+Enter hace salto de línea.
- Las listas «- » continúan con Shift+Enter.
- La vista pasa de borrador a sesión al lanzar.
- Con dos encargos de dos directoras en el mismo proyecto, el `[m]` de cada
  una abre su propio monitor.

**PR B**:

- Un proyecto importado sin sesiones se ve y su `+` abre el borrador con
  él.
- Quitarlo no toca la carpeta y sus sesiones pasan a «otras carpetas».
- La lista es la misma tras reiniciar el daemon y la ve cualquier cliente.
- La primera vez aparecen importados los proyectos que ya tenían sesiones.
- El resumen del proyecto muestra sus equipos y abre su monitor.

**Las dos**:

- Comprobaciones en verde.
- Revisión de Sol y evaluador visual.
- Prueba manual del usuario.
- PR integrada con la versión acordada.

## Verificación

**PR A** (rama `feat/v0.7.0`, 2026-11-18):

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` y `cargo test --workspace` en verde en local: **204 tests**
  (189 en `main`): 8 de aceptación de Sol (commit `9e563f5`, sin tocar) y
  los propios (modelo del campo, cuerpo de la sesión, `task_hint`,
  `crumb_name` y `crates/titan-agent-core/tests/directors.rs`). Hooks de
  gitleaks en cada commit y en el push.
- Revisión de Sol y dos rondas del evaluador visual (arriba).
- Prueba a mano del usuario: **bien** (2026-11-18).
- **PR #13** integrada por squash con autorización del usuario: commit
  `4d5172c` en `main`, con `Cambios`, `Fugas`, `Verificar` y `Windows` en
  verde. GitHub rechazó `--author-email` con el correo noreply («Invalid
  email address», como en las PR #4 y #12) y se integró sin él. Versión
  0.7.0, sin etiqueta ni release.

### Métricas de la PR A

- **Tiempo**: de 01:25 a 02:15 del 2026-11-18 (unos 50 min), más la espera
  a la prueba a mano del usuario.
- **Claude**: el director (Opus 5.5, unas 140 llamadas a herramientas) y
  dos evaluadores visuales de Opus (unos 124 k tokens cada uno).
- **Codex**:

  | Encargo | Rol | Entrada (caché) | Salida | Tiempo |
  | --- | --- | --- | --- | --- |
  | `v07a` | Sol, tests | 114 k (95 k) | 5,7 k | 138 s |
  | `v07a-revision` | Sol, revisión high con `--gate` | 984 k (888 k) | 7,3 k | 236 s |

- **Luna**: no se usó; no hubo ediciones repetitivas.
- **Hallazgos**: Sol, 3 mayores y 2 menores, todos aceptados.

## Carencias del equipo

Encontradas en esta versión (equipo v6), con su causa y una propuesta para la
siguiente:

1. **Las escenas con guion de teclas se capturaban a medio escribir**: `shot`
   esperaba un tiempo fijo (6 s) y un guion largo no había acabado (el
   evaluador de la ronda 1 lo vio). *Corregido en el kit*: la app escribe
   «guion de teclas terminado» en `ui.log` y `shot` espera esa línea (20 s
   como mucho).
2. **Capturas rechazadas cuando el usuario usa el escritorio**: `shot` no
   guarda nada si otra ventana se lleva el foco (correcto), pero hay que
   repetir a mano. *Propuesta*: que `shots` reintente una vez las que
   fallen por el foco.
3. **Sol pidió una firma de `board` para probar que monitor, `[m]` y `[*]`
   coinciden**: el contrato no la daba. Lo cubre un test propio del director
   (`crates/titan-agent-core/tests/directors.rs`). *Propuesta*: cuando un
   apartado dice «la misma regla que X», el contrato da también la firma de
   X.

4. **Criterios visuales de la B escritos tras implementar**: con la
   especificación y Sol en marcha, el director pasó directo al daemon y a la
   interfaz. *Propuesta*: que `bin/team new` recuerde el paso 2 (criterios
   visuales) cuando la especificación nombre una decisión visual.
5. **Un falso positivo de gitleaks** en `demo.sh` (la regla de rutas
   personales confunde el HOME inventado de la demo, la carpeta `home/` con
   su subcarpeta `dev/`, con una carpeta personal). Resuelto
   con `gitleaks:allow` en esa línea, como dice el README. *Propuesta*: que
   el HOME inventado de la demo no se llame `home/` (p. ej. `casa/`).
6. **Un test de Sol no compilaba** (`json!` con un entero que no cabe en
   `i32`): su turno solo compila los tests de su crate con los demás
   fallando, y no lo vio. Corregido por Sol en un segundo turno corto.
   *Propuesta*: que el prompt de Sol le pida mirar en su `.checks.txt` que
   no haya errores de compilación, solo fallos.

## Resultado
