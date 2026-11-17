---
Nombre: Versión 0.6.0 - Instalador y actualización automática
Estado: En curso
Resumen: 'Implementar ADR-0011. Hay que montar el workflow «Publicar versión» (tar.gz de Linux, zip experimental de Windows, manifiesto firmado con minisign y atestaciones) y titan-agent-launcher (instalar, desinstalar, lanzar y volver atrás). El actualizador va en el daemon (comprobar cada 6 h, descargar, verificar, preparar y aplicar cuando sea seguro, con exec), y el aviso [^] en la cabecera con su bloque de Configuración. Las builds de debug pasan al socket agentd-dev.sock. Lo hace el equipo v5 y termina publicando la v0.6.0 e instalándola en el PC del usuario.'
Decisiones: '[[Decisiones/ADR-0011 Distribución en GitHub Releases con instalación por usuario y actualización firmada]] y [[Decisiones visuales/Aviso de actualización en la cabecera]]. Versión 0.6.0 (minor), acordada con el usuario el 2026-11-17. Clave generada con minisign del paquete de Arch, que instala el usuario. Al instalar se retira la entrada «titan-agent (main)» del menú.'
Bloqueada: []
Fecha de creación: 2026-11-17T10:00:00+01:00
Última modificación: 2026-11-17T23:50:00+01:00
---

# Versión 0.6.0 - Instalador y actualización automática

## Objetivo

Que el usuario tenga titan-agent instalado en su PC y que las versiones nuevas
le lleguen solas, tal como fija
[[Decisiones/ADR-0011 Distribución en GitHub Releases con instalación por usuario y actualización firmada|ADR-0011]].
Hasta ahora no se ha publicado ninguna versión: la 0.6.0 es la primera.

Está bloqueada por la ronda 0.5.0 (sesión «titan-agent: v0.5 equipo v4», rama
`feat/v0.5-equipo-v4`). Su PR cierra
[[Acentos y teclas muertas en el terminal integrado]]. Esta tarea empieza
cuando esa PR esté integrada, sobre el `main` resultante.

## Alcance

1. **Comprobación previa (spike, primero).** Comprobar si un turno de Codex
   lanzado desde la app (`codex --remote unix://`) sigue en el
   `codex app-server` cuando su TUI muere, con un CLI real y un encargo
   inofensivo en una carpeta de prueba. El resultado se anota en el §8 de
   ADR-0011 y decide qué terminales bloquean la aplicación (§7).
2. **Previa visual.** Dibujar el aviso en `design/preview/`: la cabecera en los
   estados `ready`, confirmación, `applying` y vuelta atrás, más el bloque
   «Actualizaciones» de Configuración. Sigue
   [[Decisiones visuales/Aviso de actualización en la cabecera]] y la revisa el
   usuario antes de implementar la interfaz.
3. **Núcleo.** Lógica pura y con tests, sin E/S:
   - el manifiesto v1;
   - la comparación SemVer;
   - las seis comprobaciones del §3;
   - la validación de entradas del tar;
   - la condición segura del §7.
4. **`titan-agent-launcher`**, binario nuevo:
   - `install` (idempotente), `uninstall` y lanzar con `pending.json`;
   - vuelta atrás tras dos arranques fallidos, con `bad.json`, la parada del
     daemon malo y `notify-send`;
   - le pasa `TITAN_AGENT_LAUNCHER` a la interfaz.
5. **Daemon.**
   - Comprueba a los 30 s y cada 6 h, con `User-Agent: titan-agent` y sin
     datos del usuario.
   - Descarga, verifica, extrae a `versions/<v>` y lo deja preparado.
   - Aplica: `pending.json`, `rename` de `current`, actualizar el lanzador y
     `exec`.
   - Espera la condición segura.
   - Anota su pid junto al socket.
   - Guarda los ajustes en `update.json`.
   - Protocolo aditivo: la versión en `hello`, `update_status`,
     `update_check`, `update_apply {force}`, `update_settings` y el evento
     `update`.
   - El actualizador solo está activo dentro de `…/titan-agent/versions/` y sin
     `TITAN_AGENT_NO_UPDATE`.
6. **Interfaz.**
   - El aviso `[^]` en la cabecera con todos sus estados.
   - El bloque «Actualizaciones» en Configuración.
   - Al pulsar «actualizar», se reabre con el lanzador.
   - Confirma la versión (borra `pending.json` tras el `hello` y 10 s).
   - `app_id` `io.github.danielperezmartinez.titanagent`.
7. **Socket de desarrollo.** Las builds de debug usan `agentd-dev.sock` por
   defecto.
8. **Empaquetado.**
   - El `.desktop` y un icono SVG propio coherente con
     [[Decisiones visuales/Lenguaje visual heredado de titan-ssh]]. Hasta que
     el usuario lo apruebe, se usa `utilities-terminal`.
   - `install.sh`.
   - La clave pública en `packaging/minisign.pub` y compilada en la app.
