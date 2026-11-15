---
Nombre: Publicar los resultados del experimento de la fase 1
Estado: En curso
Resumen: Archivar en origin las tres variantes de la fase 1 (equipo v1, solo Opus, equipo v2) y publicar en GitHub Pages una página en español con el objetivo, las variantes, las métricas, capturas equivalentes, la investigación, las conclusiones y la configuración de la fase 2.
Decisiones: 'Ramas de archivo experimento/fase-1-equipo-v1, experimento/fase-1-opus y experimento/fase-1-equipo-v2 (usuario, 2026-11-15) · Página en GitHub Pages desde experimentos/ con un workflow propio, enlazada desde un README.md raíz (usuario) · Versión 0.2.1, patch (usuario) · Capturas con datos inventados: --fixture en la v2 y CLI falsos en la v1 y la opus'
Bloqueada: []
Fecha de creación: 2026-11-15T20:08:00+01:00
Última modificación: 2026-11-15T20:08:00+01:00
---

# Publicar los resultados del experimento de la fase 1

## Objetivo

La fase 1 se hizo tres veces para comparar formas de trabajar (ver
[[Tareas/Fase 1 - Daemon, árbol de sesiones y terminal integrado]]). Hay que
dejar las tres variantes como prueba en el repositorio y explicar el resultado en
una página pública: qué se comparó, con qué cifras, cómo se ve cada ventana, qué
dicen las fuentes, qué se concluye y qué se probará en la fase 2. Se cuenta con
honestidad lo que no se pudo medir igual en las tres.

## Criterios de finalización

- Las tres variantes están en origin como ramas de archivo, revisadas antes con
  gitleaks.
- Capturas de las tres ventanas en estados equivalentes, con datos inventados y
  sin rutas, correos ni sesiones reales.
- La página está integrada en `main` por PR, Pages la publica y el README raíz
  la enlaza.

## Verificación

- **Ramas de archivo** (2026-11-15): gitleaks (patrones del repo y personales)
  sobre los commits y mensajes de `main..` de cada rama, sin hallazgos; todos los
  commits con el correo noreply; revisados a ojo los PNG de la v2. Subidas tal
  cual con aprobación del usuario: `experimento/fase-1-equipo-v1` (0cf96a4),
  `experimento/fase-1-opus` (ced96bb) y `experimento/fase-1-equipo-v2`
  (910a717). El hook pre-push repitió gitleaks sin hallazgos.
- **Capturas**: cada variante compilada desde su commit en una carpeta temporal
  (sin tocar sus worktrees) y abierta en el escritorio con un daemon en un socket
  privado. La v2 con `--fixture`; la v1 y la opus con CLI falsos de `claude`,
  `codex` y `git` y un `codex app-server` simulado (WebSocket para la opus,
  stdio para la v1), todo con los datos de la previa en `/srv/demo/…`. La v1 no
  puede abrir una sesión ya elegida: en su copia se añadió un preselector por
  variable de entorno, solo para la captura. Capturas con
  `spectacle -b -n -a -e`, revisadas una a una.
- **Pendiente**: CI de la PR, integración, activar Pages y comprobar la
  ejecución del workflow `Pages` y la página publicada.

## Resultado

<Se rellena al completar.>
