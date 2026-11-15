---
Nombre: Estado de las sesiones de Claude Code en segundo plano entre turnos
Número: 10
Estado: Propuesta
Resumen: 'Claude Code 2.1.325 informa `state: done` en una sesión en segundo plano viva cada vez que termina un turno (con `status: idle` y su `pid`). ADR-0009 lo trataba como terminada (`[x]`). Se propone mirar primero si el proceso sigue vivo: `done` con `status` `idle`, `busy` o `waiting` se muestra según ese `status`; solo `done` sin `status` vivo, `stopped` o `completed` es terminada.'
Decisión: 'En el mapeo de estados de Claude Code, `state = done` con un `status` vivo (`idle`, `busy`, `waiting`) sigue el `status`; el resto de ADR-0009 no cambia.'
Consecuencias: 'Las sesiones lanzadas con `claude --bg` (también las de «Nueva sesión») aparecen `[+] en espera` entre turnos y se pueden detener, en lugar de `[x] terminada`. Si se acepta, ADR-0009 pasa a Reemplazada por esta en ese punto.'
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-15T20:46:00+01:00
Última modificación: 2026-11-15T20:46:00+01:00
---

# ADR-0010 · Estado de las sesiones de Claude Code en segundo plano entre turnos

## Contexto

[[ADR-0009 Descubrimiento de sesiones y jerarquía lanzada por]] fija que
`state = done` de `claude agents --json` es una sesión terminada. Al probar la
[[Tareas/Fase 2 - Nueva sesión con salvaguardas|fase 2]] (2026-11-15, Claude
Code 2.1.325) una sesión recién lanzada con `claude --bg` que ya respondió su
primer turno sale como `{"kind":"background","status":"idle","state":"done",
"pid":…}`: el proceso vive, admite `claude attach` y espera el siguiente
mensaje. En la lista del usuario había dos sesiones así, todas vivas. La
interfaz las mostraba como `[x] terminada` y sin `[x] detener`.

## Decisión

En el mapeo de estados de Claude Code de ADR-0009:

- `state = done` con `status` `idle`, `busy` o `waiting` (proceso vivo) se
  muestra según el `status`: `idle` → `[+]`, `busy` → `[>]`.
- `state = done` sin `status` vivo, `stopped` y `completed` → `[x]`.
- Lo demás (bloqueada, con error, Codex, jerarquía) sigue igual.

Implementado en `titan_agent_core::discovery::claude_state`, con su test en
`crates/titan-agent-core/tests/discovery.rs`.

## Alternativas consideradas

- **Dejarlo como en ADR-0009** — descartado: oculta sesiones vivas y desactiva
  `[x] detener` justo en las que se lanzan desde la app.
- **Usar solo `pid`** — descartado: `claude agents --json` puede listar sesiones
  sin `pid` y el `status` ya distingue lo que hace falta.

## Consecuencias

- Positivas: el árbol refleja que la sesión sigue viva entre turnos.
- Negativas / compromisos: depende de que `status` siga presente en las
  sesiones vivas; si el CLI cambia, cambia `claude_state` y su test.
- Pendiente del usuario: aceptarla (y marcar ADR-0009 como reemplazada en este
  punto) o rechazarla y volver al mapeo anterior.
