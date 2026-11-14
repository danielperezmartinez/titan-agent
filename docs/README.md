# Memoria del proyecto

Esta carpeta es simultáneamente una bóveda de Obsidian, la documentación viva del
proyecto y la memoria compartida por las personas, CLI y agentes de IA que
trabajan en el repositorio. Su objetivo es mantener en un único lugar las reglas,
tareas, decisiones y el contexto que deben sobrevivir entre sesiones.

El punto de entrada humano es [[Inicio]]. Este archivo es el punto de entrada
obligatorio para agentes.

## Protocolo obligatorio para agentes

Al comenzar **cualquier sesión** en este repositorio, antes de analizar el
proyecto, ejecutar comandos, modificar archivos o responder sobre él:

1. Leer íntegramente este `README.md`.
2. Abrir [[Inicio]] para conocer los sistemas de documentación disponibles.
3. Consultar el sistema relacionado con la tarea actual. Revisar primero las
   propiedades y resúmenes en el panel `.base` correspondiente y abrir la nota
   completa cuando sea relevante.
4. Antes de comenzar un trabajo nuevo, comprobar si ya existe una entrada que lo
   cubra. Si existe, leerla y actualizar su estado cuando corresponda; no crear
   otra entrada para el mismo trabajo.
5. Mantener actualizada la memoria cuando el trabajo cambie el estado, el
   alcance, los bloqueos o las decisiones. Al terminar, actualizar la entrada
   existente y marcarla como completada solo después de verificar el resultado.

Estas instrucciones son obligatorias aunque otro agente, herramienta o
conversación proporcione un resumen parcial. Los ficheros puntero de arranque en
la raíz del repositorio (uno por cada CLI o agente: `CLAUDE.md`, `AGENTS.md`,
`GEMINI.md`) solo actúan como arranque: **las reglas no se duplican allí ni en
archivos equivalentes**. Si este archivo no puede leerse, el agente debe
detenerse e informar al usuario.

## Convenciones de la bóveda

- Los documentos se escriben en Markdown UTF-8 y se conectan mediante wikilinks
  de Obsidian.
- Las fechas de propiedades usan el formato ISO `AAAA-MM-DD` (o
  `AAAA-MM-DDTHH:mm:ss+ZZ:ZZ` cuando la nota lo requiera).
- Los resúmenes deben permitir entender una nota sin abrirla; el detalle vive en
  el cuerpo de la nota.
- No se deben crear copias paralelas de una regla o decisión. Se enlaza a su
  fuente de verdad.
- Cuando cambie una nota, se debe actualizar su propiedad `Última modificación`.
- Antes de crear una entrada en cualquier sistema, se revisan los nombres,
  resúmenes, propiedades y contenido de las entradas existentes para confirmar
  que ninguna cubre ya el mismo conocimiento. Si una existente lo cubre total o
  parcialmente, se amplía o se enlaza desde ella; solo se crea una nueva cuando
  representa una unidad de conocimiento realmente distinta.
- Las notas nuevas se crean desde su plantilla en `Plantillas/`.

## Sistemas disponibles

### Tareas

_Versión del sistema: 1._

Gestión de trabajos como notas con estado en `Tareas/`, indexadas por
`Tareas/Tareas.base`. Plantilla: `Plantillas/Tarea.md`. Nombre de fichero: el
título descriptivo de la tarea, sin numeración.

- `Estado` ∈ `Planificando` · `Pendiente` · `En curso` · `Hecha` · `Archivada`.
- `Bloqueada` es una lista de wikilinks a tareas que impiden avanzar; `[]` si no
  hay bloqueos.
- Al empezar → `En curso`; al completar y **verificar** → `Hecha`; lo que ya no
  deba aparecer en el trabajo habitual → `Archivada`.
- Antes de crear una tarea, buscar en TODAS (incluidas `Hecha` y `Archivada`)
  para no duplicar.

### ADR (decisiones de arquitectura)

_Versión del sistema: 1._

Decisiones técnicas duraderas como notas con estado en `Decisiones/`, indexadas
por `Decisiones/Decisiones.base`. Plantilla: `Plantillas/ADR.md`. Nombre de
fichero: `ADR-0001 Título de la decisión.md`.

- `Estado` ∈ `Propuesta` · `Aceptada` · `Rechazada` · `Obsoleta` · `Reemplazada`.
- Una ADR **no se edita para cambiar la decisión**: se marca `Reemplazada` y se
  crea una nueva que la sustituye, enlazando ambas con `Reemplaza` /
  `Reemplazada por`.
