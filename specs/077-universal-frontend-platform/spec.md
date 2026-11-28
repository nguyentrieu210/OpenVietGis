# SPEC 077 — Universal Frontend Platform & Design System
Status: Proposed; W0–W9 foundations implemented in varying scope; W10 Web token/facade, account, CRS and table virtualization integrated; real MapLibre docking fixture and opt-in Studio Dockview beta pass; native and plugin stacked integration PRs #120/#121; full domain migration and remote/native acceptance pending.
Extends SPEC 072 (not a second framework), 038, 062, 070/071; integrates 073/074/075/076 and AI companion PRs 113–116.
Baseline main: a4ae85a5e3d956de1870dcf65ff3fb21da7f0367. Repository license AGPL-3.0.

## User stories
- GIS operator: consistent dense keyboard-first map, layers, tables, dialogs, docking and saved workspaces on browser and desktop, without loss of editing state.
- Field surveyor: touch-first native Flutter shell with shared semantics, offline operation and accessible targets, not desktop UI squeezed onto phone.
- Plugin author: register versioned commands, forms, actions and panels once; host renders allowed schema on supported platforms and denies unsupported capabilities.
- Maintainer: change tokens once; generated CSS/Dart outputs, typed components, controlled migrations, measurable performance and rollback.

## Requirements
FR01 One semantic token source (DTCG-compatible JSON), generate deterministic CSS variables, prefixed Tailwind 4 theme and Dart ThemeData; light/dark/system, density, typography, motion, contrast, locale. No global preflight.
FR02 Extract SPEC072 primitives into owned React package with compatibility facade; shadcn source patterns + Base UI candidate after peer-dependency/license/accessibility spike. Existing UI calls remain compatible during migration.
FR03 Web and Tauri share one React frontend; retain distinct HTTP/IPC transport adapters. Mobile uses native Flutter components generated from the same tokens, with Material 3 as foundation; shadcn_flutter optional pending spike. No Tailwind on Flutter, no additional desktop frontend.
FR04 Workbench supports tabs, splits, docks, keyboard, serialized versioned layout, reset/recovery; evaluate Dockview behind adapter against current DockLayout/Splitter; preserve renderer lifecycle, map focus, edit transactions and per-account/project preferences. Floating native windows only after capability proof.
FR05 Virtualized data grid, tree, property editor, form, CRS selector, symbology, legend, CAD input, processing/modeler, survey/GPS, AI panel and photo tile inspector. TanStack Table/Virtual candidates; renderer, geometry, queries and jobs stay in existing SDK/core.
FR06 Single command registry with stable IDs, localization, keyboard conflict resolution, capability flags, disabled reasons, menu/ribbon/palette adapters; extend existing CoreCommand and plugin UI registry.
FR07 Extend SPEC075 schema and renderer contracts; never create a rival plugin runtime. Versioned bounded declarative UI, allowlisted components/actions, validation on both hosts, no arbitrary JS/Dart/HTML/CSS or privileged IPC for third-party plugins. Capability checks server/native side; tenant context isolation.
FR08 Accessibility: WCAG 2.2 AA target for applicable web UI; keyboard focus, tab/escape, screen reader names, high contrast, reduced motion; Flutter semantics and >=48 logical pixel touch target where practical; desktop density does not override keyboard access.
FR09 Migrate by domain with feature gates, screenshot/interaction tests, no global destructive CSS edits. Delete only with zero references and verified parity. Map vendor styles, imagery and print output isolated.
FR10 SDK public exports, semver, changelog, compatibility adapters and examples; story catalog React/Flutter; license notices and dependency pins. No backend/API behavior change solely for cosmetics.
FR11 No regression of auth, offline, tenant guards, geospatial correctness, map/CAD/3D, plugin lifecycle and export. Preserve existing SPEC acceptance rather than marking it done by implication.

## Nonfunctional acceptance
W0 inventory and reproducible static checks; W1–W11 implementation and measured evidence separately. Baseline on pinned main and platform heads, comparable datasets/device/browser. Target p95 input response <=100ms for ordinary controls, no >10% regression in map frame time or initial JS transferred size versus measured baseline without explicit exception, 100k-row virtual table with bounded mounted rows, no outer overflow at 360/390/768/1024/1440/1920 px, no focus trap escape or hidden actions. Targets not measured at W0. Test Windows/Linux/macOS Tauri and Android/iOS on available approved runners; unavailable platforms explicitly unqualified. No automatic production rollout.
