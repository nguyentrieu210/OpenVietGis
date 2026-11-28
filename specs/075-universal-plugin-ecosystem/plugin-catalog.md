# Normative plugin catalog
Each entry is a delivery target, not a claim of existing capability. W=Web, D=Desktop, M=Mobile. Profiles are proposed; certify independently. Reuse existing engine/adapter before new implementation.

## Wave 1 — GIS Professional
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-001 | Spatial Analysis | buffer/overlay/spatial join with explicit CRS | compare polygon overlay area and attributes with independent fixtures | W/D; M field subset |
| PLG-002 | Topology Inspector | validate adjacency, gaps, overlaps, connectivity | find seeded overlap/gap and repair only approved features | W/D; M field subset |
| PLG-003 | Geometry Repair | preview and apply valid geometry repairs | bow-tie fixture repaired with audit and original retained | W/D; M field subset |
| PLG-004 | Network Analysis | graph routing, costs and restrictions | known graph shortest path and unreachable node | W/D; M field subset |
| PLG-005 | Spatial Statistics | density, clustering and spatial autocorrelation | reference statistic and reproducible seed | W/D; M field subset |
| PLG-006 | Geostatistics | IDW, kriging and uncertainty surface | holdout error and variogram parameters recorded | W/D; M field subset |
| PLG-007 | Coordinate Transformer | batch coordinate transforms and provenance | independent control points and reject missing CRS | W/D; M field subset |
| PLG-008 | Georeferencer | GCP fitting, residuals, warp and export | known affine control points and residual report | W/D; M field subset |
| PLG-009 | Advanced Digitizing | snapping, constraints and precise editing | known geometry tolerance and undo/redo | W/D; M field subset |
| PLG-010 | Spatial ETL | typed mapping, transform and scheduled import | round-trip schema, restart and idempotent rerun | W/D; M field subset |

## Wave 2 — AI & Autonomous GIS
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-011 | AI Copilot | natural-language scoped tool invocation | propose then confirm a mutation; deny cross-project tool | W/D; M capability-gated |
| PLG-012 | AI Agent Builder | versioned agent prompts, tools and policies | publish bounded agent and replay logged tool trace | W/D; M capability-gated |
| PLG-013 | Multi-Agent Studio | DAG orchestration, budgets and cancellation | parallel tasks converge and cancel without orphan jobs | W/D; M capability-gated |
| PLG-014 | AutoDigitizer | extract vectors from georeferenced imagery | labeled fixture precision/recall and review before commit | W/D; M capability-gated |
| PLG-015 | AI Object Detection | detect and geolocate objects | holdout precision/recall and pixel-to-world control | W/D; M capability-gated |
| PLG-016 | AI Land Classification | classify land cover using validated labels | confusion matrix and reproducible model version | W/D; M capability-gated |
| PLG-017 | AI Change Detection | compare georegistered imagery and detect changes | known changed/unchanged regions with uncertainty | W/D; M capability-gated |
| PLG-018 | AI Data Quality | detect schema and spatial anomalies | seeded anomalous rows and false-positive report | W/D; M capability-gated |
| PLG-019 | AI Predictive Analytics | fit spatial predictive models with uncertainty | spatial holdout baseline and no leakage | W/D; M capability-gated |
| PLG-020 | AI Report Generator | grounded summaries from scoped data | every numerical assertion traces to reproducible query | W/D; M capability-gated |

## Wave 3 — No-code & Low-code
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-021 | Form Builder | schema forms, validation and conditional fields | submit valid form and reject invalid server-side | W/D; M field subset |
| PLG-022 | Dashboard Builder | map/chart widgets and scoped filters | cross-filter consistent aggregates and permissions | W/D; M field subset |
| PLG-023 | Visual Model Builder | typed processing DAG, run and replay | execute buffer-to-overlay graph with saved outputs | W/D; M field subset |
| PLG-024 | Workflow Automation | event/schedule triggers and retry | duplicate event yields one business effect | W/D; M field subset |
| PLG-025 | App Builder | compose declarative workspaces and publish versions | publish and open a scoped application on supported hosts | W/D; M field subset |
| PLG-026 | Report Designer | templates, map frames, tables and exports | render paginated PDF with correct totals and scale | W/D; M field subset |
| PLG-027 | Business Rules Engine | versioned predicates and actions | same input yields same decisions and audit | W/D; M field subset |
| PLG-028 | Approval Workflow | state transitions, reviewer roles and separation | reject self-approval and stale transitions | W/D; M field subset |
| PLG-029 | Integration Builder | approved connectors and field mapping | map sandbox payload, handle rate limit and retries | W/D; M field subset |
| PLG-030 | Plugin Creator | generate manifest and validated contribution bundle | build, sign, install generated sample | W/D; M field subset |

