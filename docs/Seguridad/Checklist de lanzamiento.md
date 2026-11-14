---
Nombre: Checklist de lanzamiento
Tipo: Checklist
Estado: Pendiente
Resumen: Estado de la configuración fuera del código (ajustes del repo en GitHub, hooks en cada clon, firma y canales de distribución) que ningún test del repo comprueba. Pendiente hasta que exista el repo en GitHub.
Última revisión: ''
Última modificación: 2026-11-14T15:27:39+01:00
---

# Checklist de lanzamiento

Marca cada punto al revisarlo y anota el valor real. Al terminar,
`Estado: Revisada` y actualiza `Última revisión`. Se revisa antes de la
primera versión, al cambiar la distribución y, como mínimo, una vez por
trimestre.

## GitHub (repositorio público de titan-agent, aún sin crear)

- [ ] Visibilidad del repo (público) coherente con las reglas de "Repositorio y
      publicación".
- [ ] Secret scanning activado.
- [ ] Push protection activado (bloquea en el servidor los pushes con secretos
      conocidos).
- [ ] Private vulnerability reporting activado.
- [ ] Protección de `main` con los checks `Cambios`, `Verificar` y `Fugas`
      obligatorios y estrictos, `enforce_admins=false` e historial lineal.
- [ ] Hooks activos en cada clon de trabajo (`git config core.hooksPath`
      apunta a `.githooks`) y gitleaks instalado; fichero de patrones
      personales creado fuera del repo.
- [ ] Alertas de Dependabot activadas (Cargo y GitHub Actions).
- [ ] Correo privado de GitHub activado (*Keep my email address private*).

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
