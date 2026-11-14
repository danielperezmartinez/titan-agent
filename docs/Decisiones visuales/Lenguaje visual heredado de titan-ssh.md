---
Nombre: Lenguaje visual heredado de titan-ssh
Estado: Aceptada
Ámbito: Tokens
Resumen: titan-agent usa los mismos tokens que titan-ssh (formato DESIGN.md con opencode como referencia, oscuro primero, JetBrains Mono en todo, marcadores ASCII, plano con hairlines, radios 4/0 px) y añade dos colores de identidad de agente (violeta para Claude Code, cian para Codex) y tres marcadores ([?], [/], [*]).
Decisión: Heredar los tokens y la disciplina de color de titan-ssh, con colores de agente tomados de su paleta ANSI y los marcadores nuevos [?] te necesita, [/] filtrar y [*] en vivo.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-14T14:32:34+01:00
Última modificación: 2026-11-14T14:32:34+01:00
---

# Lenguaje visual heredado de titan-ssh

## Contexto

titan-agent es hermana de titan-ssh y debe compartir aspecto y *lore*. En
titan-ssh el lenguaje visual está fijado en sus decisiones visuales «Lenguaje
visual con formato DESIGN.md sobre Material 3, dark-first», «Tokens visuales
dark-first base opencode» y «Vocabulario ASCII ampliado y disciplina de color»
(en su bóveda, ). titan-agent necesita además
distinguir de un vistazo qué agente es cada sesión y nuevos estados.

## Decisión

- **Tokens de titan-ssh sin cambios**: `canvas` #201d1d, `surface` #302c2c,
  `surface-elevated` #3a3636, `terminal-bg` #161313; texto `ink` #fdfcfc,
  `body` #d8d5d5, `mute` #9a9898, `stone` #6e6e73; `hairline`
  rgba(253,252,252,0.12), `hairline-strong` #646262; `accent` #0a84ff,
  `success` #30d158, `warning` #ff9f0a, `danger` #ff453a. JetBrains Mono en
  toda la interfaz; escala, espaciado, radios (4 px interactivo, 0 px
  contenedor), sin sombras ni gradientes; paleta ANSI del terminal la de
  titan-ssh.
- **Disciplina de color de titan-ssh**: `accent` solo para interacción
  (selección, foco, enlaces, botón primario con hairline accent); verde, ámbar y
  rojo solo para estados reales; `danger` también para lo destructivo.
- **Colores de identidad de agente** (no son estados, por eso salen de la paleta
  ANSI y no de la rampa semántica): **Claude Code = violeta** `#da8fff`
  (magenta brillante ANSI), **Codex = cian** `#5ac8fa` (cian ANSI). Se usan en la
  pastilla de agente (`cl opus`, `cx luna`) y en lo que envía cada agente en
  el monitor.
- **Estados de sesión**: `[>]` trabajando (`success`, con spinner ASCII
  `| / - \` en el monitor), `[?]` te necesita (`warning`), `[+]` en espera
  (`mute`), `[x]` terminada (`mute`; `danger` solo si terminó con error).
- **Marcadores nuevos** (ASCII dentro de corchetes, como exige titan-ssh):
  `[?]` te necesita, `[/]` filtrar o buscar, `[*]` en vivo. Se reutilizan los
  de titan-ssh: `[+]` nueva sesión o añadir, `[#]` proyecto, `[x]` cerrar o
  detener, `[d]` carpeta, `[i]` acerca de, `[ok]` confirmar, `[v]`
  desplegable, `[<]` volver. Las ramas del árbol también son ASCII (`|-`,
  `` `- ``).

## Alternativas consideradas

- Colores de agente con la rampa semántica (verde, ámbar) — descartado: rompe la
  regla de que esos colores solo indican estados.
- Pastillas de agente monocromas — descartado: cuesta distinguir agentes en el
  árbol y en el monitor.

## Consecuencias

- La interfaz se reconoce como de la familia titan. Un agente nuevo (si algún
  día entra otro CLI) necesita su color de la paleta ANSI y se añade aquí.
- Si titan-ssh cambia sus tokens, hay que decidir aquí si se siguen.
