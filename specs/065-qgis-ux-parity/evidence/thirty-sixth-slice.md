# Thirty-sixth implementation slice (GitHub only)

- DOCK-01 / COLLAB-07: separator drag now tracks one primary pointer, commits only on matching pointer-up, cancels on Escape, blur, capture loss, pointer cancel or unmount, and removes ghost/listeners. Keyboard arrows, Shift fine adjustment, Home/End and ARIA numeric range added. Pure keyboard direction/bounds test cases added.
- DOCK-09: reconciled previously unverified manifest row with existing bounded secondary map source. No claim of complete QGIS parity.
- Source commits: `4d87e48244971a45d93393a58d939e692f3be901`, `846cf703efd8fe86714be25ca114131188798ee8`, `2765b5828aec9ea9ea736f9ce713e406a36c4841`.
- Tests written but not executed; no browser, typecheck, build, merge or deployment performed. All acceptance tasks stay open pending required evidence.
