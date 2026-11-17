---
Nombre: "Actualizador"
Tipo: "Modelo de datos"
Área: "Núcleo"
Feature: "Base"
Estado: "Vigente"
Ámbito: "Interno"
Fuente: "crates/titan-agent-core/src/update.rs"
Entrada pública: "titan_agent_core::update::{verify_manifest, select, check_download, check_entry, check_bundle, bundle_root, apply_blockers, safe_to_apply, launch_step, install_root, versions_to_remove, accepted_keys, Manifest, Artifact, Version, UpdateError, Pending, RolledBack, APP_ID}"
Resumen: "Lógica pura del actualizador de ADR-0011, sin E/S: el manifiesto firmado con minisign (formato 1, comentario de confianza con la versión, lista de claves aceptadas compilada desde packaging/minisign.pub), SemVer estricto 0.x.y, la elección del artefacto (estrictamente mayor, fuera de bad.json, nombre seguro, ≤ 256 MiB, URL construida por la app), tamaño y SHA-256 de la descarga, las entradas del tar.gz dentro de su carpeta y los cuatro binarios, la condición segura para aplicar, los pasos del lanzador (contar intentos y volver atrás al tercero) y qué versiones se conservan. Errores con code() estable."
Última modificación: "2026-11-17T20:40:00+01:00"
---

# Actualizador

Después de descubrir esta pieza en el catálogo, consulta [su implementación](../../crates/titan-agent-core/src/update.rs)
como fuente de verdad de su contrato detallado. Tests de aceptación en `crates/titan-agent-core/tests/update_acceptance.rs` (C1–C5 de [[Tareas/Versión 0.6.0 - Instalador y actualización automática]]).