## Wave 4 — Remote Sensing
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-031 | Sentinel Explorer | search authorized Sentinel products and load bands | known STAC fixture, attribution and cloud filter | W/D; M capability-gated |
| PLG-032 | Landsat Explorer | search Landsat scenes and bands | known scene metadata and georegistration | W/D; M capability-gated |
| PLG-033 | Vegetation Index | NDVI, NDRE, EVI and masks | pixel reference formulas with nodata and denominator guards | W/D; M capability-gated |
| PLG-034 | Water Index | NDWI/MNDWI and thresholds | reference raster values and masks | W/D; M capability-gated |
| PLG-035 | Land Cover Analysis | classify and summarize land cover | labeled reference area totals and class mapping | W/D; M capability-gated |
| PLG-036 | Time Series Analysis | align timestamps, aggregate and detect trends | known seasonal series and missing-date handling | W/D; M capability-gated |
| PLG-037 | Raster Calculator | bounded raster expressions and alignment | pixel oracle and explicit resampling | W/D; M capability-gated |
| PLG-038 | Thermal Analysis | calibrated surface temperature workflow | reference calibration and units with QA masks | W/D; M capability-gated |
| PLG-039 | SAR Processing | calibrate, filter and terrain-correct supported SAR | reference calibrated raster and provenance | W/D; M capability-gated |
| PLG-040 | Hyperspectral Analysis | spectral signatures and band analysis | known spectral library and similarity reference | W/D; M capability-gated |

## Wave 5 — CAD BIM Engineering
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-041 | OpenVietCAD Pro | advanced DXF editing and preservation | round-trip handles, layers, units and supported entities | W/D; M capability-gated |
| PLG-042 | BIM Viewer | IFC model hierarchy and selection | known IFC object GUID and coordinates | W/D; M capability-gated |
| PLG-043 | BIM-GIS Integration | georeference IFC objects and link GIS | known transform control and ID mapping | W/D; M capability-gated |
| PLG-044 | Civil Engineering | alignment and infrastructure design review | reference geometry and dimensional checks | W/D; M capability-gated |
| PLG-045 | Road Design | horizontal/vertical alignment and stationing | known chainage and curves | W/D; M capability-gated |
| PLG-046 | Pipe Network Design | network sizing and hydraulic input validation | known pipe connectivity and hydraulic reference case | W/D; M capability-gated |
| PLG-047 | Cross Section | sample terrain and produce profiles | analytic surface elevation and station oracle | W/D; M capability-gated |
| PLG-048 | Earthwork Calculator | cut/fill between surfaces | analytic prism volume with units | W/D; M capability-gated |
| PLG-049 | Construction Quantity | measure quantities by object and unit | known bill of quantities reconciles to source | W/D; M capability-gated |
| PLG-050 | As-Built Manager | version and compare as-built drawings | approved revision diff and rollback history | W/D; M capability-gated |

## Wave 6 — 3D LiDAR Drone
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-051 | Digital Twin | link asset IDs to time-varying 3D state | known event updates correct asset with stale-state warning | W/D; M capability-gated |
| PLG-052 | Point Cloud Studio | stream, clip and measure point clouds | bounded LOD memory and known distance | W/D; M capability-gated |
| PLG-053 | LiDAR Classification | classify supported point classes | labeled holdout metrics and class export | W/D; M capability-gated |
| PLG-054 | Drone Mapping | ingest georeferenced orthomosaic and footprints | control-point error and licensed output | W/D; M capability-gated |
| PLG-055 | Photogrammetry | reconstruct from calibrated photos | known GCP residual and reconstruction coverage | W/D; M capability-gated |
| PLG-056 | 3D City Builder | assemble terrain and building models | known heights and coordinates across LOD | W/D; M capability-gated |
| PLG-057 | Terrain Analysis | slope, aspect, contours and drainage | analytic DEM reference values | W/D; M capability-gated |
| PLG-058 | Viewshed Analysis | line-of-sight on elevation model | known obstruction and curvature assumptions | W/D; M capability-gated |
| PLG-059 | Solar Radiation | estimate terrain solar exposure | reference time/location and unit benchmark | W/D; M capability-gated |
| PLG-060 | Underground GIS | 3D utility depths and conflict visualization | known crossing, depth and clearance test | W/D; M capability-gated |

