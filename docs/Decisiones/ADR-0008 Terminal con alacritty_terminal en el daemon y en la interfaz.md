---
Nombre: Terminal con alacritty_terminal en el daemon y en la interfaz
Número: 8
Estado: Aceptada
Resumen: El terminal usa el crate oficial alacritty_terminal (0.26, de crates.io) y no el fork de Zed. El daemon lo usa para abrir el PTY y emular la pantalla de cada sesión, de modo que al reabrir la ventana puede enviar una instantánea regenerada como secuencia ANSI. La interfaz lo usa para emular los bytes que recibe y dibuja la rejilla con un elemento GPUI propio. Del terminal de Zed no se copia nada porque es GPL-3.0.
Decisión: alacritty_terminal 0.26 oficial para el PTY (tty) y la emulación (Term) en el daemon y para la emulación en la interfaz; instantánea ANSI al abrir un terminal; render propio en GPUI.
Consecuencias: Una sola dependencia de terminal para los dos procesos y para Linux y Windows (ConPTY). Hay que mantener la instantánea ANSI y el render y la codificación del teclado propios. Windows queda fuera de la fase 1.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-14T19:30:00+01:00
Última modificación: 2026-11-14T19:30:00+01:00
---

# ADR-0008 · Terminal con alacritty_terminal en el daemon y en la interfaz

## Contexto

La fase 1 ([[Tareas/Fase 1 - Daemon, árbol de sesiones y terminal integrado]])
pide alojar el CLI oficial de cada sesión (`claude attach <id>`,
`codex resume <id>`) en un terminal que **sobreviva al cierre de la ventana**
([[ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]]), con el
precedente de Zed (`alacritty_terminal` sobre GPUI). Comprobado el 2026-11-14 en
el tag de Zed fijado por
[[ADR-0006 GPUI desde el repositorio de Zed fijado a un tag estable]]:

- Zed usa un **fork** propio (`zed-industries/alacritty`, fijado a un commit).
- Los crates `terminal` y `terminal_view` de Zed son **GPL-3.0-or-later**;
  titan-agent es Apache-2.0, así que no se puede copiar su código. `gpui` sí es
  Apache-2.0.
- `alacritty_terminal` 0.26.0 oficial (crates.io, Apache-2.0, proyecto
  Alacritty) trae el PTY (`tty`: Unix con `rustix-openpty` y Windows con ConPTY)
  y el emulador (`Term`, parser `vte`).

Al reabrir la ventana, la interfaz tiene que ver la pantalla actual de la sesión,
no solo lo que llegue a partir de entonces.

## Decisión

- **Crate**: `alacritty_terminal = "0.26.0"` de crates.io, sin funciones por
  defecto (no hace falta `serde`), en el daemon y en la interfaz. Nada del fork
  de Zed ni de sus crates GPL.
- **Daemon**: por sesión adjuntada, un PTY con `alacritty_terminal::tty` que
  ejecuta el comando `attach` de la sesión en su `cwd`, un hilo lector propio que
  manda cada bloque de bytes a la vez a un `Term` (emulación con el `Processor`
  de `vte` que reexporta el crate) y a los clientes abiertos, y escritura
  directa en el PTY para la entrada. El PTY sigue vivo aunque no quede ningún
  cliente; muere cuando termina su proceso.
- **Instantánea**: al abrir un terminal, el daemon genera desde la rejilla de su
  `Term` una secuencia ANSI que reproduce la pantalla visible: modos (pantalla
  alternativa, teclas de cursor de aplicación, pegado entre corchetes, cursor
  visible), contenido con sus atributos y colores, y posición del cursor. Es el
  primer `terminal/output` ([[ADR-0007 Socket local y protocolo NDJSON versionado entre daemon e interfaz]]).
  El historial (scrollback) no se incluye en la fase 1.
- **Interfaz**: un `Term` local alimentado con los bytes recibidos y un elemento
  GPUI propio que dibuja la rejilla por líneas, agrupando celdas del mismo estilo
  en tramos de texto (JetBrains Mono, paleta ANSI de titan-ssh de
  [[Decisiones visuales/Lenguaje visual heredado de titan-ssh]]), y el cursor.
  El teclado se traduce a bytes con una función propia y probada del núcleo
  (sin GPUI) que respeta el modo de cursor de aplicación y el pegado entre
  corchetes.
- **Windows**: el crate ya trae ConPTY, pero Windows (PTY y named pipe) queda
  fuera de la fase 1; se aborda cuando Linux esté verificado.

## Alternativas consideradas

- **Fork de alacritty de Zed** — descartado: otro intermediario en la cadena de
  suministro ([[ADR-0006 GPUI desde el repositorio de Zed fijado a un tag estable]]).
- **Copiar `terminal_view` de Zed** — imposible por licencia (GPL).
- **`portable-pty` (WezTerm) + `vte`** — válido, pero añade un crate más cuando
  `alacritty_terminal` ya trae PTY y emulador.
- **Repetir los últimos bytes en bruto al reabrir** — más simple, pero un corte a
  mitad de una secuencia de escape ensucia la pantalla y el tamaño crece sin
  límite claro.
- **Que el daemon envíe la rejilla ya emulada** (como tmux) — la interfaz no
  emularía, pero el protocolo sería más pesado y titan-ssh ya tiene su propio
  emulador para bytes en bruto.

## Consecuencias

- Positivas: los dos procesos comparten emulador; cerrar y reabrir la ventana
  conserva la pantalla; base para leer el estado de la pantalla desde el daemon
  (monitor, fase 3).
- Negativas / compromisos: código propio para la instantánea ANSI, el render y
  el teclado (sin el terminal de Zed como atajo); la API de `alacritty_terminal`
  puede cambiar entre versiones (se sube a mano, con su PR).
