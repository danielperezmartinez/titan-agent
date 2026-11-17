---
Nombre: Conversación de sesión y campo de texto común
Estado: Aceptada
Ámbito: Layout
Resumen: '«Nueva sesión» y el chat son la misma vista «conversación» en la zona central, con dos estados: borrador (ajustes, comando exacto y campo de texto) y sesión (barra de sesión, conversación, estado del turno, cola, peticiones al usuario y el mismo campo). El campo es un componente único que se comporta como el de Claude Code Desktop: Enter envía, Shift+Enter hace salto de línea, admite tabulaciones y listas «- », y abre un menú contextual con `/` y `@`. Sin burbujas: bloques planos con hairlines y prefijos por autor. El terminal se abre con `[t]`.'
Decisión: Una vista «conversación» con estado borrador y estado sesión, y un único componente de campo de texto al estilo de Claude Code Desktop, con menú `/` y `@`, para «Nueva sesión» y el chat.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-17T23:25:00+01:00
Última modificación: 2026-11-17T23:55:00+01:00
---

# Conversación de sesión y campo de texto común

## Contexto

[[Decisiones/ADR-0012 Chat de sesión sobre las vías oficiales de cada CLI|ADR-0012]]
sustituye el terminal en crudo por un chat. Hay cuatro peticiones del
usuario, que llegaron desde la sesión de control el 2026-11-17:

- el campo donde escribe es **el mismo componente** en «Nueva sesión» y en
  el chat;
- se comporta como el de Claude Code Desktop (hoy, en «Nueva sesión», Enter
  mete un salto de línea);
- un **menú contextual** con los comandos `/` mientras escribe, como Claude
  Code Desktop o la app de ChatGPT;
- valorar si «Nueva sesión» puede servir de chat.

Hay que tener en cuenta lo que viene en la 0.7.0: un `+` por proyecto que
abre «Nueva sesión» con ese proyecto puesto, y el monitor colgado de la
sesión directora. Esta decisión amplía la zona central de
[[Ventana principal al estilo Claude Code Desktop]] con los tokens de
[[Lenguaje visual heredado de titan-ssh]].

## Decisión

### Vista «conversación» (zona central)

**Una sola vista con dos estados.** Al lanzar, pasa de uno a otro sin
cambiar de pantalla, como en Claude Code Desktop.

- **Borrador** (lo que hoy es «Nueva sesión»):
  - los botones de ajustes de siempre (agente, modelo, esfuerzo,
    permisos o sandbox, proyecto);
  - la línea pequeña con el **comando exacto**, que con Claude incluye el
    `--settings` de los hooks;
  - el campo de texto abajo, y Enter lanza (ADR-0002: acto explícito y
    comando visible);
  - el `+` de cada proyecto (0.7.0) abrirá este estado con el proyecto
    puesto.
- **Sesión**:
  - la barra de sesión de siempre (nombre, estado, agente y modelo,
    carpeta), con `[t] terminal`, `[x] detener` y, en una sesión directora,
    sitio para `[m] monitor` (0.7.0);
  - debajo, la conversación con desplazamiento, abierta por el final;
  - abajo del todo, el campo de texto y una línea de estado.

### Conversación

- **Sin burbujas.** Bloques planos separados por hairlines, con prefijo de
  autor en su color:
  - `tú ›` en `ink`;
  - `cx sol ›` y `cl opus ›` con el color del agente (cian Codex, violeta
    Claude Code).
- **El texto del agente** va en `body`, con streaming: el bloque crece
  mientras llega.
- **El razonamiento** va plegado en una línea `mute` («pensando…»), y se
  despliega.
- **Las órdenes** son líneas compactas `$ orden` con `código N`: el `$` en
  verde o rojo según el resultado, como en la vista de solo lectura de la
  0.5.0.
- **Los cambios de ficheros** son una línea `edita a, b` con `[v]` para
  desplegar el diff (`+` en `success`, `-` en `danger`, fondo
  `terminal-bg`). El diff del turno entero va al final del turno.
- **El plan** es una lista de pasos con su marcador: `[ ]` pendiente, `[>]`
  en curso y `[ok]` hecho.
