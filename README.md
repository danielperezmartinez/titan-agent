# titan-agent

Torre de control para sesiones de agentes de IA: muestra qué sesiones de
Claude Code y Codex hay abiertas, en qué proyecto trabaja cada una y quién habla
con quién, y permite escribir en cualquiera de ellas desde una sola ventana. Es
la aplicación hermana de titan-ssh. Está escrita en Rust con GPUI.

- **Estado**: en desarrollo. La fase 1 (daemon `titan-agentd`, árbol de
  sesiones y terminal integrado) está en `main`.
- **Experimento de la fase 1**: la misma fase se hizo tres veces (un equipo
  multiagente con encargos grandes, Opus solo y un equipo con encargos
  pequeños) para decidir cómo se trabaja en las siguientes. Resultados, métricas
  y capturas en la
  [página del experimento](https://danielperezmartinez.github.io/titan-agent/fase-1/)
  (fuente en [`experimentos/fase-1/`](experimentos/fase-1/index.html)).
- **Documentación**: la bóveda de Obsidian en [`docs/`](docs/Inicio.md) es la
  fuente de verdad del proyecto (reglas, tareas y decisiones).