9. **CI.**
   - El workflow `.github/workflows/release.yml` («Publicar versión») según el
     §2 de ADR-0011.
   - `scripts/comprobar-version.sh` rechaza las versiones 1.x o mayores.
   - Una prueba en el CI normal que empaqueta el tar.gz de Linux sin publicar
     nada y ejecuta `install` y `uninstall` en un `HOME` temporal.
10. **Configuración fuera del código.** La hace el usuario, o un agente con su
    autorización en esa sesión:
    - el usuario instala `minisign` (`pacman -S minisign`), genera la clave
      con contraseña y la guarda en su gestor de contraseñas;
    - el environment `release`, restringido a `main`, con
      `TITAN_AGENT_UPDATE_KEY` y `TITAN_AGENT_UPDATE_KEY_PASSWORD`;
    - las *immutable releases* y el ruleset de etiquetas `v*`.

    Todo queda anotado en [[Seguridad/Checklist de lanzamiento]].
11. **Publicar e instalar.**
    - Integrar la PR (versión 0.6.0) con autorización del usuario.
    - Lanzar «Publicar versión» con su autorización y verificar la release.
    - Primera instalación según el §9 de ADR-0011, incluida la retirada de la
      entrada «titan-agent (main)».

Fuera de alcance: el instalador y el actualizador de Windows (se hacen con el
daemon de Windows), un canal canary y pasar los PTY al daemon nuevo.

## Equipo (configuración v5, skill `equipo`)

Configuración vigente descrita en [[Equipo v5]].

- **Opus dirige e implementa.** Fija el contrato numerado y las firmas que
  usan los tests antes de lanzar a Sol, a partir de los §2 a §7 de ADR-0011 y
  de la decisión visual, sin inventar detalles. Después implementa cada parte
  con sus tests; los movimientos de código, con `scripts/mover-lineas.sh`.
- **Sol escribe pocos tests de aceptación** desde ese contrato, antes de
  implementar:
  - el manifiesto rechazado por cada una de las seis comprobaciones (firma,
    formato, versión no mayor o en `bad.json`, nombre o tamaño, SHA-256,
    entrada del tar fuera de la carpeta);
  - la condición segura (con ventana conectada, con una sesión trabajando
    y sin ninguna);
  - la vuelta atrás del lanzador al segundo arranque fallido;
  - `install` idempotente en un `HOME` temporal;
  - un workflow que rechaza `1.0.0`.

  Al menos uno por apartado del contrato; titan-team formatea y ejecuta los
  tests tras su turno.

  Las firmas de los tests usan una clave de prueba generada para ellos, nunca
  la real.
- **Revisión de Sol** con contexto limpio al final, con la compuerta
  (`ask --gate`). Este es el código más
  delicado de la app: descarga y ejecuta binarios. Revisa también
  `release.yml` (permisos mínimos, secretos solo en `publicar`, sin
  interpolar entradas en scripts).
- **Evaluador visual** (Opus nuevo) del aviso contra la previa, con capturas
  del kit `scripts/demo/` (una escena por estado del aviso: `ready`,
  confirmación, `applying`, vuelta atrás; hay que añadirlas al kit). La
  interacción la prueba el usuario con una «Prueba a mano» de pasos y
  resultado esperado.
- **Luna** solo para ediciones repetitivas; su turno pasa la compuerta.

## Diseño

Lanzada desde la sesión de control `control-3b` el 2026-11-17; rama `feat/v0.6.0`, que sale de `main` con la [[Equipo v5.1 - Compilación compartida por equipo|0.5.2]] integrada. La 0.5.0 ya está en `main`, así que el bloqueo está resuelto.

Firmas, CLI falsos y ficheros de los tests: en la especificación del encargo `v06` (fuera del repo); aquí queda el contrato que fija el comportamiento.

### Contrato (para los tests de aceptación)

#### C1. Verificación del manifiesto (`titan_agent_core::update::verify_manifest`)

`verify_manifest(manifest, signature, keys)` recibe los bytes del manifiesto
(`titan-agent-update.json`), el texto del fichero de firma minisign (`.minisig`) y las claves
públicas aceptadas (cada una, la clave en base64 tal como la da minisign: la segunda línea de un
`.pub`, p. ej. `RWQ…`). Devuelve el manifiesto si y solo si:

1. la firma verifica los bytes exactos del manifiesto con **alguna** de las claves (con cualquier
   otra clave, o con los bytes cambiados, o con una firma que no se puede leer → error `signature`);
2. el manifiesto es JSON con la forma de abajo (si no → `manifest`);
3. `format` es 1 (si no → `format`);
4. el comentario de confianza de la firma es exactamente
   `titan-agent-update format=1 version=<version del manifiesto>` (si no → `signature`).

Forma del manifiesto (formato 1; campos desconocidos se ignoran):

