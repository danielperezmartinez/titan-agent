---
Nombre: Versión 0.6.0 - Instalador y actualización automática
Estado: Pendiente
Resumen: 'Implementar ADR-0011. Hay que montar el workflow «Publicar versión» (tar.gz de Linux, zip experimental de Windows, manifiesto firmado con minisign y atestaciones) y titan-agent-launcher (instalar, desinstalar, lanzar y volver atrás). El actualizador va en el daemon (comprobar cada 6 h, descargar, verificar, preparar y aplicar cuando sea seguro, con exec), y el aviso [^] en la cabecera con su bloque de Configuración. Las builds de debug pasan al socket agentd-dev.sock. Lo hace el equipo v4 y termina publicando la v0.6.0 e instalándola en el PC del usuario.'
Decisiones: '[[Decisiones/ADR-0011 Distribución en GitHub Releases con instalación por usuario y actualización firmada]] y [[Decisiones visuales/Aviso de actualización en la cabecera]]. Versión 0.6.0 (minor), acordada con el usuario el 2026-11-17. Clave generada con minisign del paquete de Arch, que instala el usuario. Al instalar se retira la entrada «titan-agent (main)» del menú.'
Bloqueada:
  - '[[Acentos y teclas muertas en el terminal integrado]]'
Fecha de creación: 2026-11-17T10:00:00+01:00
Última modificación: 2026-11-17T10:00:00+01:00
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

## Equipo (variante v4, skill `equipo`)

- **Opus dirige e implementa.** Fija el contrato antes de los tests, a partir
  de los §2 a §7 de ADR-0011 y de la decisión visual, sin inventar detalles.
  Después implementa cada parte con sus tests.
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

  Las firmas de los tests usan una clave de prueba generada para ellos, nunca
  la real.
- **Revisión de Sol** con contexto limpio al final. Este es el código más
  delicado de la app: descarga y ejecuta binarios. Revisa también
  `release.yml` (permisos mínimos, secretos solo en `publicar`, sin
  interpolar entradas en scripts).
- **Evaluador visual** (Opus nuevo) del aviso contra la previa.
- **Luna** solo para trabajo mecánico.

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

## Verificación

## Resultado
