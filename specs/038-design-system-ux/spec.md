# Spec 038 — Design system and workspace UX
Parent R004/R006/R015; depends on Spec 033–037.
- Default dock is concise, with a clear full/compact toggle stored per workspace. Advanced commands remain available in full dock and searchable ribbon; no command implementation removed. Preserve floating palettes and custom layouts.
- Search entry appears first in ribbon, searches aliases/labels/hints, and focuses its search field. Escape and explicit close restore trigger focus. Narrow ribbon scrolls horizontally rather than overflowing viewport.
- Disabled reasons shared by command line, ribbon, toolbar tooltips and suggestions. Visible command feedback and focus indicators; use pressed state for toggles.
- GIS command keyboard, menu-dispatch and context handling must not execute commands while GIS surface is inert. Close ribbon portal when changing modes. Do not capture letters on ordinary buttons or Escape on unrelated form inputs.
- Validate compact/full access, keyboard focus, 390px viewport, alternate mode shortcut isolation and existing 13 workbench paths.
Non-goals: no new GIS feature, total visual rewrite, changed permission or backend semantics, or live deploy.