- `Número` es correlativo y no se reutiliza.
- Si una decisión afecta a una tarea, se enlazan mutuamente con wikilinks.

### Decisiones visuales y de estilos

_Versión del sistema: 1._

Acuerdos sobre lenguaje visual, tokens y componentes, en `Decisiones visuales/`,
indexadas por `Decisiones visuales/Decisiones visuales.base`. Plantilla:
`Plantillas/Decisión visual.md`. Sigue el mismo patrón que ADR.

- `Estado` ∈ `Propuesta` · `Aceptada` · `Reemplazada`.
- `Ámbito` ∈ `Tokens` · `Componente` · `Layout` (amplía el vocabulario si el
  proyecto lo necesita).
- No se edita una decisión visual aceptada para cambiar su contenido: se marca
  `Reemplazada` y se crea una nueva, enlazando ambas con `Reemplaza` /
  `Reemplazada por`.

### Catálogo técnico

_Versión del sistema: 1._

Superficie reutilizable (UI, servicios, contratos) en `Catálogo técnico/`,
indexada por `Catálogo técnico/Catálogo técnico.base` y con nota-índice en
[[Catálogo técnico]]. Plantilla: `Plantillas/Pieza del catálogo.md`.

- `Estado` ∈ `Vigente` · `En revisión` · `Obsoleta`. Se evita depender de piezas
  `En revisión` salvo que el trabajo incluya estabilizarlas.
- Se añade o actualiza la entrada **en el mismo cambio** que crea o altera una
  superficie reutilizable.
- `Tipo`, `Área`, `Feature` y `Ámbito` son vocabularios propios del proyecto.
  **Aún no están acordados**: se fijan con el usuario al registrar la primera
  pieza y la lista vigente se anota en la nota-índice [[Catálogo técnico]]. No
  se inventan valores fuera de esa lista.

### Seguridad, coste y cumplimiento

_Versión del sistema: 3._

Carpeta [[Seguridad/Seguridad.base|Seguridad/]]: excepciones justificadas a estas
reglas, la [[Seguridad/Checklist de lanzamiento|checklist de lanzamiento]] y las
referencias opacas a hallazgos privados.
Estas reglas aplican a todo cambio, lo haga una persona o un agente, y también a
lo que se haga con MCP, CLI o panel.

#### Secretos

- Ningún secreto vive en el código ni en el repo. Los que necesite el CI van en
  los secretos de la plataforma (GitHub Secrets), solo en los workflows que los
  usan.
- Los ficheros de secretos locales (`.env*` salvo `.env.example`, claves de
  firma) están en `.gitignore`.

#### Repositorio y publicación

- Todo lo que se sube queda expuesto y **no se puede retirar**: el código, esta
  bóveda `docs/`, los mensajes de commit, los nombres de rama, los títulos y
  cuerpos de PR, las issues, los workflows, los logs y artefactos de CI. Borrarlo
  después no lo quita del historial, de los forks ni de las cachés.
