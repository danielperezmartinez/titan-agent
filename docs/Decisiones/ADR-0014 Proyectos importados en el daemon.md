---
Nombre: Proyectos importados en el daemon
Número: 14
Estado: Aceptada
Resumen: 'La lista de proyectos deja de salir solo de las sesiones detectadas: el usuario importa carpetas como proyectos (como en Claude Code Desktop) y se ven aunque no tengan sesiones. La lista vive en el daemon, en projects.json de la carpeta de configuración de titan-agent, y se gestiona con peticiones nuevas del protocolo (project_add, project_remove) y un campo nuevo del evento sessions, para que la vean todos los clientes (también Android). Las sesiones de carpetas no importadas siguen visibles en un grupo «otras carpetas». La primera vez se importan los proyectos que ya tienen sesiones.'
Decisión: Proyectos importados guardados por el daemon en projects.json v1, expuestos en el protocolo de forma compatible; las sesiones de carpetas no importadas van al grupo «otras carpetas».
Consecuencias: El daemon escribe un fichero propio de configuración (hasta ahora solo lo hacía la interfaz con defaults.json). El árbol ya no depende solo de las sesiones. El daemon busca encargos del equipo también en los proyectos importados sin sesiones. Ampliación compatible de ADR-0007 sin subir PROTOCOL_VERSION.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-17T23:35:00+01:00
Última modificación: 2026-11-18T00:05:00+01:00
---

# ADR-0014 · Proyectos importados en el daemon

## Contexto

Hoy los proyectos del árbol salen solo de las sesiones detectadas: la
carpeta de trabajo de cada sesión, con los worktrees de Claude Code y del
equipo plegados en su repositorio
(`titan_agent_core::project::project_root`). Un proyecto sin sesiones no
aparece, y para lanzar la primera sesión en él hay que buscar su carpeta con
`[d]`.

Al usar la 0.6.0, el usuario pidió proyectos como en Claude Code Desktop: se
importan, se ven aunque no tengan sesiones, se pueden quitar, y un `+` junto
a cada uno abre la conversación en estado borrador con ese proyecto puesto
([[Decisiones visuales/Conversación de sesión y campo de texto común]]).
Llegó desde la sesión de control el 2026-11-17, para la 0.7.0. Ese mismo día
el usuario decidió:

- la lista vive en el daemon;
- las sesiones de carpetas no importadas van a un grupo «otras carpetas».

## Decisión

### Fichero

- **Dónde**: `projects.json` en la carpeta de configuración de titan-agent,
  la misma que `defaults.json` (`$XDG_CONFIG_HOME/titan-agent/`, o
  `~/.config/titan-agent/`; `TITAN_AGENT_CONFIG_DIR` la sustituye en tests
  y capturas). Solo lo lee y escribe el daemon.
- **Formato v1**:

  ```json
  {"version": 1, "projects": [{"root": "/ruta/absoluta", "added_ms": 1763400000000}]}
  ```

- **`root`** es la raíz del proyecto según `project_root` (importar una
  carpeta de un worktree importa su repositorio), absoluta y sin barra
  final. No se repite.
- **Lectura tolerante**: un fichero ilegible o de otra versión no se
  sobrescribe. El daemon lo trata como lista vacía y lo dice en el
  evento.
- **Escritura**: se escribe a un fichero temporal y se renombra, para no
  dejarlo a medias.
- **Primera vez**: si el fichero no existe, el daemon lo crea tras su primer
  descubrimiento con los proyectos que tengan sesiones en ese momento, para
  que el árbol no pase de golpe a «otras carpetas».

### Protocolo

Es una ampliación compatible de
[[ADR-0007 Protocolo local de titan-agentd en NDJSON versionado|ADR-0007]] y
no sube `PROTOCOL_VERSION`.

- **`project_add {path}`** → `{root}`:
  - normaliza con `project_root`;
  - errores: `not_a_folder` (no existe o no es una carpeta) y `config_error`
    (no se pudo escribir);
  - importar uno que ya está no es un error: devuelve su `root`.
- **`project_remove {root}`** → `{}`. Solo lo quita de la lista y nunca
  toca la carpeta. Si no estaba, no es un error.
- **Evento `sessions`**: gana el campo opcional `projects`, una lista de
  `{root, name, added_ms, missing}`, donde `missing` indica que la carpeta
  ya no existe, y un error de configuración si lo hubo. Se envía como el
  resto del snapshot, solo cuando cambia.
- **Lanzar** (`launch`, ADR-0002) en una carpeta que no está importada la
  importa, como al elegir carpeta en Claude Code Desktop. El usuario puede
  quitarla después.

### Árbol

En el núcleo (`titan_agent_core::tree`), sin E/S.

- **Proyectos importados**:
  - cada uno con sus sesiones, aunque no tenga ninguna;
  - ordenados por la última actividad (sesión o evento de equipo más
    reciente); los que no tienen actividad van después, por nombre;
  - una carpeta que ya no existe se marca `[!] carpeta no encontrada` y
    se puede quitar.
- **«otras carpetas»**: al final del árbol, en `mute`, están las sesiones
  cuyo proyecto no está importado.
  - Se agrupan por proyecto como hoy, cada grupo con `[+] importar`.
  - Un grupo desaparece cuando se queda sin sesiones.
  - Una torre de control no esconde sesiones vivas.
- **El filtro `[/]`** se aplica igual a los dos grupos.

### Encargos del equipo

Amplía 4.2 de
[[ADR-0010 Formato de team.toml y de los eventos del equipo|ADR-0010]]: la
raíz de cada proyecto importado es también una carpeta candidata para buscar
`.agents/team.toml`. Así el resumen del proyecto muestra sus encargos aunque
no haya ninguna sesión viva.

## Alternativas consideradas

- **Lista en la interfaz** (junto a `defaults.json`): descartada por el
  usuario. Solo la vería esta ventana, y no Android ni otro cliente.
- **Ocultar las sesiones de carpetas no importadas**, o solo un contador:
  descartada. Una torre de control no esconde sesiones vivas.
- **Mezclarlas con los importados**, como hoy: descartada. No se distinguiría
  lo que el usuario eligió de lo que el daemon detectó.
- **Guardar el orden a mano** (arrastrar): aplazado. El orden por actividad
  basta por ahora.

## Consecuencias

- **Positivas**:
  - proyectos sin sesiones a un clic, y el `+` con el proyecto puesto;
  - la misma lista en todos los clientes;
  - los encargos de un proyecto importado se ven sin sesiones vivas.
- **Negativas / compromisos**:
  - el daemon pasa a escribir un fichero de configuración propio;
  - hay una regla más en el árbol (dos grupos);
  - la importación automática la primera vez y al lanzar en una carpeta
    nueva puede llenar la lista, pero se quita con `[x]`.
