---
Nombre: Fase 0 - Preparar el workspace y el equipo del prototipo
Estado: En curso
Resumen: Copiar al repo el prototipo del equipo (bin/team, .agents/team.toml adaptado a Rust, prompts, esquema del revisor y skill equipo) desde la sandbox multiagente, sin modificar la sandbox, y crear el workspace de Cargo con la versión de Rust y de GPUI fijadas y el CI compilando.
Decisiones: 'Usuario (2026-11-14): revisor gpt-6.1-sol con esfuerzo high; diseñador de tests gpt-6.1-sol (esfuerzo medium, a criterio de Opus); vocabularios del catálogo técnico aceptados. GPUI fijado al tag estable v1.28.1 de Zed por git (ADR-0006). Rust 1.99.1.'
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-14T17:50:00+01:00
---

# Fase 0 - Preparar el workspace y el equipo del prototipo

## Objetivo

Dejar el repo listo para construir titan-agent con el equipo
([[Decisiones/ADR-0005 Herramienta del equipo en Rust dentro de titan-agent]]).

1. **Equipo del prototipo**: copiar (solo lectura sobre el origen; la sandbox no
   se modifica) desde la sandbox `multiagente` (su ruta viene en el encargo de la sesión): `bin/team`,
   `.agents/team.toml`, `.agents/prompts/`, `.agents/schemas/review.json` y
   `.claude/skills/equipo/`. Leer antes su `README.md` e `INFORME.md`.
   No copiar `.agents/state/`, `.agents/bootstrap/`, `demo/` ni `docs/` del
   prototipo. Añadir `.agents/state/` al `.gitignore` (sus salidas pueden
   contener datos del entorno).
2. **Adaptar `team.toml`** a este repo: `[project]` con rama base `main`,
   worktrees y comando de tests `cargo test --workspace`; implementador
   `gpt-6-luna`, diseñador de tests `gpt-6-luna` en otro hilo con
   `allowed_paths` limitados a los tests, revisor `gpt-6.1-sol`. El esfuerzo de
   Sol y el modelo del diseñador de tests están sin decidir en el prototipo:
   proponerlos al usuario.
3. **Workspace de Cargo** con los crates de [[README#Arquitectura objetivo]]
   (pueden empezar vacíos), `[workspace.package] version = "0.1.0"`,
   `rust-toolchain.toml` y GPUI fijado a una versión o commit concretos.
   Comprobar antes cómo se consume GPUI hoy (crates.io o git del repo de Zed).
4. **CI**: añadir al job `Verificar` de `.github/workflows/ci.yml` las
   librerías del sistema que GPUI necesita en Linux, y valorar un job de Windows.
5. Revisar los tests de los scripts del prototipo copiados y corregir lo que no
   encaje con este repo (el README del prototipo lista 4 fallos ya corregidos y
   el adaptador `cli=claude` sin probar; ese adaptador no se usa, ver ADR-0001).

## Criterios de finalización

- `bin/team roles` muestra el equipo de este repo y un encargo de prueba pequeño
  pasa por tests → implementación → revisión.
- `cargo build`, `cargo fmt --check`, `cargo clippy -D warnings` y
  `cargo test` pasan en local.
- El remoto aún no existe: si hace falta para las PR, pedir al usuario que lo cree
  (ver [[Proteger main y configurar el repo al crearlo en GitHub]]); mientras, se
  trabaja en ramas locales.

## Verificación

En local, rama `chore/fase-0-workspace` (2026-11-14):

- `cargo build --workspace`, `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` y
  `cargo test --workspace` en verde (9 tests del núcleo). Primer build con GPUI:
  unos 3,5 min.
- `python3 -m unittest discover -s bin -p 'test_*.py'`: 11 tests del prototipo
  adaptado, en verde (sin llamar a Codex).
- `bin/team roles` muestra el equipo de este repo.
- Encargo de prueba `tipos-sesion` por el flujo completo: tests (Sol medium,
  62 s), implementación (Luna medium, 56 s, 9/9), revisión (Sol high, 32 s,
  `changes_requested` por la entrada del catálogo, que añadió el director sin
  otra ronda). 3 llamadas a Codex, 2 de cuota de Sol.
- Hooks de gitleaks en cada commit, sin fugas.
- **Pendiente**: CI en GitHub (`Cambios`, `Verificar`, `Fugas` y `Windows`) y la
  PR integrada; el remoto no existe todavía.

## Resultado

- Workspace de Cargo con `titan-agent-core`, `titan-agentd`, `titan-agent`
  (GPUI) y `titan-team` (vacío hasta la fase 3); Rust 1.99.1; GPUI por git al
  tag v1.28.1 de Zed ([[Decisiones/ADR-0006 GPUI desde el repositorio de Zed fijado a un tag estable]]).
  En Linux enlaza con xcb, xkbcommon, fontconfig y freetype; el CI los instala.
- Prototipo del equipo copiado sin tocar la sandbox y adaptado: `team.toml` de
  este repo, prompts para Rust, rutas con comodines (`crates/*/tests/`),
  `project.test_timeout_s`, `team new --base <rama>` y sin el adaptador
  `claude -p` (ADR-0001). La skill `equipo` explica `--base` y la publicación
  por PR.
- Hallazgo del encargo de prueba: los roles de Codex leen `AGENTS.md` y aplican
  el protocolo de la bóveda; Luna marcó la fase 1 como `En curso`. Se revirtió y
  los prompts y `forbidden_paths` dejan `docs/` al director.
- Cuota: cada llamada a Codex entra con 130–195k tokens (la mayoría en caché)
  porque lee el protocolo de la bóveda; vigilar en encargos grandes.
- Primera pieza del catálogo: [[Catálogo técnico/Tipos de sesión]].
- CI: librerías de GPUI, caché de Cargo, tests de `bin/team` y job `Windows`
  (no obligatorio).

