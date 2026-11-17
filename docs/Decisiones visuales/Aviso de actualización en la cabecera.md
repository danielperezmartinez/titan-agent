---
Nombre: Aviso de actualización en la cabecera
Estado: Propuesta
Ámbito: Componente
Resumen: 'Cuando hay una versión nueva, la cabecera muestra a la derecha, entre los contadores de estado y los botones [i] y [*], una sola línea discreta con el marcador ASCII nuevo [^], la versión en mute y el enlace «actualizar» en accent. Si hay agentes trabajando, la confirmación se despliega en esa misma línea. No hay toasts, ventanas ni nada en el árbol. Los ajustes («Buscar actualizaciones», «Buscar ahora», la versión instalada) viven en Configuración.'
Decisión: 'Aviso de actualización como elemento en línea en el extremo derecho de la cabecera, con el marcador [^], estados en mute, «actualizar» como enlace accent y confirmación en la misma línea; sin toasts ni modales.'
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-17T10:00:00+01:00
Última modificación: 2026-11-17T10:00:00+01:00
---

# Aviso de actualización en la cabecera

## Contexto

[[Decisiones/ADR-0011 Distribución en GitHub Releases con instalación por usuario y actualización firmada|ADR-0011]]
pide que la app avise de una versión nueva **sin molestar** y ofrezca un
botón para actualizar. La cabecera (`app/header.rs`) ya tiene el título, los
contadores de estado, el estado del daemon y los avisos. La ventana sigue
[[Ventana principal al estilo Claude Code Desktop]]: una sola ventana, sin
pestañas ni pantallas intermedias. Los tokens y los marcadores son los de
[[Lenguaje visual heredado de titan-ssh]], que no tiene un marcador para
«actualización».

## Decisión

- **Dónde.** El aviso va a la derecha de la cabecera, entre los contadores de
  estado y los botones `[i]` (acerca de) y `[*]` (configuración), en una sola
  línea. Lleva la misma tipografía y separación que los contadores
  (JetBrains Mono, 12 px, `gap` de 16 px). Cuando no hay nada que decir, no
  ocupa sitio.
- **Marcador nuevo `[^]`**: «actualización». Es ASCII entre corchetes, como
  exige titan-ssh, y se añade a la lista de marcadores.
- **Estados** (texto de ejemplo con la versión 0.6.1):

  | Estado del daemon | Texto | Color |
  | --- | --- | --- |
  | `downloading` | `[^] v0.6.1 · descargando` | todo `mute` |
  | `ready` | `[^] v0.6.1 · actualizar` | `[^] v0.6.1` en `mute`; `actualizar` como enlace `accent` |
  | confirmación | `[^] se cerrarán los terminales de 2 sesiones trabajando; siguen vivas · [ok] actualizar · cancelar` | texto `body`, `2 sesiones trabajando` en `warning` (estado real), `[ok] actualizar` como botón primario con hairline `accent`, `cancelar` en `mute` |
  | `waiting` (se reabrió la ventana antes de aplicar) | `[^] v0.6.1 · actualizar` | igual que `ready` |
  | `applying` | `[^] actualizando…` | `mute` |
  | `failed` | `[^] v0.6.1 no se pudo verificar` | `mute`; no se repite como error, porque se reintenta solo |
  | vuelta atrás reciente | `[^] v0.6.1 no arrancó; sigues en v0.6.0` | `[^]` en `warning`; se descarta con `[x]` |
  | daemon de otra versión | `[^] daemon v0.6.0 · reiniciar` | `mute`; `reiniciar` como enlace `accent` |

- **`v0.6.1`** es un enlace discreto (subrayado al pasar) a las notas de la
  versión. Se abre en el navegador con `cx.open_url`.
- **Teclado.** El aviso entra en el orden de foco de la cabecera, `Enter` sobre
  «actualizar» equivale al clic y `Esc` cierra la confirmación.
- **Configuración** gana un bloque «Actualizaciones»:
  - la versión instalada;
  - el interruptor «Buscar actualizaciones» (activado por defecto);
  - el botón `Buscar ahora`;
  - una línea `mute` con el resultado de la última comprobación (hora, y error
    si lo hubo).
- **Nunca**: toasts, ventanas emergentes, notificaciones del sistema (salvo la
  de la vuelta atrás, que da el lanzador sin ventana), insignias en el árbol
  ni colores fuera de su función.

Boceto (cabecera a 1280 px, estado `ready`):

```
titan-agent torre de control      [>] 3 trabajando  [?] 1 te necesita  [+] 4 en espera   [^] v0.6.1 · actualizar   [i]  [*]
```

La previa se dibuja en `design/preview/` dentro de
[[Tareas/Versión 0.6.0 - Instalador y actualización automática]], antes de
implementarlo, y el evaluador visual la usa como referencia.

## Alternativas consideradas

- **Toast o notificación del sistema al detectar la versión**: rechazado.
  Interrumpe, y la versión ya se instala sola al cerrar.
- **Junto al aviso de daemon desconectado (a la izquierda de los
  contadores)**: rechazado. Ese hueco es para errores, y una versión nueva no
  lo es.
- **Barra a lo ancho bajo la cabecera**: rechazado. Roba alto al terminal y
  pesa demasiado para algo que no exige actuar.
- **Aviso al pie de la columna izquierda**: rechazado. Compite con el árbol de
  sesiones y queda lejos de Configuración, donde están sus ajustes.
- **Un punto de color sin texto**: rechazado. No dice qué versión es y rompe la
  regla de colores solo para estados.

## Consecuencias

- La cabecera tiene que dejar sitio al aviso a su derecha. Con poco ancho se
  acorta primero a `[^] actualizar`.
- `[^]` pasa a ser un marcador reservado y no se usa con otro significado.
- La confirmación en línea es el único caso en que la cabecera crece en texto.
  Si no cabe, se corta el texto explicativo, pero nunca `[ok] actualizar` ni
  `cancelar`.
