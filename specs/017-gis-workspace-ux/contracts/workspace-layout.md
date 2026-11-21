# UI Contract: Workspace Layout

## Desktop (≥1280 px)

- Persistent App Shell around Project Map
- Left: Layer panel
- Center: Map canvas
- Right: Inspector / contextual details
- Bottom: Attribute Table, collapsible and vertically resizable
- Top: map command area and save state

Map canvas remains mounted while sibling panels collapse/resize.

## Tablet (768–1279 px)

- Map remains primary.
- Side panels collapse or switch rather than forcing full simultaneous width.
- Attribute Table may occupy a larger temporary panel state.
- Current Map camera/valid selection remains intact through layout changes.

## Mobile (<768 px)

Initial scope:
- map viewing/navigation
- Layer visibility/list
- focused feature details/basic selection

Out of initial mobile parity:
- dense multi-panel layout
- advanced editing/processing/cartography workflows

## Personal preference

Panel collapse/size preferences may persist locally. They are not shared Map configuration and never create a shared unsaved state.
