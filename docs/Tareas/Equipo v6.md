---
Nombre: Equipo v6
Estado: En curso
Resumen: 'Corrige las carencias del equipo v5 anotadas en la 0.6.0 (incluidas las 7–9 del diseño de la 0.7.0): git de una orden por llamada y mensajes por fichero (ask --message-file, mensaje vacío rechazado), rutas en el contrato, titan-team --version y un binario fijado (bin/team pin) para las tareas sobre el propio equipo, kit de demo con tamaño de ventana, guion de teclas, flujos reales y projects.json, Luna sin dar por fallido un turno por el sandbox, y la lógica de interacción en modelos puros con tests. Deja la configuración v6 como la vigente.'
Decisiones: 'Encargo de la sesión de control control-3b (2026-11-18). Versión: patch 0.6.x, a acordar con el usuario.'
Bloqueada: []
Fecha de creación: 2026-11-18T00:10:00+01:00
Última modificación: 2026-11-18T00:10:00+01:00
---

# Equipo v6

## Objetivo

Corregir las carencias del equipo v5 anotadas en
[[Versión 0.6.0 - Instalador y actualización automática]] → «Carencias del
equipo» (1–5 y las 7–9 añadidas al diseñar la 0.7.0), y dejar la
**configuración v6** como la vigente, para que la haga servir
[[Versión 0.7.0 - Proyectos importados y monitor por sesión directora]]
(su paso 0 es esta ronda). Sustituye a la v5 de [[Equipo v5]] (con los
ajustes de [[Equipo v5.1 - Compilación compartida por equipo]]).

Se lanzó desde la sesión de control `control-3b`; trabajo en la rama
`feat/equipo-v6`, que sale de `main`.

## Carencias que corrige

Numeración de la nota de la 0.6.0 (C = carencia) y el ajuste:

| Carencia | Ajuste |
| --- | --- |
| C1. El aislamiento del worktree rechaza órdenes compuestas con git y vació el `-m` del revisor | A1: la skill pide git de una orden por llamada y mensajes escritos con la herramienta de ficheros; `ask --message-file`; un mensaje vacío se rechaza |
| C2. El contrato nombró el icono sin su ruta | A2: la skill exige la ruta exacta de todo fichero o recurso que nombre el contrato |
| C3. El kit no cambia el ancho de la ventana | A3: `TITAN_AGENT_DEMO_SIZE` y escenas con su tamaño en `demo.sh` |
| C4. Escenas fijas, no el flujo real | A3: la skill pide un kit de punta a punta para los estados que dependen de un servicio; `demo.sh flujos` los lista |
| C5. Luna da por fallido su turno por los tests de PTY del sandbox | A4: su prompt dice que la compuerta del orquestador es la que cuenta |
| C7. `bin/team` compila titan-team desde la carpeta del director | A5: `TITAN_TEAM_BIN`, `bin/team pin [REV]` y `unpin`; `titan-team --version` |
| C8. El kit no puede escribir en la ventana | A3: guion de teclas `TITAN_AGENT_DEMO_KEYS` y `projects.json` de demo (ADR-0014) |
| C9. La interfaz no tiene tests de ventana | A6: la skill exige la lógica de interacción en modelos puros con tests de aceptación; la vista solo pinta; el revisor lo comprueba |

C6 (el evaluador visual funcionó) no pide ajuste.

## Contrato (para los tests de aceptación)

### C1. `titan-team --version`

- `titan-team --version` y `titan-team -V` escriben en stdout exactamente
  `titan-team <versión>\n`, donde `<versión>` es la del paquete
  (`env!("CARGO_PKG_VERSION")` en un test de integración del mismo crate da
  la misma), y salen con 0.
- Funcionan en cualquier carpeta, sin `.agents/team.toml` y sin ninguna otra
  palabra. No leen ni escriben nada.

### C2. Mensaje por fichero y mensaje vacío en `ask`

- `ask` acepta `--message-file FICHERO` (y `--message-file=FICHERO`) como
  sinónimo de `-f FICHERO` / `--file FICHERO`, que ya existen: lee el
  fichero (ruta relativa a la carpeta actual) y lo usa como mensaje del
  director. El mensaje llega a Codex por su entrada estándar, al final del
  prompt, bajo la línea `# Mensaje del director` (como hoy con `-m`).
