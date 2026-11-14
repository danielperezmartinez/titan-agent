---
Nombre: Ventana principal al estilo Claude Code Desktop
Estado: Aceptada
Ámbito: Layout
Resumen: 'Una sola ventana con una columna izquierda (botón Nueva sesión, filtro y árbol de sesiones agrupado por proyecto, con las sesiones lanzadas por otra anidadas debajo) y una zona central que muestra lo seleccionado: el terminal de la sesión, el cuadro de nueva sesión con sus ajustes en botones, o el monitor del proyecto. Sin pestañas.'
Decisión: Columna izquierda de sesiones en árbol y zona central según la selección (sesión, nueva sesión o monitor de proyecto), sin pestañas ni pantallas intermedias.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-14T14:32:34+01:00
Última modificación: 2026-11-14T19:30:00+01:00
---

# Ventana principal al estilo Claude Code Desktop

## Contexto

El usuario quiere ver qué agentes trabajan y en qué proyecto, y hablar con
cualquier sesión (Claude Code y Codex) como hoy con `claude agents` y
`claude attach`, organizado como Claude Code Desktop. Se iteró con una previa
visual (fuentes en `design/preview/`): se descartaron las pestañas de
terminales y la pantalla aparte de nueva sesión.

## Decisión

**Cabecera** (48 px): `titan-agent` · «torre de control», contadores globales
(`[>] n trabajando · [?] n te necesita · [+] n en espera`), `[i]` y `[*]`
(configuración).

**Columna izquierda** (≈300–340 px):

1. Botón `[+] Nueva sesión` arriba (atajo `ctrl+n`).
2. Filtro `[/]` por nombre, agente o proyecto.
3. Árbol **agrupado por proyecto**. La cabecera de cada proyecto (`[#] nombre`,
   contador y `[*]` verde si tiene un equipo trabajando) es pulsable: abre el
   monitor del proyecto. Debajo, sus sesiones; las lanzadas por otra sesión
   (p. ej. Luna y Sol encargados por Opus) cuelgan de ella con ramas ASCII.
4. Cada fila: estado (`[>]` `[?]` `[+]` `[x]`), nombre y pastilla de agente
   con modelo (`cl opus`, `cx luna`), con los colores de
   [[Lenguaje visual heredado de titan-ssh]].
5. Leyenda al pie.

La fila seleccionada (sesión o proyecto) queda resaltada (fondo `surface` y
borde izquierdo `accent`) y es lo que muestra el centro.

**Zona central**, según la selección:

- **Sesión**: barra con nombre, estado, agente y modelo, carpeta y el comando de
  adjuntar; el **terminal** con el CLI oficial (`claude attach <id>` /
  `codex resume <id>`) donde escribe el usuario; y un **panel de detalle** a la
  derecha (proyecto, rol, quién la lanzó, desde cuándo está en ese estado, con
  quién habla, «ver el monitor del proyecto», `[d] abrir carpeta`,
  `[x] detener`).
  Concreciones de la fase 1 (acordadas con la sesión de diseño el 2026-11-14,
  sin cambiar lo decidido):
  - Las sesiones **interactivas de Claude Code** (abiertas en otro terminal) no
    se pueden adjuntar (`claude attach` solo admite las background), pero
    **nunca se ocultan**: en el árbol van como las demás. Al elegirlas, el
    centro muestra en `mute` (no es un error, es una limitación del CLI) «sesión
    interactiva en otro terminal: no se puede adjuntar», con pid, carpeta y
    estado. El panel de detalle sigue disponible.
  - Acción de parar según el agente: Claude `[x] detener` en `danger`
    (`claude stop`, destructivo); Codex `[-] interrumpir` en `body` con hairline
    neutro (interrumpe el turno sin descargar el hilo; `[-]` es el marcador de
    pausa de titan-ssh).
- **Nueva sesión**: cuadro de encargo vacío con los **valores por defecto ya
  puestos** y, debajo, botones que abren una lista corta para cambiar agente
  (Claude Code / Codex), modelo, esfuerzo, permisos (o sandbox en Codex) y
  proyecto (o `[d]` otra carpeta). Una línea pequeña muestra el **comando
  exacto** que se ejecutará y `[ok] lanzar` lo lanza (salvaguardas de
  ADR-0002). La sesión nueva aparece en el árbol y queda seleccionada.
- **Monitor del proyecto**: ver [[Monitor del proyecto en tres vistas]].

## Alternativas consideradas

- Pestañas de terminales arriba — descartadas por el usuario.
- Formulario aparte para nueva sesión — descartado: un paso intermedio de más.
- Árbol agrupado solo por quién lanzó a quién — se prefirió agrupar por proyecto
  y anidar dentro.

## Consecuencias

- Solo hay un terminal visible a la vez; cambiar de sesión es un clic en el árbol.
- El cuadro de nueva sesión depende de un campo de texto, que GPUI no trae
  (ver ADR-0003).
