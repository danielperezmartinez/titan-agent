---
Nombre: Fase 3 - Herramienta del equipo en Rust y monitor
Estado: Hecha
Resumen: Reescribir en Rust la herramienta del equipo (titan-team, con su skill) con un formato de eventos versionado, hacer que el daemon lo lea, y construir el monitor del proyecto con sus tres vistas (grafo, carriles, mesa de control) en la zona central.
Decisiones: '[[Decisiones/ADR-0010 Formato de team.toml y de los eventos del equipo]]. Se integra la variante equipo v4 (rama feat/fase-3-equipo-v4) por recomendación del evaluador independiente, con init/doc, catálogo y la unión exacta del director traídos de la variante A (feat/fase-3-opus). Hilo ocupado: salida 4 y status busy, conservando el hilo; la otra mitad en [[Abrir los hilos del equipo en solo lectura desde la app]]. Cuota de Codex por account/rateLimits/read; Claude «sin dato». bin/team pasa a envoltorio de titan-team. Versión 0.4.0 (minor).'
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-16T22:06:00+01:00
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

## Especificación (contrato, fijado antes de los tests)

El contrato completo (formato de `team.toml`, validación y sus códigos, órdenes
y códigos de salida de `titan-team`, formato de `events.jsonl`, modelo
`TeamRun` y `Quota` del núcleo, campos `teams` y `quotas` del protocolo) está
en [[Decisiones/ADR-0010 Formato de team.toml y de los eventos del equipo]].
Se fijó allí antes de que Sol escribiera los tests de aceptación, que lo
comprueban sin añadir detalles.

Decisiones de producto tomadas por defecto (sin pregunta abierta):

- **Cuota**: Codex sí, con `account/rateLimits/read` del app-server compartido
  (una vez por minuto). Claude Code no tiene vía oficial apta para el daemon:
  su tarjeta dice «cuota: sin dato». El puente por la statusline lo decide y lo
  configura el usuario si lo quiere (ver
  [[Investigar la cuota restante de cada CLI]]).
- **Encargo mostrado**: el más reciente sin cerrar del proyecto, con actividad
  en las últimas 12 h.
- **Ventana de carriles**: 36 s por defecto, cambiable desde la propia vista.
- `bin/team` pasa a ser un envoltorio de `titan-team` (el prototipo en Python
  y sus tests se retiran).

## Criterios visuales (escritos antes de implementar)

Variante **equipo v4**. El evaluador visual (un subagente de Opus nuevo en
cada ronda, sin el contexto de la implementación) compara capturas reales de
la ventana con `design/preview/MonitorA/B/C.dc.html` y el estado
`inicio="monitor"` de `Main.dc.html` según estos criterios, fijados antes de
implementar:

1. **Barra y estructura.** Barra de 40 px sobre hairline con `[#] proyecto`
   (13 px negrita `ink`), «encargo X · ronda r de max · N agentes» (12 px
   `mute`), `[*] en vivo` en `success` que late, y a la derecha los botones
   `grafo` `carriles` `mesa de control` de 30 px separados 4 px (el elegido
   con borde `accent`, fondo `surface-elevated` y texto `ink`; los demás con
   borde hairline y texto `mute`). Contenido sobre `terminal-bg`. Sin equipo:
   «Sin equipo trabajando en X» 20 px negrita y líneas 13 px `mute` con las
   sesiones sueltas.
2. **Grafo.** `tú` arriba, el director en el centro y los roles alrededor
   (izquierda, derecha, abajo), cada uno en una caja de radio 4 px sobre
   `canvas` con: etiqueta `cl`/`cx` en su color, nombre en negrita `ink`,
   marcador a la derecha, «rol · modelo» 11 px `mute` y la acción 12 px.
   Aristas de 2 px en hairline que se tiñen del color de quien envía mientras
   viaja un mensaje: un punto de 10 px que recorre la arista y una etiqueta
   11 px con borde de ese color sobre `surface`. Debajo, tras una hairline,
   «AHORA» con los últimos 4 eventos («ahora», «hace N s») y la leyenda a la
   derecha.
3. **Carriles.** «AHORA» 12 px y la frase de lo que pasa en 20 px negrita
   `ink`; tabla con borde hairline sobre `canvas`: columna de 240 px con
   filas de 72 px (etiqueta, nombre, rol, marcador; acción) y la pista a la
   derecha con barras `success` (relleno al 28 % y borde superior de 2 px),
   ámbar para «te necesita», líneas verticales de 2 px del color de quien
   envía con un punto de 10 px en quien recibe y su etiqueta, y la línea
   `accent` «ahora» a 40 px del borde derecho; todo corre hacia la izquierda
   de forma continua. Leyenda debajo.