```json
{"format":1,"version":"0.6.1","published":"2026-11-20T10:00:00Z",
 "notes_url":"https://github.com/danielperezmartinez/titan-agent/releases/tag/v0.6.1",
 "artifacts":[{"target":"linux-x86_64","name":"titan-agent-0.6.1-linux-x86_64.tar.gz",
               "sha256":"<64 hex>","size":12345678}]}
```

#### C2. Elección del artefacto (`titan_agent_core::update::select`)

`select(manifest, installed, bad, target, base_url)` decide si hay que instalar. En este orden:

1. `version` del manifiesto se lee como SemVer estricto `MAYOR.MENOR.PARCHE` (solo dígitos, sin
   pre-release ni build; si no → `manifest`);
2. tiene que ser **estrictamente mayor** que `installed` (si no → `not_newer`; así nunca baja);
3. no puede estar en `bad` (→ `bad_version`);
4. hay un artefacto con ese `target` (→ `no_artifact`);
5. su `name` no está vacío y no contiene `/`, `\` ni `..` (→ `bad_name`);
6. su `size` no pasa de `MAX_ARTIFACT_BYTES` = 256 MiB = 268 435 456 (→ `too_big`; justo 256 MiB
   vale).

Devuelve `Selected` con la versión, el artefacto, `notes_url` y la URL de descarga **construida**
como `<base_url>/download/v<version>/<name>` (nunca una URL leída del manifiesto).
`manifest_url(base_url)` es `<base_url>/latest/download/titan-agent-update.json` y
`signature_url(base_url)` lo mismo con `.minisig` al final. `RELEASES_URL` es
`https://github.com/danielperezmartinez/titan-agent/releases`.

`Version::parse` acepta solo `^[0-9]+\.[0-9]+\.[0-9]+$` (sin ceros a la izquierda salvo el propio
`0`, sin `v`, sin espacios); se ordena numéricamente (0.10.0 > 0.9.9) y `Display` escribe `0.6.1`.

#### C3. Integridad de la descarga (`titan_agent_core::update::check_download`)

`check_download(size, sha256_hex, artifact)`: `size` y el SHA-256 calculado del fichero bajado
deben coincidir exactamente con los del artefacto (el hex se compara sin distinguir mayúsculas).
Tamaño distinto → `size_mismatch` (se comprueba antes que el hash); hash distinto →
`hash_mismatch`. `sha256_hex(bytes)` devuelve el SHA-256 en hex minúscula (64 caracteres).

#### C4. Contenido del tar.gz (`check_entry`, `check_bundle`, `bundle_root`)

El tar.gz de la versión `<v>` contiene una sola carpeta `bundle_root(v, target)` =
`titan-agent-<v>-<target>` (p. ej. `titan-agent-0.6.1-linux-x86_64`) con `bin/` y `share/`.

`check_entry(root, path, kind, link)` valida **cada entrada** antes de extraerla
(`root` = esa carpeta; `path` = ruta de la entrada tal como viene en el tar, con `/`):

