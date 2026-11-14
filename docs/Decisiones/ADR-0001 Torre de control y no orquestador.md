---
Nombre: Torre de control y no orquestador
Número: 1
Estado: Aceptada
Resumen: titan-agent observa las sesiones de agentes (Claude Code, Codex) y aloja terminales con sus CLI oficiales para que el usuario hable con ellas, pero no envía prompts por su cuenta ni toca credenciales. La orquestación entre agentes sigue dentro de los CLI oficiales. Así cumple los términos de Anthropic, que prohíben que apps de terceros usen la suscripción.
Decisión: titan-agent es una torre de control (monitorizar y alojar terminales de los CLI oficiales sin modificar), no un orquestador que habla con los modelos.
Consecuencias: Ningún flujo de la app envía texto generado por ella a un modelo ni lee credenciales de las CLI. Lanzar sesiones nuevas desde la app (opción B) queda en estudio con salvaguardas. Que la app lance `claude -p` o que Codex lance a Claude queda fuera.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-14T12:58:56+01:00
Última modificación: 2026-11-14T12:58:56+01:00
---

# ADR-0001 · Torre de control y no orquestador

## Contexto

La idea original era un equipo multiagente con las suscripciones (Claude y
ChatGPT Plus con Codex), sin API keys, y una app desde la que monitorizar qué
agentes trabajan, en qué proyecto y quién habla con quién, y hablar con cualquier
sesión (no solo de Claude), como hoy con `claude agents` y `claude attach`.

Los términos de Anthropic (consultados el 2026-11-14) separan dos cosas:

- **Prohibido**: que desarrolladores de terceros ofrezcan el login de Claude en
  sus apps, enruten peticiones con credenciales de la suscripción, o recojan,
  guarden o intermedien credenciales o tokens de sesión; usar el Agent SDK con la
  suscripción; modificar el binario. Los Consumer Terms §3.7 prohíben además el
  acceso «through automated or non-human means… script» sin API key.
- **Permitido de forma explícita**: «Nor does it prevent an end user from signing
  in to the unmodified Claude Code binary with their own Claude subscription,
  including where a platform hosts Claude Code…».

Fuentes: <https://code.claude.com/docs/en/legal-and-compliance> y
<https://www.anthropic.com/legal/consumer-terms>. Con OpenAI el riesgo es bajo:
`codex app-server` es su interfaz programática oficial. agy ya quedó fuera por
los términos de Google (ver la nota de la idea en la bóveda de control).

## Decisión

titan-agent es una **torre de control**:

1. **Monitoriza** sin hablar con ningún modelo: `claude agents --json` (lista
   sesiones con `cwd`, nombre, tipo y estado; documentado «for scripting»),
   `codex agents` y el daemon `codex app-server`, y el estado que guarde la
   herramienta del equipo para saber quién habla con quién.
2. **Aloja terminales** con los CLI oficiales sin modificar (`claude attach <id>`,
   `codex resume <id>`), en los que escribe el usuario.
3. **No orquesta**: la coordinación entre agentes (Opus dirige, Luna implementa,
   Sol revisa) ocurre dentro de los CLI oficiales, como en el prototipo. La app
   solo la muestra.

## Alternativas consideradas

- **App orquestadora** que lanza `claude -p` y encadena agentes — descartada:
  acceso automatizado a la suscripción de Claude desde una app de terceros.
- **Usar API keys** — descartada: rompe el principio de usar solo las
  suscripciones.

## Consecuencias

- Positivas: encaja con lo permitido; la app es más simple (no gestiona
  conversaciones con modelos); el control remoto futuro desde titan-ssh por SSH
  sigue usando el binario oficial.
- Negativas / compromisos: la app depende de lo que expongan los CLI
  (`claude agents --json`, `codex app-server`), que puede cambiar entre
  versiones. Crear sesiones desde la app (opción B: lanzar `claude --bg` con un
  encargo escrito por el usuario) se decide aparte, con salvaguardas.
