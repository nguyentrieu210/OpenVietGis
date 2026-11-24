# 050 — Terrain Analysis & Release Consolidation
## Scope
- Reconcile production 0aa2cfa and GitHub main edb5175 without reverting the deployed managed-session isolation, revision validation, module compatibility, transport or QGIS snapshot/delta safeguards. Preserve main's renderer-neutral label and documented 10-million-point SQL benchmark. Both histories remain ancestors.
- Support cut/fill between two DEM surfaces only when their validated sampled grids and explicitly declared vertical reference match; require common valid cells. Do not silently reproject or vertically transform a surface.
- Condition depressions with Priority-Flood, route across flats through earlier visited cells, and compute contributing areas on the conditioned acyclic graph. NoData is a barrier and disconnected islands without an outer-edge outlet are excluded explicitly.
- Distinguish boundary-connected below-threshold inundation from isolated low cells; four-neighbour cell connectivity and all four valid corners below threshold. This is a static screening mask, not a rainfall/runoff or hydraulic model.
- Preserve existing raw D8, flat design level, original-resolution GDAL raster tools, terrain view, and project-authorized DEM endpoint.
## Limits
Still uses a bounded sampled DEM and approximate Web Mercator metric correction. No claim of field-verified engineering accuracy, constrained breakline TIN, full-resolution native project DEM streaming, calibrated vertical transformations, or dynamic hydraulic simulation.
