---
Nombre: ADR de distribución antes de la primera versión
Estado: En curso
Resumen: Antes de publicar la 0.1, decidir en una ADR cómo se distribuye titan-agent en Linux y Windows (formatos, firma, canales, canary, autoactualización, instalación del daemon) mirando cómo lo resolvió titan-ssh, y montar el workflow de releases por tag.
Decisiones: '[[Decisiones/ADR-0011 Distribución en GitHub Releases con instalación por usuario y actualización firmada]] (Propuesta, a la espera de la aprobación del usuario). Decidido con el usuario el 2026-11-17: tar.gz por usuario en Linux, zip experimental en Windows, minisign y atestaciones, publicar con un workflow manual sobre main, aplicar al cerrar solo con las sesiones quietas, builds de debug en otro socket y retirar el lanzador main. El workflow de releases y la checklist pasan a [[Versión 0.6.0 - Instalador y actualización automática]].'
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-17T10:00:00+01:00
---

# ADR de distribución antes de la primera versión

## Objetivo

De momento solo está acordado: GitHub Releases al subir un tag `vX.Y.Z`
([[README#Publicación]]). Falta decidir con el usuario, como referencia lo que
hizo titan-ssh (su ADR-0011 de distribución, ADR-0025 de autoactualización y
ADR-0027 de canary, y su `.github/workflows/release.yml`): formatos (tar.gz,
AUR, MSI, winget…), firma, canal canary, autoactualización y cómo se instala y
arranca el daemon en cada sistema.

## Criterios de finalización

- ADR aceptada y sección «Distribución» de
  [[Seguridad/Checklist de lanzamiento]] rellena.
- El workflow de releases y la revisión de la checklist se movieron el
  2026-11-17 a [[Versión 0.6.0 - Instalador y actualización automática]],
  porque los implementa el equipo junto con el instalador.

## Verificación

## Resultado