- válida: `root`, `root/`, o `root/<algo>`; relativa; ningún componente `..`; sin `\`;
- `kind`: `File` y `Dir` valen; `Symlink`: su destino `link` debe ser relativo y, resuelto desde
  la carpeta de la entrada, quedarse dentro de `root`; `Hardlink`: su `link` es otra ruta del
  archivo y cumple las mismas reglas que `path`; `Other` (dispositivo, fifo…) nunca vale;
- cualquier otra cosa → `unsafe_entry` (p. ej. `/etc/x`, `../x`, `otra-carpeta/x`,
  `root/../x`, un enlace a `../../etc/passwd` o a `/etc/passwd`).

`check_bundle(files)` recibe las rutas de ficheros extraídos **relativas a `root`**
(`bin/titan-agent`…) y exige los cuatro de `BINARIES`
(`titan-agent`, `titan-agentd`, `titan-team`, `titan-agent-launcher`) en `bin/`; si falta
alguno → `incomplete`.

#### C5. Condición segura para aplicar (`apply_blockers`, `safe_to_apply`)

`apply_blockers(sessions, terminals)` devuelve las claves (`key`) de las sesiones que tienen un
terminal en el daemon (aparecen en `terminals` como `TerminalRef.session`) **y** están
trabajando (`SessionState::Working`), ordenadas y sin repetir. Una sesión trabajando sin
terminal en el daemon no bloquea; una con terminal que está en espera, te necesita o terminada,
tampoco. (Da igual que sea de Claude Code o de Codex: un turno de Codex sigue en su app-server
aunque muera su TUI.)

`safe_to_apply(windows, blockers)` es `true` solo si no hay ninguna ventana conectada
(`windows == 0`) y `blockers` está vacío.

#### C6. Lanzador: arrancar y volver atrás (`titan-agent-launcher` sin argumentos)

Sin argumentos (lo que ejecutan el menú y `~/.local/bin/titan-agent`), el lanzador usa
`root = <data>/titan-agent` (de `HOME`/`XDG_DATA_HOME`) y lee `root/update/pending.json`:

```json
{"from":"0.6.0","to":"0.6.1","attempts":0}
```

- sin `pending.json`: ejecuta `root/current/bin/titan-agent`;
- con `attempts < 2`: reescribe `pending.json` con `attempts + 1` y ejecuta
  `root/current/bin/titan-agent`;
- con `attempts >= 2` (**vuelta atrás**):
  1. `current` pasa a apuntar a `versions/<from>`;
  2. añade `<to>` a `root/update/bad.json` (array JSON de textos de versión, sin repetir; lo crea
     si no existe);
  3. detiene el daemon de la versión mala: lee el pid de `agentd.pid`, junto al socket del daemon
     (mismo directorio que el socket; ver C8), y le envía SIGTERM **solo si** ese proceso es un
     ejecutable de `root/versions/<to>/` (si no existe o es otro, no hace nada);
  4. borra `pending.json`;
  5. escribe `root/update/rolled-back.json` con `{"from":"<from>","to":"<to>"}`;
  6. ejecuta `notify-send` (el que haya en `PATH`; si no hay, sigue) con dos argumentos:
     `titan-agent` y `La versión <to> no arrancó; se ha vuelto a la <from>.`;
  7. ejecuta `root/current/bin/titan-agent` (ya la de `<from>`).

La interfaz se ejecuta (`exec`, en Unix) con los argumentos que recibió el lanzador y con la
variable `TITAN_AGENT_LAUNCHER` = ruta absoluta de `root/titan-agent-launcher`. Si no hay
`root/current/bin/titan-agent` → mensaje en stderr y salida 1.

#### C7. Lanzador: instalar y desinstalar

- `titan-agent-launcher --version` imprime `titan-agent-launcher <versión del workspace>` y sale 0.
- `titan-agent-launcher install`: la carpeta a instalar es la que contiene el `bin/` del propio
  lanzador (el lanzador está en `<carpeta>/bin/titan-agent-launcher`). Exige en `<carpeta>/bin/`
  los cuatro binarios (si falta alguno: salida 1 y no instala nada). Instala la versión
  `<v>` = la del propio lanzador:
  - copia `<carpeta>` a `root/versions/<v>` pasando por una carpeta temporal en `root/versions/`
    y un `rename` (si `versions/<v>` ya existe, no la toca);
  - apunta `root/current` a `versions/<v>` (enlace simbólico relativo, cambiado con `rename`);
  - copia `root/titan-agent-launcher` desde `versions/<v>/bin/` (si no existe, o si el instalado
    es de una versión menor; uno de versión mayor no se toca);
  - crea los enlaces `$HOME/.local/bin/titan-agent` → `root/titan-agent-launcher` y
    `$HOME/.local/bin/titan-team` → `root/current/bin/titan-team` (rutas absolutas; reemplaza un
    enlace anterior);
  - escribe `<data>/applications/io.github.danielperezmartinez.titanagent.desktop` con, entre
    otras, las líneas `Exec=<HOME>/.local/bin/titan-agent` (ruta absoluta) y
    `StartupWMClass=io.github.danielperezmartinez.titanagent`;
  - sale 0. **Idempotente**: repetir `install` sale 0 y deja exactamente lo mismo.
- `titan-agent-launcher uninstall`: borra `root` entero, el `.desktop`, el icono y los dos
  enlaces de `$HOME/.local/bin` (solo si son enlaces que apuntan dentro de `root`); **nunca**
  `$XDG_CONFIG_HOME/titan-agent` ni `$HOME/.config/titan-agent`. Sale 0, también si no había nada.
- Una orden desconocida: salida 2.

#### C8. Daemon: protocolo de actualización, pid y socket de desarrollo

Protocolo NDJSON ya existente (`{"id":N,"method":…,"params":…}`; primero `hello`). Cambios
aditivos, sin subir `PROTOCOL_VERSION` (sigue en 1):

- `hello` ya devuelve `{"protocol":1,"daemon":"<versión>"}` (ya existe).
- Peticiones nuevas: `update_status` (sin params), `update_check` (sin params),
  `update_apply` (`{"force":bool}`, `force` por defecto `false`), `update_settings`
  (`{"auto":bool}`), `update_dismiss` (sin params).
- `update_status` responde un `UpdateStatus` (abajo). `update_settings` guarda
  `{"auto":<bool>}` en `<config>/update.json` y responde el `UpdateStatus` ya con ese `auto`
  (`<config>` = `$TITAN_AGENT_CONFIG_DIR` si está, si no `$XDG_CONFIG_HOME/titan-agent`, si no
  `$HOME/.config/titan-agent`). `auto` vale `true` si el fichero no existe.
- El actualizador solo está **activo** si el ejecutable del daemon está dentro de
  `<algo>/titan-agent/versions/<versión>/bin/` y no está definida `TITAN_AGENT_NO_UPDATE`. Si no
  está activo: `update_status` responde `active: false`, `state: "idle"`; `update_check` y
  `update_apply` responden el error `update_inactive`.
- `update_apply` con algo que aplicar y sesiones bloqueantes (C5) y `force: false` → error
  `confirm_needed`; sin nada preparado → error `nothing_to_apply`.
- Evento nuevo `{"event":"update","params":<UpdateStatus>}`, a los clientes suscritos, cada vez
  que cambia el estado.
- Al arrancar, el daemon escribe su pid (texto decimal) en `agentd.pid` **en el mismo directorio
  que su socket**, sea cual sea el nombre del socket.
- Socket por defecto: una build de debug (`cfg(debug_assertions)`, lo que compilan los tests)
  usa `<XDG_RUNTIME_DIR>/titan-agent/agentd-dev.sock`; una de release, `agentd.sock`.
  `TITAN_AGENTD_SOCKET` manda sobre ambos (ya existe). `titan-agentd --print-socket` imprime la
  ruta (ya existe).

`UpdateStatus` en JSON:

```json
{"state":"idle","active":false,"installed":"0.6.0","current":null,"version":null,
 "notes_url":null,"blocking":[],"error":null,"auto":true,"last_check_ms":null,
 "rolled_back":null}
