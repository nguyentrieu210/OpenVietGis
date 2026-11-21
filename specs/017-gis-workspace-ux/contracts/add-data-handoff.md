# UI Contract: Add Data

## Entry point

“Thêm dữ liệu” is available from the Map workspace and may also be linked from Project Home.

## Existing Layer path

1. Read currently authorized catalog Layers.
2. Exclude or clearly mark Layers already referenced by the Map.
3. Display human-readable name and user-facing usable state.
4. Add the chosen Layer using the parent R004 Map Layer reference contract.
5. Return focus/context to the workspace and expose the new reference.

Removing the reference later does not remove the catalog Layer/source.

## Import handoff

If an accepted R005 import capability is present:

1. Show “Nhập dữ liệu mới”.
2. Hand off to the canonical import flow.
3. Do not duplicate file validation, CRS confirmation, job/provenance/publication logic in Map UX.
4. After a successful published result is available, return or offer navigation back so the Layer can be added to the Map.

If R005 is absent/unavailable, the import choice is omitted or disabled with a non-sensitive explanation.

## Duplicate behavior

Silent accidental duplicate Map references are prohibited. The default is to omit already-added Layers from the normal addable list or label them as already present.