4. **Mesa de control.** Franja con «HABLANDO / TRABAJANDO / TE NECESITAN»,
   `de ----*---> a` en 18 px del color de quien envía con el `*` avanzando, la
   etiqueta, y a la derecha «ronda» con segmentos y «r de max». Tarjetas en
   rejilla de 2 columnas con 16 px de separación (director a todo el ancho):
   etiqueta, nombre 20 px negrita, «rol · cli · modelo · esfuerzo» 12 px
   `mute`, marcador 16 px; acción 14 px y «N s en este estado»; histograma
   de 24 barras de hasta 32 px (verde trabajando, color del agente en
   mensajes, ámbar «te necesita», gris en reposo); estadísticas 11 px y cuota
   en 10 segmentos (ámbar si es baja) o «cuota: sin dato».
5. **Estados, color y acabado.** Spinner `| / - \` junto a `[>]` en
   `success`; `[?]` y su borde laten en `warning`; `[+]` en `mute`. Solo
   colores de los tokens (violeta `cl`, cian `cx`, `success`, `warning`;
   `accent` solo para interacción y «ahora»); JetBrains Mono en todo; datos
   reales del daemon, no fijos; nada se corta ni se solapa a 1440×900 en las
   tres vistas.

## Experimento: variante equipo v4

Parte de la configuración v3 ([[Fase 2 - Nueva sesión con salvaguardas]],
«Experimento: variante equipo v3») con estos ajustes:

1. **Contrato antes que tests.** El director (Opus) fija en la especificación
   (aquí y en ADR-0010) nombres, formatos, límites y códigos de error antes de
   que Sol (gpt-6.1-sol, medium) escriba un número pequeño de tests de
   aceptación que comprueban comportamiento sin inventar detalles. El director
   no puede modificarlos: si uno está mal, se lo devuelve a Sol con su
   argumento o lo consulta con el usuario.
2. El director implementa cada parte con sus tests.
3. Una sola revisión final de Sol (high) en un hilo nuevo, con solo el diff,
   la especificación y la regla de estructura (§7); el director decide y anota
   por qué.
4. Evaluador visual: un subagente de Opus **nuevo en cada ronda**, sin el
   contexto de la implementación, con capturas reales solo de esta ventana y
   los criterios de arriba. Como mucho 2 rondas de corrección.
5. Luna (gpt-6-luna) solo para trabajo mecánico sin criterio, nunca la
   interfaz. En esta fase: dividir `hub.rs` bajo las 800 líneas moviendo código.

Los roles de Codex mantienen su sandbox. Inicio: 2026-11-16 18:23.

## Criterios de finalización

- Un encargo real del equipo se ve en las tres vistas en tiempo real.
- El prototipo en Python ya no está en el repo.
- Un agente monta el equipo de un proyecto nuevo solo con `team.toml`, sin
  cambiar código.

## Verificación

Variante **equipo v4** (rama `feat/fase-3-equipo-v4`, sin push), en local el
2026-11-16:

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` y `cargo test --workspace` en verde: **107 tests de Rust** (65 de
  antes y 42 nuevos: **7 de aceptación** de Sol, sin tocar desde su commit
  `4bc8129`, y **35 propios**). Los 15 tests de Python de `bin/test_team.py` se
  retiraron con el prototipo. Hooks de gitleaks en cada commit, sin fugas.
- `crates/titan-agent-core/tests/protocol.rs` (de la fase 1, no de Sol) añade
  `teams: vec![]` y `quotas: vec![]` a su literal del evento `sessions`:
  adaptación mecánica para compilar, sin tocar aserciones.
- **Encargo real del equipo en el monitor**: la propia revisión de Sol se hizo
  ya con `titan-team` en Rust (`bin/team`), y sus eventos reales (encargo,
  turno, cada orden que ejecutaba Sol) se vieron en vivo en las tres vistas,
  con un daemon de prueba en un socket propio, CLI falsos y una sesión de
  director inventada en este worktree (capturas `real-*.png`). No se tocó
  ninguna sesión real del usuario ni el socket de la build de main.
- **Equipo de un proyecto nuevo solo con `team.toml`**: en un repo vacío y
  temporal, un `team.toml` con otros roles (jefe y crítico) y un prompt;
  `titan-team check` explicó un valor erróneo (`roles.critico.effort:
  unknown_value`) y, corregido, `roles`, `new` y `ask --dry-run` funcionaron sin
  cambiar código. Se borró después.
