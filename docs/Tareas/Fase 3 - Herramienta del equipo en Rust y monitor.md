---
Nombre: Fase 3 - Herramienta del equipo en Rust y monitor
Estado: En curso
Resumen: Reescribir en Rust la herramienta del equipo (titan-team, con su skill) con un formato de eventos versionado, hacer que el daemon lo lea, y construir el monitor del proyecto con sus tres vistas (grafo, carriles, mesa de control) en la zona central.
Decisiones: '[[Decisiones/ADR-0010 Formato de eventos del equipo en JSONL versionado]]'
Bloqueada: ["[[Fase 2 - Nueva sesión con salvaguardas]]"]
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-16T20:35:00+01:00
---

# Fase 3 - Herramienta del equipo en Rust y monitor

## Objetivo

- **`titan-team`** según
  [[Decisiones/ADR-0005 Herramienta del equipo en Rust dentro de titan-agent]]:
  mismas órdenes y modelo que el prototipo (`roles/new/ask/show/diff/test/close`,
  `team.toml`, hilos por `thread_id`, worktree y commit por turno, aviso de rutas
  prohibidas, tope de rondas, informe del revisor con esquema, `--json`), sin el
  adaptador `claude -p`. **ADR** con el formato de eventos. Sustituye al
  prototipo en Python, que se retira del repo. Conserva el aislamiento de los
  roles (`[codex].config` y `project.hidden_paths`, ver
  [[Tareas/Aislar a los roles de Codex de las instrucciones del repo]]).
- **Configurable por proyecto y por una IA** (petición del usuario,
  2026-11-14): nada del equipo va fijo en el código. Roles, CLI, modelo,
  esfuerzo, sandbox, prompts, rutas, flujo y tope de rondas viven en el
  `.agents/team.toml` de cada proyecto, con su formato documentado y validado
  (error claro ante un valor desconocido), de modo que un agente director pueda
  montar o cambiar el equipo de otro proyecto editando ese fichero (o con una
  orden de `titan-team` para ello) sin tocar la herramienta.
- **Daemon**: lee los eventos y los une a las sesiones para saber quién habla con
  quién y en qué ronda va cada encargo.
- **Monitor** de [[Decisiones visuales/Monitor del proyecto en tres vistas]]:
  la cabecera del proyecto en el árbol lo abre; barra con
  `[grafo] [carriles] [mesa de control]`; animaciones (punto que viaja, carriles
  que corren, línea ASCII, spinner). Previa: `MonitorA/B/C.dc.html` y el estado
  `inicio="monitor"` de `Main.dc.html` en `design/preview/`.
- La cuota solo si [[Investigar la cuota restante de cada CLI]] confirma que se
  puede obtener; si no, la tarjeta de la mesa de control va sin ella.

## Criterios de finalización

- Un encargo real del equipo se ve en las tres vistas en tiempo real.
- El prototipo en Python ya no está en el repo.
- Un agente monta el equipo de un proyecto nuevo solo con `team.toml`, sin
  cambiar código.

## Criterios visuales

Escritos antes de implementar (variante «opus»), frente a `MonitorA/B/C.dc.html`,
`Main.dc.html` (`inicio="monitor"`) y las decisiones visuales. No se cambian para
que el resultado pase.

1. **Barra y jerarquía**: barra de 40 px con `[#] proyecto` en `ink` negrita,
   «encargo · ronda n de m · k agentes» en `mute`, `[*] en vivo` verde que late
   y, a la derecha, `[grafo] [carriles] [mesa de control]` con la vista elegida
   en `surface-elevated` y borde `accent`. Fondo del monitor `terminal-bg`.
