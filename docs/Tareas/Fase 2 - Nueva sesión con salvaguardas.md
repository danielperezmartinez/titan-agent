---
Nombre: Fase 2 - Nueva sesión con salvaguardas
Estado: En curso
Resumen: Botón Nueva sesión en la columna izquierda y cuadro de encargo en el centro con los valores por defecto y botones para cambiar agente, modelo, esfuerzo, permisos o sandbox y proyecto; muestra el comando exacto y lanza el CLI oficial desde el daemon con las cinco salvaguardas de ADR-0002.
Decisiones: 'ADR-0002 (salvaguardas, aplicadas en el daemon); ADR-0010 propuesta (estado done de Claude entre turnos). Campo de texto propio, sin crate (no requiere ADR). Variante opus del experimento de comparación (rama feat/fase-2-opus, sin push).'
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-15T20:50:00+01:00
---

# Fase 2 - Nueva sesión con salvaguardas

## Objetivo

- Interfaz: la parte «Nueva sesión» de
  [[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]]
  (previa: estado `inicio="nueva"` de `design/preview/Main.dc.html`).
  GPUI no trae campo de texto: resolverlo (ADR si se elige un crate).
- Daemon: lanzar Claude Code con `claude --bg -n <nombre> --model … --effort …
  --permission-mode … "<encargo>"` en la carpeta del proyecto, y Codex por su vía
  oficial (TUI de `codex` en un PTY o `thread/start` de `codex app-server`).
- Las **cinco salvaguardas** de
  [[Decisiones/ADR-0002 Lanzar sesiones desde la app con salvaguardas]]: el texto
  lo escribe el usuario; un clic, un lanzamiento (sin programar, repetir ni
  encadenar); binario oficial sin modificar y sin leer credenciales; el comando
  exacto visible antes de lanzar; Codex por su vía oficial. Con tests que lo
  comprueben donde se pueda (p. ej. el daemon rechaza un lanzamiento sin texto
  del usuario o duplicado).
- Los valores por defecto se configuran en `[*]` (configuración).

## Criterios de finalización

- Lanzar una sesión de cada CLI desde la app: aparece en el árbol, queda
  seleccionada y su terminal funciona.
- Las salvaguardas están cubiertas por tests o, si no se puede, verificadas a
  mano y anotadas aquí.

## Verificación

Variante **opus** del experimento (rama `feat/fase-2-opus`, sin push), en local
el 2026-11-15:

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` y `cargo test --workspace` en verde: **60 tests de Rust** (37 de las
  fases anteriores y 23 nuevos: 16 de `launch` en el núcleo, 1 del protocolo,
  3 de punta a punta del daemon con CLI falsos y 3 del campo de texto; además se
  amplió el de estados de Claude).
- **Salvaguardas de ADR-0002**, comprobadas por tests del daemon con CLI falsos
  (`crates/titan-agentd/tests/daemon.rs`) y del núcleo
  (`crates/titan-agent-core/tests/launch.rs`):
  1. Texto del usuario: el daemon rechaza el encargo vacío o en blanco
     (`empty_brief`) sin ejecutar nada; el argv lleva el encargo byte a byte
     (comillas, `$`, saltos de línea, `ñ`) y con `--` si empieza por guion. No
     hay plantillas ni texto propio.
  2. Un clic, un lanzamiento: el mismo clic no lanza dos veces y el mismo
     encargo tampoco en 10 s (`duplicate_launch`); el CLI falso confirma una
     sola ejecución. Nada se reintenta (si el daemon se cae a mitad, la interfaz
     lo dice y no repite). No hay lanzamientos programados ni por eventos.
  3. Binario oficial y sin credenciales: se ejecuta `claude`/`codex` del `PATH`
     tal cual; el código de lanzamiento no abre ningún fichero de las CLI
     (revisado a mano).
  4. Comando visible: el cliente manda el comando que mostró y el daemon lo
     compara con el suyo (`command_mismatch` si difiere).
  5. Codex por su vía oficial: el TUI de `codex --remote unix://` en un PTY del
     daemon, unido al `codex app-server` compartido.
- **Prueba real** con sesiones de prueba propias en este worktree y un daemon en
  un socket aparte (el del usuario no se tocó): Claude Code (haiku, low, plan)
  y Codex (luna, low, read-only), encargo «Responde solo: hola…». Las dos se
  lanzaron desde la interfaz con `ctrl+enter`, aparecieron en el árbol,
  quedaron seleccionadas con «lanzada por: tú (titan-agent)», su terminal
  mostró el CLI oficial respondiendo y lo tecleado llegó a su campo de entrada.
  Después se pararon y borraron (`claude stop`/`rm`, `codex delete --force`).
  La primera prueba de Codex sin `--remote` salió como «terminada» (el hilo no
  quedaba en el daemon compartido): se corrigió y se repitió.