- **Cuota**: `account/rateLimits/read` comprobado contra el Codex real (lectura
  en frío, sin hilos); en las capturas, la cuota viene de un app-server falso.
- Capturas reales solo de la ventana de esta variante (KWin confirma el PID de
  la ventana activa antes de cada captura y se descarta la que no mide
  1440×900) en `.previa/fase-3-equipo-v4/` (ignorada por git), con datos
  inventados; previas renderizadas en `previa-*.png`.
- **Pendiente del usuario**: probar la ventana (clic en los botones, en un
  agente para abrir su sesión y en `[ventana 36 s]`): no hay inyección de
  clics. Y acordar la versión (propuesta 0.4.0) antes de la PR.

## Resultado

- **`titan-team`** (crate `crates/titan-team`): mismas órdenes que el
  prototipo (`roles/new/ask/show/diff/test/close`) más `check` y `decide`;
  `team.toml` v1 validado con todos los errores a la vez (ruta y código);
  opciones por orden; bloqueo por encargo; tope de rondas antes de ejecutar
  nada; Codex sin shell, con tiempo límite que mata su grupo de procesos;
  commit por turno y avisos de rutas; `events.jsonl` versionado con la
  actividad en vivo de Codex. `bin/team` es ahora un envoltorio de shell.
- **Núcleo**: `team` (eventos v1 y `TeamRun`), `quota`
  (`parse_codex_rate_limits`) y `monitor` (el tablero que dibujan las tres
  vistas, con sus tests). Protocolo: `teams` y `quotas` en `sessions`.
- **Daemon**: `teams.rs` sigue los `events.jsonl` de las carpetas de las
  sesiones conocidas (lectura incremental, solo lectura); cuota de Codex una
  vez por minuto. `hub.rs` dividido por Luna en `hub.rs`, `hub/launch.rs` y
  `hub/terminals.rs` ([[Llevar hub.rs y brief.rs bajo el umbral de tamaño]]).
- **Interfaz**: `app/monitor.rs` (barra, pestañas y piezas comunes) y una vista
  por módulo: `monitor/graph.rs`, `monitor/lanes.rs`, `monitor/desk.rs`. El
  monitor se redibuja cada 100 ms solo mientras está en pantalla.
- Bóveda: [[Decisiones/ADR-0010 Formato de team.toml y de los eventos del
  equipo]] (con sus ampliaciones tras la revisión),
  [[Investigar la cuota restante de cada CLI]] recuperada y al día, y la
  skill `equipo` y `team.toml` descritos como v4.
- Aplazado: abrir desde el monitor un hilo de Codex que el daemon ya no lista
  (más de 20 min sin actualizarse) necesitaría una orden nueva del protocolo.
  Pendiente aparte: dividir `brief.rs`.

### Revisión de Sol (una ronda, contexto limpio)

`fase-3-revision`, gpt-6.1-sol high, hilo nuevo con la especificación y el
diff `54976da..HEAD`: `changes_requested`, **8 mayores, 10 menores y 1
pregunta**. Decisiones:

- **Aceptados (16)**: tests ejecutados antes del `--dry-run` y del tope de
  rondas; opciones aceptadas por cualquier orden (`close --dry-run` borraba de
  verdad); sin exclusión entre procesos (bloqueo por encargo); un commit
  fallido dejaba el turno sin `turn_finished`; errores de escritura de eventos
  ocultos (ahora no se lanza Codex sin sus eventos iniciales); tiempo límite
  saltado con salida continua; procesos en segundo plano que retenían las
  tuberías; el daemon cerraba encargos con un `task_closed` inválido; avisos de
  rutas sin `autocommit`; `fnmatch` incompleto; `config` sin validar
  `clave=valor`; `thread_id` viejo tras `--new`; hora del último evento en vez
  del máximo; rutas fuera de la base sin `..`; el grafo descartaba roles; la
  salida de `test` truncada.
- **Aceptado en parte (1)**: comentarios en español. Los de `bin/team` pasan
  al inglés; los de `team.toml` y `ci.yml` siguen en español como el resto de
  esos ficheros de configuración.
- **Aplazado (1)**: abrir hilos que el daemon ya no lista (arriba).
- **Pregunta (1)**: con más de 300 eventos se perdía `task_created` y el
  monitor decía «Sin equipo». Respuesta: dos campos opcionales en `TeamTask`
  (`created` y `totals`), sin tocar el límite ni el test de Sol; anotado en
  ADR-0010.

### Evaluación visual (subagente de Opus nuevo en cada ronda)