## Wave 7 — Agriculture
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-061 | AgriGIS | farm boundaries, crop lots, operations and dashboard | farm season rollup reconciles to field records | W/D; M field subset |
| PLG-062 | Plantation Manager | planting cohorts, stands and lifecycle | known stand counts and replacement history | W/D; M field subset |
| PLG-063 | Crop Monitoring | field observations and vegetation trend | time series linked to correct lot and season | W/D; M field subset |
| PLG-064 | Disease Mapping | observations, quarantine zones and spread history | seeded outbreak map and role-restricted updates | W/D; M field subset |
| PLG-065 | Irrigation Manager | zones, valves, pumps and irrigation schedules | water allocation and schedule conflict validation | W/D; M field subset |
| PLG-066 | Fertilizer Planner | nutrient targets and application records | unit-balanced nutrient plan and approval | W/D; M field subset |
| PLG-067 | Harvest Planner | cohort-based forecast and actual harvest | forecast reconciles to cohorts and actuals | W/D; M field subset |
| PLG-068 | Soil Analysis | sample locations and nutrient maps | sample units and interpolation provenance | W/D; M field subset |
| PLG-069 | Farm Machinery | equipment and field operation coverage | track-to-lot overlap and maintenance record | W/D; M field subset |
| PLG-070 | Agricultural Traceability | lot-to-batch chain and recall lookup | trace synthetic carton back to harvest lot | W/D; M field subset |

## Wave 8 — Infrastructure
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-071 | WaterGIS | water network assets, valves and isolation analysis | known connected network and isolated segment | W/D; M field subset |
| PLG-072 | PowerGIS | electrical asset topology and outage mapping | known feeder connectivity, no live switching | W/D; M field subset |
| PLG-073 | TelecomGIS | fiber routes, splices and capacity | trace known fiber path and spare capacity | W/D; M field subset |
| PLG-074 | DrainageGIS | drainage assets and catchments | known flow connectivity and outlet | W/D; M field subset |
| PLG-075 | Gas Network GIS | pipe assets and inspection map | known segment trace; no live control | W/D; M field subset |
| PLG-076 | Street Lighting | lighting assets and maintenance | fixture inventory and fault work order | W/D; M field subset |
| PLG-077 | Asset Management | asset identity, lifecycle and inspections | asset history and authorized transfer | W/D; M field subset |
| PLG-078 | Maintenance Manager | work orders, priorities and recurring plans | complete job and preserve maintenance audit | W/D; M field subset |
| PLG-079 | IoT Monitoring | authorized sensor feeds and alarms | duplicate/out-of-order telemetry and stale indicator | W/D; M field subset |
| PLG-080 | SCADA Integration | read-only industrial telemetry gateway | simulator alarms and zero write commands | W/D; M field subset |

## Wave 9 — Land Cadastral Planning
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-081 | LandGIS | parcel inventory and authorized records | parcel query/export with PII role enforcement | W/D; M field subset |
| PLG-082 | Cadastral Mapping | parcel map layout and topology checks | reference parcel topology, scale and legend | W/D; M field subset |
| PLG-083 | Parcel Editor | split, merge and lineage | area conservation within tolerance and revision audit | W/D; M field subset |
| PLG-084 | Land Registry | document linkage and review lifecycle | restricted document access and approved record history | W/D; M field subset |
| PLG-085 | PlanningGIS | planning layers and overlay queries | known parcel-zone overlay and effective dates | W/D; M field subset |
| PLG-086 | Land Valuation | scenario valuation using comparable inputs | reproducible calculation and uncertainty disclosure | W/D; M field subset |
| PLG-087 | Land Clearance | affected parcels and case tracking | affected area reconciliation and restricted records | W/D; M field subset |
| PLG-088 | Land Inventory | land use summaries by geography | known class totals reconcile to area | W/D; M field subset |
| PLG-089 | Land Change Monitoring | temporal parcel/land use comparison | known changed parcel and date provenance | W/D; M field subset |
| PLG-090 | Zoning Compliance | rule-based scenario compliance checks | versioned rule fixture and nonbinding result | W/D; M field subset |

