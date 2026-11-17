---
Nombre: Ronda de arreglos 0.5.0
Estado: En curso
Resumen: 'Primera ronda de pulido tras la v1 de escritorio (v0.4.0), versión 0.5.0 (minor): acentos en el terminal integrado, hilos del equipo en solo lectura desde la app y brief.rs bajo el umbral. Hecha con el equipo v4 en una sola variante; recoge métricas y las carencias del equipo encontradas.'
Decisiones: 'Versión 0.5.0 sin etiqueta y nada de 1.x hasta que lo decida el usuario (regla en README → Publicación). El usuario pausó la comparación de variantes durante la ronda: solo queda la variante equipo v4, y el objetivo pasa a detectar y corregir las carencias del equipo. La transcripción se lee por el app-server de Codex (decisión del usuario).'
Bloqueada: []
Fecha de creación: 2026-11-16T23:15:00+01:00
Última modificación: 2026-11-17T15:35:00+01:00
---

# Ronda de arreglos 0.5.0

## Objetivo

Después de integrar la v1 de escritorio (fases 0–3, v0.4.0) y aplazar la
[[Fase 4 - Acceso desde Android en titan-ssh|fase 4]], el usuario quiere
probar y pulir la app hasta que sea un producto sólido. Esta ronda resuelve
las tres tareas pendientes:

1. [[Acentos y teclas muertas en el terminal integrado]] (prioridad máxima).
2. [[Abrir los hilos del equipo en solo lectura desde la app]], incluido el
   clic sin efecto en un rol de Codex con un hilo de `codex exec` terminado.
3. [[Llevar hub.rs y brief.rs bajo el umbral de tamaño]] (queda `brief.rs`).

Se lanzó desde la sesión de control como experimento de dos variantes; a
mitad de la ronda el usuario detuvo la otra y cambió el objetivo: detectar
las carencias del equipo y corregirlas (sección de abajo).

## Configuración: equipo v4

La de [[Fase 3 - Herramienta del equipo en Rust y monitor]]: el director
(Opus) fija el contrato antes de los tests, Sol (gpt-6.1-sol, medium) escribe
pocos tests de aceptación que el director no toca, el director implementa
cada parte con sus tests, una revisión final de Sol (high) con contexto
limpio, un evaluador visual de Opus nuevo en cada ronda (máximo 2) y Luna
(gpt-6-luna) solo para trabajo mecánico. Los roles de Codex con su sandbox.

## Criterios de finalización

- Las tres tareas con sus criterios cumplidos y verificados (incluida la
  prueba a mano del usuario de los acentos y de los clics).
- PR con la versión 0.5.0 integrada, sin etiqueta.

## Verificación

Rama `feat/v0.5-equipo-v4`, en local el 2026-11-16 (sin push ni PR):

- fmt, clippy y `cargo test --workspace` en verde: **130 tests** (112 en
  `main`): 12 de aceptación de Sol (5 de acentos y 7 de hilos), sin tocar
  desde sus commits, y 6 propios. Hooks de gitleaks en cada commit.
- Detalle por tarea en su nota.
- **Pendiente del usuario**: probar los acentos a mano
  (`.previa/v05-equipo-v4/acentos/probar-acentos.sh`) y la ventana con los
  CLI reales (`.previa/v05-equipo-v4/probar-ventana.sh`); acordar la PR.

## Métricas

- **Tiempo**: de 22:25 a unos 23:20 del 2026-11-16, sin paradas por límites
  de uso. Esperando al usuario: poco más de 1 min (la pregunta de la
  transcripción, con Sol trabajando a la vez). Trabajo efectivo: unos
  **55 min**.
- **Uso de Claude** (transcript, Opus 5.5), hasta el cierre de las notas:
  sesión principal unas 185 respuestas y 190 llamadas a herramientas, unos
  **155 k tokens de salida, 370 k de escritura en caché y 45 M de lectura
  de caché**. Subagentes (evaluadores visuales, Opus): ronda 1 19 k de
  salida, 105 k de escritura y 1,7 M de lectura; ronda 2 21 k, 82 k y 1,9 M.
- **Codex**: 6 llamadas, 2,18 M de entrada (1,98 M en caché) y 35 k de
  salida.

  | Encargo | Rol | Entrada (caché) | Salida | Tiempo |
  | --- | --- | --- | --- | --- |
  | `v05-acentos` t1 | Sol, tests | 74 k (64 k) | 3,3 k | 80 s |
  | `v05-acentos` t2 | Sol, formato | 115 k (101 k) | 3,4 k | 16 s |
  | `v05-brief-division` | Luna | 293 k (277 k) | 1,8 k | 240 s |
  | `v05-hilos` t1 | Sol, tests | 346 k (319 k) | 9,2 k | 227 s |
  | `v05-hilos` t2 | Sol, monitor | 475 k (444 k) | 11 k | 56 s |
  | `v05-revision` | Sol, revisión high | 878 k (772 k) | 6,5 k | 171 s |

  Cuota de ChatGPT Plus: 22 % usado en 5 h a mitad de la ronda; 22 % en la
  semana al acabar.