- Ronda 1: **8/10** (barra 9, grafo 7,5, carriles 6,5, mesa 9, acabado 7). Las
  etiquetas de los mensajes se solapaban o cortaban en carriles y las tapaban
  las cajas en el grafo; al nodo «tú» le faltaba «usuario»; el director no
  mostraba un recuento; «ahora» repetido en el feed. Corregido todo salvo el
  espaciado entre letras de los rótulos (GPUI no lo tiene).
- Ronda 2: **9/10** (barra 10, grafo 9, carriles 8,5, mesa 8,5, acabado 8,5).
  Quedaban: la franja no decía cuál de los dos «sol» hablaba, «1 hallazgos»,
  etiquetas de carriles demasiado cortas y acción cortada en tarjetas
  estrechas. Corregidos (segunda y última ronda de correcciones). La flecha
  `->` no se convierte en ligadura porque la ventana las desactiva a propósito.

### Métricas (experimento: variante equipo v4)

- **Tiempo**: de 18:23 a 20:55, con una parada por el límite de uso de Claude
  de unos 78 min (19:02–20:20). Trabajo efectivo: unos **74 min**.
- **Uso de Claude** (transcript, Opus 5.5): sesión principal 178 respuestas y
  unas 300 llamadas a herramientas; **251 k tokens de salida, 881 k de
  escritura en caché y 62,9 M de lectura de caché**. Subagentes (los tres de
  Opus): render de las previas 13 k de salida, 112 k de escritura y 1,4 M de
  lectura; evaluador ronda 1 25 k, 129 k y 2,2 M; evaluador ronda 2 24 k,
  123 k y 2,4 M.
- **Codex**: 3 llamadas. Luna (mecánico, división de `hub.rs`): 308 k de
  entrada (245 k en caché), 5,8 k de salida, 145 s. Sol (tests de
  aceptación, medium): 298 k (283 k en caché), 11 k de salida, 251 s. Sol
  (revisión, high): 2,59 M (2,44 M en caché), 14 k de salida, 458 s. Cuota de
  ChatGPT Plus al empezar: 3 % usado en 5 h.
- **Tests**: 42 nuevos (7 de aceptación + 35 propios); 107 en total.
- **Hallazgos**: Sol 8 mayores + 10 menores + 1 pregunta (16 aceptados, 1 en
  parte, 1 aplazado, la pregunta resuelta); evaluador visual 6 en la ronda 1
  (5 corregidos) y 5 en la ronda 2 (4 corregidos).
- **Autoevaluación visual**: fiel a las previas en medidas y colores (9/10
  del evaluador independiente, medido al píxel). Diferencias deliberadas:
  datos y roles reales, cuota por cuenta y «sin dato» para Claude, control de
  ventana en carriles, «encargos» en lugar de «turnos» para el director, sin
  espaciado de letras ni ligaduras.
- **Incidencia**: la primera captura cogió por error una ventana de terminal
  del usuario, con la sesión de la otra variante (el foco no había llegado a
  la ventana nueva). Se borró sin usarla y desde entonces cada captura comprueba antes el PID de la ventana
  activa.

## Cierre: integración de la variante equipo v4

Lanzado desde la sesión de control el 2026-11-16 a las 21:23. Un evaluador
independiente recomendó integrar esta variante (la A, de Opus solo, no tenía
bloqueo por encargo y su tiempo límite no mataba el grupo de procesos) y
traer cosas de A. La rama de A no se tocó.

- **De A**: `titan-team init` (con plantillas genéricas de `team.toml`, dos
  prompts y el esquema del revisor, para que `check` pase al momento) y
  `titan-team doc` (con `crates/titan-team/FORMAT.md` embebido, escrito para
  el formato de esta variante); las entradas del
  [[Catálogo técnico]] (titan-team, eventos del equipo, modelo y monitor,
  cuota) adaptadas a lo que hay aquí; y la unión exacta del director por
  `CLAUDE_CODE_SESSION_ID`, que mejora la de B (la sesión más activa de la
  carpeta, que falla con varias): campo opcional `director_session` en cada
  evento.
- **Hilo ocupado**: si el hilo de un rol está abierto en la app, Codex
  rechaza el turno («already has an active writer»). Antes el rol podía
  perder su hilo; ahora `titan-team` lo conserva, no abre otro, escribe
  `turn_finished` con `status = "busy"` y sale con 4 y un mensaje claro; la
  skill `equipo` hace que el director avise y repita cuando el usuario lo
  diga. Contrato en ADR-0010 («Ampliaciones compatibles al cerrar la fase»).
  La otra mitad, en la app, es
  [[Abrir los hilos del equipo en solo lectura desde la app]] (Pendiente).
