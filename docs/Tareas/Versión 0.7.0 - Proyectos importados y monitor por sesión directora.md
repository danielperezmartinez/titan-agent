---
Nombre: Versión 0.7.0 - Proyectos importados y monitor por sesión directora
Estado: Planificando
Resumen: 'Dos PR para el equipo, en orden. A: la vista «conversación» (borrador con el campo de texto común, que sustituye a «Nueva sesión»; sesión con su barra y el terminal) y el monitor por sesión directora con `[m]`. B: proyectos importados en el daemon (ADR-0014) con `+` por proyecto, grupo «otras carpetas» y resumen del proyecto. Después de la ronda «Equipo v6». El chat va después, en la 0.8.0 (tarea «Chat de sesión con Codex y Claude Code»).'
Decisiones: 'Diseño con el usuario el 2026-11-17 (sesión «chat-sesion», encargo de control-3b): 0.7.0 = estructura (proyectos, conversación sin chat, monitor por directora) y chat en 0.8.0; lista de proyectos en el daemon; sesiones de carpetas no importadas en «otras carpetas»; la cabecera del proyecto abre un resumen; para elegir proyecto en el borrador basta el selector [d]. Versión de cada PR: se acuerda en ella (propuesta: A 0.7.0 y B 0.7.1, y una release cuando estén las dos).'
Bloqueada: []
Fecha de creación: 2026-11-17T23:45:00+01:00
Última modificación: 2026-11-17T23:50:00+01:00
---

# Versión 0.7.0 - Proyectos importados y monitor por sesión directora

## Objetivo

Estos son los comentarios del usuario al usar la 0.6.0:

- proyectos como en Claude Code Desktop (importar, verlos sin sesiones,
  quitar, `+` por proyecto);
- el monitor por sesión directora y no por proyecto, porque con dos equipos
  en un proyecto no se sabe qué grafo se ve;
- la base de la vista «conversación», que el chat de la 0.8.0 llenará.

Las decisiones están en:

- [[Decisiones/ADR-0014 Proyectos importados en el daemon|ADR-0014]];
- [[Decisiones visuales/Ventana principal con proyectos importados y monitor por sesión directora]];
- [[Decisiones visuales/Conversación de sesión y campo de texto común]].

Previas:

- `design/preview/Proyectos.dc.html`, en la rama `feat/v0.7-proyectos`, que
  es de donde salen las dos PR;
- `design/preview/Conversacion.dc.html`, del borrador y del campo, que está
  en `feat/chat-sesion`. La PR A la trae con `git cherry-pick` de su commit
  («Add the session conversation preview»).

## Plan y orden

| Orden | Trabajo | Depende de | Versión (propuesta) |
| --- | --- | --- | --- |
| 0 | Ronda «Equipo v6» (la planifica la sesión de control; carencias en [[Versión 0.6.0 - Instalador y actualización automática]] → «Carencias del equipo», con las 7 a 9 añadidas para esta versión) | — | la suya |
| 1 | **PR A**: conversación y monitor por sesión directora | 0 | 0.7.0 |
| 2 | **PR B**: proyectos importados y resumen del proyecto | A | 0.7.1 |
| 3 | Release 0.7.x, con autorización del usuario | A y B | — |
| 4 | [[Chat de sesión con Codex y Claude Code]] (0.8.0, tres PR) | A (y B para la estructura final) | 0.8.x |

- Cada PR la hace el [[Equipo v5]] (o el v6, si ya está) con su encargo: un
  contrato numerado con firmas, tests de aceptación de Sol, revisión de Sol,
  evaluador visual con el kit de `scripts/demo/` y una prueba manual del
  usuario con pasos numerados.
- B empieza cuando A está integrada (rebase sobre `main`).

## PR A · Conversación y monitor por sesión directora