- **Nunca se sube:**
  - Secretos: claves, contraseñas, tokens, ficheros `.env`, keystores.
  - Datos del entorno personal: nombres de host, de máquina o de dispositivo,
    usuarios del sistema operativo, IP (de red local, VPN o públicas), dominios y
    redes propios, rutas personales (`C:\Users\<nombre>`, `/home/<nombre>`,
    unidades y carpetas propias) y correos personales.
  - Vulnerabilidades sin corregir (ver "Vulnerabilidades y hallazgos
    sensibles").
- En la documentación se usan **marcadores genéricos** (`<usuario>`,
  `<host-de-pruebas>`, `<ruta-al-proyecto>`). Los valores reales viven fuera del
  repo: variables de entorno, la memoria local del agente (que no se versiona) o
  el fichero de patrones personales.
- **Antes de cada commit y de cada push** se revisan el diff y los mensajes. Los
  hooks versionados en `.githooks/` lo comprueban con gitleaks (patrones
  genéricos de `.gitleaks.toml` y patrones personales fuera del repo), y el CI lo
  repite. Los hooks no se saltan (`--no-verify`) sin permiso explícito del
  usuario. Un falso positivo se resuelve ajustando el patrón o con
  `gitleaks:allow` en esa línea, nunca desactivando la revisión.
- Los logs de CI no imprimen secretos ni datos personales. Los secretos de CI van
  en los secretos de la plataforma (GitHub Secrets), y el CI no depende de
  máquinas, redes ni cuentas personales del usuario.
- **Si algo sensible llega a subirse:** se avisa al usuario **de inmediato**. No
  se reescribe el historial ni se fuerza un push sin su permiso explícito. Si era
  un secreto, se da por comprometido y se revoca o se rota, aunque se borre del
  repo.

#### Vulnerabilidades y hallazgos sensibles

- **Criterio.** Un hallazgo es sensible si describe cómo dañar, espiar o engañar
  a usuarios de la versión publicada y todavía no hay una versión publicada que
  lo corrija. Ante la duda, es sensible.
- **Dónde vive.** Cada hallazgo sensible es un **aviso privado de GitHub**
  (*Security Advisory* en borrador, pestaña *Security* del repo), uno por
  hallazgo. Es su única fuente de verdad mientras no se publique.
- **Qué nunca lo cuenta** mientras no se publique el arreglo: esta bóveda, los
  mensajes de commit, los nombres de rama, los títulos y cuerpos de PR, las
  issues, los comentarios de código y los nombres de test.
- **En la bóveda** solo hay una referencia opaca: una nota `SEC-AAAA-NN`
  (correlativo por año, no se reutiliza) de tipo `Hallazgo` en `Seguridad/`, con
  severidad, estado, el ID `GHSA-…` del aviso y la versión que lo corrige. Nunca
  un título ni una descripción.
  - `Estado` ∈ `Abierto` · `Corregido` · `Publicado` · `Descartado`.
    `Corregido` significa que el arreglo está listo pero aún no publicado: sigue
    siendo sensible. Al pasar a `Publicado` (arreglo y aviso publicados), la nota
    ya puede enlazar el aviso y describir el fallo.
- **El arreglo** se prepara en local, con mensajes de commit, nombres de rama y
  nombres de test neutros que no expliquen el fallo, y se sube junto con la
  publicación de la versión que lo corrige. Entonces se publica el aviso y la
  nota pasa a `Publicado`; desde ese momento ya se puede describir.
- **Los agentes no crean, publican ni cierran avisos por su cuenta.** Preparan el
  borrador del aviso y el usuario lo da de alta, o lo autoriza en esa sesión.
  Publicar un aviso es irreversible.
- Lo no sensible (endurecimiento, CVE públicos de dependencias) se gestiona con
  tareas normales.

#### Comprobaciones y compuerta de "Hecha"

- Comprobaciones del proyecto: gitleaks en `.githooks/` (pre-commit, commit-msg
  y pre-push) y en el job `Fugas` de `.github/workflows/ci.yml`; formato, clippy
  y tests de Rust en el job `Verificar` del mismo workflow.
- **Una tarea que toque código no pasa a `Hecha`** hasta que esas
  comprobaciones estén en verde, y su sección "Verificación" lo deja escrito (qué
  se ejecutó y dónde). Si el cambio añade superficie nueva (un comando, un
  protocolo, un fichero que se lee de otra herramienta), añade también su test en
  el mismo cambio.
- **Ninguna tarea pasa a `Hecha` con commits sin revisar:** los hooks de gitleaks
  han pasado en cada commit y push, y el job de fugas del CI está en verde.
- Una excepción a estas reglas es una nota `Excepción` en `Seguridad/` con su
  justificación. Sin nota, no hay excepción.
- Sus jobs son checks obligatorios para integrar en la rama principal; el flujo
  de publicación está en la sección [[#Publicación]] y no se repite aquí.

### Publicación

_Versión del sistema: 1._

Esta es la única regla de publicación del proyecto; las demás secciones la
enlazan en vez de repetirla.

| Cambio | Cómo se publica | Versión | Despliegue |
| --- | --- | --- | --- |
| **Código**: cualquier archivo fuera de `docs/` | Rama corta y pull request contra `main`, integrada con squash | Sube en la propia PR | Ninguno al integrar; una versión se publica con un tag `vX.Y.Z` (ver abajo) |
| **Solo bóveda**: todo dentro de `docs/` | Commit y push directo a `main` | No cambia | Ninguno |

- Un commit o una PR que mezcle bóveda y código cuenta como **código**.
- **Nunca se hace push directo de código a `main`**, aunque la plataforma lo
  permita a administradores. `main` exige los checks `Cambios`, `Verificar` y
  `Fugas`, actualizados con `main`, para integrar una PR. La guarda de
  `.githooks/pre-push` lo impide en local; no se salta (`--no-verify`) sin
  permiso explícito del usuario.
- **Cada PR de código sube la versión** (`version` de `[workspace.package]` en
  el `Cargo.toml` raíz) y no se integra si no supera la de `main` (lo comprueba
  el CI con `scripts/comprobar-version.sh`). El incremento y el número exacto se
  acuerdan con el usuario (`major` para cambios incompatibles, `minor` para
  funcionalidades compatibles, `patch` para correcciones), sin inferirlo del
  silencio, y el acuerdo queda en la PR.
- **Publicar código** significa: rama corta (`feat/*`, `fix/*`, `chore/*`,
  `docs/*`), PR contra `main`, esperar los checks, integrarla solo si el usuario
  lo autorizó para esa PR (si no, detenerse con la PR abierta) y, si la PR
  publica una versión, verificar su release.
- **Publicar la bóveda** significa: commit y push directo a `main`, sin PR, sin
  subir la versión y sin despliegue.
- **Publicar una versión** (cuando exista el pipeline de releases): tag
  `vX.Y.Z` sobre `main`, que construye los paquetes y los publica en GitHub
  Releases. Formatos, canal canary y autoactualización se deciden en una ADR
  antes de la primera versión. Solo se crea un tag con autorización del usuario.
- **Verificar una release**: localizar la ejecución del workflow del tag exacto y
  comprobar su estado final, sus logs y los artefactos publicados. Ante un fallo,
  avisar al usuario con un reporte y detenerse: no reintentar, rehacer el tag ni
  retirar la release por iniciativa propia. La corrección es una PR nueva con su
  propia versión.
- Los commits, incluidos los squash que hace la plataforma, usan el correo
  privado de la forja (p. ej. `…@users.noreply.github.com`), nunca uno personal.
- Configuración: job `Cambios` en el CI (un cambio de solo bóveda omite
  `Verificar`); guarda en `.githooks/pre-push`. En GitHub: protección de `main`
  con los checks estrictos, `enforce_admins=false`, historial lineal e
  integración solo con squash, y los ajustes de la checklist de lanzamiento
  (aplicados en [[Tareas/Proteger main y configurar el repo al crearlo en GitHub]]).
- Una tarea de código no pasa a `Hecha` hasta que su PR está integrada (y, si
  publica una versión, su release verificada); su sección "Verificación" anota la
  PR, el commit y el estado de la release. Si la PR espera autorización, la tarea
  sigue `En curso`.

## Reglas fundamentales del proyecto

### 1. Idioma de código y documentación

- **Código en inglés**: identificadores, comentarios y mensajes de código en
  inglés.
- **Documentación en español**: esta bóveda y la documentación del proyecto en
  español.

### 2. Gestor de paquetes de JavaScript

- Si el proyecto usa JavaScript o TypeScript, el gestor es **pnpm**, nunca npm
  ni npx (`pnpm dlx` en lugar de `npx`), para reducir el riesgo de ataques a la
  cadena de suministro. La versión de pnpm la fija el campo `packageManager`
  del `package.json` (corepack).

### 3. Versionado

- El proyecto se versiona con **git**, rama principal `main`. El repositorio es
  **público** en GitHub (remoto `origin`, creado el 2026-11-14). No se sube nada
  sin aprobación explícita del usuario. El flujo de ramas, PR y
  versiones está en [[#Publicación]].

### 4. Repositorio público: nada sensible ni personal

Las reglas generales están en [[#Seguridad, coste y cumplimiento]] →
"Repositorio y publicación". Propio de titan-agent, por trabajar con las CLI de
los agentes:

- Nunca se suben ficheros de login o de sesión de las CLI (los de `~/.claude/`,
  `~/.codex/` o equivalentes), ni transcripts o salidas de sesiones de agentes
  que contengan datos del entorno personal. Los ejemplos y fixtures de tests se
  escriben a mano con datos inventados.

### 5. Framework/stack

- **Rust** para toda la aplicación, con el objetivo de minimizar memoria y CPU.
- **Escritorio (Linux y Windows): GPUI.** Android no tiene app propia: será un
  plugin de titan-ssh, después del escritorio. Ver
  [[Decisiones/ADR-0003 Rust con GPUI en escritorio y Android dentro de titan-ssh]].
- La app es una **torre de control**, no un orquestador: no envía prompts por su
  cuenta ni toca credenciales de las CLI, y solo lanza sesiones con las
  salvaguardas de
  [[Decisiones/ADR-0002 Lanzar sesiones desde la app con salvaguardas]]. Ver
  [[Decisiones/ADR-0001 Torre de control y no orquestador]].

- **Cargo** con un workspace; la versión de Rust se fija con
  `rust-toolchain.toml` y GPUI a una versión o commit concretos (su API cambia
  entre versiones).
- Arquitectura en dos procesos (daemon e interfaz) y herramienta del equipo en
  Rust: ver [[#Arquitectura objetivo]].

### 6. Despliegue y distribución

Ver [[#Publicación]]. La distribución concreta (formatos, canales,
autoactualización) se decide en una ADR antes de la primera versión.

## Propósito del proyecto

**titan-agent** es una aplicación hermana de **titan-ssh** (cliente SSH en
Kotlin para Android y escritorio, con un agente en Go). Comparte con ella el
aspecto y el *lore*, y en el futuro tendrán funcionalidad compartida.

Nace de una idea del usuario: **un equipo multiagente por proyecto** que trabaja
con las suscripciones que ya paga (Claude y ChatGPT Plus con Codex), **sin API
keys**. Cada agente tiene su propósito y su modelo, y la composición del equipo
(roles, herramienta, modelo y orden de paso del trabajo) se configura en cada
proyecto y se puede ir cambiando hasta dar con la que encaje.

Antecedentes, como material de partida y no como diseño cerrado: la nota de la
idea en la bóveda de control (`Ideas/Equipo multiagente con mis
suscripciones.md`), y el informe de investigación y el prototipo del sandbox
`multiagente` (`INFORME.md` y `README.md`).

Qué hace, en una frase: **ver qué agentes trabajan, en qué proyecto y quién habla
con quién, y hablar con cualquier sesión** de Claude Code o Codex desde una sola
ventana.

- **Sesiones**: árbol por proyecto con las sesiones de todos los CLI, su estado y
  su agente; al elegir una se ve su terminal con el CLI oficial y se escribe en
  él. Ver [[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]].
- **Nueva sesión**: un encargo escrito por el usuario y los ajustes por defecto,
  con las salvaguardas de
  [[Decisiones/ADR-0002 Lanzar sesiones desde la app con salvaguardas]].
- **Monitor del proyecto**: en tiempo real, con tres vistas. Ver
  [[Decisiones visuales/Monitor del proyecto en tres vistas]].
- **Equipo**: la herramienta que reparte el trabajo entre roles (Opus dirige,
  Luna implementa, un diseñador de tests distinto del implementador, Sol revisa).
  Ver [[Decisiones/ADR-0005 Herramienta del equipo en Rust dentro de titan-agent]].
- **Remoto**: más adelante, desde Android a través de titan-ssh.

Plan de construcción por fases:
[[Tareas/Implementar titan-agent v1 por fases]].

## Arquitectura objetivo

Objetivo inicial; la fase 1 lo confirma o lo ajusta con una ADR.

### Piezas

- **`titan-agentd`**: daemon sin interfaz. Descubre y vigila sesiones, mantiene
  los PTY, lanza procesos y expone el socket local. Ver
  [[Decisiones/ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]].
- **`titan-agent`**: interfaz GPUI, cliente del daemon.
- **Núcleo compartido**: tipos del protocolo y modelo de sesiones, proyectos y
  eventos, sin dependencias de GPUI ni de E/S de plataforma.
- **`titan-team`** (fase 3): CLI de la herramienta del equipo y su skill.
- **`design/preview/`**: fuentes de la previa visual acordada (referencia, no
  se compila).

### Reglas de dependencia

- La interfaz **no ejecuta CLI ni lee ficheros de las CLI**: todo pasa por el
  daemon.
- Solo el daemon lanza procesos, y siempre con las salvaguardas de ADR-0002.
- El núcleo no depende de GPUI ni del daemon; la interfaz y el daemon dependen
  del núcleo.
- `titan-team` no depende del daemon: escribe sus eventos en disco y el daemon
  los lee.
- Nada lee ni guarda credenciales de las CLI
  ([[Decisiones/ADR-0001 Torre de control y no orquestador]]).