2. **Grafo**: «tú» arriba, el director en el centro y los roles alrededor
   (primer paso a la izquierda, revisor a la derecha, el resto abajo), nodos de
   220×96 en `canvas` con hairline; el que trabaja con borde verde y `[>]` con
   spinner `| / - \`; el mensaje viaja como punto del color de quien envía con su
   etiqueta en una pastilla; abajo «AHORA» con los últimos eventos que se
   apagan.
3. **Carriles**: frase «AHORA» en 20 px negrita; columna de 240 px con una fila
   de 72 px por agente; barras verdes (trabajando) y ámbar (te necesita) con
   borde superior de 2 px, líneas verticales del color de quien envía con el
   punto en quien recibe; línea `accent` «ahora» a la derecha; el tiempo corre
   hacia la izquierda; leyenda al pie.
4. **Mesa de control**: franja HABLANDO / TRABAJANDO / TE NECESITAN con la
   línea ASCII animada (`opus ----*---> sol`) y la ronda en bloques; tarjetas
   con borde de 4 px de radio, director a todo el ancho y el resto en dos
   columnas; nombre en 20 px negrita, estado, acción, «n s en este estado»,
   histograma de actividad de 24 barras, turnos y tokens, y cuota en 10 bloques
   (ámbar y «· baja» si queda poca).
5. **Color y acabado**: violeta solo para Claude Code y cian solo para Codex
   (pastillas `cl`/`cx` y lo que envía cada uno); verde, ámbar y rojo solo para
   estados; `accent` solo para interacción y la línea «ahora»; JetBrains Mono en
   todo, sin sombras ni gradientes; nada se corta ni se solapa a 1440×900.

## Verificación

Variante **opus** (rama `feat/fase-3-opus`, sin push), en local el 2026-11-16.
Opus implementa solo: sin la herramienta del equipo, sin subagentes y sin Codex
salvo dos llamadas mínimas para la prueba real.

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` y `cargo test --workspace` en verde: **102 tests de Rust** (64 de
  las fases anteriores y 38 nuevos). Los 15 tests de Python de `bin/` se
  retiraron con el prototipo; sus casos tienen equivalente en Rust
  (`crates/titan-team/tests/cli.rs` y los tests de `team::config`). Hooks de
  gitleaks en cada commit, sin fugas.
- **Encargo real** (criterio 1): repositorio desechable con solo un
  `.agents/team.toml` nuevo (un rol Luna, esfuerzo bajo y solo lectura),
  `titan-team new` + dos `titan-team ask` reales (un hilo nuevo y su
  continuación). Un daemon de prueba con los CLI reales en un socket propio
  (no el de main ni el de la otra variante) unió el director por su id de
  sesión y el rol por su `thread_id`, y la ventana, filtrada a ese proyecto,
  mostró en vivo el punto viajando de opus a luna, luna trabajando con su
  spinner, y la vuelta y el feed en las otras vistas. El hilo de prueba se
  borró después (`codex delete --force`) y el encargo se cerró (`titan-team
  close --delete-branch`). No se tocó ninguna sesión real del usuario.
- **Sin Python** (criterio 2): `bin/team` y `bin/test_team.py` se borraron,
  igual que su paso del CI. `bin/titan-team` compila y ejecuta el crate.
- **Equipo solo con `team.toml`** (criterio 3): la prueba real usó un proyecto
  nuevo sin tocar código. `titan-team init` crea uno de partida, `check` lo
  valida (una clave o un valor desconocido sale con su línea y los valores
  válidos) y `doc` imprime el formato (`crates/titan-team/FORMAT.md`). Tests
  en `check_init_and_doc_let_an_agent_set_up_a_team`.
- **Capturas** de la ventana real (spectacle; datos inventados en un `--fixture`
  y un log de eventos escrito por un guion) en el directorio temporal del
  trabajo, no versionadas. **Incidente**: dos capturas cogieron la ventana de la
  otra variante, que estaba haciendo sus propias capturas a la vez. El script de
  enfoque usaba una ruta relativa que KWin no cargaba, y luego hubo una carrera
  de foco. Se borraron sin usarlas y el script solo da una captura por buena si
  KWin confirma el PID de mi ventana **antes y después** del disparo; si no,
  borra el fichero. Todas las capturas restantes se compararon por contenido con
  una de referencia.
- **Pendiente del usuario**: pulsar en la ventana (clic en un nodo, una
  tarjeta o un carril abre su sesión; los tres botones de vista; `ventana 36 s
  [v]`). No hay inyección de clics en esta sesión.

### Criterios visuales (autoevaluación con capturas, sin evaluador independiente)