1. **Campo de texto común** (`app/composer.rs`):
   - las teclas de la decisión visual: Enter envía o lanza, Shift+Enter
     hace salto de línea, Tab, listas «- » y su continuación, ↑ para
     recuperar, borrador por sesión, Esc y crecer hasta 8 líneas;
   - la lógica de edición va en un **modelo puro con tests**. GPUI no tiene
     tests de ventana, así que la vista solo pinta;
   - el menú `/` y `@` no entra aquí: es de la 0.8.0, porque necesita el
     daemon del chat.
2. **Vista «conversación»** (sustituye a `app/new_session.rs`):
   - **borrador**: ajustes, comando exacto y el campo; Enter lanza con las
     salvaguardas de ADR-0002;
   - **sesión**: barra de sesión y el terminal de hoy en el cuerpo, con el
     panel de detalle;
   - al lanzar, pasa de borrador a sesión sin cambiar de pantalla;
   - `ctrl+n` y `[+] Nueva sesión` abren el borrador.
3. **Monitor por sesión directora**:
   - en el núcleo, una función que da los encargos abiertos de una sesión
     (regla de ADR-0010 al revés: `director_session` o la regla de la
     carpeta base) y otra que dice si una sesión es directora;
   - en la interfaz, `[m] monitor` en la barra de una directora, el
     selector de encargo si hay más de uno, la miga
     `[#] proyecto › directora` y el `[*]` en la fila de la directora en el
     árbol;
   - mientras no llegue B, la cabecera del proyecto sigue abriendo el
     monitor del encargo más reciente, como hoy.
4. **Escenas del kit de demo**:
   - borrador con texto escrito y una lista;
   - sesión directora con `[m]` y monitor con dos encargos;
   - dos equipos en el mismo proyecto.

## PR B · Proyectos importados y resumen del proyecto

1. **Daemon** (ADR-0014):
   - `projects.json` v1, con lectura tolerante y escritura atómica;
   - la importación la primera vez y al lanzar en una carpeta nueva;
   - `project_add` y `project_remove`, y el campo `projects` del evento
     `sessions`;
   - las raíces importadas como carpetas candidatas de los encargos;
   - ampliación anotada en ADR-0007 y ADR-0010 en el mismo cambio.
2. **Núcleo**: el árbol con proyectos importados (con y sin sesiones, orden
   por actividad, carpeta no encontrada) y el grupo «otras carpetas»; el
   filtro igual en los dos.
3. **Interfaz**:
   - el título `PROYECTOS` con `[+] importar` (selector `[d]`);
   - el `+` por proyecto, que abre el borrador con el proyecto puesto;
   - el grupo «otras carpetas» con `[+] importar`;
   - el **resumen del proyecto** al pulsar su cabecera: acciones, equipos con
     `[m]` y sesiones. Sustituye al monitor por proyecto.
4. **Escenas del kit de demo**: un `projects.json` de demo con un proyecto
   sin sesiones, uno con la carpeta borrada, sesiones en otras carpetas y el
   resumen de un proyecto con dos equipos.

## Criterios de finalización

**PR A**:

- Enter lanza desde el borrador y Shift+Enter hace salto de línea.
- Las listas «- » continúan con Shift+Enter.
- La vista pasa de borrador a sesión al lanzar.
- Con dos encargos de dos directoras en el mismo proyecto, el `[m]` de cada
  una abre su propio monitor.

**PR B**:

- Un proyecto importado sin sesiones se ve y su `+` abre el borrador con
  él.
- Quitarlo no toca la carpeta y sus sesiones pasan a «otras carpetas».
- La lista es la misma tras reiniciar el daemon y la ve cualquier cliente.
- La primera vez aparecen importados los proyectos que ya tenían sesiones.
- El resumen del proyecto muestra sus equipos y abre su monitor.

**Las dos**:

- Comprobaciones en verde.
- Revisión de Sol y evaluador visual.
- Prueba manual del usuario.
- PR integrada con la versión acordada.
- Decisión visual de la ventana aceptada tras la previa.

## Verificación

## Resultado
