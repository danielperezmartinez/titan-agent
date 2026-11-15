---
Nombre: Checklist de lanzamiento
Tipo: Checklist
Estado: Pendiente
Resumen: Estado de la configuración fuera del código (ajustes del repo en GitHub, hooks en cada clon, firma y canales de distribución) que ningún test del repo comprueba. Sección GitHub revisada el 2026-11-14 salvo el correo privado de la cuenta; distribución y accesibilidad pendientes.
Última revisión: 2026-11-15
Última modificación: 2026-11-15T20:02:00+01:00
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

<!-- Se completa con la ADR de distribución (formatos, firma, canales,
autoactualización) antes de la primera versión. -->

## Accesibilidad

- [ ] Revisión manual con teclado de los flujos principales (árbol de sesiones,
      nueva sesión, monitor) y contraste de los textos según los tokens.

## Revisiones

| Fecha | Quién | Notas |
| --- | --- | --- |
| 2026-11-14 | diseño: titan-agent | Checklist creada; repo aún sin remoto. |
| 2026-11-14 | titan-agent: implementación | Repo creado; sección GitHub revisada salvo el correo privado de la cuenta. Squash de la PR #1 con el correo principal: aceptado por el usuario. |
| 2026-11-15 | titan-agent: fase 1 opus | PR #4: GitHub rechaza `--author-email` con el noreply; squash con el correo principal, permitido desde ahora por el usuario solo como autor de commits. |
