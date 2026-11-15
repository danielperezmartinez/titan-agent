---
Nombre: Fase 2 - Nueva sesión con salvaguardas
Estado: En curso
Resumen: Botón Nueva sesión en la columna izquierda y cuadro de encargo en el centro con los valores por defecto y botones para cambiar agente, modelo, esfuerzo, permisos o sandbox y proyecto; muestra el comando exacto y lanza el CLI oficial desde el daemon con las cinco salvaguardas de ADR-0002.
Decisiones: 'Variante equipo v3 del experimento (rama feat/fase-2-equipo-v3, sin push). Campo de texto propio sobre el input handler de GPUI (sin crate: sin ADR). Versión: pendiente de acordar con el usuario (propuesta 0.3.0, minor).'
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-15T20:43:00+01:00
---

# Fase 2 - Nueva sesión con salvaguardas

## Objetivo

- Interfaz: la parte «Nueva sesión» de
  [[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]]
  (previa: estado `inicio="nueva"` de `design/preview/Main.dc.html`).
  GPUI no trae campo de texto: resolverlo (ADR si se elige un crate).
- Daemon: lanzar Claude Code con `claude --bg -n <nombre> --model … --effort …
  --permission-mode … "<encargo>"` en la carpeta del proyecto, y Codex por su vía
  oficial (TUI de `codex` en un PTY o `thread/start` de `codex app-server`).
- Las **cinco salvaguardas** de
  [[Decisiones/ADR-0002 Lanzar sesiones desde la app con salvaguardas]]: el texto
  lo escribe el usuario; un clic, un lanzamiento (sin programar, repetir ni
  encadenar); binario oficial sin modificar y sin leer credenciales; el comando
  exacto visible antes de lanzar; Codex por su vía oficial. Con tests que lo
  comprueben donde se pueda (p. ej. el daemon rechaza un lanzamiento sin texto
  del usuario o duplicado).
- Los valores por defecto se configuran en `[*]` (configuración).

## Criterios de finalización

- Lanzar una sesión de cada CLI desde la app: aparece en el árbol, queda
  seleccionada y su terminal funciona.
- Las salvaguardas están cubiertas por tests o, si no se puede, verificadas a
  mano y anotadas aquí.

## Criterios visuales (escritos antes de implementar)

Variante **equipo v3** del experimento. El evaluador visual (un subagente de
Opus nuevo, sin el contexto de la implementación) compara capturas reales de la
ventana con el estado `inicio="nueva"` de `design/preview/Main.dc.html` según
estos criterios, fijados antes de implementar:

