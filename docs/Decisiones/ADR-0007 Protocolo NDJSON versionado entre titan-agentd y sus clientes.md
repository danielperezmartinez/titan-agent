---
Nombre: Protocolo NDJSON versionado entre titan-agentd y sus clientes
Número: 7
Estado: Propuesta
Resumen: 'La interfaz habla con titan-agentd por un socket Unix local con un protocolo propio de líneas JSON (NDJSON): peticiones con id del cliente, respuestas o errores con ese id y eventos sin id (lista de sesiones, instantánea y salida de terminales). Versión 1, comprobada en un Hello obligatorio. Los bytes del terminal viajan en base64. Los tipos viven en titan-agent-core.'
Decisión: NDJSON sobre socket Unix ($TITAN_AGENT_SOCKET o $XDG_RUNTIME_DIR/titan-agent/agentd.sock), versión 1 con Hello obligatorio, tipos en titan_agent_core::protocol.
Consecuencias: Un cambio incompatible sube PROTOCOL_VERSION y el daemon rechaza versiones distintas (protocol_mismatch). Windows (named pipe) y el modo --stdio para titan-ssh reutilizan el mismo framing.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-15T11:20:00+01:00
Última modificación: 2026-11-15T11:20:00+01:00
---

# ADR-0007 · Protocolo NDJSON versionado entre titan-agentd y sus clientes

## Contexto

[[ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]] separa el daemon
de la interfaz y deja para la fase 1 el formato del protocolo
([[Fase 1 - Daemon, árbol de sesiones y terminal integrado]]). El mismo
protocolo servirá en la fase 4 a titan-ssh por SSH (stdio), así que debe ser
texto, fácil de depurar y no depender de Unix.

## Decisión

- **Transporte**: socket Unix por usuario. Ruta: `$TITAN_AGENT_SOCKET` si está
  definida; si no, `$XDG_RUNTIME_DIR/titan-agent/agentd.sock`; si no,
  `/tmp/titan-agent-<uid>/agentd.sock`. Carpeta `0700`. Un fichero
  `agentd.sock.lock` con bloqueo exclusivo garantiza un único daemon.
- **Framing**: una línea JSON compacta por mensaje (NDJSON, UTF-8), máximo
  8 MiB por línea. Campos desconocidos se ignoran.
- **Cliente → daemon**: `{"id": n, "request": {"kind": …}}`. Peticiones:
  `hello`, `subscribe`, `open_terminal`, `input`, `resize`, `detach`,
  `close_terminal`, `stop_session`, `shutdown`.
- **Daemon → cliente**: `{"type":"response","id":n,"response":{…}}`,
  `{"type":"error","id":n,"error":{"code":…,"message":…}}` o
  `{"type":"event","event":{"kind":…}}` con `sessions`, `terminal_snapshot`,
  `terminal_output` y `terminal_exited`.
- **Versión**: `PROTOCOL_VERSION = 1`. La primera petición es `hello` con la
  versión del cliente; si no coincide, error `protocol_mismatch` y cierre.
- **Bytes**: la entrada y la salida de los terminales van en base64 estándar.
- **Orden por terminal**: al abrir un terminal el cliente recibe, en este orden,
  la respuesta `terminal_opened`, una `terminal_snapshot` (pantalla visible en
  ANSI) y después solo `terminal_output`; nunca se pierde ni se repite un byte
  entre la instantánea y la salida.
- Los tipos están en `titan_agent_core::protocol` y el modelo de sesión en
  `titan_agent_core::session`, compartidos por daemon e interfaz.

## Alternativas consideradas

- **JSON-RPC 2.0** (como `codex app-server`) — más ceremonia (`jsonrpc`,
  `params`, errores numéricos) sin ganancia: solo hay dos extremos propios.
- **Binario (bincode, protobuf)** — más eficiente para la salida del terminal,
  pero ilegible y con más dependencias; el volumen de un terminal de texto no lo
  justifica.
- **WebSocket** — no aporta nada en un socket local y complica el modo stdio.

## Consecuencias

- Positivas: depurable con `socat`; el mismo framing sirve por stdio para la
  fase 4; tipos únicos en el núcleo.
- Negativas / compromisos: base64 infla la salida un 33 %; cada cambio
  incompatible exige subir la versión y actualizar a la vez daemon e interfaz.