- Dar a la vez `-m`/`--message` y `-f`/`--file`/`--message-file` es un error
  de uso: salida 2.
- Un fichero de mensaje que no existe: salida 2 (como hoy con `-f`).
- **Mensaje vacío**: un `-m ""`, un `-m` con solo espacios o saltos de línea,
  o un fichero vacío o con solo espacios o saltos de línea → salida 2, con un
  mensaje en stderr que contiene `vacío`. No se lanza Codex, no se escribe
  ningún evento en `events.jsonl` ni ninguna entrada nueva en el historial.
  Vale igual para el primer turno de un rol (sin hilo) que para continuar
  un hilo.
- No dar ningún mensaje en el primer turno de un rol sigue siendo válido
  (como hoy).

### C3. `bin/team` con binario fijado

`bin/team` es un script `sh` en `<raíz>/bin/team`; `<raíz>` es la carpeta
padre de la carpeta `bin/` donde está el script (se calcula desde la ruta del
propio script, no desde la carpeta actual). Orden de elección del binario de
titan-team:

1. Si `TITAN_TEAM_BIN` está definida y no vacía: ejecuta ese fichero con los
   mismos argumentos, intactos (con espacios y en el mismo orden), y sale
   con su código de salida. Si no existe o no es ejecutable: salida 2 y
   stderr con `TITAN_TEAM_BIN`, sin ejecutar nada más.
2. Si no, si existe y es ejecutable `<raíz>/.agents/team-bin/titan-team`
   (el binario fijado): lo ejecuta igual (mismos argumentos, mismo código de
   salida).
3. Si no, como hoy: `cargo run --quiet --release --manifest-path
   <raíz>/Cargo.toml -p titan-team -- <argumentos>`.

Dos órdenes las resuelve el propio script y nunca llegan a titan-team (aunque
haya `TITAN_TEAM_BIN` o binario fijado):

- `bin/team pin [REV]` (REV por defecto `main`): resuelve REV con
  `git -C <raíz> rev-parse --verify "REV^{commit}"`. Si no es una revisión
  válida (o `<raíz>` no es un repositorio git): salida 2, stderr con
  `revisión`, y **no crea** `<raíz>/.agents/team-bin/`. Si lo es: extrae el
  árbol de ese commit (`git archive`) en una carpeta temporal, compila
  `cargo build --release -p titan-team` con
  `CARGO_TARGET_DIR=<raíz>/.agents/target`, copia el binario a
  `<raíz>/.agents/team-bin/titan-team`, escribe
  `<raíz>/.agents/team-bin/rev` con una línea `REV <sha completo>`, borra la
  carpeta temporal e imprime
  `titan-team fijado en REV (<sha de 7>): titan-team <versión>`. Si falla la
  compilación: salida 1 y no deja un binario fijado nuevo.
- `bin/team unpin`: borra `<raíz>/.agents/team-bin/` (si no existía, no es
  un error), imprime una línea que contiene `sin fijar` y sale con 0.

### C4. Análisis de las variables de demo de la interfaz (funciones puras)

La app lee dos variables nuevas solo para el kit de demo:

- `TITAN_AGENT_DEMO_SIZE=<ancho>x<alto>`: tamaño inicial de la ventana en
  píxeles lógicos (hoy siempre 1440x900).
- `TITAN_AGENT_DEMO_KEYS=<guion>`: teclas que la app aplica a la ventana tras
  abrirse, como si se pulsaran (para capturar el campo de texto con texto,
  listas, menús…).

Su análisis está en el módulo público `titan_agent::demo`
(`crates/titan-agent/src/demo.rs`), sin ventana:

**`parse_size(text: &str) -> Result<(u32, u32), String>`**

- Acepta exactamente `<ancho>x<alto>`: dos enteros decimales sin signo, sin
  espacios, separados por una `x` minúscula. `"1100x800"` → `Ok((1100, 800))`.
