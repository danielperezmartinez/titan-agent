---
Nombre: Acentos y teclas muertas en el terminal integrado
Estado: Pendiente
Resumen: 'En el terminal integrado de titan-agent, una tecla muerta (el acento agudo de un teclado español) no se compone con la letra siguiente: se escribe una comilla simple y luego la letra (á sale como ''a). Hay que componer las teclas muertas (y, si GPUI lo da, el método de entrada) antes de mandar los bytes al PTY.'
Decisiones: 'Visto por el usuario al escribir desde titan-agent en la prueba manual del cierre de la fase 3 (2026-11-16). Para una fase futura; no se arregla en el cierre.'
Bloqueada: []
Fecha de creación: 2026-11-16T21:50:00+01:00
Última modificación: 2026-11-16T21:50:00+01:00
---

# Acentos y teclas muertas en el terminal integrado

## Objetivo

Al escribir en una sesión desde el terminal integrado, con un teclado español
en Linux (KDE Wayland), los acentos salen mal: la tecla muerta del acento
agudo llega como una comilla simple y la vocal va aparte (el usuario escribió
«sí» y salió «s'í»). Escribir en español desde la app es el uso normal, así
que hay que arreglarlo.

El terminal traduce cada `KeyDownEvent` de GPUI a bytes
(`crates/titan-agent/src/keys.rs`, `keystroke_bytes`, y `terminal.rs`,
`key_down`), y
[[Decisiones/ADR-0008 Terminal integrado con portable-pty en el daemon y alacritty_terminal en la interfaz]]
dejó fuera el método de entrada («sin IME más allá de lo que llega como
carácter»). Hay que ver qué da GPUI en ese caso (`key_char`, el texto
compuesto o un manejador de entrada de texto como el del editor de Zed) y
usar el carácter ya compuesto en lugar de la tecla.

## Criterios de finalización

- Con un teclado español, `´` + vocal escribe la vocal acentuada en el
  terminal integrado; `´` + espacio escribe el acento solo; la `ñ` y la `ü`
  funcionan.
- Las teclas de control y los atajos del terminal siguen igual.
- Tests de la traducción de teclas para los casos compuestos; comprobado a
  mano en Linux (y en Windows si la fase lo cubre).

## Verificación

<Se rellena al completar: pruebas, build, comprobación real.>

## Resultado

<Se rellena al completar: qué se hizo finalmente.>
