---
Nombre: Fase 1 - Daemon, árbol de sesiones y terminal integrado
Estado: En curso
Resumen: Primera versión usable - titan-agentd descubre las sesiones de Claude Code y Codex y mantiene los terminales; la interfaz GPUI muestra la ventana principal con el árbol de sesiones por proyecto y, al elegir una, su terminal con el CLI oficial en el que escribe el usuario.
Decisiones: '[[Decisiones/ADR-0007 Protocolo NDJSON versionado entre titan-agentd y sus clientes]] · [[Decisiones/ADR-0008 Terminal con alacritty_terminal en el daemon y en la interfaz]]'
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-15T13:10:00+01:00
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

Variante «equipo v2» del experimento de comparación (rama `feat/fase-1-equipo-v2`,
sin push ni PR). Comprobado el 2026-11-15 en local (Linux, Wayland):

- **Compuerta**: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
  y `cargo test --workspace` en verde, tres ejecuciones seguidas de la suite
  (253 tests, 0 fallos). Los hooks de gitleaks pasaron en cada commit. Falta el
  CI (no hay push).
- **Sesiones reales**: el daemon con la fuente real (`claude agents --json` y el
  socket de control de `codex app-server`) mostró en la ventana las sesiones de
  Claude Code con su modelo (leído del transcript) y los hilos de Codex cargados,
  agrupados por proyecto y con su estado; se refrescan cada 3 s mientras hay una
  ventana suscrita.
- **Terminal y escritura**: con una sesión propia de prueba (`claude --bg`, haiku,
  ya borrada) la ventana adjunta `claude attach <id>` y muestra el TUI oficial
  completo. Por protocolo (cliente de prueba) se comprobó escribir en el terminal,
  cerrar el cliente, reconectar y recibir el mismo terminal (`reused: true`) con
  la pantalla conservada en la instantánea. SIGTERM apaga el daemon, mata los
  `claude attach` y borra el socket.
- **No probado a mano**: teclear en la ventana real (no hay herramienta de
  inyección de teclado instalada); el teclado está cubierto por tests de
  `keys` y por el ejemplo `terminal_demo`. Pendiente de que lo pruebe el usuario.
- **ADR-0001**: nada lee credenciales; de `~/.claude` solo se leen los
  transcripts `projects/*/<sessionId>.jsonl` (sin seguir enlaces simbólicos); de
  Codex solo se usan `initialize`, `thread/loaded/list` y `thread/read`; no se
  envía texto a ningún modelo; detener usa `claude stop`.

## Resultado

Hecho en la rama (pendiente de PR, versión y CI, que decide el usuario):

- **Núcleo** (`titan-agent-core`): modelo de sesión serializable, protocolo NDJSON
  versionado ([[Decisiones/ADR-0007 Protocolo NDJSON versionado entre titan-agentd y sus clientes]]),
  rutas del socket, descubrimiento puro de Claude Code y Codex, árbol por
  proyecto con anidado por `parentThreadId`, «estado desde» y codificación de teclado.
- **Daemon** (`titan-agentd`): PTY con apagado acotado (SIGHUP → SIGKILL al
  grupo), emulador sin interfaz que contesta las consultas del programa e
  instantánea ANSI, servidor con colas acotadas por cliente, terminales
  compartidos que sobreviven a la ventana, descubrimiento solo con suscritos,
  fuentes reales (`claude agents --json`, WebSocket propio al `codex app-server`)
  y `--fixture` para demos ([[Decisiones/ADR-0008 Terminal con alacritty_terminal en el daemon y en la interfaz]]).
- **Interfaz** (`titan-agent`): cliente con autoarranque del daemon y cola FIFO,
  modelo de terminal con la paleta de titan-ssh, vista GPUI del terminal con
  JetBrains Mono embebida (OFL), y ventana principal con cabecera, árbol,
  barra de sesión, terminal, detalle, monitor y «Nueva sesión» provisionales.
  `TITAN_AGENT_SELECT` fija la selección inicial (demos y capturas).
- **Anidado «lanzada por»**: solo Codex lo expone (`parentThreadId`); Claude Code
  no tiene dato fiable, así que sus sesiones no se anidan en esta fase.
- **Fuera**: Windows (ConPTY y named pipe), lanzar sesiones (fase 2), monitor real
  (fase 3). Codex no tiene comando oficial para detener un hilo: el botón cierra
  el terminal alojado.

### Autoevaluación visual

Capturas reales de la ventana (daemon con `--fixture` y los datos inventados de la
previa) frente a la previa `design/preview/Main.dc.html` renderizada:

| Previa | Ventana real |
| --- | --- |
| ![[Adjuntos/Fase 1 equipo v2/previa-sesion.png]] | ![[Adjuntos/Fase 1 equipo v2/real-sesion.png]] |
| ![[Adjuntos/Fase 1 equipo v2/previa-monitor.png]] | ![[Adjuntos/Fase 1 equipo v2/real-monitor.png]] |
| ![[Adjuntos/Fase 1 equipo v2/previa-nueva.png]] | ![[Adjuntos/Fase 1 equipo v2/real-nueva.png]] |

Muy fiel en estructura, medidas, tokens, tipografía y colores: cabecera de 48 px
con contadores, árbol con ramas ASCII y pastillas, barra de sesión, terminal
sobre `terminal-bg`, franja de escritura con foco en `accent`, detalle con
«habla con» y botones. Diferencias conocidas: el orden de proyectos es alfabético
(la previa no tiene orden definido), el monitor es un marcador de la fase 3, y las
ligaduras de JetBrains Mono (`|-`, `->`) se ven igual que en la previa. Pendiente
de opinión del usuario.

### Métricas del experimento (variante «equipo v2»)

- **Tiempo total**: 11:01 → 13:10 (≈ 2 h 10 min), con hasta cuatro encargos en
  paralelo.
- **Claude (director, Opus)**: 320 turnos; 242 527 tokens de salida, 118,2 M
  leídos de caché y 0,54 M escritos en caché (del registro de la sesión; no hay
  `/usage` dentro de una sesión en segundo plano).
- **Codex**: 77 llamadas (22 diseño de tests con Sol medium, 38 implementación con
  Luna, 17 revisiones con Sol high); 38,4 M tokens de entrada (35,9 M en caché) y
  0,36 M de salida; 3 h 44 min de cómputo sumado. Cuota de ChatGPT Plus al final:
  63 % de la ventana de 5 h y 17 % de la semanal.
- **Encargos**: 12, uno por pieza (protocolo, instantánea, PTY, emulador,
  descubrimiento, árbol, cliente, servidor, modelo de vista, vista del terminal,
  fuentes, ventana).
- **Tests**: 253 (núcleo 92, daemon 93, interfaz 68).
- **Revisión**: 17 informes de Sol con 43 hallazgos mayores y 12 menores; casi
  todos aceptados (sobre todo concurrencia y ciclo de vida del daemon, seguridad
  de transcripts y lógica de la ventana); descartados 4 menores sin efecto. El
  servidor agotó las 3 rondas y su último arreglo lo verificó el director.
  Además, el director encontró en capturas 6 defectos visuales que no detectan
  los tests (canvas sin tamaño, fuente no aplicada, desbordes con muchas sesiones…).
- **Incidencias del flujo**: gitleaks rechazó dos veces tests con rutas
  inventadas bajo `/home/…`; el revisor leyó `AGENTS.md` de las carpetas padre (se
  añadió al prompt que la bóveda no está a propósito); Luna no escribe tests
  unitarios aunque se le pidan («no toques los tests» lo interpreta en sentido
  amplio); la ventana no cupo en un turno y se partió en cinco.
