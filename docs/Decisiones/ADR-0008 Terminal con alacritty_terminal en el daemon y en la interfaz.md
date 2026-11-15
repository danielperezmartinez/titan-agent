---
Nombre: Terminal con alacritty_terminal en el daemon y en la interfaz
Número: 8
Estado: Propuesta
Resumen: 'Los terminales de las sesiones se apoyan en alacritty_terminal 0.26 de crates.io, como el de Zed: el daemon crea el PTY con su módulo tty, mantiene un emulador sin interfaz que contesta las consultas del programa y genera una instantánea ANSI para quien se conecta; la interfaz tiene su propio emulador alimentado con esa instantánea y la salida en bruto, y lo pinta en una rejilla medida con JetBrains Mono embebida.'
Decisión: alacritty_terminal 0.26 (crates.io) para PTY y emulación en daemon e interfaz, rustix para el control seguro del proceso, y JetBrains Mono embebida en la interfaz.
Consecuencias: Dos emuladores por terminal (daemon e interfaz) a cambio de que el terminal sobreviva al cierre de la ventana. Windows (ConPTY) queda para después de la fase 1.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-15T11:20:00+01:00
Última modificación: 2026-11-15T11:20:00+01:00
---

# ADR-0008 · Terminal con alacritty_terminal en el daemon y en la interfaz

## Contexto

[[ADR-0003 Rust con GPUI en escritorio y Android dentro de titan-ssh]] apoya el
terminal en el precedente de Zed (`alacritty_terminal` sobre GPUI) y
[[ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]] pide que los PTY
vivan en el daemon y sobrevivan al cierre de la ventana. La interfaz necesita
además reconstruir la pantalla al reconectarse.

## Decisión

- **Crates**: `alacritty_terminal = "0.26"` de crates.io (Zed usa un fork en
  git; la versión publicada basta y evita otra dependencia git), `rustix` 1 con
  `fs` y `process` para poner el maestro del PTY en modo bloqueante y para
  `kill`/`waitpid` sin `unsafe`, y `signal-hook` para el apagado ordenado del
  daemon.
- **Daemon**: cada terminal es un `PtyProcess` (PTY de `alacritty_terminal::tty`,
  hilo lector, hilo escritor con cola acotada, `SIGHUP` → `SIGKILL` al cerrar)
  y un `Emulator` sin interfaz que sigue la salida, **contesta** las consultas
  del programa (DSR, DA) una sola vez aunque haya varias ventanas o ninguna, y
  produce la **instantánea ANSI** de la pantalla visible para quien se conecta.
- **Interfaz**: `TerminalModel` con su propio `Term` alimentado con la
  instantánea y la salida; produce un fotograma de tramos con los colores ya
  resueltos a la paleta ANSI de titan-ssh, y la ventana lo pinta en una rejilla
  cuyo tamaño de celda se mide con la fuente.
- **Fuente**: JetBrains Mono (licencia SIL OFL 1.1) embebida en el binario de la
  interfaz, para no depender de que esté instalada.

## Alternativas consideradas

- **portable-pty** (wezterm) — más dependencias y otra pila distinta de la del
  emulador; `alacritty_terminal::tty` ya crea el PTY con `setsid` y terminal de
  control, y soporta ConPTY.
- **Reenviar solo la salida en bruto sin emulador en el daemon** — al reabrir la
  ventana se perdería la pantalla y nadie contestaría las consultas del programa
  con la ventana cerrada.
- **Fork de Zed en git** — sus cambios son para el editor; añade otra
  dependencia git.

## Consecuencias

- Positivas: terminal probado; reconexión sin perder la pantalla; las
  consultas del programa se contestan exactamente una vez.
- Negativas / compromisos: el historial (scrollback) no viaja en la instantánea;
  dos emuladores por terminal; Windows (ConPTY y named pipe) queda fuera de la
  fase 1.
