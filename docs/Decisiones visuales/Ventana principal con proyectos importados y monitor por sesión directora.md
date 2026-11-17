---
Nombre: Ventana principal con proyectos importados y monitor por sesión directora
Estado: Aceptada
Ámbito: Layout
Resumen: 'La columna izquierda lista los proyectos importados (aunque no tengan sesiones), cada uno con un `+` que abre la conversación en borrador con ese proyecto, y al final un grupo «otras carpetas» con las sesiones de carpetas no importadas. La cabecera de un proyecto abre su resumen (carpeta, sesiones, equipos y acciones). El monitor del equipo deja de colgar del proyecto: se abre con `[m]` en la barra de la sesión directora y, si esa directora lleva varios encargos, se elige cuál. La zona central es la vista «conversación» (borrador o sesión), el resumen del proyecto o el monitor.'
Decisión: Árbol de proyectos importados con `+` por proyecto y grupo «otras carpetas»; la cabecera abre el resumen del proyecto; el monitor se abre con `[m]` desde la sesión directora.
Reemplaza: ["[[Ventana principal al estilo Claude Code Desktop]]"]
Reemplazada por: []
Fecha de creación: 2026-11-17T23:40:00+01:00
Última modificación: 2026-11-18T00:05:00+01:00
---

# Ventana principal con proyectos importados y monitor por sesión directora

## Contexto

[[Ventana principal al estilo Claude Code Desktop]] fijó una ventana con el
árbol de sesiones a la izquierda y el centro según la selección. En ese
diseño, la cabecera de un proyecto abría su monitor. Al usar la 0.6.0, el
usuario pidió tres cambios para la 0.7.0, que llegaron desde la sesión de
control el 2026-11-17:

1. **Proyectos como en Claude Code Desktop**: importarlos, verlos sin
   sesiones, quitarlos y un `+` por proyecto
   ([[Decisiones/ADR-0014 Proyectos importados en el daemon|ADR-0014]]).
2. **El monitor por sesión directora, no por proyecto**: con dos equipos en
   el mismo proyecto no se sabe qué grafo se ve.
3. **La vista «conversación»** de
   [[Conversación de sesión y campo de texto común]] como centro de una
   sesión y de «Nueva sesión».

El usuario eligió ese mismo día el grupo «otras carpetas» y que la cabecera
del proyecto abra un resumen.

## Decisión

Lo que no se nombra aquí no cambia respecto a la decisión que sustituye:

- la cabecera de 48 px;
- el filtro `[/]`;
- las filas de sesión con estado y pastilla de agente;
- la fila seleccionada con fondo `surface` y borde `accent`;
- el panel de detalle;
- los tokens de [[Lenguaje visual heredado de titan-ssh]].

### Columna izquierda

1. **Arriba**, el botón `[+] Nueva sesión` (`ctrl+n`), que abre la
   conversación en borrador en el proyecto seleccionado, y el filtro `[/]`.
2. **Título `PROYECTOS`** con `[+] importar` a la derecha, que abre el
   selector de carpetas `[d]` de siempre.
3. **Cada proyecto importado**:
   - la cabecera muestra `[#] nombre`, el número de sesiones vivas y, al
     pasar por encima o con la cabecera seleccionada, un `+` que abre el
     borrador con ese proyecto;
   - debajo, sus sesiones con las ramas ASCII de siempre (las lanzadas por
     otra cuelgan de ella);
   - un proyecto sin sesiones muestra una línea `mute`: «sin sesiones ·
     `[+]` nueva»;
   - una carpeta que ya no existe muestra `[!] carpeta no encontrada`.
   - Orden: por la última actividad, y los que no tienen actividad, después
     y por nombre.
4. **Una sesión directora con un equipo trabajando** lleva `[*]` (en vivo,
   `success`) junto a su nombre. Hasta ahora ese `[*]` iba en la cabecera
   del proyecto.
5. **Título `OTRAS CARPETAS`** (en `mute`, solo si hay alguna): las sesiones
   de proyectos no importados, agrupadas por proyecto. Cada grupo lleva
   `[+] importar` y desaparece cuando se queda sin sesiones.
6. **Leyenda** al pie, como hoy.

### Zona central, según la selección

- **Sesión**:
  - la vista «conversación» en estado sesión, con la barra de sesión y
    `[t] terminal`;
  - en la 0.7.0, el cuerpo sigue siendo el terminal; el chat llega en la
    0.8.0.
  - En una **sesión directora**, la barra lleva `[m] monitor`, que cambia el
    cuerpo por el monitor; `[m]` otra vez, o `[<] conversación`, lo
    devuelve.
- **Borrador** (`[+] Nueva sesión` o el `+` de un proyecto): la vista
  «conversación» en estado borrador, con el proyecto puesto.
- **Proyecto** (clic en su cabecera): **resumen del proyecto**, en una
  columna:
  - cabecera `[#] nombre` y la carpeta en `mute`;
  - las acciones `[+] nueva sesión`, `[d] abrir carpeta` y `[x] quitar`
    (en «otras carpetas», `[+] importar` en vez de `[x] quitar`);
  - «EQUIPOS»: una línea por encargo abierto, con estado, encargo, ronda,
    sesión directora (pulsable) y `[m] monitor`. Un encargo sin sesión
    directora conocida pone «sin sesión directora» y su `[m]` abre el
    monitor aquí mismo;
  - «SESIONES»: las sesiones vivas del proyecto, como en el árbol pero con
    su estado y desde cuándo.
  - Nada más: es un resumen, no un panel de control.
- **Monitor** (desde `[m]`):
  - las tres vistas de [[Monitor del proyecto en tres vistas]] sin cambios;
  - en su barra, el proyecto pasa a ser la miga `[#] proyecto › directora`;
  - si la directora lleva más de un encargo abierto, un selector
    `encargo nombre [v]`, con el más reciente por defecto;
  - el clic en un agente abre su sesión (la directora, su conversación).

## Alternativas consideradas

- **Que la cabecera del proyecto siga abriendo el monitor**: descartada.
  Con dos equipos en el proyecto no se sabe cuál se ve.
- **Que la cabecera abra directamente el borrador**: descartada por el
  usuario; para eso está el `+`.
- **El monitor en un panel lateral junto a la conversación**: descartado. El
  grafo y los carriles necesitan el ancho del centro.
- **Pestañas para conversación y monitor**: descartadas, como las pestañas
  de terminales de la decisión anterior. `[m]` es un conmutador de la barra.

## Consecuencias

- La regla de qué sesión es la directora de un encargo es la de
  [[Decisiones/ADR-0010 Formato de team.toml y de los eventos del equipo|ADR-0010]]
  (`director_session` o, sin ella, la sesión de Claude Code de la carpeta
  base). El monitor la usa al revés: de la sesión a sus encargos.
- [[Monitor del proyecto en tres vistas]] sigue vigente para el contenido de
  las vistas. Esta decisión cambia solo desde dónde se abre y su barra.
- Previa: `design/preview/Proyectos.dc.html`, en la rama de la 0.7.0.
