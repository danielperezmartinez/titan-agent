---
Nombre: Proteger main y configurar el repo al crearlo en GitHub
Estado: Hecha
Resumen: Cuando el usuario cree el repositorio público en GitHub, configurar la protección de main con los checks Cambios, Verificar y Fugas, revisar los ajustes de la checklist de lanzamiento y comprobar las trampas conocidas de Publicación. Cada cambio externo con confirmación del usuario.
Decisiones: ''
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-14T18:10:00+01:00
---

# Proteger main y configurar el repo al crearlo en GitHub

## Objetivo

El remoto no existe. Crearlo y subir nada requiere aprobación explícita del
usuario. Cuando exista:

1. Primer push de `main` (lo permite la guarda: la rama aún no existe en el
   remoto) y ver los nombres reales de los checks en el primer CI.
2. Protección de `main` según [[README#Publicación]]: checks `Cambios`,
   `Verificar` y `Fugas` estrictos (`app_id` 15368), `enforce_admins=false`,
   historial lineal, sin force-push ni borrado. Con confirmación.
3. Ajustes de [[Seguridad/Checklist de lanzamiento]] (secret scanning, push
   protection, private vulnerability reporting, Dependabot, correo privado).
4. Trampas: `remote.origin.push`, referencias remotas al día, auto-merge.

## Criterios de finalización

- La protección está aplicada y la checklist actualizada con el estado real.

## Verificación

2026-11-14, con aprobación del usuario en la sesión de implementación:

- Repo público creado con `gh`; primer push de `main` (b4e577a) tras revisar
  todo el historial con gitleaks (patrones genéricos y personales) y comprobar
  que los commits usan el correo noreply. El primer CI dio los checks
  `Cambios`, `Verificar` y `Fugas` de la app 15368, en verde.
- Protección leída de vuelta por la API: checks estrictos, `enforce_admins`
  falso, historial lineal, sin force-push ni borrado.
- Ajustes leídos de vuelta: solo squash, sin auto-merge, borrado de rama al
  integrar, secret scanning y push protection activos, private vulnerability
  reporting activo, alertas de Dependabot activas.
- Trampas: `remote.origin.push` sin definir; la rama de la fase 0 sigue a
  `origin/chore/fase-0-workspace`.

## Resultado

Hecho salvo el ajuste de correo privado de la cuenta, que no se puede comprobar
por API (ver [[Seguridad/Checklist de lanzamiento]]).