- **Versión 0.4.0** (minor), acordada en el encargo.

### Revisión de Sol del cierre (una ronda, contexto limpio)

`fase-3-cierre-revision`, gpt-6.1-sol high, hilo nuevo con la especificación
del cierre y el diff `1411551..1e780bd`: `changes_requested`, **1 mayor, 1
menor y 1 nit**. Decisiones:

- **Aceptado (mayor)**: la sesión del director se perdía si dejaba de estar
  en los últimos 300 eventos. El daemon manda ahora la última de todo el
  fichero en `TeamTask.director_session` (como `created` y `totals`), con su
  test.
- **Aceptado (menor)**: tras un turno `busy` del revisor, el monitor seguía
  mostrando la ronda como gastada. `TeamRun` devuelve la ronda que había
  adelantado su `turn_started`, con su test.
- **Descartado (nit)**: comentarios en español en la plantilla de
  `team.toml`. Es un fichero de configuración para el usuario, y en esta fase
  ya se decidió que los comentarios de `team.toml` y `ci.yml` siguen en
  español como el resto de esos ficheros.

### Prueba manual del cierre (usuario, 2026-11-16)

- Ventana 0.4.0 con su propio daemon en un socket aparte y los CLI reales: el
  encargo `fase-3-cierre-revision` se ve en las tres vistas, y el director
  abre esta sesión (unión por `director_session`).
- **Hilo ocupado, con el Codex real**: el usuario abrió el hilo del revisor
  con `codex resume` en su terminal y `bin/team ask revisor … --force` salió
  con **4** en 0,4 s, sin tokens. El rol conservó su hilo y su turno, la
  ronda guardada no cambió, y `turn_finished` quedó con `busy`. Codex solo da
  el error por stderr («thread-store conflict: thread … already has an active
  writer»), que también se revisa. Arreglado de paso: el informe impreso de
  un turno `busy` ya no dice «ronda de revisión 2/1».
- Fallos anotados para fases futuras (sin arreglar en el cierre): el clic en
  un rol de Codex no abre su hilo (en
  [[Abrir los hilos del equipo en solo lectura desde la app]]) y los acentos
  del terminal integrado
  ([[Acentos y teclas muertas en el terminal integrado]]).

### Publicación

- **PR #7** integrada con squash en `main` el 2026-11-16, commit `b5ce3a1`,
  versión **0.4.0**. Checks `Cambios`, `Fugas`, `Verificar` y `Windows` en
  verde. El primer CI falló en Windows: un test de la fase esperaba rutas
  relativas con `/` y `relative()` las daba con `\`. Se arregló haciendo que
  las rutas del estado usen siempre `/` (commit `6022bfa`), y el segundo CI
  salió en verde. GitHub rechazó `--author-email` con el correo noreply
  («Invalid email address»), así que el squash sale con el correo principal
  de la cuenta, como permite [[README#Repositorio y publicación]]. No hay tag
  ni release, porque el pipeline de versiones aún no existe.
- El encargo de revisión `fase-3-cierre-revision` se cerró con su worktree y
  su rama. La rama de la variante A (`feat/fase-3-opus`) y su worktree no se
  tocaron; retirarlos lo decide el usuario.

### Métricas del cierre

Sesión de Claude Code (Opus 5.5) del 2026-11-16, de 21:23 a 22:06 (unos
43 min) desde el encargo hasta este commit. Incluye unos 12 min de espera
por la prueba manual del usuario y unos 8 min de CI. Hubo una interrupción
por el límite de uso de Claude, sin hueco medible en el transcript.

- **Uso de Claude** (transcript): 113 respuestas y unas 106 llamadas a
  herramientas; 70 k tokens de salida, 220 k de escritura en caché y 20,5 M
  de lectura de caché. Sin subagentes.
- **Codex**: 2 llamadas del equipo.
  - Revisión de Sol (gpt-6.1-sol, high): 488 k tokens de entrada (409 k en
    caché), 4,6 k de salida y 144 s.
  - Comprobación del hilo ocupado con el Codex real: rechazada en 0,4 s, sin
    tokens.
- **Tests**: 112 (5 nuevos: el turno `busy` y la sesión del director en el
  núcleo, la sesión exacta y la de todo el fichero en el monitor, `init` y
  `doc`, y el hilo ocupado de punta a punta; la detección por la salida JSON
  y por stderr se añadió a un test existente).
- **Hallazgos de Sol**: 1 mayor y 1 menor, aceptados; 1 nit, descartado.

