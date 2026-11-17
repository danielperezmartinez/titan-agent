---
Nombre: Checklist de lanzamiento
Tipo: Checklist
Estado: Pendiente
Resumen: Estado de la configuración fuera del código (ajustes del repo en GitHub, hooks en cada clon, firma y canales de distribución) que ningún test del repo comprueba. Sección GitHub revisada el 2026-11-14 salvo el correo privado de la cuenta; distribución (puntos de ADR-0011, a revisar al publicar la 0.6.0) y accesibilidad pendientes.
Última revisión: 2026-11-15
Última modificación: 2026-11-17T21:05:00+01:00
---

# Checklist de lanzamiento

Marca cada punto al revisarlo y anota el valor real. Al terminar,
`Estado: Revisada` y actualiza `Última revisión`. Se revisa antes de la
primera versión, al cambiar la distribución y, como mínimo, una vez por
trimestre.

## GitHub (repositorio público de titan-agent)

- [x] Visibilidad del repo (público) coherente con las reglas de "Repositorio y
      publicación". Historial revisado con gitleaks (genéricos y personales)
      antes del primer push.
- [x] Secret scanning activado.
- [x] Push protection activado (bloquea en el servidor los pushes con secretos
      conocidos).
- [x] Private vulnerability reporting activado.
- [x] Protección de `main` con los checks `Cambios`, `Verificar` y `Fugas`
      obligatorios y estrictos (`app_id` 15368), `enforce_admins=false` e
      historial lineal; sin force-push ni borrado. Integración solo con squash,
      sin auto-merge, y la rama se borra al integrar. El job `Windows` no es
      obligatorio.
- [x] Hooks activos en cada clon de trabajo (`git config core.hooksPath`
      apunta a `.githooks`) y gitleaks instalado; fichero de patrones
      personales creado fuera del repo. Comprobado en el clon principal.
- [x] Alertas de Dependabot activadas (Cargo y GitHub Actions), con
      `.github/dependabot.yml`; GPUI se excluye porque se sube a mano (ADR-0006).
- [ ] Correo privado de GitHub activado (*Keep my email address private*). Los
      commits usan el correo noreply; el ajuste de la cuenta no se puede
      comprobar por API y lo revisa el usuario. Incidencia del 2026-11-14: el
      squash de la PR #1 (`317eea3`) salió con el correo principal de la
      cuenta; el usuario decidió dejarlo (ya es público en su perfil). Desde
      entonces se integra con `--author-email`. 2026-11-15: GitHub rechazó
      `--author-email` con el noreply en la PR #4 y se integró con el correo
      principal; el usuario permite ese correo en la autoría de los commits
      (excepción en [[README#Repositorio y publicación]]). Conviene activar
      también *Block command line pushes that expose my email*.

## Distribución

Según [[Decisiones/ADR-0011 Distribución en GitHub Releases con instalación por usuario y actualización firmada|ADR-0011]].
Se revisa al publicar la 0.6.0
([[Tareas/Versión 0.6.0 - Instalador y actualización automática]]).

- [x] Clave minisign de actualizaciones generada con contraseña en el PC del
      usuario. Copia de la clave privada y de su contraseña en su gestor de
      contraseñas. Ninguna copia en el repo, en una carpeta de trabajo ni en un
      log. (2026-11-17, *key id* `9E6E48234B02DF9C`; el usuario comprobó la
      contraseña firmando un fichero de prueba y borró la carpeta local.)
- [x] Clave pública en `packaging/minisign.pub`, con el mismo *key id* que la
      lista de claves aceptadas compilada en la app (la app la lee de ese
      fichero; un test falla si no hay clave válida).
- [x] Environment `release` restringido a la rama `main`, con los secretos
      `TITAN_AGENT_UPDATE_KEY` y `TITAN_AGENT_UPDATE_KEY_PASSWORD`. Solo el job
      `publicar` de `release.yml` los usa. (2026-11-17, creado por el usuario.)
- [x] *Immutable releases* activadas en el repo (2026-11-17).
- [x] Ruleset de etiquetas `v*`: sin mover ni borrar (`etiquetas-version`,
      activo: *restrict updates*, *restrict deletions* y *block force pushes*,
      sin restringir la creación ni excepciones; 2026-11-17). Solo afecta a
      etiquetas: los pushes de ramas, incluida la bóveda a `main`, no cambian.
- [x] `release.yml` rechaza versiones que no sean `0.x.y` y exige el CI en
      verde del commit (`scripts/version-publicable.sh`, con su test C9, y
      los checks `Cambios`, `Verificar` y `Fugas` del commit; 0.6.0).
- [ ] La última release verifica: `minisign -V` del manifiesto con
      `packaging/minisign.pub` y `gh attestation verify` del tar.gz.
- [x] La comprobación de versiones no envía datos del usuario (sin
      autenticación, cookies ni versión en el `User-Agent`). Comprobado
      mirando las peticiones en `crates/titan-agentd/tests/update_e2e.rs`
      (0.6.0).
- [ ] Windows: el zip sale sin firmar (aviso de SmartScreen) y marcado como
      experimental; se acepta mientras no haya daemon de Windows.
- [x] Rotación de la clave ensayada en los tests (lista de claves aceptadas:
      el test C1 verifica con la segunda clave de la lista; 0.6.0).

## Accesibilidad

- [ ] Revisión manual con teclado de los flujos principales (árbol de sesiones,
      nueva sesión, monitor) y contraste de los textos según los tokens.

## Revisiones

| Fecha | Quién | Notas |
| --- | --- | --- |
| 2026-11-14 | diseño: titan-agent | Checklist creada; repo aún sin remoto. |
| 2026-11-14 | titan-agent: implementación | Repo creado; sección GitHub revisada salvo el correo privado de la cuenta. Squash de la PR #1 con el correo principal: aceptado por el usuario. |
| 2026-11-15 | titan-agent: fase 1 opus | PR #4: GitHub rechaza `--author-email` con el noreply; squash con el correo principal, permitido desde ahora por el usuario solo como autor de commits. |
| 2026-11-17 | titan-agent: distribución | Sección Distribución rellena con los puntos de ADR-0011; sin revisar hasta la 0.6.0. |
| 2026-11-17 | titan-agent: v0.6.0 | Clave, environment `release`, releases inmutables y ruleset de etiquetas hechos por el usuario y comprobados; quedan las comprobaciones de la release publicada y el aviso de SmartScreen. |
