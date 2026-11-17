---
Nombre: Distribución en GitHub Releases con instalación por usuario y actualización firmada
Número: 11
Estado: Aceptada
Resumen: 'Aceptada por el usuario el 2026-11-17. Cómo se distribuye y se actualiza titan-agent desde la 0.6.0. El CI publica en GitHub Releases un tar.gz para Linux y un zip experimental para Windows, desde un workflow lanzado a mano sobre main que crea él mismo la etiqueta v0.x.y. Nunca una 1.x sin decisión del usuario. En Linux se instala por usuario en ~/.local, sin sudo: una carpeta por versión, el enlace current, un lanzador estable y la entrada en el menú de Plasma. El daemon comprueba cada 6 h un manifiesto firmado con minisign, descarga y verifica en segundo plano y deja la versión preparada. La cabecera muestra un aviso discreto con el botón «actualizar». Al cerrar la ventana, el daemon aplica la versión cuando no hay ventana conectada ni sesiones trabajando con terminal en él, y se reinicia con exec. Si la versión nueva no arranca dos veces, el lanzador vuelve a la anterior. La comprobación no envía datos del usuario.'
Decisión: 'Artefactos solo en GitHub Releases generados por un workflow manual sobre main que valida 0.x.y y crea la etiqueta. En Linux, tar.gz instalado por usuario con carpetas por versión, enlace current y lanzador estable con vuelta atrás. En Windows, zip portable experimental. Manifiesto con SHA-256 firmado con minisign (clave privada en el environment release y en una copia fuera de línea del usuario, pública dentro de la app, con lista de claves para rotar), más atestaciones de GitHub para la primera instalación. El daemon comprueba, descarga, verifica y aplica cuando es seguro. El aviso va en la cabecera.'
Consecuencias: 'La clave privada de minisign pasa a ser el secreto más crítico del proyecto: con ella se ejecuta código en cada instalación. El daemon gana red, descarga y su propio reemplazo, así que el actualizador debe ser lo más estable y lo más probado del código. Hay un binario más, titan-agent-launcher, que cambia poco. Las builds de debug pasan a otro socket. Reiniciar el daemon corta los terminales adjuntos (las sesiones siguen vivas en su CLI) y pierde su historial. Windows se publica sin firma y sin actualizador hasta que exista su daemon.'
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-17T10:00:00+01:00
Última modificación: 2026-11-17T17:00:00+01:00
---

# ADR-0011 · Distribución en GitHub Releases con instalación por usuario y actualización firmada

## Contexto