- **Peticiones al usuario** (aprobaciones de Codex; permisos, preguntas y
  planes de Claude con hooks):
  - un bloque con borde izquierdo `warning` de 2 px y el marcador `[?]`;
  - qué pide (orden, ficheros con su diff, pregunta con sus opciones);
  - los botones (`[ok] permitir`, `permitir en esta sesión`, `denegar`, o
    una opción por fila con «otra»).
  - Al responderla, desde aquí o desde otra pantalla, queda una línea
    `mute` con la respuesta.
  - Sin hooks, el bloque dice «responde en el terminal» con `[t] abrir`.
- **Mensajes en cola** (rol trabajando):
  - bajo la conversación, en `mute`, con el marcador `[+] en cola` y
    `[x] quitar`;
  - al arrancar, pasan a ser un mensaje normal.
- **La línea de estado**, sobre el campo, en `mute`:
  - «libre · se envía ya»;
  - «trabajando · irá a la cola» (con `[x] detener`);
  - «te necesita»;
  - «solo lectura: sesión interactiva en otro terminal»;
  - «no se puede leer la conversación con esta versión: usa el terminal».

### Campo de texto (componente único)

**Es el mismo componente en los dos estados** (`app/composer.rs`).

- **Teclas pedidas**:
  - **Enter** envía (o lanza, en el borrador);
  - **Shift+Enter** hace salto de línea;
  - **Tab** inserta una tabulación;
  - «- » al principio de línea empieza una lista, y Shift+Enter en una
    línea de lista continúa con «- ».
- **Propuestas** (aceptadas con la previa el 2026-11-17):
  - Shift+Enter en un elemento vacío quita la viñeta y acaba la lista;
  - Tab y Shift+Tab sangran y quitan la sangría de un elemento de lista;
  - ↑ con el campo vacío recupera el último mensaje enviado a esa sesión;
  - el campo crece hasta unas 8 líneas y luego desplaza, y al pegar se
    conservan los saltos;
  - un borrador por sesión, que se conserva al cambiar de sesión;
  - Esc cierra el menú y, sin menú, quita el foco.
- **Menú contextual** (pedido):
  - `/` al principio del campo abre los comandos y `@` abre los ficheros
    del proyecto;
  - se filtra al escribir; ↑ y ↓ eligen, y Tab o Enter aceptan;
  - flota sobre el campo, en `surface-elevated` con `hairline-strong`, como
    los menús de ajustes. Cada fila lleva el nombre y una descripción en
    `mute`;
  - qué contiene, según el agente (ADR-0012):
    - **en Codex**: comandos, modelos, permisos y skills del app-server;
    - **en Claude Code**: las skills, que se envían como mensaje, y los
      comandos propios del CLI, con la etiqueta `[t]` porque se ejecutan
      en el terminal.
- **Aspecto**:
  - fondo `surface`, borde `hairline` (`accent` con el foco) y radio
    4 px;
  - el texto de ayuda en `stone`: «escribe… · / comandos · @ ficheros ·
    shift+enter nueva línea».

### Previa

Previa: `design/preview/Conversacion.dc.html`, en la rama
`feat/chat-sesion`; se integra con la PR de la implementación. Estados que
muestra:

- borrador;
- chat de un rol de Codex libre;
- mensaje en cola con el rol trabajando;
- menú `/` abierto;
- aprobación con el diff desplegado;
- pregunta de Claude con sus opciones.

## Alternativas consideradas

- **Pantallas distintas para «Nueva sesión» y el chat**, compartiendo solo
  el componente: descartada. Al lanzar habría un salto de pantalla, y el
  borrador ya es la conversación vacía de la sesión que va a nacer.
- **Burbujas de chat**: descartadas. Rompen el lenguaje plano de titan-ssh
  (sin sombras ni fondos de color) y gastan ancho en una ventana de
  escritorio.
- **Enter salta de línea y Ctrl+Enter envía** (como hoy en «Nueva sesión»):
  descartada por el usuario.
- **El terminal debajo del chat siempre visible**: descartado. Se abre con
  `[t]` y al elegir un comando del CLI de Claude.

## Consecuencias

- El cuadro de «Nueva sesión» y la vista de solo lectura de la 0.5.0
  (`app/thread_view.rs`) se sustituyen por la vista «conversación».
- El campo de texto deja de ser propio de «Nueva sesión» y pasa a ser un
  componente compartido. GPUI no trae uno (ADR-0003), así que crece el
  código propio.
- La 0.7.0 (proyectos importados, `+` por proyecto y monitor colgado de la
  directora) se apoya en esta vista sin cambiarla.
