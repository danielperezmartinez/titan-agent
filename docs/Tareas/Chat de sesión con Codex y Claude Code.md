---
Nombre: Chat de sesión con Codex y Claude Code
Estado: Planificando
Resumen: 'Sustituir el terminal en crudo por una vista «conversación» (que también es «Nueva sesión») con un campo de texto común al estilo de Claude Code Desktop y menú `/`. En Codex, por el app-server compartido, con titan-team ejecutando sus turnos en el mismo app-server para que no haya dos escritores. En Claude Code, envío con `claude --resume` en un PTY del daemon, lectura de la transcripción y hooks al lanzar para responder permisos, preguntas y planes. Empieza con un spike con los CLI reales. La hace el equipo v5.'
Decisiones: 'Investigación y diseño con el usuario el 2026-11-17 (sesión «chat-sesion», lanzada desde control-3b): Codex y Claude a la vez; titan-team con cliente JSON-RPC propio (ADR-0013); rol ocupado → cola oficial; previa y decisión visual; ADR-0012 y ADR-0013 aceptadas por el usuario el mismo día; decisión visual aceptada tras revisar la previa; un solo campo de texto para «Nueva sesión» y el chat; menú `/`; diffs, permisos, preguntas y planes en el chat. La versión se acuerda en la PR.'
Bloqueada: []
Fecha de creación: 2026-11-17T23:30:00+01:00
Última modificación: 2026-11-17T23:55:00+01:00
---

# Chat de sesión con Codex y Claude Code

## Objetivo

Que el usuario hable con cualquier sesión desde un chat, sin romper la
comunicación del equipo y con un protocolo que sirva luego a Android:

- **Arquitectura**:
  [[Decisiones/ADR-0012 Chat de sesión sobre las vías oficiales de cada CLI|ADR-0012]].
- **Turnos de titan-team**:
  [[Decisiones/ADR-0013 Turnos de titan-team por el app-server de Codex|ADR-0013]].
- **Aspecto**:
  [[Decisiones visuales/Conversación de sesión y campo de texto común]].

Sustituye la vista de solo lectura y «tomar el control» de
[[Abrir los hilos del equipo en solo lectura desde la app]] y resuelve sus
hallazgos de la prueba manual.

**Cómo se trabaja**:

- con el [[Equipo v5]] (skill `equipo`), en la rama `feat/chat-sesion`, que
  ya trae la previa `design/preview/Conversacion.dc.html`;
- no bloquea nada en curso; si coincide con la 0.7.0 (proyectos
  importados), la que llegue después hace rebase;
- la versión se acuerda con el usuario en la PR.

## Parte 0. Spike con los CLI reales (antes del contrato)

**Reglas**:

- datos inventados, un `CODEX_HOME` y una carpeta de proyecto de prueba
  cuando se pueda, y sesiones de Claude creadas para el spike;
- nada de sesiones reales del usuario;
- resultados anotados en esta nota, en «Spike»;
- **lo que el spike contradiga de las ADR se decide con el usuario antes de
  seguir.**

**Codex 0.169**:

1. `thread/queue/add` con el hilo libre (¿arranca ya?) y con un turno en
   curso (arranca al acabar, sin dirigirlo). `thread/queue/delete` y
   `thread/queue/changed`.
2. Dos clientes del mismo app-server sobre un hilo: los dos reciben las
   notificaciones; una aprobación la responde el primero y el otro recibe
   `serverRequest/resolved`.
3. Los parámetros exactos de `thread/start`, `thread/resume` y `turn/start`
   (sandbox, configuración, esfuerzo, `outputSchema`), y las formas de
   `thread/turns/list`, `item/tool/requestUserInput`, `collaborationMode`
   y `skills/list`.
4. Que un turno lanzado así por titan-team (prototipo) da el mismo
   resultado que `codex exec`: respuesta, informe con esquema, uso y
   actividad.

**Claude Code 2.1.x**:

5. `claude --resume <id> "texto"` en un PTY contra una sesión `--bg`:
   - libre y con un turno en curso (¿lo encola?);
   - cuándo se puede cerrar el PTY;
   - que cerrarlo no detiene la sesión.
6. Hooks puestos con `--settings`:
   - que la decisión de `PermissionRequest` evita el diálogo del TUI;
   - `PreToolUse` sobre `AskUserQuestion` con `updatedInput.answers`;
   - `ExitPlanMode`: aprobar y rechazar;
   - que el hook que termina sin decidir deja el diálogo;
   - el tope de espera.
