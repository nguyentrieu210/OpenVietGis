# Research — QGIS 3.44 làm chuẩn chức năng

## Reference baseline
Chọn tài liệu QGIS Desktop 3.44 (không giả định đây là bản mới nhất) vì cần mốc cố định, có hướng dẫn chính thức đủ chi tiết. Đối chiếu theo hành vi và kết quả, không buộc dùng Qt/PyQGIS trong web.

- GUI, menu, panels, toolbar, status bar: https://docs.qgis.org/3.44/en/docs/user_manual/introduction/qgis_gui.html
- UI training: https://docs.qgis.org/3.44/en/docs/training_manual/introduction/overview.html
- Configuration, profiles, shortcuts: https://docs.qgis.org/3.44/en/docs/user_manual/introduction/qgis_configuration.html
- Browser: https://docs.qgis.org/3.44/en/docs/user_manual/introduction/browser.html
- General tools, map interaction and layers: https://docs.qgis.org/3.44/en/docs/user_manual/introduction/general_tools.html
- Source manager: https://docs.qgis.org/3.44/en/docs/user_manual/managing_data_source/opening_data.html
- Properties: https://docs.qgis.org/3.44/en/docs/user_manual/working_with_vector/vector_properties.html
- Attribute table: https://docs.qgis.org/3.44/en/docs/user_manual/working_with_vector/field_calculator.html
- Editing: https://docs.qgis.org/3.44/en/docs/user_manual/working_with_vector/editing_geometry_attributes.html
- Expressions: https://docs.qgis.org/3.44/en/docs/user_manual/expressions/expression.html
- Raster properties: https://docs.qgis.org/3.44/en/docs/user_manual/working_with_raster/raster_properties.html
- Processing framework/toolbox/history/modeler/batch: https://docs.qgis.org/3.44/en/docs/user_manual/processing/index.html
- Georeferencer: https://docs.qgis.org/3.44/en/docs/user_manual/working_with_raster/georeferencer.html
- Layout: https://docs.qgis.org/3.44/en/docs/user_manual/print_layout/overview_layout.html
- 3D: https://docs.qgis.org/3.44/en/docs/user_manual/map_views/3d_map_view.html

## Source inventory and findings
Reviewed at 5bfc83826161acd83b98a925e85690cebde53ae1: QgisWorkspace (toolbar, tree, table, CRS/status), DockLayout (limited dock types with localStorage), LayerProperties (7 tabs), CoreMapTools and AdvancedEditor (navigation/edit/undo), ProfessionalTools (topology/statistics/field calculator), ProcessingToolbox/Modeler, LayoutDesigner, SpatialWorkspace, LiveApp, CadEditorPanel, AI copilot. Existing tests in apps/web-next and tests/e2e; their presence is not proof of latest live parity.

- Spec 017 establishes coherent GIS workspace and synchronized attribute selection.
- Spec 038 already requires concise commands, focus and isolated shortcuts.
- Spec 056/057 cover layer grouping and docking; verify before rewriting.
- Spec 062 requires single authenticated frontend, protected offline and no regression.
- Spec 064 requires capability-driven Rust platform. Do not reintroduce Python services for migrated paths.
- Latest #81 intentionally removed GuestStudio. Revert only relevant features, preserve prior fixes.
- Most significant source-visible gap: table filtering scoped to loaded input rows for remote page; full-source filters/sort/count must be distinguished from local page filters. Dock storage currently uses unscoped localStorage key(s); audit all keys. Seven properties tabs cannot imply complete QGIS layer property parity.
- AI live server native source exists; real full-session MCP consumer validation is still separate gate, not inferred from route health.

## Decisions
1. Extend canonical shell and existing components; reject duplicate SPA or general rewrite.
2. Define one command/capability model, separate execution from presentation; reuse existing commands rather than parallel implementations.
3. Personal preferences separate shared versioned project config; no automatic promotion of local state to shared map.
4. Full-source query semantics server-authoritative; client caches only safe view.
5. QGIS 3.44 fixture project and same authorized OVG test data establish reproducible comparison. Do not copy user production data into fixtures.
6. Retain engine-specific CAD/3D controls and Python desktop plugin as interoperability adapters; parity target applies GIS workflow.
7. No screenshots or login browser access in this run. Visual audit, keyboard behavior, latency and parity verdict remain explicit baseline tasks. No numeric UX score claimed.

## Baseline evidence workflow
Capture same steps in QGIS 3.44 and OVG on same viewport and fixture; record version, resolution, OS/browser, source size, network/CPU profile, steps, clicks, time, expected/result and screenshot paths. Pair source references with real authenticated browser evidence. Missing capability or blocked access is recorded, not waived.