- **Tests**: 18 nuevos (12 de aceptación y 6 propios); 130 en total.
- **Hallazgos**: Sol, 2 mayores y 3 menores (4 aceptados y 1 en parte).
  Evaluador visual: ronda 1 7,5/10 con unos 15 puntos (13 corregidos y 2
  descartados); ronda 2 8/10 con unos 10 (6 corregidos, 1 aplazado y 3
  descartados).
- **Autoevaluación visual**: la vista de solo lectura sigue el lenguaje de
  la barra de sesión y los tokens; el monitor no cambia salvo las marcas
  nuevas. Queda por ver con un hilo largo que abre abajo (las capturas
  cabían enteras) y el estado del rol en la barra.

## Hallazgos de la prueba manual (usuario, 2026-11-17)

Con `probar-ventana.sh` (CLI reales): el usuario entró en una sesión de
Codex, le salió la confirmación de «tomar el control», no la aceptó y cerró
la ventana; al volver a entrar en la misma sesión ya no salió. No entendió
por qué tenía que aceptar nada ni qué probaba. Análisis (sin cambios de
código hasta que el usuario decida):

- **Comportamiento actual.** La confirmación solo existe en la vista de
  solo lectura que abre el clic en un rol del **monitor**, y solo aparece al
  pulsar `[>] tomar el control`; al abrir el hilo no se pregunta nada. El
  clic en una sesión de Codex del **árbol** sigue como en la 0.4.0: abre
  `codex resume` directamente, sin vista de solo lectura ni confirmación. Lo
  más probable es que la segunda vez entrara por el árbol (o por el monitor
  sin pulsar el botón): ninguna de las dos pregunta. No es un fallo del
  código respecto al contrato, pero sí una incoherencia: un hilo del equipo
  abierto desde el árbol pasa a tener escritor sin aviso. Tras la prueba no
  hay ningún hilo de Codex cargado (`thread/loaded/list` vacío), así que
  rechazar no abrió nada.
- **Aceptar** abre `codex resume <hilo>` en un terminal del daemon: la app
  pasa a ser escritora y el monitor marca `[w]`. Ese terminal sigue vivo en
  el daemon aunque se cierre la ventana, hasta cerrarlo o parar el daemon.
  **Rechazar** (`[esc] cancelar` o cerrar) no abre nada.
- **El texto no basta.** Dice qué pasa al aceptar pero no por qué se
  pregunta: falta que la vista diga que lo que se ve es el hilo sin abrirlo
  (el director puede seguir encargándole turnos) y que «tomar el control»
  es para escribir en él, y que el escritor sigue tras cerrar la ventana.
- **Propuestas** (decide el usuario):
  1. El clic en una sesión de Codex del árbol abre también la vista de
     solo lectura (o al menos la de un rol del equipo), con «tomar el
     control» para escribir, para que haya un solo camino.
  2. Una línea fija en la vista de solo lectura: «Estás leyendo el hilo sin
     abrirlo: el director puede seguir encargando turnos a este rol. Para
     escribir en él, tomar el control.»
  3. En la confirmación, añadir que el hilo sigue abierto tras cerrar la
     ventana y cómo soltarlo (cerrar el terminal).
- **Carencias que destapa:**
  - El traspaso al usuario no explicaba qué probar ni qué esperar: «probar
    los clics» no basta. Propuesta: cada prueba manual con pasos numerados y
    el resultado esperado de cada uno, en la nota y en el resumen.
  - El daemon no registra peticiones, así que su log no sirve para
    reconstruir una prueba manual. Propuesta: un log opcional de peticiones
    (método y sesión, sin datos), activado con una variable de entorno.
  - `probar-acentos.sh` dejó vivo su daemon al cerrar la ventana de otra
    forma que la normal. Propuesta: `trap` que lo pare siempre (corregido en
    el script local, que no se versiona).

## Carencias del equipo

Fricciones encontradas en esta ronda, con su causa probable y una propuesta.

