---
Nombre: Lanzar sesiones desde la app con salvaguardas
Número: 2
Estado: Aceptada
Resumen: titan-agent puede crear sesiones nuevas de Claude Code (`claude --bg`) y de Codex desde un formulario, como hace la vista oficial de `claude agents`, siempre con un encargo escrito por el usuario, un clic suyo por lanzamiento, el binario oficial sin modificar, sin tocar credenciales y mostrando antes el comando exacto.
Decisión: Adoptar la opción B (formulario que lanza sesiones con los CLI oficiales) con cinco salvaguardas obligatorias.
Consecuencias: La app no puede programar, repetir, encadenar ni disparar lanzamientos por eventos, ni generar o completar encargos con plantillas. Queda un riesgo residual por la ambigüedad de los Consumer Terms §3.7 sobre el acceso por «script».
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-14T12:58:56+01:00
Última modificación: 2026-11-14T12:58:56+01:00
---

# ADR-0002 · Lanzar sesiones desde la app con salvaguardas

## Contexto

[[ADR-0001 Torre de control y no orquestador]] deja la app como torre de control.
Quedaba por decidir si la app puede crear sesiones nuevas. La vista oficial de
`claude agents` ya lo hace (su ayuda habla de «sessions dispatched from agent
view»), y `claude --bg -n <nombre> … "<encargo>"` crea una sesión en segundo
plano que aparece en `claude agents` y admite `claude attach`. Los Consumer Terms
§3.7 prohíben el acceso «through automated or non-human means… script» sin API
key, y no aclaran este caso.

## Decisión

La app ofrece un formulario **Nueva sesión** que lanza el CLI oficial (Claude
Code con `claude --bg`; Codex por su vía oficial: el CLI o `thread/start` de
`codex app-server`). Salvaguardas obligatorias:

1. **El texto lo escribe siempre el usuario.** La app no genera ni completa
   encargos con plantillas.
2. **Cada lanzamiento sale de un clic del usuario.** Nada de lanzamientos
   programados, en bucle, con reintentos automáticos ni disparados por eventos.
3. **Ni credenciales ni binario.** La app nunca lee los ficheros de login de las
   CLI (`~/.claude`, `~/.codex/auth.json`) y no modifica los CLI.
4. **El usuario ve lo que se ejecuta.** El formulario muestra el comando exacto
   antes de lanzarlo.
5. **Codex va por su vía oficial**, con las mismas reglas.

## Alternativas consideradas

- **A: solo abrir un terminal con el CLI interactivo** — más claro aún, pero
  menos cómodo. Sigue disponible.
- **C: no crear sesiones en la v1** — descartada por el usuario.
- **Preguntar antes a Anthropic** — no se hace por ahora; se puede retomar si
  cambian los términos o aparece una duda concreta.

## Consecuencias

- Positivas: crear sesiones de cualquier agente desde un solo sitio.
- Negativas / compromisos: riesgo residual por la ambigüedad de §3.7. Hay que
  revisar los términos de Anthropic si cambian. Toda función nueva que lance
  algo debe respetar las cinco salvaguardas.