7. La transcripción de una sesión de prueba: qué tipos de entrada hay para
   mensajes, razonamiento, herramientas (`Edit`, `Write`, `Bash`) y
   resultados. Con eso se escriben las fixtures a mano.
8. Escribir `/compact` en el PTY de `claude attach` sin Enter: queda escrito
   y el usuario lo lanza.

## Partes (el contrato numerado lo cierra el director tras el spike)

**Reglas comunes**:

- cada parte lleva sus firmas y al menos un test de aceptación de Sol;
- los nombres de abajo son la propuesta de las ADR, y el director los fija.

**1. titan-team por el app-server** (ADR-0013):

- cliente JSON-RPC con conexión al daemon compartido o servidor propio
  por stdio;
- turno nuevo y continuación, mapeo de notificaciones a `activity`,
  respuesta, esquema, uso, tiempo límite y `busy`;
- detección de turnos del usuario, commit y evento `user_turn`;
- `--dry-run`, y `FORMAT.md` y la skill `equipo` actualizados;
- tests con un app-server falso, reutilizando el de los tests del daemon.

**2. Núcleo**:

- tipos de la conversación: elementos (ampliar `TranscriptItem` o un tipo
  nuevo, con diff, plan y petición), estado del chat y cola;
- conversión de las notificaciones de Codex;
- parser tolerante de la transcripción de Claude, con «formato no
  reconocido» explícito;
- tipos de las peticiones al usuario y sus respuestas;
- el menú `/`: entradas por agente y la lista fija de comandos del CLI de
  Claude;
- el monitor lee el evento `user_turn`.

**3. Protocolo**, como ampliación compatible de ADR-0007:

- peticiones `chat_open`, `chat_close`, `chat_send`, `chat_unqueue`,
  `chat_interrupt`, `chat_answer`, `chat_menu` y `chat_command`;
- eventos `chat_item`, `chat_state` y `chat_request`;
- errores y su comportamiento con varios clientes;
- se anota en ADR-0007 en el mismo cambio.

**4. Daemon, Codex**:

- suscripción por chat abierto;
- `turn/start` o `thread/queue/add` según el estado, sin dirigir nunca un
  turno en curso;
- aprobaciones, preguntas, plan y elicitaciones;
- el menú (`model/list`, `skills/list`, `permissionProfile/list`…) y los
  comandos con método;
- `@` con `fuzzyFileSearch`.

**5. Daemon, Claude Code**:

- envío con `claude --resume` en un PTY propio;
- seguimiento de la transcripción;
- estado y `waitingFor` de `claude agents --json`;
- el subcomando `titan-agentd hook`: pregunta por el socket, espera solo
  si el chat de esa sesión está abierto, y si no, termina sin decidir;
- el `--settings` de los hooks en los lanzamientos de la app, visible en el
  comando exacto;
- el fragmento para los ajustes del usuario en la configuración, sin
  editarlos nunca.

**6. Interfaz**:

- `app/composer.rs`, el campo de texto con su menú (teclas de la decisión
  visual);
- la vista «conversación» con sus estados borrador y sesión, que sustituye
  a `new_session.rs` y `thread_view.rs`;
- las peticiones al usuario, la cola, la línea de estado y `[t] terminal`;
- el clic en un rol del monitor o en una sesión del árbol abre la
  conversación.

## Criterios de finalización

**Con Codex**:

- con un rol del equipo libre y con el chat abierto, `titan-team ask` a ese
  rol funciona sin `busy`;
- un mensaje del usuario mientras el rol trabaja queda en cola y arranca al
  acabar el turno;
- el siguiente `ask` del director informa del turno del usuario
  (`user_turn`), con su commit.

**Con Claude Code**:

- un mensaje a una sesión `--bg` llega como turno del usuario y su
  respuesta aparece en el chat;
- con los hooks, un permiso y una pregunta se responden desde el chat;
- sin el chat abierto, el diálogo sale en el terminal.

**En la interfaz**:

- el menú `/` funciona en las dos sesiones;
- «Nueva sesión» lanza con Enter y se convierte en la conversación;
- con una transcripción de formato desconocido, el chat lo dice y ofrece el
  terminal.

**Proceso**:

- revisión de Sol y evaluación visual del equipo v5;
- prueba manual del usuario con pasos numerados y el resultado esperado de
  cada uno;
- comprobaciones de «Comprobaciones y compuerta de "Hecha"» en verde;
- PR integrada con la versión acordada.

## Spike

## Verificación

## Resultado
