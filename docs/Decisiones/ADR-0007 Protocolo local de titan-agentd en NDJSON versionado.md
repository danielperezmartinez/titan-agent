---
Nombre: Protocolo local de titan-agentd en NDJSON versionado
Número: 7
Estado: Aceptada
Resumen: 'titan-agentd escucha en un socket Unix por usuario ($XDG_RUNTIME_DIR/titan-agent/agentd.sock, o TITAN_AGENTD_SOCKET) y habla JSON delimitado por líneas con un sobre al estilo JSON-RPC sin el campo jsonrpc: peticiones {id, method, params}, respuestas {id, result | error} y eventos {event, params}. La versión del protocolo (1) se negocia con hello; los bytes de terminal viajan en base64. Un candado de fichero garantiza un solo daemon por socket.'
Decisión: Protocolo propio NDJSON v1 sobre socket Unix, con hello obligatorio, instantáneas completas de sesiones como evento y bytes de terminal en base64; tipos en titan-agent-core::protocol.
Consecuencias: Cualquier cambio incompatible sube PROTOCOL_VERSION y el daemon rechaza clientes de otra versión. Windows (named pipe) queda pendiente. El mismo protocolo servirá por stdio a titan-ssh (fase 4).
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-15T11:36:00+01:00
Última modificación: 2026-11-15T11:36:00+01:00
---

# ADR-0007 · Protocolo local de titan-agentd en NDJSON versionado

## Contexto

[[ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]] separa el daemon de
la interfaz y deja para la fase 1 el formato del protocolo. Lo que circula es
poco y simple: una lista de sesiones que cambia cada pocos segundos, órdenes
sueltas (adjuntar, escribir, redimensionar, detener) y el flujo de bytes de los
terminales. En la fase 4 lo hablará titan-ssh a través de SSH
([[ADR-0003 Rust con GPUI en escritorio y Android dentro de titan-ssh]]), así que
debe poder ir por stdio sin cambios.

## Decisión

- **Transporte**: socket Unix por usuario en
  `$XDG_RUNTIME_DIR/titan-agent/agentd.sock` (carpeta `0700`, socket `0600`);
  sin `XDG_RUNTIME_DIR`, `<tmp>/titan-agent-<usuario>/agentd.sock`. La variable
  `TITAN_AGENTD_SOCKET` (o `--socket`) la sustituye. Un candado
  (`agentd.sock.lock`, `File::try_lock`) impide un segundo daemon en el mismo
  socket. La interfaz arranca el daemon (en su propio grupo de procesos) si no
  responde.
- **Formato**: una línea JSON por mensaje.
  - Petición: `{"id":1,"method":"attach","params":{...}}`. Sin `id` no hay
    respuesta (lo usan `input` y `resize`).
  - Respuesta: `{"id":1,"result":...}` o
    `{"id":1,"error":{"code":"...","message":"..."}}`.
  - Evento: `{"event":"sessions","params":{...}}`.
- **Versión**: `hello {protocol, client}` es obligatorio y va primero; el daemon
  responde `{protocol, daemon}` o el error `protocol_mismatch`. Hoy
  `PROTOCOL_VERSION = 1`. Añadir campos opcionales no cambia la versión; quitar o
  cambiar el significado de algo sí.
- **Métodos v1**: `hello`, `subscribe`, `attach {session, cols, rows}` →
  `{terminal, created}`, `input {terminal, data}`, `resize`, `detach`,
  `close_terminal`, `stop {session}`, `open_folder {session}`.
- **Eventos v1**: `sessions {sessions, sources, terminals}` (instantánea
  completa, solo cuando cambia), `output {terminal, data}` y
  `terminal_closed {terminal, session, exit_code}`.
- **Bytes de terminal** en base64 dentro del JSON. Al adjuntarse a un terminal
  que ya existe, el daemon repite su historial (hasta 2 MiB) y fuerza un
  redibujado cambiando el tamaño del PTY.
- Los tipos viven en `titan_agent_core::protocol` (sin E/S) y los comparten el
  daemon, la interfaz y los tests.

## Alternativas consideradas

- **JSON-RPC 2.0 estricto** — descartado por poco: el campo `jsonrpc` y las
  notificaciones con `method` no aportan nada aquí, y los eventos con nombre
  propio son más legibles. El sobre se parece lo bastante para cambiar si hiciera
  falta.
- **Formato binario (MessagePack, protobuf)** — descartado: más dependencias y
  peor depuración; el volumen no lo justifica.
- **Diferencias de la lista de sesiones** en lugar de instantáneas — descartado
  por ahora: la lista es pequeña (decenas de sesiones) y la instantánea evita
  estados desincronizados.
- **Un socket o canal por terminal** — descartado: complica el ciclo de vida y
  el paso por SSH; un único canal multiplexado basta.

## Consecuencias

- Positivas: se depura con `socat` o unas líneas de Python; el mismo protocolo
  irá por stdio; los tests de integración del daemon lo usan tal cual.
- Negativas / compromisos: base64 añade un 33 % al tráfico de terminal (local,
  irrelevante). En Windows falta el named pipe: el daemon compila pero no
  escucha. El historial de repetición puede empezar a mitad de una secuencia de
  escape; el redibujado forzado lo corrige en los TUI de Claude Code y Codex.