| Criterio | Resultado |
| --- | --- |
| 1. Barra y jerarquía | Cumple. |
| 2. Grafo | Cumple. Ajustes deliberados: el nodo «tú» mide 184 px y no 160, para que quepa «responde en la sesión», y la etiqueta del mensaje va encima de los nodos, centrada en el hueco (en la previa queda debajo de los nodos). |
| 3. Carriles | Cumple. Las etiquetas de mensajes muy seguidos se pisan, igual que en la previa. Añadido: `ventana 36 s [v]` cambia la ventana a 5 y 30 min, porque la decisión dice «configurable». |
| 4. Mesa de control | Cumple, incluida la cuota real de Codex (bloques ámbar y «· baja» con un 30 % o menos). |
| 5. Color y acabado | Cumple. GPUI no tiene espaciado entre letras, así que «AHORA» y «HABLANDO» van sin él. El latido ámbar de «te necesita» es un borde que pulsa, no un halo de sombra. |

Nota global propia: 9/10.

### Métricas del experimento (variante opus)

- **Tiempo**: de 18:23 a ~20:40. Unos **55 min de trabajo activo**, más 81 min
  de pausa (de 19:00 a 20:21) porque el usuario alcanzó su límite de uso.
- **Uso de Claude** (transcript, Opus 5.5, hasta las 20:26): 137 respuestas;
  200 k tokens de salida, 799 k de escritura en caché y 40,2 M de lectura de
  caché. Sin subagentes.
- **Codex**: 2 llamadas (Luna, low) solo para la prueba real: 53,9 k de entrada
  (47,5 k en caché) y 77 de salida. Sol: 0.
- **Tests**: 38 nuevos (102 en total).
- **Hallazgos**: no hubo revisión independiente en esta variante. Fallos que
  encontré yo al probar con la ventana real, todos corregidos: el daemon no
  releía un log reemplazado (ahora lo detecta por inodo); el nodo «tú» perdía
  una línea; la etiqueta del mensaje quedaba tapada; el director que te necesita
  decía «envía: te necesita»; el rol no enlazaba su sesión hasta acabar el turno
  (ahora la primera `action` lleva el `thread_id`). Y el incidente de las
  capturas.

## Resultado

- **titan-team** (`crates/titan-team`), la reescritura en Rust de `bin/team`.
  Mantiene las mismas órdenes y el mismo estado (compatible con los encargos del
  prototipo): `roles/new/ask/show/diff/test/close`, hilos por `thread_id`,
  worktree con sparse checkout (`hidden_paths`), `[codex].config`, commit por
  turno, avisos de rutas, tope de rondas (salida 3), informe del revisor con
  esquema y `--json`. Añade `check`, `init`, `doc` y `note`, la acción en curso
  del rol leída en vivo de `codex exec --json`, y el log de eventos de
  [[Decisiones/ADR-0010 Formato de eventos del equipo en JSONL versionado]]. No
  lanza Claude Code.
- **Núcleo**: `team::config` (`team.toml` estricto, con `label` por rol),
  `team::events` (formato v1), `team::run` (encargo plegado y unido a las
  sesiones), `quota` y `monitor` (modelo puro de las tres vistas, con sus
  tests).
- **Daemon**: lee los logs de eventos junto a las sesiones cada segundo y de
  forma incremental, lee la cuota de Codex cada minuto
  (`account/rateLimits/read`) y la de Claude si existe el puente de la
  statusline. `--fixture` sirve datos de prueba. `teams` y `quotas` van en el
  snapshot. Los lanzamientos se movieron a `hub/launches.rs` (`hub.rs` ya no
  pasa de 800 líneas).
- **Interfaz**: `app/monitor.rs` (barra, botones y estado sin equipo), y una
  vista por fichero: `monitor_graph.rs`, `monitor_lanes.rs` y
  `monitor_desk.rs`. Anima a 20 fps solo mientras se ve un monitor con equipo;
  el resto del tiempo, una vez por segundo. `[*]` en el árbol cuando un
  proyecto tiene un encargo activo. Para capturas: `TITAN_AGENT_MONITOR_VIEW` y
  `TITAN_AGENT_LANES_WINDOW`.
- Skill `equipo` y `.agents/team.toml` pasados a `titan-team`.
- **Falta para cerrar**: acordar la versión con el usuario (propuesta: 0.4.0,
  `minor`), abrir la PR e integrarla, y la prueba de clics del usuario. Sigue
  `En curso`.
