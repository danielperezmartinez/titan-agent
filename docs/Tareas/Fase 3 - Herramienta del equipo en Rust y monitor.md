---
Nombre: Fase 3 - Herramienta del equipo en Rust y monitor
Estado: En curso
Resumen: Reescribir en Rust la herramienta del equipo (titan-team, con su skill) con un formato de eventos versionado, hacer que el daemon lo lea, y construir el monitor del proyecto con sus tres vistas (grafo, carriles, mesa de control) en la zona central.
Decisiones: '[[Decisiones/ADR-0010 Formato de team.toml y de los eventos del equipo]]'
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-16T18:42:00+01:00
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

## Resultado