- **Capturas** de la ventana (XWayland, capturada con `XGetImage` para no
  depender de la ventana activa del escritorio) con datos inventados en
  `.previa/fase-2-opus/` (ignorada por git): cuadro vacío, encargo escrito,
  sesión de Claude lanzada, listas de proyecto y permisos, configuración,
  sesión de Codex lanzada y encargo de varias líneas con tildes. Kit de demo
  (CLI falsos y `codex app-server` simulado) en `.previa/fase-2-opus/demo/`.
- Pendiente: PR, CI, versión (proponer 0.3.0, minor, a acordar con el usuario)
  y aceptar o no ADR-0010.

## Resultado

- **Núcleo** (`launch.rs`): `LaunchSpec` con validación, argv oficial, comando
  visible citado para shell, nombre de la sesión de Claude, valores por defecto
  y emparejado del TUI de Codex con su hilo. Protocolo: método `launch` y
  `Session.launched_here` (aditivos, v1).
- **Daemon**: `launch` con las salvaguardas, `claude --bg` o TUI de Codex en un
  PTY, sondeo inmediato tras lanzar, vinculación del PTY con su hilo y respuesta
  antes de leer el terminal (no se pierde su primera salida).
- **Interfaz**: campo de texto multilínea propio con IME
  ([[Catálogo técnico/Campo de texto]]), cuadro de nueva sesión con sus listas
  y el comando exacto, `[*]` configuración con los valores por defecto
  ([[Catálogo técnico/Cuadro de nueva sesión]]); la sesión lanzada queda
  seleccionada (Claude al aparecer en el árbol; Codex al instante, con su
  terminal, y pasa a su hilo cuando aparece).
- Corrección de la fase 1: sesiones de Claude en segundo plano vivas con
  `state: done` salían `[x] terminada`
  ([[Decisiones/ADR-0010 Estado de las sesiones de Claude Code en segundo plano entre turnos]],
  propuesta).
- Catálogo: [[Catálogo técnico/Lanzamiento de sesiones]],
  [[Catálogo técnico/Campo de texto]], [[Catálogo técnico/Cuadro de nueva sesión]]
  nuevas; [[Catálogo técnico/Protocolo de titan-agentd]],
  [[Catálogo técnico/titan-agentd]], [[Catálogo técnico/Tipos de sesión]] y
  [[Catálogo técnico/Descubrimiento de sesiones]] actualizadas.

### Experimento: variante opus

Opus implementa solo, sin equipo, subagentes ni Codex.

- **Tiempo**: de 20:06 a 20:45 (unos 40 min) hasta el resultado
  verificado, documentado y en un commit local.
- **Uso de Claude** (transcript de la sesión, Opus 5.5): unas 165 respuestas y 170 llamadas a
  herramientas; ~185 k tokens de salida, ~390 k de escritura en caché, ~44 M de
  lectura de caché y ~300 de entrada sin caché (medido al cerrar la nota).
- **Codex**: ninguna llamada para desarrollar; dos hilos de prueba con luna/low
  para verificar el lanzamiento real (borrados).
- **Tests**: 23 nuevos (60 en total).
- **Hallazgos de revisión** (revisión propia, sin revisor externo): 3
  corregidos antes del commit: la salida inicial del TUI de Codex podía llegar
  antes que la respuesta de `launch` y perderse (lo destapó un test); un
  lanzamiento sin respuesta dejaba el botón en «lanzando…» si se caía el
  daemon; y el TUI de Codex sin `--remote` no quedaba en el daemon compartido
  (lo destapó la prueba real). Además, la prueba real destapó el fallo de la
  fase 1 de ADR-0010.
- **Autoevaluación visual** frente a `design/preview/Main.dc.html`
  (`inicio="nueva"`): título 28 px, subtítulo «en [#] proyecto con agente» con
  el color del agente, cuadro de 820 px con borde `accent` sobre `canvas`,
  marcador gris, botones de 32 px con `[v]` y borde `accent` al abrirse, lista
  sobre `surface-elevated` con `[ok]`, etiqueta, nombre y pista a la derecha,
  `[ok] lanzar` con borde `accent` y la línea «se ejecutará» en 11 px coinciden
  con la previa. Diferencias: `[ok] lanzar` se ve apagado sin encargo (la previa
  siempre lo muestra activo; aquí no hay encargo de prueba, ADR-0002); el
  comando lleva `-n <nombre>` y, en Codex, `--remote unix://`; los modos de
  Claude son los del CLI actual (`manual`, `plan`, `acceptEdits`, `auto`; la
  previa decía `default`) y los modelos incluyen `fable` y `gpt-6-astra`; las
  listas llevan una pista corta en permisos; el comando largo parte línea en
  los guiones de `--permission-mode`; `[*]` configuración no tiene previa (se
  hizo con las mismas piezas). Una incidencia del entorno: en el escritorio
  compartido otra sesión activaba sus ventanas a la vez y una captura con
  `spectacle -a` cogió por error la ventana de la variante hermana; se
  descartó sin usarla y se pasó a `XGetImage` sobre la ventana propia.