## Wave 10 — Smart City Government
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-091 | House Numbering | reuse existing address proposal and approval | existing approved reference workflow without duplicate IDs | W/D; M field subset |
| PLG-092 | Address Registry | standardized address records and lookup | deduplicate known addresses and enforce publication scope | W/D; M field subset |
| PLG-093 | Administrative Boundary | versioned boundary layers and hierarchy | historical boundary fixture and valid parent relationships | W/D; M field subset |
| PLG-094 | Population GIS | aggregate demographic layers | small-cell suppression and totals reconciliation | W/D; M field subset |
| PLG-095 | Smart City Dashboard | scoped city indicators and alerts | known indicator totals with timestamps | W/D; M field subset |
| PLG-096 | Public Asset Manager | public assets and inspection history | inventory reconciliation and access controls | W/D; M field subset |
| PLG-097 | Citizen Reporting | geolocated issue intake and assignment | submit synthetic issue and track redacted status | W/D; M field subset |
| PLG-098 | Public Service GIS | service coverage and accessibility | known service catchment and routing assumptions | W/D; M field subset |
| PLG-099 | Urban Planning 3D | 3D planning scenario visualization | known height envelope and georeferencing | W/D; M field subset |
| PLG-100 | Waste Management | collection routes, points and service records | known service route and missed stop report | W/D; M field subset |

## Wave 11 — Environment
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-101 | ForestGIS | forest inventory and cover monitoring | known forest area and change reconciliation | W/D; M capability-gated |
| PLG-102 | Carbon Mapping | estimate stocks from documented coefficients | reference arithmetic and uncertainty range | W/D; M capability-gated |
| PLG-103 | Biodiversity GIS | species observations and restricted locations | sensitive coordinates hidden by role | W/D; M capability-gated |
| PLG-104 | Air Quality | sensor maps and temporal aggregates | known units, timestamps and missing data | W/D; M capability-gated |
| PLG-105 | Water Quality | sampling and indicator trends | reference units and threshold configuration | W/D; M capability-gated |
| PLG-106 | Flood Simulation | bounded calibrated inundation model | independent benchmark extent/depth and model limits | W/D; M capability-gated |
| PLG-107 | Landslide Risk | susceptibility mapping | holdout benchmark and non-deterministic risk disclaimer | W/D; M capability-gated |
| PLG-108 | Wildfire Monitoring | authorized fire observations and alerts | historical fixture and freshness/provenance | W/D; M capability-gated |
| PLG-109 | MiningGIS | authorized concession and extraction footprints | known boundary overlap and area calculation | W/D; M capability-gated |
| PLG-110 | Climate Risk | scenario exposure mapping | documented scenario, period and uncertainty | W/D; M capability-gated |

## Wave 12 — Transport Commerce
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-111 | LogisticsGIS | shipment locations and network visibility | synthetic shipment status and scoped map | W/D; M field subset |
| PLG-112 | Fleet Management | authorized vehicle tracks and status | out-of-order GPS and retention enforcement | W/D; M field subset |
| PLG-113 | Route Optimization | capacity/time-window routing | known feasible benchmark and objective value | W/D; M field subset |
| PLG-114 | Traffic Analysis | aggregate traffic flows and congestion | known temporal aggregates and units | W/D; M field subset |
| PLG-115 | Public Transport | routes, stops and schedules | GTFS fixture valid trips and stop sequence | W/D; M field subset |
| PLG-116 | Delivery Management | delivery stops and proof records | synthetic delivery completion and access rules | W/D; M field subset |
| PLG-117 | Retail Location | catchment and site scoring | known catchment and transparent weights | W/D; M field subset |
| PLG-118 | RealEstateGIS | authorized property inventory and comparisons | filter/compare with source dates and private fields | W/D; M field subset |
| PLG-119 | Market Intelligence | aggregated spatial market indicators | reproducible aggregates with privacy threshold | W/D; M field subset |
| PLG-120 | TourismGIS | points of interest and itineraries | known route and licensed POI attribution | W/D; M field subset |

