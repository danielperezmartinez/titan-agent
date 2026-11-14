---
Nombre: Rust con GPUI en escritorio y Android dentro de titan-ssh
Número: 3
Estado: Aceptada
Resumen: 'titan-agent se escribe en Rust para minimizar memoria y CPU. En escritorio (Linux y Windows) la interfaz usa GPUI, el framework de Zed que también ha elegido Discord, porque Zed ya integra un terminal completo sobre él. Android no tiene app propia: será un plugin de titan-ssh que controla el equipo por SSH, y llega después del escritorio.'
Decisión: Rust + GPUI para la app de escritorio (Linux y Windows); el acceso desde Android se hará como plugin de titan-ssh, después del escritorio.
Consecuencias: GPUI no tiene soporte oficial para móvil ni un campo de texto incluido, y su API cambia entre versiones. Hará falta un protocolo entre el equipo y titan-ssh para el control remoto, que se decide aparte.
Reemplaza: []
Reemplazada por: []
Fecha de creación: 2026-11-14T12:58:56+01:00
Última modificación: 2026-11-14T12:58:56+01:00
---

# ADR-0003 · Rust con GPUI en escritorio y Android dentro de titan-ssh

## Contexto

El usuario quiere minimizar el consumo de recursos y las mismas plataformas que
titan-ssh: Linux, Windows y Android. Referencia: Discord anunció el 2026-11-04
que reescribe sus clientes en Rust; en escritorio todo en Rust con **GPUI** (una
build temprana marcaba unos 290 MB de RAM con dos cuentas, frente a 500 MB–1 GB+
del cliente Electron) y en móvil interfaces nativas (Kotlin, Swift) sobre un
núcleo Rust compartido. Fuentes: heise
(<https://www.heise.de/en/news/Discord-is-rebuilding-in-Rust-leaving-React-and-Electron-behind-11516113.html>),
gHacks
(<https://www.ghacks.net/2026/11/06/discord-is-rebuilding-its-desktop-app-in-rust-with-the-new-client-using-290-mb-of-ram/>).

GPUI no tiene soporte oficial para Android (solo un crate de la comunidad aún
sin funcionar). En Android no se pueden ejecutar los agentes: allí la app sería
siempre un mando a distancia del equipo. titan-ssh ya tiene en Android SSH,
terminal, el lenguaje visual `DESIGN.md` y un sistema de plugins incluidos que se
activan (ADR-0032 de titan-ssh).

## Decisión

- **Lenguaje: Rust.**
- **Escritorio (Linux y Windows): GPUI.** El terminal integrado se apoya en el
  precedente de Zed (`alacritty_terminal` sobre GPUI).
- **Android: plugin de titan-ssh**, no una app propia de titan-agent. Llega
  **después del escritorio**.

## Alternativas consideradas

- **Modelo Discord completo** (GPUI en escritorio y Kotlin/Compose sobre núcleo
  Rust con UniFFI en Android) — descartado: dos interfaces y una segunda app
  Android que repetiría lo que titan-ssh ya tiene.
- **Slint en todas las plataformas** — descartado: el terminal habría que hacerlo
  desde cero y su licencia exige revisión.
- **Iced** — sin soporte móvil como objetivo y sin el precedente del terminal.
- **Tauri / Electron** — webview: contradice el objetivo de consumo mínimo.

## Consecuencias

- Positivas: consumo bajo; terminal con precedente probado; la funcionalidad
  compartida con titan-ssh nace del propio diseño.
- Negativas / compromisos: GPUI cambia entre versiones (fijar versión) y no trae
  campo de texto propio (el formulario de [[ADR-0002 Lanzar sesiones desde la app con salvaguardas]]
  necesitará uno). Falta decidir la arquitectura interna (núcleo o daemon frente
  a interfaz) y el protocolo de control remoto con titan-ssh.
