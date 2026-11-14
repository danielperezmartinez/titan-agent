---
Nombre: Fase 0 - Preparar el workspace y el equipo del prototipo
Estado: Pendiente
Resumen: Copiar al repo el prototipo del equipo (bin/team, .agents/team.toml adaptado a Rust, prompts, esquema del revisor y skill equipo) desde la sandbox multiagente, sin modificar la sandbox, y crear el workspace de Cargo con la versión de Rust y de GPUI fijadas y el CI compilando.
Decisiones: ''
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-14T15:28:51+01:00
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

## Resultado