## Wave 13 — Mobile Survey Field
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-121 | SurveyGIS | field survey forms, features and review | offline capture, sync and conflict resolution | M; W/D management |
| PLG-122 | GNSS Professional | supported RTK/NMEA/NTRIP adapters | real device fix metadata and independent control points | M; W/D management |
| PLG-123 | Offline Field Mapping | licensed scoped offline packages | airplane-mode edit and replay without duplicate | M; W/D management |
| PLG-124 | Field Task Manager | assign field tasks with map context | task state and permission across devices | M; W/D management |
| PLG-125 | QR Asset Tracking | scan asset code and scoped lookup | duplicate/unknown QR handling and no token disclosure | M; W/D management |
| PLG-126 | Photo Mapping | geotag photos with consent | capture and sync with configurable metadata stripping | M; W/D management |
| PLG-127 | Field Inspection | inspection checklists and evidence | signed-off inspection history and export | M; W/D management |
| PLG-128 | Workforce Tracking | consented work-session location collection | stop/revoke collection and retention verified | M; W/D management |
| PLG-129 | Mobile Data Collector | schema-driven offline data collection | valid submission and conflict replay after restart | M; W/D management |
| PLG-130 | AR Survey | device-supported AR measurements | real device calibration, uncertainty and fallback | M; W/D management |

## Wave 14 — Enterprise Integration
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-131 | QGIS Connector | reuse bidirectional sync and plugin registry | revision conflict and round-trip geometry | W/D; M capability-gated |
| PLG-132 | ArcGIS Connector | authorized service adapter | supported fixture import and explicit capability | W/D; M capability-gated |
| PLG-133 | MapInfo Connector | TAB/MIF data interchange | schema/CRS/geometry round-trip for supported subset | W/D; M capability-gated |
| PLG-134 | AutoCAD Connector | authorized CAD interchange | DXF entity mapping and loss report | W/D; M capability-gated |
| PLG-135 | ERP Connector | authorized mapping of business records | sandbox idempotent sync and scoped credentials | W/D; M capability-gated |
| PLG-136 | PostGIS Manager | scoped source administration via broker | deny arbitrary SQL and cross-tenant reads | W/D; M capability-gated |
| PLG-137 | Cloud Storage Connector | authorized object store access | scoped signed URLs and expiry test | W/D; M capability-gated |
| PLG-138 | OGC Service Manager | publish/consume qualified OGC services | protocol fixture and authorization enforcement | W/D; M capability-gated |
| PLG-139 | Realtime Data Stream | event ingestion and replay | duplicate/out-of-order and backpressure | W/D; M capability-gated |
| PLG-140 | Data Marketplace | licensed dataset discovery and distribution | license grant, provenance and download permission | W/D; M capability-gated |

## Wave 15 — Vietnam Professional
| ID | Plugin | Required workflow | Acceptance oracle | Target profile |
|---|---|---|---|---|
| PLG-141 | VN-2000 Professional | regional transforms using approved parameters | independent control points per enabled region | W/D; M field subset |
| PLG-142 | VietTools | Vietnamese workflow command collection | invoke existing commands without duplicate algorithms | W/D; M field subset |
| PLG-143 | VietMap Layout | localized map templates | known scale, legend, font and export | W/D; M field subset |
| PLG-144 | VietLand Records | localized authorized land record forms | required fields, review and PII policy | W/D; M field subset |
| PLG-145 | VietPlanning | localized planning schemas and symbols | effective-date overlay and source provenance | W/D; M field subset |
| PLG-146 | VietAddress | Vietnamese address normalization | labeled historic/current address fixture | W/D; M field subset |
| PLG-147 | VietAdministrative | effective-dated administrative hierarchy | known split/merge fixture with historical query | W/D; M field subset |
| PLG-148 | VietSurvey | localized survey workflow and field exports | control-point report and interoperable output | W/D; M field subset |
| PLG-149 | VietMap Symbols | licensed symbol catalog | render known legend and preserve attribution | W/D; M field subset |
| PLG-150 | VietPublic GIS | publish authorized public maps | redaction, review and revoked publication | W/D; M field subset |