---
Nombre: Fase 1 - Daemon, árbol de sesiones y terminal integrado
Estado: En curso
Resumen: Primera versión usable - titan-agentd descubre las sesiones de Claude Code y Codex y mantiene los terminales; la interfaz GPUI muestra la ventana principal con el árbol de sesiones por proyecto y, al elegir una, su terminal con el CLI oficial en el que escribe el usuario.
Decisiones: 'ADR-0007 (protocolo NDJSON v1 en socket Unix), ADR-0008 (portable-pty + alacritty_terminal), ADR-0009 (descubrimiento y jerarquía). Variante opus del experimento de comparación (rama feat/fase-1-opus), elegida por el usuario (2026-11-15) para cerrar la fase. Versión 0.2.0 (minor), acordada con el usuario vía la sesión de control.'
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-15T18:57:00+01:00
---

# Fase 1 - Daemon, árbol de sesiones y terminal integrado

## Objetivo

- **Daemon** según [[Decisiones/ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]]:
  - sesiones de Claude Code con `claude agents --json` (campos vistos el
    2026-11-14: `id`, `pid`, `cwd`, `kind` interactive/background,
    `startedAt`, `sessionId`, `name`, `status`/`state`); comprobar el
    formato con la versión instalada y no depender de campos no documentados sin
    tolerar su ausencia;
  - sesiones de Codex con `codex agents` / el daemon `codex app-server`
    (JSON-RPC; `codex app-server generate-json-schema` da los tipos);
  - jerarquía «lanzada por» para anidar sesiones (de dónde sale: investigarlo; si
    no hay dato fiable, sin anidar en esta fase);
  - PTY de `claude attach <id>` y `codex resume <id>` que sobreviven al cierre
    de la ventana; detener una sesión con el comando oficial;
  - socket local y protocolo versionado: **ADR nueva** con el formato.
- **Interfaz GPUI**: ventana principal de
  [[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]] con los
  tokens de [[Decisiones visuales/Lenguaje visual heredado de titan-ssh]]:
  cabecera con contadores, columna izquierda (filtro y árbol por proyecto con
  estado y pastilla de agente), zona central con el terminal de la sesión
  seleccionada y el panel de detalle. El terminal se apoya en el precedente de
  Zed (`alacritty_terminal` sobre GPUI): **ADR** con los crates elegidos.
- Linux primero; Windows en esta fase si el coste es razonable (PTY con ConPTY,
  named pipe).

## Criterios de finalización

- Con varias sesiones reales de Claude Code y Codex abiertas, el árbol las muestra
  con su estado actualizado en segundos.
- Elegir una sesión muestra su terminal y se puede escribir en ella; cerrar y
  reabrir la ventana conserva el terminal.
- Ninguna parte lee credenciales de las CLI ni envía texto propio a un modelo
  (ADR-0001).
- Comprobaciones de la compuerta en verde.

## Verificación

Variante **opus** del experimento (rama `feat/fase-1-opus`, sin push), en local
el 2026-11-15:

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` y `cargo test --workspace` en verde: **37 tests de Rust** (9 de la
  fase 0 y 28 nuevos: 19 del núcleo, 5 de punta a punta del daemon con CLI
  falsos y 4 del teclado del terminal). Hooks de gitleaks en cada commit, sin fugas.
- Con sesiones reales abiertas (12 de Claude Code y varios hilos de Codex), el
  daemon en un socket propio las lista con su estado; un hilo de Codex pasa de
  `[x]` a `[+]` en el árbol en unos 2 s al abrirlo.
- Sesiones de prueba propias (una de Claude Code con haiku y un hilo de Codex con
  luna): elegirlas abre su TUI oficial dentro de la ventana, lo escrito llega al
  CLI y, tras cerrar y abrir la ventana, el mismo terminal (`t1`) vuelve con su
  historial. No se adjuntó ni detuvo ninguna sesión real del usuario.
- Capturas reales de la ventana (spectacle) en `.previa/fase-1-opus/` (ignorada
  por git: muestran nombres de sesiones reales).
- Versión 0.2.0 en `Cargo.toml` (supera la 0.1.1 de `main`,
  `scripts/comprobar-version.sh` en verde).
- Pendiente: CI (incluido Windows), la PR y su integración.

## Resultado

- **Núcleo** (`titan-agent-core`): `Session`, agrupado por proyecto plegando
  worktrees, protocolo v1, interpretación de `claude agents --json` y de los
  hilos de `codex app-server`, filas del árbol y tiempos relativos.
- **Daemon** (`titan-agentd`): sondeo cada 2 s, enlace director–equipo, PTY de
  `claude attach` / `codex resume --remote unix://` que sobreviven a la ventana,
  `stop` oficial, abrir carpeta y un solo daemon por socket. Windows compila
  previsiblemente pero sin named pipe (aplazado).
- **Interfaz** (`titan-agent`): ventana de la decisión visual con cabecera y
  contadores, árbol con filtro, terminal integrado, panel de detalle, monitor sin
  vistas (fase 3) y cuadro de nueva sesión estático (fase 2). Arranca el daemon si
  no está.
- Catálogo: [[Catálogo técnico/Protocolo de titan-agentd]],
  [[Catálogo técnico/Descubrimiento de sesiones]],
  [[Catálogo técnico/Árbol de sesiones]], [[Catálogo técnico/Terminal integrado]],
  [[Catálogo técnico/titan-agentd]] y [[Catálogo técnico/Tipos de sesión]]
  ampliada.
- Falta para «Hecha»: PR con versión acordada, CI en verde e integración
  autorizada. Mejoras anotadas: selección y copiar en el terminal, named pipe en
  Windows, vistas del monitor (fase 3).

### Experimento: variante opus

Opus implementa solo, sin equipo, subagentes ni Codex.

- **Tiempo**: de 11:01 a 11:40 (unos 40 min) hasta el resultado verificado y
  documentado.
- **Uso de Claude** (transcript de la sesión, Opus 5.5): 121 respuestas, 128
  llamadas a herramientas; 169 k tokens de salida, 328 k de escritura en caché,
  26,9 M de lectura de caché, 229 de entrada sin caché.
- **Codex**: 1 llamada (`codex exec` de prueba con luna, low: 6 061 tokens);
  ninguna para desarrollar.
- **Tests**: 28 nuevos (37 en total).
- **Hallazgos de revisión** (revisión propia del diff, sin revisor externo): 2
  menores corregidos (nombre de reserva cortado por bytes; «estado desde» de los
  hilos de Codex cargados tomaba su creación). Antes, las capturas detectaron 3
  fallos visuales corregidos: ligaduras de JetBrains Mono (`|-` salía como `⊢`),
  columna a 320 px en vez de 340 y nombres de proyecto largos que desbordaban.
- **Autoevaluación visual** frente a `design/preview/Main.dc.html`: cabecera,
  columna izquierda (botón, filtro, árbol con pastillas, ramas, selección con
  borde `accent`, leyenda), barra de sesión, terminal sobre `terminal-bg` con
  padding 16 y panel de detalle siguen la previa en tamaños, colores y
  marcadores. Diferencias: la barra de título de KDE encima de la cabecera; sin
  espaciado de letras en las etiquetas en mayúsculas (GPUI no lo tiene); los
  campos del detalle en columna fija de 120 px en lugar de la rejilla; el cuadro
  «escribe aquí» de la previa es el propio campo del CLI; monitor y nueva sesión
  son marcadores de sus fases; «lanzada por» dice «sin dato del CLI» en las
  sesiones de Claude en segundo plano.
