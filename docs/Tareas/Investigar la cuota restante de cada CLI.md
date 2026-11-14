---
Nombre: Investigar la cuota restante de cada CLI
Estado: Pendiente
Resumen: Averiguar si Claude Code y Codex exponen por una vía oficial la cuota restante de la suscripción (ventana de 5 h y semanal) para mostrarla en la mesa de control del monitor, sin leer credenciales ni llamar a APIs con el login.
Decisiones: ''
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-14T15:28:51+01:00
---

# Investigar la cuota restante de cada CLI

## Objetivo

La mesa de control ([[Decisiones visuales/Monitor del proyecto en tres vistas]])
muestra la cuota restante por agente. Saber si se puede obtener:

- Codex: `/status` en el TUI, métodos de `codex app-server` (buscar algo tipo
  límites o cuenta en su esquema JSON) o los eventos de turno.
- Claude Code: `/status`, `/usage` u otra salida oficial apta para scripts.
- Restricción: solo vías oficiales del CLI; nada de leer tokens ni llamar a
  APIs con las credenciales de la suscripción
  ([[Decisiones/ADR-0001 Torre de control y no orquestador]]).

## Criterios de finalización

- Respuesta por CLI (sí / no / parcial) con el comando o método concreto,
  anotada aquí y, si cambia el diseño, en la decisión visual del monitor.

## Verificación

## Resultado