1. **Jerarquía y espaciado.** Zona central sobre `terminal-bg` con padding
   16 px: arriba, centrados, el título «Nueva sesión» y el subtítulo «en [#]
   proyecto con Agente»; abajo, el cuadro de encargo de hasta 820 px de ancho.
   8 px entre la lista desplegada, el cuadro y la línea del comando; botones
   de ajuste de 32 px de alto separados 4 px, `[ok] lanzar` alineado a la
   derecha.
2. **Tipografía.** JetBrains Mono en todo, con la escala de la previa: título
   28 px negrita, subtítulo y encargo 13 px, botones y lista 12 px, línea del
   comando 11 px, `[ok] lanzar` 13 px medium.
3. **Color y disciplina.** Cuadro con borde `accent` y fondo `canvas`, hairline
   entre el texto y los botones; agente en su color de identidad (violeta
   `cl`, cian `cx`) en el subtítulo, el botón y la lista; `accent` solo para
   interacción (borde del botón abierto, `[ok]` de la opción elegida, botón
   primario, `[d]`); textos `ink`/`body`/`mute`/`stone` como en la previa;
   radios 4 px, sin sombras ni gradientes.
4. **Estados e interacción.** Botón de ajuste abierto con borde `accent` y su
   lista encima del cuadro (fondo `surface-elevated`, borde `hairline-strong`,
   opción elegida con `[ok]` y fondo `surface`); marcador de posición en `mute`
   con el encargo vacío y cursor visible al escribir; `[ok] lanzar` atenuado
   sin encargo; un error del daemon se ve en `danger` junto al comando; «Nueva
   sesión» resaltado en la columna izquierda mientras se ve el cuadro.
5. **Acabado.** La línea «se ejecutará» muestra el comando exacto y se ajusta
   sin desbordar; un encargo largo o de varias líneas se ajusta dentro del
   cuadro; nada se corta ni se solapa a 1440×900; el resto de la ventana
   (cabecera, columna, árbol) sigue como en la fase 1.

## Verificación

Variante **equipo v3** (rama `feat/fase-2-equipo-v3`, sin push), en local el
2026-11-15:

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` y `cargo test --workspace` en verde: **64 tests de Rust** (37 de las
  fases anteriores y 27 nuevos: **8 de aceptación** de Sol, sin tocar desde su
  commit `95f7498`, y **19 propios**). `bin/test_team.py`: 15 en verde. Hooks
  de gitleaks en cada commit, sin fugas.
- **CLI reales**, por el daemon y con la misma petición `launch` que manda la
  ventana: una sesión de prueba de Claude Code (haiku, low, plan) y un hilo de
  Codex (luna, low, read-only) en este worktree. Las dos se resolvieron de
  `launch:<n>` a su sesión real (`cl:…` por el id que imprime `claude --bg`;
  `cx:…` por carpeta y hora) y su terminal mostró la respuesta. Se borraron al
  terminar (`claude stop` + `claude rm`, `codex delete --force`, y los
  transcripts que deja `claude rm`). No se tocó ninguna sesión real del usuario.
- **Pendiente del usuario**: el criterio «lanzar desde la app» de punta a punta
  con un clic real. En esta sesión no hay herramientas para inyectar clics ni
  teclas y activar la accesibilidad del escritorio para pulsar por AT-SPI fue
  denegado. Prueba sugerida (2 min): `ctrl+n`, escribir un encargo corto, elegir
  haiku/plan o luna/read-only, `[ok] lanzar`; la sesión aparece en el árbol, queda
  seleccionada y su terminal funciona; después borrarla.
- Capturas reales de la ventana (spectacle, datos inventados en `/srv/demo/…`
  con CLI falsos) en `.previa/fase-2-equipo-v3/` (ignorada por git), ronda 2 en
  `r2/`; referencia de la previa en `previa-nueva*.png`.

### Salvaguardas de ADR-0002

| Salvaguarda | Cómo se garantiza | Comprobación |
| --- | --- | --- |
| 1. El texto lo escribe el usuario | El encargo va literal tras `--`; nada lo genera ni completa; el daemon rechaza el vacío (`empty_brief`) | Tests de aceptación (núcleo y daemon). `TITAN_AGENT_DEMO_BRIEF` solo rellena el campo para capturas y nunca lanza |
| 2. Un clic, un lanzamiento | Token por clic de un solo uso (aunque falle), mismo agente+carpeta+encargo rechazado 30 s, sin reintentos, sin campos de programar/repetir (`bad_request`) | Tests de aceptación `duplicate_…` y `failed_cli_is_run_once…` |
| 3. Ni credenciales ni binario | Se ejecuta el binario oficial sin shell; nada lee `~/.claude/.credentials*` ni `~/.codex/auth.json` | `grep` del código (solo el transcript para el modelo, como en la fase 1) |
| 4. Se ve el comando exacto | La ventana muestra `command_line()`; el daemon rechaza otra línea (`command_mismatch`) y ejecuta exactamente `argv()` | Tests de aceptación (`sh -c` con CLI falso y argumentos registrados) |
| 5. Codex por su vía oficial | TUI de `codex` en un PTY del daemon | Test de aceptación con PTY; prueba con Codex real |

## Resultado

- **Núcleo** (`launch.rs`): `LaunchSpec` (validación, argv, línea mostrada,
  nombre, sesión provisional), `LaunchDefaults`, listas de modelos, esfuerzos y
  permisos (sin `bypassPermissions`, `dontAsk` ni `danger-full-access`).
  Protocolo: petición `launch`, `LaunchResult`, `launches` en el snapshot y
  `id: null` para líneas ilegibles.
- **Daemon**: `claude --bg` una vez (salida a un temporal exclusivo 0600), TUI de
  Codex en un PTY con fallo temprano como `launch_failed`, sesión provisional
  hasta resolver, hilos lanzados mostrados vivos mientras su TUI sigue abierta,
  errores explicados en español con el texto del CLI.
- **Interfaz**: campo de encargo de varias líneas ([[Catálogo técnico/Campo de
  encargo]]), botones con lista corta para agente, modelo, esfuerzo,
  permisos/sandbox y proyecto (o `[d]` otra carpeta con el selector del
  sistema), línea «se ejecutará» que solo parte antes de una opción, `[ok]
  lanzar` (o `ctrl+enter`), error en `danger`, y `[*]` Configuración con los
  valores por defecto (`~/.config/titan-agent/defaults.json`).
- Catálogo: [[Catálogo técnico/Lanzamiento de sesiones]], [[Catálogo
  técnico/Campo de encargo]], [[Catálogo técnico/Protocolo de titan-agentd]] y
  [[Catálogo técnico/titan-agentd]].
- Aplazado: recuperar tras reabrir la ventana el terminal de un Codex aún
  provisional (hallazgo menor de Sol).

### Revisión de Sol (una ronda, contexto limpio)

Revisó el commit `fa367c5` (los cambios visuales posteriores son solo de
estilo): `changes_requested`, 7 mayores, 5 menores y 1 pregunta. Decisiones:

- **Aceptados (10)**: deduplicar por identidad de carpeta (`canonicalize`);
  fallo inmediato de la TUI de Codex como `launch_failed` con su salida;
  resolver Codex solo con su PTY vivo y sin ambigüedad; temporal exclusivo
  0600; snapshot que llega antes que la respuesta; formulario bloqueado al
  perder la conexión (se avisa y no se reenvía); no borrar un encargo editado
  durante la espera; la rueda no pelea con el cursor; `\` solo separa en
  Windows; la pregunta (`cd` y `..` tras un enlace): se rechazan `.` y `..` en
  la carpeta (`bad_folder`).
- **Descartados (2)**: que un lanzamiento fallido bloquee 30 s el mismo encargo
  — la salvaguarda prohíbe reintentos *automáticos*; repetir a mano tras, por
  ejemplo, aceptar la confianza de la carpeta debe poder hacerse, y el token ya
  impide reenvíos. Quitar la caducidad de 120 s de la sesión provisional de
  Claude — sin ella, una sesión que el CLI nunca registra se quedaría para
  siempre.
- **Aplazado (1)**: el terminal de un Codex provisional tras reabrir la ventana.

### Evaluación visual (subagente de Opus nuevo, sin contexto)

- Ronda 1: **9/10** — criterios 1–3 cumplen; 4 y 5 con matices: error del CLI
  en inglés, la línea del comando partía `--effort` de `high`, etiquetas de
  Configuración desiguales, cursor sobre la «E» del marcador. Corregidos los
  cuatro (el quinto, 8 px de sangría en la lista de modelos, inocuo, no).
- Ronda 2: **9,5/10** — los cinco criterios cumplen, sin regresiones.

### Experimento: variante equipo v3

Configuración (solo en este worktree; `.agents/team.toml`, prompts y skill
«equipo»): reparto por contexto. Sol (gpt-6.1-sol, medium) escribe pocos tests
de aceptación desde la especificación, sin el diseño interno, y Opus no puede
tocarlos; Opus implementa cada parte con sus tests; una revisión de Sol (high)
en un hilo nuevo con solo la especificación y el diff, de la que Opus decide;
evaluador visual de Opus nuevo con criterios escritos antes de implementar (2
rondas como máximo); Luna (`mecanico`) solo para trabajo mecánico, nunca la
interfaz. Flujo `disenador_tests → (Opus) → revisor`, `max_review_rounds = 1`,
`fix_role = director`.

- **Tiempo**: de 20:06 a 20:43 (unos 37 min) hasta el resultado
  verificado y documentado.
- **Uso de Claude** (transcript, Opus 5.5): sesión principal 151 respuestas, 152
  llamadas a herramientas; 170 k tokens de salida, 378 k de escritura en caché,
  39,4 M de lectura de caché. Evaluador visual (2 rondas): 22 k de salida, 103 k
  de escritura y 2,0 M de lectura de caché.
- **Codex**: 2 llamadas. Tests de Sol: 279 k de entrada (262 k en caché), 9 k
  de salida, 4 min. Revisión de Sol: 1,37 M de entrada (1,25 M en caché), 11 k
  de salida, 6 min. Luna: 0 llamadas (no hubo trabajo mecánico que delegar).
- **Tests**: 27 nuevos (8 de aceptación de Sol + 19 propios); 64 en total.
- **Hallazgos**: revisión de Sol 7 mayores + 5 menores + 1 pregunta (10
  aceptados, 2 descartados, 1 aplazado); evaluador visual 5 (4 corregidos).
- **Autoevaluación visual**: fiel a la previa (9,5/10 del evaluador
  independiente, medido al píxel). Diferencias deliberadas: `manual` en lugar de
  `default` (el CLI ya no tiene ese modo), comando exacto más largo que el de la
  previa (`-n`, `--`, comillas POSIX), `fable` en la lista de modelos.