```

`state` ∈ `idle`, `checking`, `downloading`, `ready`, `waiting`, `applying`, `failed`;
`installed` = versión del daemon que responde; `rolled_back` = `{"from","to"}` o `null`.

#### C9. Scripts de versión

- `scripts/comprobar-version.sh <sha base>` (ya existe: falla si la versión de `Cargo.toml` no
  supera la del commit base) además **falla (exit 1) si la versión es 1.0.0 o mayor**, aunque
  supere la base, con un mensaje que menciona `1.x`.
- `scripts/version-publicable.sh` (nuevo; lo usa el workflow «Publicar versión»): lee
  `version` de `[workspace.package]` del `Cargo.toml` del directorio actual; si cumple
  `^0\.[0-9]+\.[0-9]+$` la imprime y sale 0; si no (1.x, pre-release `0.6.0-rc.1`, vacía), sale
  1 con un mensaje en stderr.

Ambos se ejecutan con `sh <script>` desde la raíz de un repo git; leen `./Cargo.toml`.

### Previa y criterios visuales

Previa en `design/preview/Actualizacion.dc.html` (tablero 4 de `canvas.json`),
**aprobada por el usuario el 2026-11-17 sin el icono propuesto**: el
`.desktop` usa `utilities-terminal` hasta que haya otro.

Criterios para el evaluador visual, fijados antes de implementar:

1. **Sitio y jerarquía.** El aviso va en una sola línea a la derecha de la
   cabecera, entre los contadores y `[i]` `[*]`, con la misma tipografía
   (12 px) y separación (16 px) que los contadores. Sin nada que decir no
   ocupa sitio, y nunca empuja ni parte el título.
2. **Disciplina de color.** `[^]` y la versión en `mute`; `actualizar` y
   `reiniciar` en `accent`; solo «2 sesiones trabajando» y el `[^]` de la
   vuelta atrás en `warning`. Ningún otro color, sin fondos ni insignias.
3. **Estados.** Cada estado de la previa (`ready`, descargando,
   confirmación, actualizando, fallo, vuelta atrás con `[x]`, daemon de otra
   versión) dice exactamente su texto. La confirmación lleva `[ok]
   actualizar` como botón primario con hairline `accent` y `cancelar` en
   `mute`, y si falta sitio se corta el texto explicativo, no los botones.
4. **Configuración.** El bloque «ACTUALIZACIONES» sigue la forma del de
   «Nueva sesión» (panel `surface`, título en mayúsculas de 12 px, etiquetas
   de la misma anchura, chips). Muestra la versión instalada, el interruptor,
   `Buscar ahora` y una línea `mute` con el resultado de la última
   comprobación.
5. **Acabado.** Nada desalineado en la línea base de la cabecera, ningún
   texto partido en dos líneas y nada que no sea ASCII en los marcadores.

### Prueba a mano

Qué se prueba: los clics del aviso `[^]` con una actualización real de la
ventana instalada (la 0.6.0 que se actualiza a una 0.6.1 servida en local),
porque el evaluador visual solo ve capturas. Todo vive en una carpeta de
`/tmp`, con su propio socket, datos inventados del kit de demo, CLI falsos y
una clave de prueba: no toca la instalación real, el daemon de cada día ni el
menú. Builds de debug, que son las únicas que aceptan otra URL y otra clave.

Preparación (compila dos veces, unos minutos):
`scripts/demo/actualizacion.sh preparar`

1. `scripts/demo/actualizacion.sh abrir` → se abre titan-agent con las
   sesiones inventadas; en unos 10 s la cabecera muestra
   `[^] v0.6.1 · actualizar` entre los contadores y `[i]` `[*]`.
2. Pasa el ratón por `v0.6.1` → se subraya; clic → abre en el navegador la
   página de la versión (no existe todavía: 404 de GitHub, es lo esperado).
3. `[*]` → Configuración muestra el bloque «ACTUALIZACIONES»: versión
   instalada v0.6.0, `[ok] activado`, `Buscar ahora` y «v0.6.1 lista; se
   aplica al cerrar la ventana».
4. Clic en `[ok] activado` → pasa a `[ ] desactivado`; otra vez → vuelve a
   `[ok] activado`.
5. Elige en el árbol una sesión trabajando (`[>]`, p. ej. «titan-ssh: sftp
   renombrar») para abrir su terminal y pulsa `actualizar` → la cabecera
   cambia a «se cerrarán los terminales de 1 sesión trabajando; siguen vivas
   · [ok] actualizar · cancelar».
6. Pulsa `Esc` → vuelve a `[^] v0.6.1 · actualizar`. Repite 5 y pulsa
   `cancelar` → lo mismo.
7. Pulsa `actualizar` y luego `[ok] actualizar` → «[^] actualizando…»; la
   ventana se cierra y en 2 o 3 s se abre otra, ya de la 0.6.1, sin aviso.
   La sesión del paso 5 sigue en el árbol y su terminal se vuelve a abrir al
   elegirla.
8. Espera 10 s y, en otra terminal, `scripts/demo/actualizacion.sh estado`
   → `current: versions/0.6.1`, `pending.json: -`.
9. Cierra la ventana. `scripts/demo/actualizacion.sh down`.

Vuelta atrás (opcional): `preparar`, `romper` y `abrir`; espera al aviso y
cierra la ventana (se aplica sola) → `estado` dice `current:
versions/0.6.1`. Abre tres veces seguidas `abrir`: las dos primeras la
ventana no aparece (la 0.6.1 «rota» falla al arrancar); la tercera sale una
notificación del sistema «La versión 0.6.1 no arrancó; se ha vuelto a la
0.6.0.» y se abre la 0.6.0 con `[^] v0.6.1 no arrancó; sigues en v0.6.0
[x]` en `warning`. Clic en `[x]` → el aviso desaparece. `down` al acabar.

No se puede probar en la demo: el ancho estrecho (`[^] actualizar`), porque
el kit abre la ventana a 1440 × 900; redimensiona la ventana del paso 1 por
debajo de 1100 px para verlo.

## Criterios de finalización

- PR integrada con la versión 0.6.0, los checks en verde y la revisión de Sol
  resuelta.
- La release `v0.6.0` publicada por el workflow y verificada: ejecución en
  verde, assets, manifiesto que verifica con `minisign -V` contra
  `packaging/minisign.pub` y `gh attestation verify` correcto.
- La app instalada en el PC del usuario desde la release, abierta desde el
  menú de Plasma con su icono, y la entrada «titan-agent (main)» retirada.
- La actualización probada de punta a punta sin publicar una versión falsa en
  el repo real. Se hace con un manifiesto y artefactos servidos en local,
  firmados con la clave de prueba y con la URL base cambiada solo en los
  tests. Se comprueban:
  - el aviso;
  - «actualizar» (sin sesiones y con una sesión trabajando);
  - aplicar al cerrar;
  - el reinicio del daemon y la vuelta a adjuntar;
  - la vuelta atrás.
- El §8 de ADR-0011 anota el resultado del spike de Codex.
- La sección «Distribución» de [[Seguridad/Checklist de lanzamiento]] queda
  revisada.

## Implementación

- **Núcleo** (`titan_agent_core::update`, sin E/S): manifiesto y firma,
  `Version`, elección, descarga, entradas del tar, condición segura, pasos
  del lanzador, versiones que se conservan; tipos del protocolo en
  `protocol.rs` y `socket_name` (debug → `agentd-dev.sock`).
- **Lanzador** `crates/titan-agent-launcher`: `install`, `uninstall`,
  arranque con `pending.json` y vuelta atrás.
- **Daemon**: `update.rs` y `update/{fetch,prepare,apply}.rs`, peticiones en
  `hub/update.rs`, `agentd.pid` junto al socket. Solo las builds de debug
  aceptan `TITAN_AGENT_UPDATE_URL`, `TITAN_AGENT_UPDATE_KEYS` y
  `TITAN_AGENT_UPDATE_FIRST_CHECK_S` (tests y kit); una de release, la URL
  fija y la clave de `packaging/minisign.pub`.
- **Interfaz**: `app/update_notice.rs` (aviso, Configuración, relanzar con el
  lanzador, confirmar la versión), `app_id`
  `io.github.danielperezmartinez.titanagent`, socket de desarrollo.
- **Empaquetado y CI**: `packaging/` (`package.sh`, `manifest.sh`,
  `install.sh`, `LEEME.txt`, `minisign.pub` aún sin clave),
  `scripts/version-publicable.sh`, `scripts/probar-instalacion.sh` (en el job
  `Verificar`), `.github/workflows/release.yml` y el kit
  `scripts/demo/actualizacion.sh` (escenas `act-*` en `demo.sh`).

## Revisión de Sol (high, con `--gate`)

Hilo nuevo (`v06-revision`) sobre el commit `8f02603`, compuerta en verde
(exit 0, 132 s en un worktree nuevo con la carpeta de compilación del
equipo). Veredicto `changes_requested`, 6 mayores y 5 menores; el director
decidió (todo corregido en `f6b7ae5` y `3bf101f`, sin revisar por Sol):

| Hallazgo | Severidad | Decisión |
| --- | --- | --- |
| Un enlace simbólico como raíz del tar (`root` → `.`) pasaba `check_entry` | mayor | Aceptado: la raíz solo puede ser carpeta, y se comprueba antes del `rename` (tests propios) |
| Si falla el descubrimiento de una fuente, sus sesiones desaparecen y dejan de bloquear | mayor | Aceptado: un terminal cuya sesión no se ve con su fuente caída bloquea (test propio) |
| Aplicar no era transaccional (fallo al copiar el lanzador dejaba `current` cambiado) | mayor | Aceptado: el lanzador se copia antes y todo fallo posterior deshace (tests propios) |
| Comprobar y aplicar a la vez: una comprobación lenta pisaba `applying` | mayor | Aceptado: ningún cambio de una comprobación sobrescribe `applying` |
| «reiniciar» (daemon de otra versión) no mostraba la confirmación | mayor | Aceptado: misma confirmación (test propio) |
| La confirmación de versión borraba cualquier `pending.json` | mayor | Aceptado: solo si `to` y `current` son la versión de la ventana y la conexión es la misma (test propio) |
| Los cambios de `blocking` no se publicaban sin cambio de estado | menor | Aceptado: se publican, también al abrir y cerrar terminales; la interfaz pide el estado al recibir `confirm_needed` |
| `uninstall` decidía por prefijo de texto del enlace | menor | Aceptado: se resuelve y normaliza el destino (test propio) |
| `comprobar-version.sh` aceptaba 1.x si la base no tenía `Cargo.toml` | menor | Aceptado: se comprueba antes (test propio) |
| `version-publicable.sh` más estricto que el patrón de C9 (rechaza `0.06.0`) | menor | Descartado: la app rechaza ceros a la izquierda (`Version::parse`), así que una `0.06.0` publicada no se podría instalar |
| Comentarios en español en los scripts y workflows nuevos | menor | Aceptado: los tradujo Luna (encargo `v06-comentarios`) |

## Verificación

Rama `feat/v0.6.0`, en local el 2026-11-17:

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` y `cargo test --workspace` en verde: **178 tests** (144 en
  `main`). Los 9 de aceptación de Sol (commit `bd9debd`, sin tocar) pasaron
  a la primera; propios: de punta a punta del daemon con un servidor local y
  una clave de prueba (`update_e2e.rs`: preparar, esperar por una sesión
  trabajando, `confirm_needed`, aplicar con `exec`, archivo manipulado y
  privacidad de las peticiones), extracción, cambio transaccional, poda,
  lanzador, aviso y scripts. Hooks de gitleaks en cada commit.
