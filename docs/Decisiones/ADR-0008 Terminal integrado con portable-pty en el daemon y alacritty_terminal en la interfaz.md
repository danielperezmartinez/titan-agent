---
Nombre: Terminal integrado con portable-pty en el daemon y alacritty_terminal en la interfaz
Número: 8
Estado: Aceptada
Resumen: 'El daemon abre los PTY con portable-pty (el crate de WezTerm, con ConPTY en Windows) y ejecuta en ellos el comando oficial de adjuntar (claude attach <id>, codex resume --remote unix:// <id>); guarda un historial de 2 MiB por terminal. La interfaz emula el terminal con alacritty_terminal 0.26 de crates.io (el mismo motor que el terminal de Zed) y lo pinta con GPUI con código propio, en JetBrains Mono empaquetada y sin ligaduras.'
Decisión: portable-pty 0.9 para los PTY del daemon y alacritty_terminal 0.26 (crates.io) para emular y pintar el terminal en GPUI; renderizado y teclado escritos aquí, sin copiar código de Zed.
Consecuencias: El terminal no tiene aún selección con ratón ni copiar; pegar es ctrl+shift+v. La fuente JetBrains Mono va dentro del binario con su licencia OFL. alacritty_terminal se fija a 0.26 y subirlo es un cambio propio.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-15T11:36:00+01:00
Última modificación: 2026-11-15T11:36:00+01:00
---

# ADR-0008 · Terminal integrado con portable-pty en el daemon y alacritty_terminal en la interfaz

## Contexto

La fase 1 pide que, al elegir una sesión, se vea su terminal con el CLI oficial y
se pueda escribir, y que el terminal sobreviva al cierre de la ventana
([[ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]]).
[[ADR-0003 Rust con GPUI en escritorio y Android dentro de titan-ssh]] señala el
precedente de Zed: `alacritty_terminal` sobre GPUI. Zed usa un fork propio de
alacritty (por git) y su crate `terminal_view` tiene licencia GPL, incompatible
con la Apache-2.0 de este repo.

## Decisión

- **PTY en el daemon: `portable-pty` 0.9** (crates.io, de WezTerm). Abre el PTY,
  lanza `claude attach <id>` o `codex resume --remote unix:// <id>` con
  `TERM=xterm-256color` en la carpeta de la sesión, y separa lectura y escritura
  en hilos. El daemon guarda los últimos 2 MiB de salida para repetirlos a quien
  se adjunte después. Funciona igual en Windows (ConPTY).
- **Codex con `--remote unix://`**: así el TUI abierto desde la app usa el daemon
  compartido de Codex y su estado se ve en el árbol en vivo.
- **Emulación en la interfaz: `alacritty_terminal` 0.26** de crates.io (no el
  fork de Zed), con 5000 líneas de historial. Responde a las peticiones del
  programa (posición del cursor, colores, tamaño) por el propio terminal.
- **Pintado propio en GPUI** (`crates/titan-agent/src/terminal.rs`): por fila,
  fondos agrupados con `paint_quad` y texto en segmentos con `shape_line` y
  ancho de celda forzado; los caracteres anchos empiezan segmento nuevo. Cursor
  de bloque `accent` (contorno si el terminal no tiene el foco). Paleta ANSI de
  titan-ssh ([[Decisiones visuales/Lenguaje visual heredado de titan-ssh]]).
- **Teclado propio** (`keys.rs`): secuencias xterm, con modo de cursor de
  aplicación y modificadores. Rueda: historial local, o flechas / eventos SGR si
  el programa está en pantalla alternativa o captura el ratón.
- **Fuente**: JetBrains Mono 2.304 (Regular, Medium, Bold) empaquetada con su
  licencia OFL en `crates/titan-agent/assets/fonts/`, con `calt` desactivado para
  que `|-` y `->` no se conviertan en ligaduras.

## Alternativas consideradas

- **Fork de alacritty de Zed por git** — descartado: otro repositorio fijado más
  y sin ventaja para este uso; la versión de crates.io basta.
- **Copiar `terminal_view` de Zed** — descartado por la licencia GPL.
- **PTY con el módulo `tty` de alacritty_terminal en el daemon** — posible, pero
  portable-pty da una API más simple (hijo, `wait`, `kill`, `resize`) y el daemon
  no necesita emular nada.
- **Emular también en el daemon y mandar pantallas** — descartado por ahora: más
  CPU en el daemon; repetir el historial y forzar un redibujado basta para los
  TUI de los CLI.

## Consecuencias

- Positivas: los TUI oficiales se ven con su color y se usan con el teclado;
  cerrar y abrir la ventana conserva el terminal; todo con licencias
  compatibles.
- Negativas / compromisos: falta selección con ratón y copiar, búsqueda y
  enlaces; sin método de entrada (IME) más allá de lo que llega como carácter.
  Pendiente para una fase posterior.
