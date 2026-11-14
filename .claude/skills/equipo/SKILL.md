---
name: equipo
description: Dirige el equipo multiagente definido en .agents/team.toml (Opus dirige; Codex Luna escribe los tests y, en otro hilo, implementa; Codex Sol revisa). Úsala cuando el usuario pida "encargar al equipo", "que lo haga Luna", "pásalo por el equipo" o un desarrollo con el bucle diseño de tests → implementación → revisión → correcciones.
---

# Equipo multiagente: bucle del director

Tú eres el **director** (rol `director` de `.agents/team.toml`). Tu valor está en el diseño y las decisiones; el código y lo mecánico lo escriben los otros roles. **No escribas tú el código del encargo**, aunque sea corto: redacta el encargo y deja que lo hagan ellos. Tu cuota es la cara; la de Codex (Plus) también es limitada: **cada `team ask` es una llamada**, no repitas llamadas para nada.

Herramienta: `bin/team` (en la carpeta que contiene `.agents/`). Lee `team roles` al empezar para saber quién hace cada paso: el TOML manda, no esta skill.

## 0. Antes de empezar
- `bin/team roles` → roles, modelos, `flow.steps`, `max_review_rounds`.
- Si el encargo no está claro, **debate con el usuario** antes de gastar llamadas. Pregunta solo lo que cambie el diseño.

## 1. Diseño y encargo (tú)
Escribe el encargo en un fichero fuera del repo (p. ej. `$CLAUDE_JOB_DIR/tmp/<id>.md`); `team new` guarda su copia en `.agents/state/<id>/` (no versionado). Debe bastar por sí solo, porque los otros roles no ven esta conversación:
- Objetivo en una frase.
- **Interfaz exacta**: crate y módulo, nombres, firmas, tipos, errores (tipo y cuándo), y en qué `crates/<crate>/tests/` van los tests de aceptación.
- Comportamiento: casos normales, bordes, errores, ejemplos entrada → salida.
- Fuera de alcance, y las dependencias nuevas que se autorizan (los roles trabajan sin red: hay que descargarlas antes con `cargo fetch` en el worktree).
- Cómo se ejecutan los tests (`project.test_cmd`).

Enséñale al usuario el diseño en pocas líneas y **pide su visto bueno** si hay decisiones abiertas. Luego:

```
bin/team new <id> --brief <fichero> [--base <rama>]
```

`--base` hace salir el encargo de otra rama que `project.base` (p. ej. la rama de la fase en curso, que aún no está en `main`). Para no recompilar GPUI desde cero en el worktree nuevo, puedes copiar antes el `target/` de la rama base con `cp -a --reflink=auto`.

## 2. Pasos del flujo (en el orden de `flow.steps`)
Por defecto: `disenador_tests` → `implementador` → `revisor`. Lanza cada `team ask` con Bash en **segundo plano** (`run_in_background`, timeout ≥ el `timeout_s` del rol) y espera la notificación; no hagas polling.

```
bin/team ask disenador_tests <id>
bin/team ask implementador <id> -m "Los tests ya están en tests/. Implementa hasta que pasen."
bin/team ask revisor <id> --diff --test
```

Tras cada paso lee el informe que imprime `team` (estado, commit, **avisos**, respuesta):
- Aviso de rutas (p. ej. el implementador tocó `crates/*/tests/`): decide tú si se acepta o se revierte (`git -C <worktree> revert`/`checkout`) y díselo al rol en el siguiente mensaje.
- Si un rol hace preguntas de diseño, respóndelas tú si son tuyas; si son del usuario, pregúntale.
- Si el diseñador de tests se ha desviado del encargo, corrígelo con `bin/team ask disenador_tests <id> -m "…"` (continúa su hilo) antes de implementar.

## 3. Revisión → correcciones (tope de rondas)
El revisor devuelve `veredicto` + hallazgos. Tú **decides** cada hallazgo; el revisor informa, no manda:
- `ok` → paso 4.
- `changes_requested` → elige qué se corrige (descarta nits o lo que contradiga el diseño) y manda **un único** mensaje al `fix_role` con la lista concreta:
  `bin/team ask implementador <id> -m "Corrige: 1) … 2) … No toques crates/*/tests/."`
  Después, nueva revisión **en el mismo hilo del revisor**: `bin/team ask revisor <id> --diff --test -m "Ronda N: he pedido corregir X e Y. Comprueba solo eso y lo que haya podido romper."`
- `blocked` o `questions_for_director` con decisiones de producto → **para y pregunta al usuario**.
- Si un hallazgo afecta a los tests, el que corrige es `disenador_tests`, no el implementador.

`bin/team` cuenta las rondas del revisor y se niega a pasar de `max_review_rounds` (exit 3). Al llegar al tope **no uses `--force` por tu cuenta**: resume al usuario lo pendiente y que decida (otra ronda, aceptarlo así o rediseñar).

## 4. Cierre (con el usuario)
- `bin/team test <id>` y `bin/team diff <id> --stat` para comprobarlo tú (no confíes solo en los informes).
- `bin/team show <id>` → llamadas hechas y tokens.
- Resume al usuario: qué se hizo, rondas usadas, hallazgos aceptados/descartados y por qué, y dónde está (rama `encargo/<id>`, worktree).
- La rama `encargo/<id>` se publica según `docs/README.md` → Publicación (rama corta y PR contra `main`). **Hacer push, abrir o integrar la PR y borrar el worktree lo decide el usuario.** Al terminar: `bin/team close <id> [--delete-branch]`.

## Reglas
- Una llamada por paso; no relances un paso que ha ido bien "por si acaso".
- Si un `team ask` falla (exit 1), lee el `.stderr` del turno en `.agents/state/<id>/` antes de reintentar; un reintento como mucho y luego al usuario.
- No edites el código del worktree tú mismo salvo un arreglo trivial que el usuario apruebe; lo normal es pedírselo al rol.
- Variantes: si el usuario quiere otro reparto (p. ej. sin diseñador de tests, otro modelo de revisor), se cambia en `.agents/team.toml`, no aquí.
