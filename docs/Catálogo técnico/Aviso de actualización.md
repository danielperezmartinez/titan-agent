---
Nombre: "Aviso de actualización"
Tipo: "Componente UI"
Área: "Interfaz"
Feature: "Base"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent/src/app/update_notice.rs"
Entrada pública: "MainView::update_notice (cabecera) y MainView::update_settings (Configuración)"
Resumen: "El aviso [^] de la cabecera con sus estados (lista, descargando, confirmación en línea, actualizando, fallo, vuelta atrás con [x], daemon de otra versión) y el bloque ACTUALIZACIONES de Configuración (versión instalada, interruptor, Buscar ahora, última comprobación). Tras actualizar se reabre con el lanzador; confirma la versión nueva borrando update/pending.json tras 10 s con un daemon de su versión. TITAN_AGENT_DEMO_UPDATE fija un estado para las capturas del kit de demo."
Última modificación: "2026-11-17T20:40:00+01:00"
---

# Aviso de actualización

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent/src/app/update_notice.rs)
como fuente de verdad de su contrato detallado. Decisión visual: [[Decisiones visuales/Aviso de actualización en la cabecera]]; previa en `design/preview/Actualizacion.dc.html`.