- **Spike de Codex**: resultado en el §8 de ADR-0011 (el turno sigue).
- **Paquete**: `package.sh` + `probar-instalacion.sh` sobre el tar.gz:
  instalar dos veces (idéntico) y desinstalar conservando la configuración.
- **Actualización de punta a punta con la ventana real**
  (`scripts/demo/actualizacion.sh`, prefijo en `/tmp`, socket propio):
  0.6.0 instalada → aviso `ready` → al cerrar la ventana el daemon aplicó
  solo y se reemplazó con `exec` (mismo pid, desde `versions/0.6.1`) →
  al reabrir, `attempts` 1 y a los 10 s `pending.json` borrado. Vuelta
  atrás con una 0.6.1 que no arranca: dos fallos, al tercero `current` →
  0.6.0, `bad.json`, `rolled-back.json`, `notify-send` con el texto exacto,
  el daemon malo parado y la 0.6.1 borrada.
- **Evaluador visual**: ronda 1 (subagente nuevo) con 2 fallos de acabado
  (corte sin «…» en la confirmación, línea partida en Configuración) y 2
  detalles; corregidos. Ronda 2 (otro subagente nuevo): **cumplen los 5
  criterios**.
- Pendiente: CI de la PR, la «Prueba a mano» del usuario, la clave y los
  ajustes de GitHub, integrar, publicar y la primera instalación.

