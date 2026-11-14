---
Nombre: Investigar la cuota restante de cada CLI
Estado: Hecha
Resumen: Averiguar si Claude Code y Codex exponen por una vía oficial la cuota restante de la suscripción (ventana de 5 h y semanal) para mostrarla en la mesa de control del monitor, sin leer credenciales ni llamar a APIs con el login.
Decisiones: 'Codex - sí, en frío, con account/rateLimits/read de codex app-server. Claude Code - parcial, solo por el JSON de la statusline (rate_limits) tras la primera respuesta de una sesión; necesita que el usuario configure un puente en su statusline. Cómo se muestra en la mesa de control se decide en la fase 3 con el usuario.'
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-14T19:25:00+01:00
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

Investigado el 2026-11-14 con codex-cli 0.164.0 (app-server 0.169.0) y Claude
Code 2.1.325. No se leyeron credenciales, no se llamó a ninguna API con el login
ni se envió ningún prompt.

- **Codex**: comprobado de verdad. `codex app-server` por stdio, `initialize`,
  notificación `initialized` y `account/rateLimits/read` con
  `{"excludeResetCreditDetails": true}` (la opción pensada para consultas
  periódicas). Responde en ~0,6 s sin abrir hilos ni turnos. Repetido dos veces.
  El método está en el esquema sin `--experimental`.
- **Claude Code**: comprobado que no hay subcomando ni salida JSON con la cuota
  (`claude --help`, `claude agents --json --all`, `claude auth status --json`;
  `/usage` y `/status` son pantallas del TUI). Lo de la statusline viene de la
  documentación oficial (<https://code.claude.com/docs/en/statusline>); no se
  capturó su entrada real porque exige una respuesta del modelo.

## Resultado

| CLI | Respuesta | Vía | Datos | ¿En frío? |
| --- | --- | --- | --- | --- |
| Codex | **Sí** | `codex app-server` (stdio): `account/rateLimits/read`; notificación `account/rateLimits/updated` | `rateLimits.primary` / `secondary` (y `rateLimitsByLimitId`): `usedPercent` 0–100 usado, `windowDurationMins` (300 = 5 h, 10080 = semanal), `resetsAt` en segundos epoch | Sí |
| Claude Code | **Parcial** | JSON de entrada de la statusline: `rate_limits.five_hour` / `seven_day` | `used_percentage` 0–100 usado (decimal), `resets_at` en segundos epoch | No: solo después de la primera respuesta de una sesión |

Detalles y riesgos:

- **Codex**: `app-server` figura como experimental y su protocolo cambia entre
  versiones. Hay que deserializar con tolerancia (aparecen campos nuevos) y sin
  depender de un `limitId` concreto. Las notificaciones pueden ser parciales: se
  fusionan con el último valor. No se llama a nada más del espacio `account/`:
  `account/rateLimitResetCredit/consume` gasta un crédito y `account/read` da
  datos de la cuenta que no hacen falta.
- **Claude Code**: la documentación dice que `rate_limits` aparece en las
  suscripciones Pro, Max y Ultra, y solo tras la primera respuesta de la sesión.
  Cada ventana puede faltar y desaparece al pasar su `resets_at`. Con varias
  sesiones abiertas a la vez, cada una envía su último dato, que puede ir
  desfasado. Para que el daemon la vea, el usuario tendría que ampliar su
  statusline para que, además de imprimir, escriba `rate_limits` de forma
  atómica en un fichero de titan-agent que vigile el daemon. Es un cambio de su
  configuración: lo decide y lo hace él.
- Los hooks no traen la cuota: solo señales de evento (`StopFailure` con
  matcher `rate_limit`, notificaciones `quota_auto_resume_*`).

Pendiente para la fase 3 (con el usuario): la tarjeta de la mesa de control
muestra la cuota de Codex y, para Claude, la del puente de la statusline si el
usuario lo configura, o «desconocida». Si se adopta, se cambia
[[Decisiones visuales/Monitor del proyecto en tres vistas]] con una decisión que
la reemplace. Hay que confirmar también qué devuelve la statusline con el plan
de la cuenta del usuario.
