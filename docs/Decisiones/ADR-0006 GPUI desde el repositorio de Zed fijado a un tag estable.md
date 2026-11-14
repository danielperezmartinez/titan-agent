---
Nombre: GPUI desde el repositorio de Zed fijado a un tag estable
Número: 6
Estado: Aceptada
Resumen: 'GPUI se consume como dependencia git del repositorio oficial de Zed, fijada con `rev` al commit de un tag estable de Zed (hoy v1.28.1), y no desde crates.io: el crate `gpui` publicado (0.2.2) lleva desde octubre de 2025 sin actualizarse y las copias publicadas por terceros (`gpui-pre`, `gpui-ce`) añaden un intermediario en la cadena de suministro. Rust se fija en `rust-toolchain.toml` a una versión ya instalada igual o posterior a la del tag.'
Decisión: gpui y gpui_platform como dependencias git de zed-industries/zed con rev = commit de un tag estable, actualizadas a mano y en una PR propia.
Consecuencias: El primer build descarga el repositorio de Zed (unos 500 MB en ~/.cargo/git) y tarda unos minutos. Al subir de tag hay que revisar los cambios de API de GPUI y el `rust-toolchain.toml` de Zed.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-14T17:30:00+01:00
Última modificación: 2026-11-14T17:30:00+01:00
---

# ADR-0006 · GPUI desde el repositorio de Zed fijado a un tag estable

## Contexto

[[ADR-0003 Rust con GPUI en escritorio y Android dentro de titan-ssh]] pide fijar
GPUI a una versión o commit concretos porque su API cambia entre versiones. En la
fase 0 ([[Fase 0 - Preparar el workspace y el equipo del prototipo]]) se comprobó
cómo se consume GPUI hoy (2026-11-14):

- **crates.io, `gpui` oficial**: la última versión es la 0.2.2, del 2025-10-22.
  Va más de un año por detrás de Zed, que desde entonces separó la plataforma en
  `gpui_platform`, `gpui_linux`, `gpui_windows`, etc. El terminal de Zed, que es
  el precedente para el nuestro, usa la API actual.
- **crates.io, copias de terceros**: `gpui-pre` (instantáneas de Zed publicadas
  por el autor de gpui-component) y `gpui-ce` (fork de la comunidad). Están al
  día, pero el código pasa por un intermediario antes de llegar al repo.
- **git del repositorio oficial de Zed** fijado a un commit: lo que hacen Zed y
  la mayoría de proyectos sobre GPUI.

## Decisión

- `gpui` y `gpui_platform` se declaran en `[workspace.dependencies]` del
  `Cargo.toml` raíz como dependencias git de
  `https://github.com/zed-industries/zed` con `rev` igual al commit de un **tag
  estable** de Zed (sin `-pre`). Hoy: `v1.28.1` →
  `e8bc16acee5ae1937600d571d441c4b5d7b0d288`. `Cargo.lock` se versiona.
- La interfaz activa en `gpui_platform` las mismas funciones que Zed en Linux:
  `font-kit`, `wayland` y `x11` (sin ellas no hay plataforma Linux).
- `rust-toolchain.toml` fija una versión de Rust **ya instalada** igual o
  posterior a la del `rust-toolchain.toml` de Zed en ese tag (Zed v1.28.1 usa
  1.98.1; aquí 1.99.1). Cambiar de versión de Rust que obligue a instalar otra
  toolchain requiere la aprobación del usuario.
- Subir de tag es un cambio propio (una PR), con la API de GPUI revisada y el
  build, clippy y tests en verde.

## Alternativas consideradas

- **`gpui` 0.2.2 de crates.io** — descartada: anticuada respecto a Zed y a su
  terminal.
- **`gpui-pre` o `gpui-ce`** — descartadas: añaden un publicador intermedio a la
  cadena de suministro sin aportar nada que no dé el repo oficial.
- **Seguir la rama `main` de Zed** — descartada: la API cambia a diario; un tag
  estable ya se ha usado en una versión publicada de Zed.

## Consecuencias

- Positivas: código oficial y actual, el mismo que usa el terminal de Zed;
  builds reproducibles con `rev` y `Cargo.lock`.
- Negativas / compromisos: el primer build descarga el repositorio de Zed (unos
  500 MB en `~/.cargo/git`) y compila unos 350 crates (unos 3,5 min en local).
  El CI necesita caché de Cargo. Cada subida de tag puede romper la interfaz.