[[README#Publicación]] fijaba GitHub Releases al subir una etiqueta y dejaba
para esta ADR los formatos, la firma, los canales y la autoactualización
([[Tareas/ADR de distribución antes de la primera versión]]). Hasta ahora no
se ha publicado ninguna versión. El usuario usa a diario una build compilada
desde el código, con el lanzador «titan-agent (main)» o con builds de debug de
las fases, y quiere:

- tener la app instalada en su PC (CachyOS, KDE Plasma en Wayland) y usarla a
  diario;
- que la app detecte una versión nueva y muestre un aviso que no moleste, con
  un botón para actualizar;
- que se actualice sola al cerrarse.

Ya acordado con el usuario antes de esta ADR:

- Las versiones se publican en GitHub Releases, compiladas por el CI para
  Linux y Windows. En el PC del usuario nunca se compila desde el código.
- Las etiquetas son `v0.x.y`, y **nada de 1.x hasta que el usuario lo
  decida**.
- La instalación es por usuario (en `~/.local`, con entrada en el menú de
  Plasma) y sin sudo, para que la app pueda actualizarse sola. El daemon
  también pasa a la versión nueva.

Hechos del código que condicionan el diseño:

- **Las sesiones no viven en `titan-agentd`.** Sus PTY
  ([[ADR-0008 Terminal integrado con portable-pty en el daemon y alacritty_terminal en la interfaz|ADR-0008]])
  solo ejecutan clientes: `claude attach <id>` y, en Codex, el TUI
  `codex --remote unix://` unido al `codex app-server` compartido.
  - Si el daemon se reinicia, las sesiones siguen en su CLI y se vuelven a
    adjuntar.
  - Se pierde el historial de 2 MiB del daemon.
  - **Falta comprobar** si un turno de Codex lanzado desde la app sobrevive a
    que muera su TUI.
- **La interfaz arranca el daemon** que encuentra junto a su propio ejecutable
  (`current_exe`), en su propio grupo de procesos. Un candado garantiza un solo
  daemon por socket
  ([[ADR-0007 Protocolo local de titan-agentd en NDJSON versionado|ADR-0007]]).
  Las builds de debug usan hoy el mismo socket por defecto que usaría la app
  instalada.
- **Windows todavía no tiene daemon** (el named pipe está pendiente según
  ADR-0007).
- El repo no tiene licencia, y la app y los usuarios son de momento solo el
  usuario.

Referencia: titan-ssh resolvió lo mismo en su bóveda (ADR-0011 de
distribución, ADR-0025 de actualización automática y ADR-0027 de canary). De
allí se toman el manifiesto firmado, la regla de no cortar sesiones y la
negativa a volver a una versión anterior. Los canales externos (Flathub, AUR,
winget) no se toman: aquí no aportan nada.

## Decisión

### 1. Artefactos de cada versión

Cada Release `v0.x.y` publica:

| Asset | Contenido |
| --- | --- |
| `titan-agent-<v>-linux-x86_64.tar.gz` | Carpeta `titan-agent-<v>-linux-x86_64/` con `bin/` (`titan-agent`, `titan-agentd`, `titan-team`, `titan-agent-launcher`), `share/` (el `.desktop` y el icono) e `install.sh`. |
| `titan-agent-<v>-windows-x86_64.zip` | **Experimental**: los mismos binarios `.exe`, portables y sin firmar. |
| `titan-agent-update.json` | Manifiesto (§3). |
| `titan-agent-update.json.minisig` | Firma minisign del manifiesto. |
| `SHA256SUMS` | Comodidad para comprobar a mano. No es la verificación. |

Además, el CI genera una **atestación de procedencia** de GitHub
(`actions/attest-build-provenance`) para el `tar.gz` y el `zip`.

Linux se compila en `ubuntu-22.04` (glibc 2.35), con las mismas librerías
de sistema que el job `Verificar`. Usa `cargo build --release --locked`, con
el perfil de release del workspace.

### 2. Publicar una versión

- Se publica con el workflow **`Publicar versión`**
  (`.github/workflows/release.yml`), con `on: workflow_dispatch` y solo
  desde `main`. Lo lanza un agente con autorización del usuario para esa
  versión (`gh workflow run release.yml --ref main`) o el propio usuario.
- El workflow hace, por orden:
  1. **Preparar.** Lee `version` de `[workspace.package]`. **Falla si no
     cumple `^0\.[0-9]+\.[0-9]+$`** (ni 1.x ni pre-releases), si la etiqueta
     `v<v>` ya existe o si los checks `Cambios`, `Verificar` y `Fugas` del
     commit no terminaron en verde.
  2. **Compilar** Linux y Windows en paralelo y empaquetar.
  3. **Publicar**, en el environment `release`, que es el único que tiene los
     secretos de firma:
     - calcula los SHA-256 y escribe el manifiesto;
     - lo firma y comprueba la firma con la clave pública del repo
       (`packaging/minisign.pub`) antes de subir nada;
     - genera las atestaciones;
     - crea la Release como borrador con todos los assets
       (`gh release create v<v> --draft --target <sha> --generate-notes`);
     - la publica al final, y al publicarla se crean la etiqueta y la
       inmutabilidad.
- Las notas son las que **genera GitHub** a partir de los títulos de las PR
  desde la versión anterior.
- **Etiquetas solo `v0.x.y` y solo por este workflow**: nadie crea, mueve ni
  borra etiquetas a mano.
- **Ajustes de GitHub**, que autoriza el usuario:
  - el environment `release`, restringido a la rama `main`;
  - las *immutable releases*, para que ni la etiqueta ni los assets de una
    Release publicada se puedan cambiar;
  - un ruleset de etiquetas `v*` que impide moverlas y borrarlas.
- **Encaje con la regla actual**: el código sigue yendo por PR con el CI en
  verde, y cada PR sube la versión. Publicar es un paso aparte y opcional:
  no toda versión integrada se publica, y la primera publicada es la 0.6.0.
- `scripts/comprobar-version.sh` también rechaza una PR que lleve la versión a
  1.x o más.
- **Verificar la release** sigue la regla de [[README#Publicación]]: la
  ejecución del workflow, sus logs y los assets. Además, el manifiesto debe
  verificar con la clave pública.

### 3. Manifiesto firmado y verificación

`titan-agent-update.json` (versión de formato 1):

```json
{
  "format": 1,
  "version": "0.6.1",
  "published": "2026-11-20T10:00:00Z",
  "notes_url": "https://github.com/danielperezmartinez/titan-agent/releases/tag/v0.6.1",
  "artifacts": [
    { "target": "linux-x86_64", "name": "titan-agent-0.6.1-linux-x86_64.tar.gz", "sha256": "<64 hex>", "size": 12345678 },
    { "target": "windows-x86_64", "name": "titan-agent-0.6.1-windows-x86_64.zip", "sha256": "<64 hex>", "size": 12345678 }
  ]
}
```

- **Firma.** Va firmado con **minisign** (Ed25519). El comentario de
  confianza, que también queda firmado, es
  `titan-agent-update format=1 version=<v>`. La app lo verifica con el crate
  `minisign-verify`.
- **Claves.**
  - La app trae **una lista de claves públicas aceptadas** (por *key id*).
    La 0.6.0 trae una sola.
  - La privada, cifrada, y su contraseña viven solo en los secretos
    `TITAN_AGENT_UPDATE_KEY` y `TITAN_AGENT_UPDATE_KEY_PASSWORD` del
    environment `release`, y en una **copia fuera de línea** del usuario (su
    gestor de contraseñas).
  - Se genera una vez en el PC del usuario con `minisign -G`, del paquete
    `minisign` de Arch que instala el usuario.
  - La pública también se versiona en `packaging/minisign.pub`.
- **Rotación.** Una versión firmada con la clave antigua añade la nueva a la
  lista. La siguiente ya se firma con la nueva, y una posterior retira la
  antigua. Si la clave privada se filtra, se publica cuanto antes una versión
  que la retire. Si se pierde, hace falta una instalación manual.
- **La app solo instala si se cumple todo esto**:
  1. la firma es válida con una clave aceptada y el comentario de confianza
     coincide con el manifiesto;
  2. `format` es 1;
  3. `version` es **estrictamente mayor** que la instalada (SemVer), así que
     nunca baja de versión, y no está en la lista de versiones malas (§7);
  4. hay un artefacto de su `target`. Su `name` no lleva `/`, `\` ni `..`, y
     su `size` no pasa de 256 MiB;
  5. el fichero se descarga de
     `https://github.com/danielperezmartinez/titan-agent/releases/download/v<version>/<name>`
     (URL construida por la app, nunca leída del manifiesto), con el tamaño y
     el SHA-256 exactos;
  6. al extraerlo, toda entrada es relativa, sin `..` ni enlaces que salgan
     de la carpeta, y la carpeta resultante tiene los cuatro binarios.

  Si algo falla, no instala, anota el error en el estado de la actualización
  (§6) y lo reintenta en la siguiente comprobación. No hay diálogos.
- **Primera instalación.** Se verifica con `gh attestation verify <tar.gz>
  --repo danielperezmartinez/titan-agent`, que no exige instalar nada más.

### 4. Instalación en Linux (por usuario, sin sudo)

Con `$XDG_DATA_HOME` (o `~/.local/share`) como base:

```
~/.local/share/titan-agent/
  versions/0.6.0/{bin,share}/   una carpeta por versión, que nunca se modifica
  versions/0.6.1/…
  current -> versions/0.6.1     enlace relativo; se cambia con rename atómico
  titan-agent-launcher          copia estable del lanzador
  update/                       descargas, pending.json, bad.json, state.json
~/.local/bin/titan-agent  -> ~/.local/share/titan-agent/titan-agent-launcher
~/.local/bin/titan-team   -> ~/.local/share/titan-agent/current/bin/titan-team
~/.local/share/applications/io.github.danielperezmartinez.titanagent.desktop
~/.local/share/icons/hicolor/scalable/apps/io.github.danielperezmartinez.titanagent.svg
```

- **Identificador.** Es `io.github.danielperezmartinez.titanagent`, como en
  titan-ssh. La interfaz usa el mismo `app_id` de Wayland para que Plasma
  asocie la ventana a su entrada del menú. El `Exec=` del `.desktop` es la
  ruta absoluta de `~/.local/bin/titan-agent`.
- **Instalar.** `install.sh` solo llama a `bin/titan-agent-launcher install`,
  que:
  - copia la carpeta a `versions/<v>` pasando por una carpeta temporal y un
    `rename`;
  - apunta `current` a ella;
  - escribe el `.desktop`, el icono y los enlaces;
  - copia el lanzador si el suyo es más nuevo.

  Instalar es idempotente.
- **Desinstalar.** `titan-agent-launcher uninstall` lo quita todo menos la
  configuración (`~/.config/titan-agent`).
- **Versiones guardadas.** Se conservan la actual y la anterior. Las más
  antiguas se borran cuando la nueva queda confirmada (§7).
- **Cuándo está activo el actualizador.** Solo cuando el ejecutable que corre
  está dentro de `…/titan-agent/versions/`. Una build de desarrollo
  (`cargo run`, el lanzador «main», las fases) nunca comprueba ni aplica
  nada. `TITAN_AGENT_NO_UPDATE=1` también lo apaga.
- **Builds de debug.** Usan por defecto el socket `agentd-dev.sock` en lugar de
  `agentd.sock` (`cfg(debug_assertions)`), para no pisarse nunca con la app
  instalada. `TITAN_AGENTD_SOCKET` sigue mandando sobre ambos.

### 5. Windows (experimental)

En la 0.6.0 solo se publica el `zip` portable, **sin firmar** (SmartScreen
avisará) y sin instalador ni actualizador. Para cuando exista el daemon de
Windows, queda decidido el diseño equivalente:

- carpeta por versión en `%LOCALAPPDATA%\Programs\titan-agent\versions\<v>\`;
- un fichero `current` con la versión, porque sin modo de desarrollador no hay
  enlaces simbólicos;
- el lanzador estable `titan-agent.exe` en la raíz;
- un acceso directo en el menú Inicio.

Las versiones en carpetas aparte evitan sustituir un `.exe` en uso.

### 6. Comprobación, descarga y aviso

- **Quién lo hace.** El **daemon**: vive aunque la ventana esté cerrada, es
  único por usuario y es el único que lanza procesos (ADR-0004). La lógica
  pura va en el núcleo o en un crate propio, sin E/S. Esa lógica es el
  manifiesto, la comparación de versiones, las comprobaciones del §3 y la
  decisión de cuándo aplicar.
- **Cuándo comprueba.** A los 30 s de arrancar el daemon y luego cada 6 h.
  También con «Buscar ahora». Un interruptor en Configuración («Buscar
  actualizaciones», activado por defecto) apaga las comprobaciones
  automáticas. El daemon guarda ese ajuste en
  `$XDG_CONFIG_HOME/titan-agent/update.json`.
- **Privacidad.** Es una sola petición `GET` a
  `https://github.com/danielperezmartinez/titan-agent/releases/latest/download/titan-agent-update.json`,
  más la del `.minisig`, y la descarga del artefacto solo si hay versión
  nueva.
  - Sin autenticación, cookies, identificadores ni telemetría.
  - `User-Agent: titan-agent`, sin versión.
  - GitHub solo ve la IP y qué se descarga.
  - `latest` nunca es una pre-release.
- **Preparar.** Si hay versión nueva, la descarga en `update/`, la verifica
  (§3) y la extrae a `versions/<v>`. Queda **preparada**, pero `current` no
  cambia.
- **Protocolo.** Son cambios aditivos, sin subir `PROTOCOL_VERSION`:
  - el `hello` devuelve la versión de la app del daemon;
  - peticiones `update_status`, `update_check`, `update_apply {force}` y
    `update_settings`;
  - evento `update` con `state`, que puede ser `idle`, `checking`,
    `downloading`, `ready`, `waiting`, `applying` o `failed`, más `version`,
    `notes_url`, `blocking` (las sesiones que impiden aplicar) y `error`.
- **Aviso.** Va en la cabecera, según
  [[Decisiones visuales/Aviso de actualización en la cabecera]]: discreto, sin
  toasts ni ventanas. En Configuración están la versión instalada, el
  interruptor, «Buscar ahora» y el resultado de la última comprobación.

### 7. Aplicar, reiniciar el daemon y volver atrás

- **Condición segura.** Ninguna interfaz está conectada y ninguna sesión con
  terminal en el daemon está trabajando (`[>]`). Hasta que se compruebe lo
  contrario (§8), un TUI de Codex lanzado desde la app cuenta como trabajando
  mientras su sesión no esté en espera o terminada.
- **Al cerrar la ventana.** Si hay una versión preparada, el daemon la aplica
  en cuanto se cumple la condición segura. Mientras no se cumple, espera
  (estado `waiting`) y lo vuelve a mirar en cada cambio de sesiones. Si el
  usuario reabre antes, se abre la versión anterior con su daemon, y el aviso
  sigue ahí.
- **Botón «actualizar».** Pide `update_apply`.
  - Si hay sesiones trabajando con terminal en el daemon, el aviso lo dice
    («se cerrarán los terminales de 2 sesiones; las sesiones siguen vivas») y
    hay que confirmarlo (`force`).
  - Después, el daemon aplica la versión y la interfaz se vuelve a abrir con el
    lanzador. El lanzador le pasa su ruta en `TITAN_AGENT_LAUNCHER`.
- **Aplicar** consiste en:
  1. escribir `update/pending.json` (`from`, `to`, `attempts: 0`);
  2. cambiar `current` con un `rename` atómico;
  3. actualizar el lanzador si cambió;
  4. el daemon se **reemplaza a sí mismo con `exec`** del `titan-agentd` de
     la versión nueva, en el mismo socket.

  Los terminales del daemon se cierran. Las sesiones siguen vivas en Claude
  Code o Codex, y la interfaz las vuelve a adjuntar al elegirlas. Pasar los
  PTY abiertos al proceso nuevo queda como mejora futura: `portable-pty` no
  permite reconstruirlos desde el descriptor.
- **Lanzador y vuelta atrás.** El menú y `~/.local/bin/titan-agent` ejecutan
  `titan-agent-launcher`, que hace esto antes de ejecutar
  `current/bin/titan-agent`:
  - Si hay `pending.json` con `attempts >= 2`, **vuelve atrás**:
    1. apunta `current` a `from`;
    2. anota `to` en `update/bad.json`, para no instalarla otra vez hasta que
       haya una mayor;
    3. detiene el daemon de la versión mala, con el pid que el daemon anota
       junto al socket;
    4. borra `pending.json`;
    5. avisa con `notify-send`.
  - Si no, incrementa `attempts`.
- **Confirmar la versión nueva.** La interfaz borra `pending.json` cuando ha
  completado el `hello` con un daemon de su misma versión y lleva 10 s
  abierta. Entonces se borran las versiones más antiguas que la anterior.
- **Interfaz y daemon de versiones distintas** (p. ej. tras volver atrás con
  un daemon vivo). La interfaz lo indica en el aviso y ofrece reiniciar el
  daemon con la condición segura.

### 8. A comprobar al implementar

- Si un turno de Codex lanzado desde la app (TUI con `--remote unix://`)
  sigue corriendo en el `codex app-server` cuando su TUI muere. Si sigue, sus
  terminales dejan de bloquear la aplicación igual que los de Claude. El
  resultado se anota aquí.
- Las dependencias concretas, a elegir con criterio de tamaño y sin OpenSSL:
  cliente HTTP con rustls (p. ej. `ureq`), `tar` + `flate2`, `sha2` y
  `minisign-verify`.

### 9. Primera instalación (desde la build de debug)

Cuando se publique la 0.6.0:

1. Detener el daemon de debug que use el socket por defecto.
2. `gh release download v0.6.0 -p '*linux-x86_64.tar.gz'` en una carpeta
   vacía.
3. `gh attestation verify` sobre el tar.gz.
4. Descomprimir y ejecutar `./install.sh`.
5. Abrir «titan-agent» desde el menú.
6. Retirar la entrada «titan-agent (main)» del menú
   (`~/.local/share/applications/titan-agent.desktop`). Su worktree y su
   script los borra el usuario cuando quiera.

A partir de ahí, las versiones llegan solas.

## Alternativas consideradas

- **AppImage** (rechazada): el daemon queda colgado del montaje FUSE de la
  imagen mientras vive, `titan-team` no queda en el PATH y sustituir la imagen
  con el daemon vivo es delicado.
- **Paquete de pacman o AUR** (rechazado): necesita sudo para instalar y
  actualizar, así que la app no podría actualizarse sola.
- **Flatpak** (rechazado): el sandbox impide ejecutar `claude` y `codex` en
  el sistema y leer sus ficheros, que es el núcleo de la app.
- **Compilar desde el código en el PC del usuario** (rechazado por el
  usuario): es lo que hace hoy el lanzador «main».
- **Windows sin publicar** (rechazado): se prefiere el zip experimental para
  que el CI ya lo empaquete.
- **Solo `SHA256SUMS`** (rechazado): el hash viaja en la misma Release que el
  binario y no protege si se compromete la Release.
- **Solo atestaciones de GitHub** (rechazado): verificarlas dentro de la app
  exige `sigstore-rs` y la raíz de TUF, y quien controla el repo puede
  generarlas. Se quedan para la primera instalación.
- **Etiqueta a mano** (rechazado): puede no coincidir con el `Cargo.toml` o
  con `main`.
- **Cada PR integrada publica** (rechazado): toda PR sube la versión,
  incluidas las de `experimentos/`.
- **Aplicar siempre al cerrar** (rechazado): cortaría los terminales de
  agentes que están trabajando.
- **Canal canary** (aplazado): con un solo usuario, `main` ya se prueba antes
  de publicar. Si hace falta, se decide en otra ADR.

## Consecuencias

- Positivas:
  - Una sola fuente de artefactos, compilados por el CI.
  - Instalación sin sudo e integrada en Plasma.
  - Las versiones llegan solas, verificadas con una clave propia y gratuita.
  - Ninguna actualización corta a un agente que trabaja sin pedir
    confirmación.
  - Una versión que no arranca se deshace sola.
  - La instalada y las de desarrollo ya no se pisan.
- Negativas / compromisos:
  - La clave privada de minisign es tan crítica como el acceso al repo. Si se
    filtra, alguien puede actualizar la instalación del usuario. Si se pierde,
    hace falta instalar a mano.
  - El daemon gana red, descarga, extracción y su propio reemplazo. El
    actualizador y el lanzador deben ser lo más estable del código, con tests
    de cada comprobación del §3 y de la vuelta atrás.
  - Al reiniciar el daemon se pierde el historial de los terminales y hay que
    volver a adjuntarlos.
  - Windows sale sin firma y sin actualizador hasta que tenga daemon.
  - `[workspace.package].version` no puede pasar a 1.x. Mientras sea 0.x, un
    cambio incompatible sube la `minor`.
- Aceptada por el usuario el 2026-11-17, tras ver un boceto de la cabecera,
  de Configuración y del flujo.
- Afecta a [[Tareas/Versión 0.6.0 - Instalador y actualización automática]],
  que lo implementa, y a la sección «Distribución» de
  [[Seguridad/Checklist de lanzamiento]].
