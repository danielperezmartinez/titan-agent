---
Nombre: Implementar titan-agent v1 por fases
Estado: Pendiente
Resumen: Tarea paraguas de la construcción de titan-agent en cinco fases, cada una usable por sí sola - 0 workspace y equipo del prototipo, 1 daemon + árbol de sesiones + terminal, 2 nueva sesión, 3 herramienta del equipo en Rust + monitor, 4 Android en titan-ssh. Se construye con el equipo del prototipo (Opus dirige, Luna implementa y diseña los tests, Sol revisa).
Decisiones: ''
Bloqueada: []
Fecha de creación: 2026-11-14T15:28:51+01:00
Última modificación: 2026-11-14T15:28:51+01:00
---

# Implementar titan-agent v1 por fases

## Objetivo

Construir titan-agent según lo acordado en la sesión de diseño. Todo lo
decidido está en esta bóveda; no hay acuerdos fuera de ella:

- Qué es y para qué: [[README#Propósito del proyecto]].
- Decisiones: [[Decisiones/ADR-0001 Torre de control y no orquestador]],
  [[Decisiones/ADR-0002 Lanzar sesiones desde la app con salvaguardas]],
  [[Decisiones/ADR-0003 Rust con GPUI en escritorio y Android dentro de titan-ssh]],
  [[Decisiones/ADR-0004 Daemon titan-agentd e interfaz GPUI como cliente]],
  [[Decisiones/ADR-0005 Herramienta del equipo en Rust dentro de titan-agent]].
- Aspecto: [[Decisiones visuales/Lenguaje visual heredado de titan-ssh]],
  [[Decisiones visuales/Ventana principal al estilo Claude Code Desktop]],
  [[Decisiones visuales/Monitor del proyecto en tres vistas]]. Fuentes de la
  previa interactiva acordada en `design/preview/` (ficheros `.dc.html`:
  HTML con una plantilla y una clase JS; los datos y la lógica de cada pantalla
  están en el bloque `<script>` del final).
- Arquitectura objetivo y reglas de dependencia: [[README#Arquitectura objetivo]].

## Fases

Cada fase es usable por sí sola y tiene su tarea:

0. [[Fase 0 - Preparar el workspace y el equipo del prototipo]]
1. [[Fase 1 - Daemon, árbol de sesiones y terminal integrado]] (en paralelo:
   [[Investigar la cuota restante de cada CLI]])
2. [[Fase 2 - Nueva sesión con salvaguardas]]
3. [[Fase 3 - Herramienta del equipo en Rust y monitor]]
4. [[Fase 4 - Acceso desde Android en titan-ssh]]

Fuera de las fases, cuando toque: [[Proteger main y configurar el repo al crearlo en GitHub]]
y [[ADR de distribución antes de la primera versión]].

## Cómo se trabaja

- **Con el equipo del prototipo** (decisión del usuario): Opus dirige, debate con
  el usuario, diseña y redacta los encargos; **Luna** (`gpt-6-luna`) implementa;
  otro hilo de **Luna** diseña los tests a partir del mismo encargo sin ver la
  implementación; **Sol** (`gpt-6.1-sol`) revisa y entrega un informe. Opus
  decide sobre cada informe. Tope de rondas en `.agents/team.toml`. Ojo con la
  cuota de ChatGPT Plus, sobre todo la de Sol.
- Opus se reserva lo que más pesa (dirigir, diseñar, decidir); el código y lo
  mecánico va a Luna.
- Flujo de ramas, PR y versiones: [[README#Publicación]]. Compuerta de
  «Hecha»: [[README#Seguridad, coste y cumplimiento]].
- Cada decisión técnica nueva y duradera (protocolo del daemon, formato de
  eventos, crates de terminal, etc.) es una ADR; cada superficie reutilizable,
  una entrada del catálogo técnico (sus vocabularios se acuerdan con el usuario
  al registrar la primera).

## Criterios de finalización

- Las fases 0 a 3 están `Hecha`; la 4 tiene su propia planificación con
  titan-ssh.

## Verificación

## Resultado