1. **Los tests de Sol no pasaban `cargo fmt`** (acentos). Sol formateó con
   `rustfmt` sin la edición del workspace (2024) y hubo que gastar una
   llamada más para que lo corrigiera, porque el director no puede tocar sus
   tests. *Causa*: el prompt de `disenador_tests` no dice cómo formatear y
   su sandbox no le deja usar Cargo. *Propuesta*: que el prompt pida
   `rustfmt --edition 2024` (o que titan-team ejecute `cargo fmt` sobre los
   ficheros del turno antes del commit automático).
2. **A Sol le faltaron las firmas del monitor** (hilos). La especificación
   remitía a «los tests existentes» para `board`, `run_of` y `TerminalRef`,
   pero esos tests no los usaban, y Sol no lee código de producción: dejó
   el contrato 4 sin test y otra llamada (475 k de entrada) para completarlo.
   *Causa*: el contrato no listaba completas las firmas de lo que los tests
   tienen que construir o llamar. *Propuesta*: en el paso 0 de la skill
   `equipo`, listar la firma de todo tipo o función que los tests usen,
   también las ya existentes.
3. **El contrato 3 (adjuntar un hilo no listado) se quedó sin test de
   aceptación**. «Qué pedir a los tests» no lo mencionaba y Sol no lo cubrió;
   lo cubrió un test propio. *Propuesta*: que «qué pedir» salga de numerar
   los apartados del contrato (al menos un test por apartado).
4. **Luna entregó sin compilar y rompió un import** (`brief.rs`). Respondió
   que Cargo se quedó esperando el bloqueo del directorio de compilación y
   aun así dio el trabajo por hecho; quitó un `use` que seguía haciendo
   falta. *Causa probable*: compilaciones en paralelo (la del director) o el
   sandbox sin escritura en la caché de Cargo. *Propuesta*: que titan-team
   ejecute `test_cmd` tras los turnos de `mecanico` y marque el turno como
   fallido si no pasa, y que el prompt diga que «no pude compilar» es un
   fallo, no un éxito.
5. **Luna no compensa para un movimiento puro de código**: 293 k de entrada
   para mover un bloque que el director tuvo que comprobar (líneas ordenadas)
   y arreglar igualmente. *Propuesta*: los movimientos sin criterio los hace
   el director con un script de una llamada; Luna, solo para ediciones
   repetitivas en muchos sitios.
6. **Avisos de `forbidden_paths` que no aportan**: el encargo autorizaba a
   Luna a tocar `brief.rs` y titan-team avisó igual (dos avisos). *Propuesta*:
   `ask --allow <ruta>` para autorizar una ruta en un turno concreto.
7. **No hay entorno de demo reutilizable para las capturas**: hubo que montar
   desde cero un app-server falso de Codex, CLI falsos, eventos inventados y
   el script de captura con comprobación del PID (el de la fase 3 era de la
   otra variante y de otro formato de eventos). Es lo que más tiempo del
   director se llevó tras implementar. *Propuesta*: un kit versionado en
   `scripts/demo/` (datos inventados, app-server falso, `ui.sh` y `shot.sh`
   por escenas) que use cada ronda.
8. **Un error del director al capturar**: un bucle de zsh no partió los
   argumentos y tres capturas salieron de la vista equivocada; se detectó al
   mirarlas y se repitieron. *Propuesta*: que `shot.sh` reciba el nombre de
   la escena y fije él sus variables (punto 7).
9. **El evaluador visual no puede probar la interacción** (clics, hover,
   cursor de mano ni «lo más reciente abajo» con un hilo largo), y en esta
   ronda la interacción es la mitad del cambio. *Propuesta*: escenas de
   captura para cada estado (incluido un hilo largo) y dejar al usuario una
   lista corta de clics a comprobar, como aquí.
10. **La revisión de Sol no puede ejecutar nada** (sandbox de solo lectura:
    ni tests ni clippy) y sus hallazgos visuales repiten parte del trabajo
    del evaluador. *Propuesta*: pasarle al revisor la salida de la compuerta
    (fmt, clippy y tests) en el mensaje y pedirle que no evalúe lo visual.

## Resultado

- **Publicación**: PR #8 integrada con squash en `main` el 2026-11-17, commit `b1237e8`, versión **0.5.0** sin etiqueta. Checks `Cambios`, `Fugas`, `Verificar` y `Windows` en verde. GitHub rechazó `--author-email` con el correo noreply, así que el squash sale con el correo principal de la cuenta.
- El usuario pidió integrarla tal cual: tiene ideas futuras que quizá resuelvan por otro camino los hallazgos de «tomar el control».
- Quedan en curso [[Acentos y teclas muertas en el terminal integrado]] (prueba a mano) y [[Abrir los hilos del equipo en solo lectura desde la app]] (prueba a mano y hallazgos).
