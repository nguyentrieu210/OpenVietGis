# Wave 6 — context compaction
Base Wave 5 PR #109 at 5494933029959b0484a3becbb0847e9ef66e1bcc. Adapted RustGIS assistant/compact.rs at aea84131974b57ff6359666dbe79f88054a64bf1, Apache-2.0.

- Preserve original user input and full bounded in-memory execution ledger (8 MB). Deterministic projections only compact complete successful call/result groups; retain recent full groups where possible.
- Soft threshold 400,000 serialized UTF-8 bytes, target 200,000; on recognized context overflow try once with 48,000-byte target. Bytes are not exact token counts.
- Checkpoint keeps every compacted call ID, tool name, SHA-256 arguments/results, selected exact scalar GIS/receipt metadata and explicitly truncated result excerpt. No claim all feature rows, geometries or arbitrary fields remain in context. Re-read if needed, never repeat uncertain writes.
- Original authorization, workspace scope, duplicate mutation fingerprints, unresolved receipt and shared budget remain outside projection. No compaction during an incomplete tool batch. Provider-native histories keyed by monotonic local call ID, preserving opaque reasoning blocks/signatures for retained groups. No cross-run cache or persistence.
- No secondary model summary in this wave: deterministic checkpoint avoids additional token spend and fabricated receipt state. Native provider body cap remains 2 MB; raw history cap and deadline fail closed. Overflow retry counted in shared model attempt budget.
- Report compaction statistics in Copilot and child reports. Tests cover UTF-8, exact facts, unchanged ledger, incomplete/failed pairs and provider error classification. Real provider compatibility and token/cost benchmark not yet verified.
