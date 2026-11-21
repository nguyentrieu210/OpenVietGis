# UI Contract: Accessibility & Responsive Behavior

## Map

- Expose map as a named region.
- Keep supported keyboard pan/zoom enabled.
- Provide visible zoom/navigation controls.
- Respect reduced motion for non-essential camera animation.
- Provide synchronized textual/table feature information for assistive technology.

## Layer Tree

- Every icon-only action has an accessible name.
- Visibility/active state is communicated beyond color alone.
- Drag reorder, if present, has Move Up / Move Down controls and keyboard operation.

## Attribute Table

- Header/row semantics remain understandable.
- Selected state is exposed programmatically.
- On narrow screens, table containment must not break page width; use an intentional scroll/card/sheet strategy.
- Do not render an unbounded row set.

## Resizable panels

If custom drag separators are used:
- expose separator semantics/orientation;
- support keyboard size adjustments or preset collapse/expand controls;
- never make dragging the only means of changing layout.

## Focus

- Visible focus for all primary non-canvas controls.
- Dialog/drawer focus returns to invoking control when closed.
- No keyboard traps.