## Métricas

- **Tiempo**: de 18:01 a 20:30 del 2026-11-17, unas 2 h 30 min, con la 0.5.2
  (PR #10) incluida y una pausa por el límite de uso. Esperas al usuario:
  tres preguntas (PR previa, publicar la 0.5.2 y la previa).
- **Claude** (Opus 5.5, transcript de la sesión): unas 280 respuestas y 375
  llamadas a herramientas, unos **255 k tokens de salida, 550 k de escritura
  en caché y 94 M de lectura de caché**; más dos evaluadores visuales (86 k y
  78 k tokens).
- **Codex**:

  | Encargo | Rol | Entrada (caché) | Salida | Tiempo | Comprobaciones |
  | --- | --- | --- | --- | --- | --- |
  | `v06` | Sol, tests | 194 k (164 k) | 10,5 k | 223 s | fmt 0,8 s · test 120 s |
  | `v06-revision` | Sol, revisión high | 1,15 M (1,04 M) | 10,7 k | 342 s | compuerta 132 s |
  | `v06-comentarios` | Luna, traducir comentarios | 214 k (179 k) | 2,7 k | 96 s | fmt 0,5 s · compuerta 17,5 s |

- **Compuerta** (0.5.2): 221 s en frío → 52 s con la carpeta del equipo
  caliente; en esta ronda, 132 s la del revisor en un worktree nuevo (en la
  0.5.1, unos 8 min) y 17,5 s la de Luna.
- **Hallazgos**: Sol, 6 mayores y 5 menores (10 aceptados, 1 descartado).
  Evaluador visual: 2 rondas.

## Carencias del equipo

1. **El aislamiento del worktree rechaza órdenes compuestas que nombran git**
   (también `github` en una ruta, o `git` dentro de un mensaje de `-m`) y
   obliga a partirlas o a editar con scripts de Python en `/tmp`. Costó
   unas 15 llamadas y un `-m` vacío al revisor (el fichero del mensaje no
   llegó a escribirse). *Propuesta*: que la skill diga que en una sesión en
   segundo plano el director use una orden de git por llamada y escriba los
   mensajes largos con la herramienta de ficheros, nunca con `printf` en una
   orden compuesta; y que `bin/team ask` acepte `--message-file`.
2. **El contrato nombraba el icono sin su ruta** y Sol lo preguntó.
   *Propuesta*: en la skill, «todo fichero que nombre el contrato lleva su
   ruta exacta».
3. **El kit de demo no cambia el ancho de la ventana**, así que el estado
   estrecho del aviso no se pudo capturar. *Propuesta*: una variable de la
   app para el tamaño inicial de la ventana y escenas con otro tamaño en
   `demo.sh`.
4. **Las escenas del aviso son estados fijados** (`TITAN_AGENT_DEMO_UPDATE`),
   no el flujo real; el flujo real necesitó un kit nuevo
   (`scripts/demo/actualizacion.sh`), que ya queda versionado.
   *Propuesta*: que la skill pida, para una tarea con estados que dependen
   de un servicio, un kit de punta a punta además de las escenas.
5. **Luna no puede pasar los tests de PTY dentro de su sandbox** y lo
   reporta como fallo aunque la compuerta de titan-team (fuera del sandbox)
   pasa. *Propuesta*: que su prompt diga que la compuerta del orquestador es
   la que cuenta y que, si su `cargo test` falla por el sandbox, lo diga sin
   darlo por fallido.
6. **El evaluador visual encontró fallos reales en la ronda 1** (corte de
   texto y línea partida) y aprobó en la 2: el reparto funcionó como se
   esperaba.

### Añadidas al diseñar la 0.7.0 y la 0.8.0 (2026-11-17)

Para la ronda «Equipo v6», desde
[[Versión 0.7.0 - Proyectos importados y monitor por sesión directora]] y
[[Chat de sesión con Codex y Claude Code]]:

7. **`bin/team` compila titan-team desde la carpeta del director.** En una
   tarea que cambia titan-team (la primera PR de la 0.8.0 lo reescribe para
   el app-server, ADR-0013), el director usaría la herramienta a medio
   hacer. *Propuesta*:
   - que `bin/team` pueda usar un binario fijado (`TITAN_TEAM_BIN`, o
     compilado desde `main` en la carpeta de compilación del equipo);
   - que la skill diga que se use así en las tareas sobre el propio equipo.
8. **El kit de demo no puede escribir en la ventana.** La 0.7.0 necesita
   capturas del campo de texto con texto, listas y el borrador lleno, y la
   0.8.0 el menú `/`. *Propuesta*:
   - un guion de teclas que la app aplica al arrancar en modo demo (una
     variable del kit, como las de las escenas);
   - una carpeta de configuración de demo con `projects.json` para las
     escenas de proyectos.
9. **La interfaz no tiene tests de ventana**, y la 0.7.0 trae lógica de
   interacción nueva: la edición del campo, las reglas del árbol con dos
   grupos y qué encargos tiene una directora. *Propuesta*: que la skill
   exija que esa lógica vaya en funciones o modelos puros con tests de
   aceptación, y que la vista solo pinte. Así Sol puede escribir tests de
   lo que hoy solo comprueba el evaluador visual.

## Resultado