- Límites: ancho 640..=7680 y alto 480..=4320, ambos incluidos. Fuera de
  ellos, o cualquier otra forma (`""`, `"1100"`, `"1100X800"`,
  `" 1100x800"`, `"1100x800x2"`, `"-1100x800"`, `"axb"`) → `Err`.

**`parse_keys(script: &str) -> Result<Vec<DemoStep>, String>`**

```rust
pub enum DemoStep {
    /// A keystroke dispatched to the window as if it were pressed.
    Key(gpui::Keystroke),
    /// Wait before the next step.
    Pause(std::time::Duration),
}
```

`DemoStep` deriva `Debug`, `Clone` y `PartialEq`. `gpui::Keystroke` es el de
GPUI (dependencia `gpui` del crate, ya disponible en sus tests):

```rust
pub struct Keystroke { pub modifiers: Modifiers, pub key: String, pub key_char: Option<String> }
pub struct Modifiers { pub control: bool, pub alt: bool, pub shift: bool, pub platform: bool, pub function: bool }
```

(`Modifiers` y `Keystroke` implementan `Default`.)

Sintaxis del guion:

- **Texto**: cada carácter fuera de llaves es una tecla que lo escribe:
  `Key(Keystroke { key, key_char: Some(<el carácter>), modifiers })` con
  `key` = el carácter en minúscula (`to_lowercase`) y `modifiers.shift` =
  `true` solo si el carácter es mayúscula (`char::is_uppercase`); el resto
  de modificadores a `false`. El espacio es `key = "space"`,
  `key_char = Some(" ")`. Ejemplos: `a` → key `"a"`, key_char `"a"`; `A` →
  key `"a"`, shift, key_char `"A"`; `ñ` → key `"ñ"`, key_char `"ñ"`; `Á` →
  key `"á"`, shift, key_char `"Á"`; `-` → key `"-"`, key_char `"-"`.
- **Tecla con nombre**: `{<modificadores><tecla>}`. Modificadores, cero o
  más, en cualquier orden y cada uno como mucho una vez: `ctrl-`
  (`control`), `alt-` y `shift-`. Teclas: `enter`, `tab`, `escape`,
  `backspace`, `delete`, `up`, `down`, `left`, `right`, `home`, `end`,
  `pageup`, `pagedown`, `space`, o un único carácter imprimible (`{ctrl-n}`).
  Da `Key(Keystroke { key: <tecla en minúscula>, key_char: None, modifiers })`.
  Ejemplos: `{enter}` → key `"enter"`; `{shift-enter}` → key `"enter"` con
  shift; `{ctrl-n}` → key `"n"` con control; `{alt-shift-up}` → key `"up"`
  con alt y shift.
- **Pausa**: `{pause N}` (un espacio; N entero decimal 0..=10000) →
  `Pause(Duration::from_millis(N))`.
- **Llaves literales**: `{{` escribe `{` y `}}` escribe `}` (como texto).
- **Errores** (`Err` con un texto que nombra el fragmento): una `{` sin
  cerrar, una `}` suelta, una tecla o modificador desconocido (`{f13}`,
  `{meta-a}`, `{}`), un modificador repetido, una pausa fuera de rango o no
  numérica, y un salto de línea, retorno de carro o tabulador reales en el
  texto (se escriben `{enter}`, `{shift-enter}`, `{tab}`).
- Un guion vacío → `Ok(vec![])`. El orden de los pasos es el del guion.

Comportamiento en la app (no lo prueban los tests: lo verifica el director
con capturas): un valor inválido en cualquiera de las dos variables escribe
el error en stderr y sale con 2 sin abrir la ventana; las teclas empiezan
1,5 s después de abrir la ventana, con 20 ms entre una y otra.

### C5. Documentación

- `titan-team --help` (y `titan-team` sin argumentos) nombra
  `--message-file` y `--version`.
- `titan-team doc` nombra `--message-file`.


## Criterios de finalización

- A1–A6 implementados, con los tests de aceptación de Sol y los propios.
- La skill `equipo`, los prompts y `.agents/team.toml` describen la v6.
- El kit captura una escena estrecha y una con texto escrito por el guion.
- fmt, clippy `-D warnings`, tests y gitleaks en verde; PR integrada por
  squash con la versión acordada con el usuario.

## Verificación

## Resultado
