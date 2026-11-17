---
Nombre: Equipo v5.1 - Compilación compartida por equipo
Estado: En curso
Resumen: 'Ajustes previos a la 0.6.0 sobre las carencias del equipo v5. Los encargos, sus comprobaciones y la compuerta de un mismo equipo compilan en una carpeta compartida (project.target_dir, exportada como CARGO_TARGET_DIR y abierta en el sandbox de los roles que escriben), ligada al checkout del director: otros equipos en paralelo tienen la suya. La compuerta de un encargo nuevo pasa de 221 s a 52 s. Además, la línea «se ejecutará» abrevia HOME a ~, el kit de demo captura sin estilos de hover y la skill pasa la skill al revisor en las tareas sobre el propio equipo. Versión 0.5.2.'
Decisiones: 'Acordado con el usuario el 2026-11-17 (sesión de control): carpeta de compilación por equipo, no global; sccache queda como mejora futura. Va en una PR previa a la 0.6.0 con la versión 0.5.2 (patch), acordada con el usuario el 2026-11-17.'
Bloqueada: []
Fecha de creación: 2026-11-17T18:10:00+01:00
Última modificación: 2026-11-17T18:10:00+01:00
---

# Equipo v5.1 - Compilación compartida por equipo

## Objetivo

Corregir, antes de la [[Versión 0.6.0 - Instalador y actualización automática|ronda 0.6.0]],
las carencias baratas que dejó [[Equipo v5]] en su sección «Carencias nuevas
encontradas»:

| Carencia de la v5 | Ajuste |
| --- | --- |
| 5. Cada encargo compila desde cero | `[project] target_dir`: carpeta de compilación del equipo |
| 2. La línea «se ejecutará» muestra la carpeta absoluta | Se abrevia `HOME` a `~` |
| 3. El hover depende de dónde quedó el puntero | `shot` abre la ventana con `TITAN_AGENT_DEMO_NO_HOVER` |
| 4. El revisor no ve la skill | La skill pide añadirla a su especificación en las tareas sobre el equipo |
| 1. Los datos de la demo dentro de un worktree | Ya corregido en el kit en la v5 (datos en `$XDG_RUNTIME_DIR`); nada que hacer |

## Diseño

### Carpeta de compilación por equipo

Acordado con el usuario: **por equipo, no global**. Los encargos y la
compuerta de un mismo equipo comparten carpeta; equipos o sesiones en
paralelo tienen la suya, para no hacer cola en el bloqueo de Cargo ni pisarse
los artefactos.

- Clave opcional `[project] target_dir` (ampliación compatible de
  [[Decisiones/ADR-0010 Formato de team.toml y de los eventos del equipo|ADR-0010]]).
  Es relativa a la carpeta que contiene `.agents/`, que es el checkout del
  director. Por eso queda ligada a su worktree: dos directores en dos
  worktrees tienen dos carpetas. En este repo, `.agents/target` (ignorada por
  git).
- titan-team la exporta como `CARGO_TARGET_DIR` a `test_cmd`, a las
  comprobaciones tras el turno y a `ask --gate`.
- Un rol `workspace-write` (Luna, que compila dentro de su turno) también la
  recibe, y titan-team la añade a `sandbox_workspace_write.writable_roots`.
  Sin eso, el sandbox de Codex no le deja escribir fuera de su worktree
  (comprobado con `codex sandbox` 0.169.0: «sistema de ficheros de sólo
  lectura» sin la clave, escribe con ella). Un rol `read-only` no la recibe.
- El director sigue compilando en su propio `target/`: no espera al bloqueo
  de Cargo de los encargos.
- Lo que se reaprovecha entre worktrees son las dependencias (GPUI y el
  resto), cuyas rutas no cambian. Los crates del workspace se recompilan,
  porque su ruta es otra en cada worktree.
- sccache queda como mejora futura; no se instala.

## Criterios de finalización

- `target_dir` validado, documentado (`titan-team doc`) y con su test de
  punta a punta.
- La compuerta medida antes y después.
- fmt, clippy `-D warnings`, tests y gitleaks en verde; PR integrada por
  squash con la versión 0.5.2, con autorización del usuario.

## Verificación

Rama `chore/equipo-v5.1`, en local el 2026-11-17:

- **Compuerta** (`gate_cmd` de `.agents/team.toml`) en un worktree de encargo
  recién creado, con la máquina cargada por otras sesiones (load ≈ 19 en 8
  núcleos):
  - antes, con su propio `target/` vacío: **221 s**;
  - después, con la carpeta del equipo ya caliente desde otro worktree (lo
    que deja el turno de Sol con sus `checks`): **52 s** (−76 %).
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` y `cargo test --workspace`: **144 tests** en verde (142 en
  `main`): el de punta a punta de `target_dir` en `crates/titan-team/tests/cli.rs`
  y el de la línea con `~` en `titan_agent_core::launch`, más un caso
  nuevo en el test de `run_shell`.
- Kit de demo: `demo.sh shots inicio nueva` captura con `shot` sin hover, y
  la línea «se ejecutará» sale como `cd ~/dev/titan-ssh && …`.

## Resultado
