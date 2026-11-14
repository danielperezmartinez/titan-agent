---
Nombre: Daemon titan-agentd e interfaz GPUI como cliente
Número: 4
Estado: Aceptada
Resumen: titan-agent se divide en un daemon sin interfaz (titan-agentd) que descubre y vigila las sesiones de Claude Code y Codex, recoge los eventos del equipo, mantiene vivos los terminales adjuntos y es el único que lanza procesos, y una interfaz GPUI que se conecta a él por un socket local. Cerrar la ventana no corta nada, y el mismo protocolo servirá a titan-ssh por SSH.
Decisión: Separar desde la fase 1 un daemon titan-agentd (estado, terminales, lanzamientos) y una interfaz GPUI cliente que habla con él por un socket local.
Consecuencias: Hace falta definir y versionar un protocolo entre daemon e interfaz (fase 1). Las salvaguardas de ADR-0002 se implementan solo en el daemon. El control remoto desde Android reutiliza el protocolo sobre SSH.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-14T15:27:18+01:00
Última modificación: 2026-11-14T15:27:18+01:00
---

# ADR-0004 · Daemon titan-agentd e interfaz GPUI como cliente

## Contexto

La app debe monitorizar sesiones en tiempo real y alojar terminales con los CLI
oficiales ([[ADR-0001 Torre de control y no orquestador]]), lanzar sesiones con
salvaguardas ([[ADR-0002 Lanzar sesiones desde la app con salvaguardas]]) y, más
adelante, dejarse controlar desde Android a través de titan-ssh
([[ADR-0003 Rust con GPUI en escritorio y Android dentro de titan-ssh]]).
titan-ssh ya resolvió algo parecido con un agente propio en el destino.

## Decisión

- **`titan-agentd`** (Rust, sin interfaz, uno por usuario y equipo):
  - descubre y vigila las sesiones: Claude Code con `claude agents --json` y,
    para la acción en curso, el transcript local de cada sesión; Codex con su
    daemon `codex app-server` (`codex agents`, eventos de hilos y turnos);
  - lee los eventos de la herramienta del equipo
    ([[ADR-0005 Herramienta del equipo en Rust dentro de titan-agent]]);
  - mantiene los **PTY** de los terminales adjuntos (`claude attach <id>`,
    `codex resume <id>`), que sobreviven al cierre de la ventana;
  - es el **único** componente que lanza procesos, y aplica las salvaguardas de
    ADR-0002;
  - expone un **socket local** (Unix en Linux, named pipe en Windows) con
    peticiones y un flujo de eventos.
- **`titan-agent`** (interfaz GPUI): cliente del daemon. No ejecuta CLI ni lee
  ficheros de las CLI por su cuenta. Si el daemon no está en marcha, lo arranca.
- **Remoto (fase 4)**: titan-ssh abre una sesión SSH al equipo y habla el mismo
  protocolo por stdio (un modo `titan-agentd --stdio` o un proxy, como hace
  `codex app-server proxy`).
- El formato del protocolo (p. ej. JSON-RPC o NDJSON) se fija y se versiona en
  la fase 1, en una ADR propia.

## Alternativas consideradas

- **Un solo proceso** — más simple al principio, pero cerrar la ventana
  cortaría los terminales y habría que separar el daemon al llegar el control
  remoto.

## Consecuencias

- Positivas: la interfaz se puede cerrar sin perder nada; un único sitio lanza
  procesos y aplica las salvaguardas; base directa para el control remoto.
- Negativas / compromisos: dos binarios, un protocolo que mantener y versionar,
  y gestión del ciclo de vida del daemon (arranque, actualización, un solo
  daemon por usuario).
