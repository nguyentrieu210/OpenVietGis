# W00/W01 provider qualification checkpoint
Status: partial, no production provider enabled. This is not the full Revision 2 kit.

COLMAP 4.x command planner: CPU sparse and candidate CUDA/HIP dense. The host must stage canonical inputs and select registered model, validate a per-image source-view list, verify executable digest and signed pack, apply process containment and collect measured evidence. The planner does not invoke a process. PLY remains in reconstruction coordinates until geodetic qualification. No claims of Metal dense support.

Reference: https://colmap.github.io/cli.html and https://colmap.github.io/faq.html . Exact installed release CLI help and real fixture are required before accepting each flag; current docs do not prove compatibility with the chosen 4.2.1 binary.

Resource accounting: checked arithmetic, duplicate reservation refusal, atomic fail/no mutation, explicit release. Shared host plus per-GPU accounting must be composed atomically; real memory, scratch and process-tree enforcement are still open.

## Qualification gates
- W00-T03: real rights-cleared fixture hash manifest, actual binary SHA256, exact CLI help, CPU sparse registered cameras and GPU dense point output, measured host/GPU/scratch peaks, compare expected coordinates and completeness.
- W00-T04: audit exact license texts, third-party models, dependencies and redistribution terms, signed SBOM and pack hash. No approval implied by this document.
- W00-T05: Windows reference machine IPC, host grant, bounded local data plane preview; no silent upload.
- W00-T07: hostile image, path/symlink, worker escape, cancellation, timeout, expiry, replay, OOM and disk-full cases.
- W01: real hardware probes, Windows Job Objects, Linux containment, macOS qualified watchdog, device-specific telemetry and scheduler integration remain open.

The existing provider negotiation and new planner/accounting tests are unit tests only. No photogrammetry result has been produced.
