---
Nombre: Abrir los hilos del equipo en solo lectura desde la app
Estado: Pendiente
Resumen: 'En la app, el clic en un rol del equipo abre su hilo en solo lectura (el daemon sirve la transcripción) y solo un «tomar el control» explícito y avisado lanza codex resume; mientras el hilo esté abierto así, el monitor marca el rol. Es la otra mitad del hilo ocupado de titan-team (salida 4), para la fase 4.'
Decisiones: 'Acordado al cerrar la fase 3 (2026-11-16): titan-team ya conserva el hilo y sale con 4 si está abierto en otro sitio; esta tarea evita que la app lo abra como escritor sin querer. Sin cola en titan-team.'
Bloqueada: []
Fecha de creación: 2026-11-16T21:40:00+01:00
Última modificación: 2026-11-16T21:50:00+01:00
---

# Abrir los hilos del equipo en solo lectura desde la app

## Objetivo

Hoy, el clic en un rol de Codex en el monitor abre su sesión (`cx:<thread_id>`)
con `codex resume` en un PTY: la app pasa a ser **escritora** del hilo y, si el
director encarga a ese rol un turno mientras tanto, Codex lo rechaza («thread
already has an active writer»). Desde el cierre de la
[[Fase 3 - Herramienta del equipo en Rust y monitor|fase 3]], `titan-team`
conserva el hilo, escribe `turn_finished` con `status = "busy"` y sale con 4,
y la skill `equipo` hace que el director avise al usuario y repita cuando se lo
diga (contrato en
[[Decisiones/ADR-0010 Formato de team.toml y de los eventos del equipo]]).
Falta la otra mitad, en la app:

- **Clic en un rol del equipo → su hilo en solo lectura.** El daemon sirve la
  transcripción del hilo (petición nueva del protocolo, aditiva) y la interfaz
  la muestra sin abrir un escritor. Sirve también para los hilos que el daemon
  ya no lista (más de 20 min sin actualizarse), lo aplazado en la revisión de
  la fase 3.
- **«Tomar el control» explícito y avisado.** Solo esa acción lanza
  `codex resume` (con las salvaguardas de
  [[Decisiones/ADR-0002 Lanzar sesiones desde la app con salvaguardas]]),
  avisando de que el director no podrá encargar turnos a ese rol mientras el
  hilo siga abierto.
- **El monitor marca el rol** mientras su hilo está abierto como escritor en
  la app (y muestra el `busy` de un turno rechazado), para que se vea por qué
  el director espera.

Se decide en una ADR o en una ampliación de
[[Decisiones/ADR-0007 Protocolo local de titan-agentd en NDJSON versionado]]
antes de implementar: cómo lee el daemon la transcripción por una vía oficial,
sin leer credenciales ([[Decisiones/ADR-0001 Torre de control y no orquestador]]).

## Observado en la prueba manual del cierre de la fase 3 (2026-11-16)

El usuario probó la ventana de la versión 0.4.0 con un encargo real
(`fase-3-cierre-revision`, revisor de Codex con su hilo ya terminado):

- En el **grafo**, el cursor cambia a mano sobre el revisor, pero el clic no
  hace nada.
- En **carriles** y **mesa de control**, la tarjeta del revisor sale
  deshabilitada.
- Aparece un **error abajo a la izquierda** de la ventana (texto por
  apuntar en la siguiente prueba). Lo más probable: el hilo de `codex exec`
  no está en la lista de sesiones del daemon (los hilos de `codex exec` no se
  cargan en el app-server compartido), así que no hay sesión `cx:<thread_id>`
  que abrir.
- El director sí abre su sesión (la de Claude Code, unida por
  `director_session`).

Esta tarea debe resolverlo: el clic en cualquier rol del equipo abre su hilo
en solo lectura aunque el daemon no lo liste, y el grafo no muestra el cursor
de clic donde no hay acción.

## Criterios de finalización

- Clic en un rol del equipo en el monitor: se ve su hilo y no se abre ningún
  escritor (un `titan-team ask` a ese rol a la vez funciona).
- «Tomar el control» pide confirmación, avisa y abre `codex resume`; mientras
  tanto el monitor marca el rol, y un `ask` a ese rol sale con 4.
- Tests del protocolo nuevo y del daemon con CLI falsos.

## Verificación

<Se rellena al completar: pruebas, build, comprobación real.>

## Resultado

<Se rellena al completar: qué se hizo finalmente.>
