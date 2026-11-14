---
Nombre: Monitor del proyecto en tres vistas
Estado: Aceptada
Ámbito: Componente
Resumen: 'Al pulsar un proyecto en la columna izquierda, el centro muestra su monitor en tiempo real con tres vistas seleccionables con botones arriba: grafo (agentes como nodos y mensajes que viajan entre ellos), carriles (una fila por agente con el tiempo corriendo a la izquierda) y mesa de control (una tarjeta por agente con estado, acción, actividad reciente y cuota).'
Decisión: Monitor en tiempo real por proyecto en la zona central, con tres vistas (grafo, carriles, mesa de control) que se cambian con tres botones en su barra superior.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-14T14:32:34+01:00
Última modificación: 2026-11-14T14:32:34+01:00
---

# Monitor del proyecto en tres vistas

## Contexto

Una primera pantalla de comunicaciones (histórico en tabla) mostraba mucho y no
se entendía a simple vista. El usuario pidió algo dinámico, con animación, que
muestre en tiempo real quién trabaja y quién se comunica, por proyecto. De tres
propuestas le gustaron las tres y decidió tenerlas como vistas del mismo monitor.
Fuentes de la previa en `design/preview/` (`MonitorA`, `MonitorB`,
`MonitorC`).

## Decisión

Barra superior: `[#] proyecto`, encargo y ronda, `[*] en vivo` y tres botones
`[grafo] [carriles] [mesa de control]`. Si el proyecto no tiene un equipo
trabajando, el centro lo dice y lista sus sesiones sueltas.

Elementos comunes: el estado de cada agente (`[>]` con spinner ASCII
`| / - \`, `[?]` que late en ámbar, `[+]`), su acción actual y los mensajes
con el **color de quien los envía** (violeta Claude, cian Codex). Clic en un
agente: abre su sesión.

1. **Grafo**: nodos (tú arriba, el director en el centro y los demás alrededor)
   unidos por líneas; un mensaje es un punto que viaja por la línea con una
   etiqueta corta. Debajo, «ahora»: los últimos eventos.
2. **Carriles**: una fila por agente con su estado a la izquierda; a la derecha
   el tiempo corre hacia la izquierda (ventana configurable, p. ej. 36 s):
   barras verdes = trabajando, ámbar = te necesita, líneas verticales =
   mensajes con un punto en quien recibe; línea `accent` «ahora». Arriba, la
   frase de lo que pasa en este momento.
3. **Mesa de control** (estilo `htop`): franja «hablando / trabajando / te
   necesitan» con una línea ASCII animada (`opus ----*---> sol`) y la ronda de
   revisión; una tarjeta por agente (director a todo el ancho) con estado,
   acción, tiempo en el estado, histograma de actividad reciente, turnos y
   tokens, y **cuota restante** (ámbar si es baja).

## Alternativas consideradas

- Histórico en tabla — descartado: no se entiende a simple vista.
- Elegir una sola vista — el usuario prefirió las tres.

## Consecuencias

- Hace falta una fuente de eventos en tiempo real (estado de sesiones de Claude
  Code, eventos de `codex app-server` y el estado de la herramienta del
  equipo). La acción en curso de Claude exige leer su transcript local.
- **La cuota restante está sin confirmar**: solo se mostrará si cada CLI la
  expone. Hay que investigarlo antes de construir la tarjeta tal cual.
