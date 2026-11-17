---
Nombre: Acentos y teclas muertas en el terminal integrado
Estado: En curso
Resumen: 'En el terminal integrado de titan-agent, una tecla muerta (el acento agudo de un teclado español) no se compone con la letra siguiente: se escribe una comilla simple y luego la letra (á sale como ''a). Hay que componer las teclas muertas (y, si GPUI lo da, el método de entrada) antes de mandar los bytes al PTY.'
Decisiones: 'Visto por el usuario en la prueba manual del cierre de la fase 3 (2026-11-16). Ronda 0.5.0: como el terminal de Zed, el texto lo entrega el manejador de entrada de la plataforma (EntityInputHandler), ya compuesto, y el manejador de teclas solo manda teclas especiales y combinaciones con control o alt. Integrado en la PR #8 (v0.5.0); falta la prueba a mano del usuario.'
Bloqueada: []
Fecha de creación: 2026-11-16T21:50:00+01:00
Última modificación: 2026-11-17T15:35:00+01:00
---

# Acentos y teclas muertas en el terminal integrado

## Objetivo

Al escribir en una sesión desde el terminal integrado, con un teclado español
en Linux (KDE Wayland), los acentos salen mal: la tecla muerta del acento
agudo llega como una comilla simple y la vocal va aparte (el usuario escribió
«sí» y salió «s'í»). Escribir en español desde la app es el uso normal, así
que hay que arreglarlo.

El terminal traduce cada `KeyDownEvent` de GPUI a bytes
(`crates/titan-agent/src/keys.rs`, `keystroke_bytes`, y `terminal.rs`,
`key_down`), y
[[Decisiones/ADR-0008 Terminal integrado con portable-pty en el daemon y alacritty_terminal en la interfaz]]
dejó fuera el método de entrada («sin IME más allá de lo que llega como
carácter»). Hay que ver qué da GPUI en ese caso (`key_char`, el texto
compuesto o un manejador de entrada de texto como el del editor de Zed) y
usar el carácter ya compuesto en lugar de la tecla.

## Especificación (contrato, fijado antes de los tests)

Ronda de arreglos 0.5.0, variante equipo v4. Contrato cerrado por el director
antes de que Sol escriba los tests de aceptación (encargo `v05-acentos`).

### Objetivo y comportamiento visible

titan-agent es una interfaz de escritorio en Rust con GPUI (el framework de
Zed, fijado a un commit) que incluye un terminal integrado: cada tecla que el
usuario pulsa en él se traduce a bytes y se manda al PTY de una sesión (Claude
Code o Codex). Hoy, con un teclado español en Linux (KDE Wayland), los acentos
salen mal: el usuario escribió «sí» y salió «s'í».

Causa: en Linux, GPUI compone las teclas muertas con xkb. Al pulsar la tecla
muerta del acento agudo llega un `KeyDownEvent` cuyo `Keystroke` tiene
`key = "'"` (la tecla física equivalente en el teclado estadounidense) y
`key_char = None`, y GPUI muestra el acento como texto provisional (IME
«marked text»). Al pulsar después la vocal llega otro `KeyDownEvent` con
`key = "i"` (o el nombre de la tecla compuesta) y `key_char = Some("í")`. La
función de traducción de teclas manda `'` en el primer evento porque cae en
`key` cuando no hay `key_char`.

Arreglo acordado: como el terminal de Zed, el texto escrito ya no lo manda el
manejador de teclas sino el **manejador de entrada de texto** de la plataforma
(`EntityInputHandler` de GPUI), que recibe el carácter ya compuesto, el texto
de la composición cancelada (`´` + `s` da `´s`) y el texto de un método de
entrada (IME). El manejador de teclas solo manda las teclas especiales y las
combinaciones con control o alt.

Criterios del usuario:
- Con un teclado español, `´` + vocal escribe la vocal acentuada; `´` +
  espacio escribe el acento solo; la `ñ` y la `ü` funcionan.
- Las teclas de control y los atajos del terminal siguen igual.

### Contrato público (lo que comprueban los tests de aceptación)

Crate `titan-agent` (`crates/titan-agent`). Tiene un target de biblioteca
`titan_agent` que expone el módulo `titan_agent::keys`. Los tests de
aceptación van en `crates/titan-agent/tests/` (tests de integración de Cargo)
y construyen los `gpui::Keystroke` a mano con sus campos públicos `key`,
`key_char` y `modifiers` (`gpui::Modifiers { control, alt, shift, platform,
function }`); `gpui` ya es dependencia del crate. No usan ventana ni contexto
de GPUI.

```rust
// titan_agent::keys
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyModes {
    /// DECCKM: las flechas mandan `ESC O x` en lugar de `ESC [ x`.
    pub app_cursor: bool,
}

/// Bytes que el manejador de teclas manda al PTY, o `None` cuando no manda
/// nada (el texto lo entrega el manejador de entrada, o la tecla no es para
/// el terminal).
pub fn keystroke_bytes(keystroke: &gpui::Keystroke, modes: KeyModes) -> Option<Vec<u8>>;
```

Reglas de `keystroke_bytes` (las de teclas especiales no cambian):

1. **Texto escrito → `None`.** Sin control, sin alt y sin platform (la tecla
   «super»/«cmd»), cualquier tecla que no sea especial devuelve `None`, tenga
   `key_char` o no. Incluye: letras y signos (`key_char` presente), `ñ`, `ü`,
   la vocal ya compuesta (`key_char = "é"`), el acento solo (`key_char = "´"`)
   y la tecla muerta (`key_char = None`, `key = "'"` u otro signo). Con shift
   igual (`key_char = "A"`, `"?"`…).
2. **Teclas especiales (sin cambios).** `enter` → `\r` (alt: `ESC \r`);
   `tab` → `\t`, shift-tab → `ESC [Z`; `escape` → `ESC`; `backspace` → `0x7f`
   (control: `0x08`, alt: `ESC 0x7f`); flechas `ESC [A/B/C/D` (o `ESC O x` con
   `app_cursor`, o `ESC [1;<mod>x` con modificadores, `<mod>` = 1 + shift +
   2·alt + 4·control); `home`/`end` `ESC [H` / `ESC [F` (mismas reglas);
   `insert` `delete` `pageup` `pagedown` → `ESC [2~` `ESC [3~` `ESC [5~`
   `ESC [6~` (con modificadores `ESC [<n>;<mod>~`); `f1`–`f4` → `ESC OP`…`ESC
   OS`; `f5`–`f12` → `ESC [15~ 17~ 18~ 19~ 20~ 21~ 23~ 24~`. `space` sin
   modificadores → `None` (es texto); control-space → `0x00`; alt-space →
   `ESC ` + espacio.
3. **Control (sin platform).** control + letra → código C0 (`ctrl-a` = 1 …
   `ctrl-z` = 26); `@`/`2` → 0, `[`/`3` → `0x1b`, `\`/`4` → `0x1c`, `]`/`5` →
   `0x1d`, `^`/`6` → `0x1e`, `_`/`-`/`7` → `0x1f`, `?`/`8` → `0x7f`; con alt
   además, precedido de `ESC`. Otra tecla con control → `None`.
4. **AltGr de Windows.** Con control **y** alt a la vez y un `key_char`
   distinto de `key` (Windows informa AltGr como control+alt, p. ej. `key =
   "2"`, `key_char = "@"`), es texto → `None`. (En Linux AltGr no activa ni
   control ni alt y cae en la regla 1.)
5. **Alt como meta.** Alt (sin control ni platform) + una tecla cuyo
   `key_char` es igual a `key` → `ESC` seguido de esos bytes (`alt-b` → `ESC
   b`). Si alt da otro carácter (`key_char` distinto de `key`) es texto →
   `None`. Alt con `key_char = None` → `None`.
6. **Platform.** Cualquier tecla no especial con platform → `None` (son
   atajos de la ventana).
7. Un modificador solo (`key = "shift"`, `"control"`…) → `None`.

El texto que llega por el manejador de entrada se manda al PTY tal cual en
UTF-8 (sin tests de aceptación: necesita una ventana).

### Secuencias reales de GPUI en Linux (para los tests)

- `´` + `e`: `{key: "'", key_char: None}` y luego `{key: "e", key_char:
  Some("é")}` → ambos `None` (el `é` llega por el manejador de entrada).
- `´` + espacio: `{key: "'", key_char: None}` y luego `{key: "acute",
  key_char: Some("´")}` → ambos `None`.
- `ñ`: `{key: "ñ", key_char: Some("ñ")}` → `None`.
- `¨` + `u` (shift + tecla muerta): `{key: "'", modifiers.shift, key_char:
  None}` y `{key: "u", key_char: Some("ü")}` → ambos `None`.


## Criterios de finalización

- Con un teclado español, `´` + vocal escribe la vocal acentuada en el
  terminal integrado; `´` + espacio escribe el acento solo; la `ñ` y la `ü`
  funcionan.
- Las teclas de control y los atajos del terminal siguen igual.
- Tests de la traducción de teclas para los casos compuestos; comprobado a
  mano en Linux (y en Windows si la fase lo cubre).

## Verificación

Rama `feat/v0.5-equipo-v4` (sin push), 2026-11-16:

- **Tests de aceptación de Sol** (`crates/titan-agent/tests/accents_acceptance.rs`,
  5 tests, encargo `v05-acentos`, commits `ffc2475` y `0892d9f`): las
  secuencias reales de GPUI en Linux (`´`+vocal, `´`+espacio, `ñ`, `¨`+`u`) no
  mandan nada desde el manejador de teclas; el texto normal tampoco; control,
  alt como meta, AltGr de Windows y las teclas especiales siguen igual. Pasan
  sin tocarlos. El segundo commit es de Sol: formateó su fichero con la
  edición 2024, que no había usado.
- Tests propios en `keys.rs` (texto y tecla muerta, alt y AltGr).
- fmt, clippy y `cargo test --workspace` en verde; gitleaks en cada commit.
- **Pendiente: la prueba a mano del usuario** con teclado español (no hay
  simulación de teclado). Preparado: `.previa/v05-equipo-v4/acentos/probar-acentos.sh`
  (ignorado por git) abre la ventana de esta rama contra un daemon propio
  con una sesión falsa cuyo terminal muestra el texto y los bytes que
  recibe: «sí» debe llegar como `73 c3 ad`.

## Resultado

- **Publicación**: PR #8 integrada con squash en `main` el 2026-11-17, commit `b1237e8`, versión **0.5.0** sin etiqueta. Checks `Cambios`, `Fugas`, `Verificar` y `Windows` en verde. GitHub rechazó `--author-email` con el correo noreply, así que el squash sale con el correo principal de la cuenta.

- Causa: en Linux GPUI compone las teclas muertas con xkb. La tecla muerta
  llega como `KeyDownEvent` con `key = "'"` (la tecla del teclado
  estadounidense) y sin `key_char`, y `keystroke_bytes` mandaba esa `'`; la
  vocal llegaba luego ya compuesta (`í`).
- `keystroke_bytes` (ahora en la biblioteca `titan_agent::keys`, para poder
  probarlo desde `tests/`) devuelve `None` para el texto escrito, tenga
  `key_char` o no, y para AltGr de Windows (control+alt con otro carácter).
- El terminal implementa `EntityInputHandler` (`terminal/input.rs`): el texto
  confirmado va al PTY tal cual en UTF-8 y la composición en curso (el `´`
  de la tecla muerta o un IME) se dibuja subrayada en el cursor hasta que se
  confirma. `´` + consonante da `´s` (GPUI confirma el acento al cancelar).
